//! Builds the `Roo::Excelx::Cell` objects that
//! `Roo::Excelx::SheetDoc#cell_from_xml` builds, by calling roo's own
//! constructors with the same arguments, or only the values those cells
//! return from `#value`.

use std::cell::{Cell as StdCell, RefCell};
use std::os::raw::c_int;

use magnus::gc::Marker;
use magnus::rb_sys::{protect, AsRawValue, FromRawValue};
use magnus::value::{Lazy, Opaque, ReprValue};
use magnus::{
    prelude::*, DataTypeFunctions, Error, IntoValue, RArray, RClass, RHash, RString, Ruby,
    StaticSymbol, Symbol, TypedData, Value,
};

use crate::reader::{parse_index, Body, Kind, Mode, RawCell, RawRow, Reference, Style};

fn constant<T: magnus::TryConvert>(ruby: &Ruby, path: &str) -> T {
    ruby.eval(path).expect("roo is loaded before roo/calamine")
}

static COORDINATE: Lazy<RClass> = Lazy::new(|r| constant(r, "Roo::Excelx::Coordinate"));
static CELL_STRING: Lazy<RClass> = Lazy::new(|r| constant(r, "Roo::Excelx::Cell::String"));
static CELL_BOOLEAN: Lazy<RClass> = Lazy::new(|r| constant(r, "Roo::Excelx::Cell::Boolean"));
static CELL_NUMBER: Lazy<RClass> = Lazy::new(|r| constant(r, "Roo::Excelx::Cell::Number"));
static CELL_DATE: Lazy<RClass> = Lazy::new(|r| constant(r, "Roo::Excelx::Cell::Date"));
static CELL_DATETIME: Lazy<RClass> = Lazy::new(|r| constant(r, "Roo::Excelx::Cell::DateTime"));
static CELL_TIME: Lazy<RClass> = Lazy::new(|r| constant(r, "Roo::Excelx::Cell::Time"));
static CELL_EMPTY: Lazy<RClass> = Lazy::new(|r| constant(r, "Roo::Excelx::Cell::Empty"));
static NUMERIC_OR_FORMULA: Lazy<StaticSymbol> = Lazy::new(|r| r.sym_new("numeric_or_formula"));

/// `string[/<([biu]|sup|sub)>/]`, the test in `SharedStrings#use_html?`.
fn has_html_tag(string: &[u8]) -> bool {
    string.windows(3).enumerate().any(|(i, w)| {
        w[0] == b'<'
            && (matches!(w, [_, b'b' | b'i' | b'u', b'>'])
                || string[i..].starts_with(b"<sup>")
                || string[i..].starts_with(b"<sub>"))
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ValueType {
    Date,
    DateTime,
    Time,
    /// `:float`, `:percentage`, and any other format type roo builds a
    /// `Cell::Number` for.
    Number,
}

/// Whether a format string matches `/%/` or `/\.0/`, which makes
/// `Cell::Number#create_numeric` read every number as a Float.
fn reads_as_float(format: Value) -> Result<bool, Error> {
    let format = RString::try_convert(format)?;
    let format = unsafe { format.as_slice() };
    Ok(format.contains(&b'%') || format.windows(2).any(|pair| pair == b".0"))
}

/// `/\A[-+]?\d+E[-+]?\d+\z/i`, the exponent test in `Cell::Number#create_numeric`.
fn is_exponent_number(text: &str) -> bool {
    let digits = |part: &str| {
        let part = part.strip_prefix(['-', '+']).unwrap_or(part);
        !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit())
    };
    match text.split_once(['E', 'e']) {
        Some((mantissa, exponent)) => digits(mantissa) && digits(exponent),
        None => false,
    }
}

/// Ruby's `Integer(text, 10)` when `badcheck`, else `text.to_i`: the C
/// function both call, on a String, so errors read as Ruby's do.
fn ruby_integer(ruby: &Ruby, text: &str, badcheck: bool) -> Result<Value, Error> {
    let text = ruby.str_new(text);
    let raw =
        protect(|| unsafe { rb_sys::rb_str_to_inum(text.as_raw(), 10, c_int::from(badcheck)) })?;
    Ok(unsafe { Value::from_raw(raw) })
}

/// Ruby's `Float(text)`: the C function it calls, on a String.
fn ruby_float(ruby: &Ruby, text: &str) -> Result<Value, Error> {
    let text = ruby.str_new(text);
    let mut number = 0.0;
    protect(|| {
        number = unsafe { rb_sys::rb_str_to_dbl(text.as_raw(), 1) };
        rb_sys::Qnil as rb_sys::VALUE
    })?;
    Ok(ruby.float_from_f64(number).as_value())
}

/// Turns raw rows into roo cells for one worksheet. The Ruby objects it keeps
/// are pinned by [`DataTypeFunctions::mark`]; caches that may be filled while
/// Ruby runs live in `Cell`/`RefCell` fields that `mark` does not borrow.
#[derive(TypedData)]
#[magnus(
    class = "Roo::Calamine::Native::Converter",
    mark,
    free_immediately,
    size
)]
pub struct Converter {
    mode: Mode,
    empty_cell: bool,
    context: Opaque<Value>,
    hyperlinks: Option<Opaque<RHash>>,
    base_date: Opaque<Value>,
    base_timestamp: Opaque<Value>,
    /// Format string per style index, filled on first use.
    formats: Opaque<RArray>,
    types: RefCell<Vec<Option<ValueType>>>,
    /// roo's own shared-string arrays: plain text, and the HTML version
    /// (`nil` when the HTML wrapper is disabled). Fetched on first use.
    shared_plain: StdCell<Option<Opaque<RArray>>>,
    shared_html: StdCell<Option<Opaque<Value>>>,
    /// `Roo::Excelx::ERROR_VALUES`, which `Cell::Number` keeps as text.
    error_values: Vec<String>,
}

