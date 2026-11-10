//! Builds the `Roo::Excelx::Cell` objects that
//! `Roo::Excelx::SheetDoc#cell_from_xml` builds, by calling roo's own
//! constructors with the same arguments.

use std::cell::{Cell as StdCell, RefCell};

use magnus::gc::Marker;
use magnus::value::{Lazy, Opaque, ReprValue};
use magnus::{
    prelude::*, DataTypeFunctions, Error, IntoValue, RArray, RClass, RHash, Ruby, StaticSymbol,
    Symbol, TypedData, Value,
};

use crate::reader::{Body, Mode, RawCell, RawRow};

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum ValueType {
    Date,
    DateTime,
    Time,
    /// `:float`, `:percentage`, and any other format type roo builds a
    /// `Cell::Number` for.
    Number,
}

/// `Roo::Utils.extract_coordinate` for well-formed references. `None` means
/// roo raises, so the caller lets roo's own method produce the error.
pub fn parse_reference(reference: &str) -> Option<(i64, i64)> {
    let (mut column, mut row) = (0i64, 0i64);
    let mut digits = false;
    for byte in reference.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' if !digits => {
                column = column
                    .checked_mul(26)?
                    .checked_add(i64::from(byte & 0x1f))?;
            }
            b'0'..=b'9' => {
                digits = true;
                row = row.checked_mul(10)?.checked_add(i64::from(byte - b'0'))?;
            }
            _ => return None,
        }
    }
    (column != 0 && digits).then_some((row, column))
}

/// `String#to_i` for plain decimal digits; `None` defers to Ruby.
pub fn parse_index(text: &str) -> Option<usize> {
    if text.is_empty() || text.len() > 18 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
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
    shared_strings: StdCell<Option<Opaque<RArray>>>,
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
        if let Some(shared_strings) = self.shared_strings.get() {
            marker.mark(shared_strings);
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
            shared_strings: StdCell::new(None),
        })
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// `SheetDoc#each_cell` for a streamed row: every cell, empty ones too.
    pub fn row_cells(ruby: &Ruby, rb_self: &Self, row: &crate::Row) -> Result<RArray, Error> {
        let cells = ruby.ary_new_capa(row.raw().cells.len());
        for (index, raw) in row.raw().cells.iter().enumerate() {
            if let Some((_, cell)) = rb_self.cell(ruby, raw, 0, index + 1)? {
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
            if let Some((coordinate, cell)) = self.cell(ruby, raw, row_number, index + 1)? {
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
        match (&raw.r, self.mode) {
            (Some(reference), _) => match parse_reference(reference) {
                Some((row, column)) => coordinate.new_instance((row, column)),
                None => self
                    .context(ruby)
                    .funcall("coordinate", (ruby.str_new(reference),)),
            },
            (None, Mode::Document) => coordinate.new_instance((row, column)),
            (None, Mode::Stream) => self.context(ruby).funcall("coordinate", (ruby.qnil(),)),
        }
    }

    fn style(&self, ruby: &Ruby, raw: &RawCell) -> Result<Value, Error> {
        match raw.s.as_deref() {
            None => Ok(0i64.into_value_with(ruby)),
            Some(text) => match parse_index(text).filter(|i| *i < 1 << 30) {
                Some(index) => Ok((index as i64).into_value_with(ruby)),
                None => self
                    .context(ruby)
                    .funcall("style_index", (ruby.str_new(text),)),
            },
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

    fn shared_string(&self, ruby: &Ruby, text: &str) -> Result<Value, Error> {
        if let Some(index) = parse_index(text) {
            let table = match self.shared_strings.get() {
                Some(table) => ruby.get_inner(table),
                None => {
                    let table: RArray = self.context(ruby).funcall("shared_strings", ())?;
                    self.shared_strings.set(Some(table.into()));
                    table
                }
            };
            if index < table.len() {
                return table.entry(index as isize);
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
        raw: &RawCell,
        row: usize,
        column: usize,
    ) -> Result<Option<(Value, Value)>, Error> {
        let coordinate = self.coordinate(ruby, raw, row, column)?;
        let style = self.style(ruby, raw)?;
        let link = match self.hyperlinks {
            Some(hyperlinks) => ruby
                .get_inner(hyperlinks)
                .get(coordinate)
                .unwrap_or_else(|| ruby.qnil().as_value()),
            None => ruby.qnil().as_value(),
        };
        let formula = raw.formula.as_deref().map(|f| ruby.str_new(f));
        let cell: Value =
            match &raw.body {
                Body::Empty => {
                    if !self.empty_cell {
                        return Ok(None);
                    }
                    ruby.get_inner(&CELL_EMPTY).new_instance((coordinate,))?
                }
                Body::Inline(text) => ruby.get_inner(&CELL_STRING).new_instance((
                    ruby.str_new(text),
                    formula,
                    style,
                    link,
                    coordinate,
                ))?,
                // roo looks up the style format for every `v` cell, which raises when
                // the workbook has no styles part, so this does too.
                Body::Value(text) => {
                    match (raw.t.as_deref(), self.style_info(ruby, style)?) {
                        (Some("s"), _) => {
                            let value = self.shared_string(ruby, text)?;
                            ruby.get_inner(&CELL_STRING)
                                .new_instance((value, formula, style, link, coordinate))?
                        }
                        (Some("b"), _) => ruby.get_inner(&CELL_BOOLEAN).new_instance((
                            ruby.str_new(text),
                            formula,
                            style,
                            link,
                            coordinate,
                        ))?,
                        (Some("str"), _) => ruby.get_inner(&CELL_STRING).new_instance((
                            ruby.str_new(text),
                            formula,
                            style,
                            link,
                            coordinate,
                        ))?,
                        (kind, (format, format_type)) => {
                            let value_type = if kind == Some("inlineStr") {
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
    use super::*;

    #[test]
    fn parses_references_like_roo() {
        assert_eq!(parse_reference("A1"), Some((1, 1)));
        assert_eq!(parse_reference("aa10"), Some((10, 27)));
        assert_eq!(parse_reference("XFD1048576"), Some((1048576, 16384)));
        assert_eq!(parse_reference("1A"), None);
        assert_eq!(parse_reference("A"), None);
        assert_eq!(parse_reference("A1B"), None);
        assert_eq!(parse_reference("$A$1"), None);
    }

    #[test]
    fn parses_only_plain_indexes() {
        assert_eq!(parse_index("12"), Some(12));
        assert_eq!(parse_index("007"), Some(7));
        assert_eq!(parse_index(""), None);
        assert_eq!(parse_index("+1"), None);
        assert_eq!(parse_index(" 1"), None);
    }
}