impl DataTypeFunctions for Converter {
    fn mark(&self, marker: &Marker) {
        marker.mark(self.context);
        if let Some(hyperlinks) = self.hyperlinks {
            marker.mark(hyperlinks);
        }
        marker.mark(self.base_date);
        marker.mark(self.base_timestamp);
        marker.mark(self.formats);
        if let Some(plain) = self.shared_plain.get() {
            marker.mark(plain);
        }
        if let Some(html) = self.shared_html.get() {
            marker.mark(html);
        }
    }
}

impl Converter {
    /// `context` is a `Roo::Calamine::Context`.
    pub fn new(ruby: &Ruby, context: Value, stream: bool) -> Result<Self, Error> {
        let hyperlinks: RHash = context.funcall("hyperlinks", ())?;
        Ok(Self {
            mode: if stream { Mode::Stream } else { Mode::Document },
            empty_cell: context.funcall::<_, _, Value>("empty_cell", ())?.to_bool(),
            context: context.into(),
            hyperlinks: (!hyperlinks.is_empty()).then(|| hyperlinks.into()),
            base_date: context.funcall::<_, _, Value>("base_date", ())?.into(),
            base_timestamp: context.funcall::<_, _, Value>("base_timestamp", ())?.into(),
            formats: ruby.ary_new().into(),
            types: RefCell::new(Vec::new()),
            shared_plain: StdCell::new(None),
            shared_html: StdCell::new(None),
            error_values: ruby.eval("Roo::Excelx::ERROR_VALUES.to_a")?,
        })
    }

    /// `Roo::Calamine.each_row_values` for a streamed row: the `value` of each
    /// cell that `Sheet#each_row(pad_cells: true)` yields, and `nil` for each
    /// cell it pads.
    pub fn row_values(ruby: &Ruby, rb_self: &Self, row: &crate::Row) -> Result<RArray, Error> {
        let row = row.raw();
        let values = ruby.ary_new_capa(row.cells.len());
        let mut last_column = 0;
        for raw in &row.cells {
            let (column, value) = rb_self.value(ruby, row, raw)?;
            for _ in last_column + 1..column {
                values.push(ruby.qnil())?;
            }
            values.push(value)?;
            last_column = column;
        }
        Ok(values)
    }

    /// The column of a streamed cell and its `value`. A cell with a
    /// hyperlink, a DateTime, or a Time is built as a roo cell and asked for
    /// its value; every other value follows roo's rule without a cell.
    fn value(&self, ruby: &Ruby, row: &RawRow, raw: &RawCell) -> Result<(i64, Value), Error> {
        let column = match (&raw.reference, self.hyperlinks) {
            (Reference::At { column, .. }, None) => *column,
            _ => {
                let coordinate = self.coordinate(ruby, raw, 0, 0)?;
                let column = coordinate.funcall("column", ())?;
                if !self.link(ruby, coordinate).is_nil() {
                    return Ok((column, self.cell_value(ruby, row, raw)?));
                }
                column
            }
        };
        let text = match &raw.body {
            Body::Empty => return Ok((column, ruby.qnil().as_value())),
            Body::Inline(text) => return Ok((column, ruby.str_new(row.text(text)).as_value())),
            Body::Value(text) => row.text(text),
        };
        let (format, format_type) = self.style_info(ruby, self.style(ruby, raw)?)?;
        let value = match (raw.kind, format_type) {
            (Kind::Shared, _) => self.shared_string(ruby, text)?,
            (Kind::Boolean, _) => {
                let one = i64::try_convert(ruby_integer(ruby, text, false)?).ok() == Some(1);
                one.into_value_with(ruby)
            }
            (Kind::Str, _) => ruby.str_new(text).as_value(),
            (Kind::InlineStr, _) | (Kind::ByFormat, ValueType::Number) => {
                self.number(ruby, text, reads_as_float(format)?)?
            }
            (Kind::ByFormat, ValueType::Date) => ruby
                .get_inner(self.base_date)
                .funcall("+", (ruby_integer(ruby, text, false)?,))?,
            (Kind::ByFormat, ValueType::DateTime | ValueType::Time) => {
                self.cell_value(ruby, row, raw)?
            }
        };
        Ok((column, value))
    }

    /// `Cell::Number#create_numeric`.
    fn number(&self, ruby: &Ruby, text: &str, float_format: bool) -> Result<Value, Error> {
        if self.error_values.iter().any(|error| error == text) {
            return Ok(ruby.str_new(text).as_value());
        }
        if text.is_empty() {
            return Ok(ruby.qnil().as_value());
        }
        if float_format || text.contains('.') || is_exponent_number(text) {
            ruby_float(ruby, text)
        } else {
            ruby_integer(ruby, text, true)
        }
    }

    /// The `value` of the roo cell built for a streamed cell.
    fn cell_value(&self, ruby: &Ruby, row: &RawRow, raw: &RawCell) -> Result<Value, Error> {
        match self.cell(ruby, row, raw, 0, 0)? {
            Some((_, cell)) => cell.funcall("value", ()),
            None => Ok(ruby.qnil().as_value()),
        }
    }

    /// The hyperlink target `SheetDoc#hyperlinks` holds for a coordinate, or `nil`.
    fn link(&self, ruby: &Ruby, coordinate: Value) -> Value {
        match self.hyperlinks {
            Some(hyperlinks) => ruby
                .get_inner(hyperlinks)
                .get(coordinate)
                .unwrap_or_else(|| ruby.qnil().as_value()),
            None => ruby.qnil().as_value(),
        }
    }

    /// `SheetDoc#each_cell` for a streamed row: every cell, empty ones too.
    pub fn row_cells(ruby: &Ruby, rb_self: &Self, row: &crate::Row) -> Result<RArray, Error> {
        let cells = ruby.ary_new_capa(row.raw().cells.len());
        for (index, raw) in row.raw().cells.iter().enumerate() {
            if let Some((_, cell)) = rb_self.cell(ruby, row.raw(), raw, 0, index + 1)? {
                cells.push(cell)?;
            }
        }
        Ok(cells)
    }

    /// `SheetDoc#extract_cells` for one row, adding to `cells`.
    pub fn add_row(
        &self,
        ruby: &Ruby,
        cells: RHash,
        row: &RawRow,
        row_number: usize,
    ) -> Result<(), Error> {
        for (index, raw) in row.cells.iter().enumerate() {
            if let Some((coordinate, cell)) = self.cell(ruby, row, raw, row_number, index + 1)? {
                cells.aset(coordinate, cell)?;
            }
        }
        Ok(())
    }

    fn context(&self, ruby: &Ruby) -> Value {
        ruby.get_inner(self.context)
    }

    fn coordinate(
        &self,
        ruby: &Ruby,
        raw: &RawCell,
        row: usize,
        column: usize,
    ) -> Result<Value, Error> {
        let coordinate = ruby.get_inner(&COORDINATE);
        match (&raw.reference, self.mode) {
            (Reference::At { row, column }, _) => coordinate.new_instance((*row, *column)),
            (Reference::Invalid(reference), _) => self
                .context(ruby)
                .funcall("coordinate", (ruby.str_new(reference),)),
            (Reference::Missing, Mode::Document) => coordinate.new_instance((row, column)),
            (Reference::Missing, Mode::Stream) => {
                self.context(ruby).funcall("coordinate", (ruby.qnil(),))
            }
        }
    }

    fn style(&self, ruby: &Ruby, raw: &RawCell) -> Result<Value, Error> {
        match &raw.style {
            Style::Index(index) => Ok(index.into_value_with(ruby)),
            Style::Other(text) => self
                .context(ruby)
                .funcall("style_index", (ruby.str_new(text),)),
        }
    }

    /// `[format.to_s, Excelx::Format.to_type(format)]` for a style index.
    fn style_info(&self, ruby: &Ruby, style: Value) -> Result<(Value, ValueType), Error> {
        let formats = ruby.get_inner(self.formats);
        let cacheable = usize::try_convert(style).ok().filter(|i| *i < 1 << 16);
        if let Some(index) = cacheable {
            if let Some(Some(value_type)) = self.types.borrow().get(index) {
                return Ok((formats.entry(index as isize)?, *value_type));
            }
        }
        let info: RArray = self.context(ruby).funcall("style", (style,))?;
        let format: Value = info.entry(0)?;
        let type_symbol: Symbol = info.entry(1)?;
        let value_type = match type_symbol.name()?.as_ref() {
            "date" => ValueType::Date,
            "datetime" => ValueType::DateTime,
            "time" => ValueType::Time,
            _ => ValueType::Number,
        };
        if let Some(index) = cacheable {
            formats.store(index as isize, format)?;
            let mut types = self.types.borrow_mut();
            if types.len() <= index {
                types.resize(index + 1, None);
            }
            types[index] = Some(value_type);
        }
        Ok((format, value_type))
    }

    /// `shared_strings.use_html?(i) ? shared_strings.to_html[i] : shared_strings[i]`,
    /// decided again for every cell, as roo does, since a caller may change a
    /// shared string in place.
    fn shared_string(&self, ruby: &Ruby, text: &str) -> Result<Value, Error> {
        if let Some(index) = parse_index(text) {
            if self.shared_plain.get().is_none() {
                let tables: RArray = self.context(ruby).funcall("shared_strings", ())?;
                let plain: RArray = tables.entry(0)?;
                let html: Value = tables.entry(1)?;
                self.shared_plain.set(Some(plain.into()));
                self.shared_html.set(Some(html.into()));
            }
            let plain = ruby.get_inner(self.shared_plain.get().expect("fetched above"));
            let html = ruby.get_inner(self.shared_html.get().expect("fetched above"));
            let chosen = match RArray::from_value(html) {
                None => Some(plain),
                Some(html) if index < html.len() => {
                    match RString::from_value(html.entry(index as isize)?) {
                        Some(string) if has_html_tag(unsafe { string.as_slice() }) => {
                            return Ok(string.as_value());
                        }
                        Some(_) => Some(plain),
                        None => None,
                    }
                }
                Some(_) => None,
            };
            if let Some(plain) = chosen.filter(|plain| index < plain.len()) {
                return plain.entry(index as isize);
            }
        }
        self.context(ruby)
            .funcall("shared_string", (ruby.str_new(text),))
    }

    /// Builds the cell `SheetDoc#cell_from_xml` builds for `raw`. `None` means
    /// roo stores no cell (an empty cell without the `empty_cell` option).
    fn cell(
        &self,
        ruby: &Ruby,
        row: &RawRow,
        raw: &RawCell,
        row_number: usize,
        column: usize,
    ) -> Result<Option<(Value, Value)>, Error> {
        let coordinate = self.coordinate(ruby, raw, row_number, column)?;
        let style = self.style(ruby, raw)?;
        let link = self.link(ruby, coordinate);
        let formula = raw.formula.as_ref().map(|f| ruby.str_new(row.text(f)));
        let cell: Value =
            match &raw.body {
                Body::Empty => {
                    if !self.empty_cell {
                        return Ok(None);
                    }
                    ruby.get_inner(&CELL_EMPTY).new_instance((coordinate,))?
                }
                Body::Inline(text) => ruby.get_inner(&CELL_STRING).new_instance((
                    ruby.str_new(row.text(text)),
                    formula,
                    style,
                    link,
                    coordinate,
                ))?,
                // roo looks up the style format for every `v` cell, which raises when
                // the workbook has no styles part, so this does too.
                Body::Value(text) => {
                    let text = row.text(text);
                    match (raw.kind, self.style_info(ruby, style)?) {
                        (Kind::Shared, _) => {
                            let value = self.shared_string(ruby, text)?;
                            ruby.get_inner(&CELL_STRING)
                                .new_instance((value, formula, style, link, coordinate))?
                        }
                        (Kind::Boolean, _) => ruby.get_inner(&CELL_BOOLEAN).new_instance((
                            ruby.str_new(text),
                            formula,
                            style,
                            link,
                            coordinate,
                        ))?,
                        (Kind::Str, _) => ruby.get_inner(&CELL_STRING).new_instance((
                            ruby.str_new(text),
                            formula,
                            style,
                            link,
                            coordinate,
                        ))?,
                        (kind, (format, format_type)) => {
                            let value_type = if kind == Kind::InlineStr {
                                ValueType::Number
                            } else {
                                format_type
                            };
                            let numeric_or_formula = ruby.get_inner(&NUMERIC_OR_FORMULA).as_value();
                            let excelx_type =
                                ruby.ary_new_from_values(&[numeric_or_formula, format]);
                            let content = ruby.str_new(text);
                            match value_type {
                                ValueType::Number => ruby.get_inner(&CELL_NUMBER).new_instance(
                                    (content, formula, excelx_type, style, link, coordinate),
                                )?,
                                ValueType::Date => ruby.get_inner(&CELL_DATE).new_instance((
                                    content,
                                    formula,
                                    excelx_type,
                                    style,
                                    link,
                                    ruby.get_inner(self.base_date),
                                    coordinate,
                                ))?,
                                ValueType::DateTime | ValueType::Time => {
                                    let number: f64 = content.funcall("to_f", ())?;
                                    let (class, base) = if number < 1.0 {
                                        (&CELL_TIME, self.base_timestamp)
                                    } else if (number - number.floor()).abs() > 0.000001 {
                                        (&CELL_DATETIME, self.base_timestamp)
                                    } else {
                                        (&CELL_DATE, self.base_date)
                                    };
                                    ruby.get_inner(class).new_instance((
                                        ruby.float_from_f64(number),
                                        formula,
                                        excelx_type,
                                        style,
                                        link,
                                        ruby.get_inner(base),
                                        coordinate,
                                    ))?
                                }
                            }
                        }
                    }
                }
            };
        Ok(Some((coordinate, cell)))
    }
}

#[cfg(test)]
mod tests {
    use super::has_html_tag;

    #[test]
    fn finds_the_tags_roo_looks_for() {
        for html in [
            "<html><b>x</b></html>",
            "a<i>",
            "<u>",
            "x<sup>2</sup>",
            "<sub>",
        ] {
            assert!(has_html_tag(html.as_bytes()), "{html}");
        }
        for plain in ["<html>x</html>", "<br>", "<bb>", "b>", "<s>", "<su>", "<"] {
            assert!(!has_html_tag(plain.as_bytes()), "{plain}");
        }
    }
}
