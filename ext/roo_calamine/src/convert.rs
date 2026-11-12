//! Builds the `Roo::Excelx::Cell` objects that
//! `Roo::Excelx::SheetDoc#cell_from_xml` builds ([`cells`]), or only the
//! values those cells return from `#value`.

mod cells;
mod csv;

pub use csv::CsvOutput;

use std::cell::{Cell as StdCell, RefCell};
use std::collections::HashMap;
use std::os::raw::c_int;

use magnus::encoding::{self, EncodingCapable};
use magnus::gc::Marker;
use magnus::rb_sys::{protect, AsRawValue, FromRawValue};
use magnus::value::{Lazy, Opaque, ReprValue};
use magnus::{
    prelude::*, DataTypeFunctions, Error, IntoValue, RArray, RClass, RHash, RString, Ruby,
    StaticSymbol, Symbol, TypedData, Value,
};

use crate::numeric::{create_numeric, plain_number};
use crate::reader::{parse_index, Body, Kind, Mode, RawCell, RawRow, Reference, Style};
use crate::shared_strings::{has_html_tag, HtmlSelection, HtmlStrings};

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

/// How `Cell::Number#create_numeric` reads text under a format.
#[derive(Clone, Copy, PartialEq, Eq)]
enum NumberFormat {
    /// The format matches `/%/` or `/\.0/`: always `Float(text)`.
    Float,
    /// The text decides between `Float` and `Integer`.
    ByText,
    /// Matching the format raises, so roo's constructor runs.
    Unmatchable,
}

/// What a style index means for a value cell, kept with the text and encoding
/// of its format as first read.
#[derive(Clone)]
struct StyleInfo {
    format_text: Vec<u8>,
    format_encoding: encoding::Index,
    value_type: ValueType,
    number_format: NumberFormat,
}

impl StyleInfo {
    /// Whether `format` still has the text and encoding this was read from.
    fn read_from(&self, format: Value) -> bool {
        RString::from_value(format).is_some_and(|text| {
            text.enc_get() == self.format_encoding && unsafe { text.as_slice() } == self.format_text
        })
    }
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
    /// Whether this roo's cells may be built without their constructors.
    build_cells: bool,
    context: Opaque<Value>,
    hyperlinks: Option<Opaque<RHash>>,
    /// Document reads snapshot hyperlinks; streams keep roo's live hash.
    link_index: Option<HashMap<(i64, i64), Opaque<Value>>>,
    base_date: Opaque<Value>,
    base_timestamp: Opaque<Value>,
    /// `base_timestamp`, when it is an integer a double holds exactly.
    base_seconds: Option<i64>,
    /// Format string per style index, filled on first use.
    formats: Opaque<RArray>,
    styles: RefCell<Vec<Option<StyleInfo>>>,
    /// roo's plain shared-string array and deferred or public HTML table
    /// (`nil` when the HTML wrapper is disabled). Fetched on first use.
    shared_plain: StdCell<Option<Opaque<RArray>>>,
    shared_html: StdCell<Option<Opaque<Value>>>,
    /// `Roo::Excelx::ERROR_VALUES`, which `Cell::Number` keeps as text.
    error_values: Vec<String>,
    /// Document mode: the string picked for each shared-string index. roo
    /// picks every cell's string in one pass over the sheet, with no Ruby
    /// code in between, so one pick per index gives the cells it gives, even
    /// when a caller later changes a picked string in place.
    picked: Option<Opaque<RHash>>,
    picked_indices: StdCell<Option<Opaque<RArray>>>,
}

impl DataTypeFunctions for Converter {
    fn mark(&self, marker: &Marker) {
        marker.mark(self.context);
        if let Some(hyperlinks) = self.hyperlinks {
            marker.mark(hyperlinks);
        }
        if let Some(index) = &self.link_index {
            for value in index.values() {
                marker.mark(*value);
            }
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
        if let Some(picked) = self.picked {
            marker.mark(picked);
        }
        if let Some(picked) = self.picked_indices.get() {
            marker.mark(picked);
        }
    }
}

impl Converter {
    pub fn formatter_type(
        &self,
        ruby: &Ruby,
        raw: &RawCell,
    ) -> Result<Option<&'static str>, Error> {
        let Reference::At { row, column } = raw.reference else {
            return Ok(None);
        };
        if !self.build_cells || self.has_link(ruby, row, column) {
            return Ok(None);
        }
        let kind = match (&raw.body, raw.kind) {
            (Body::Empty, _) => return Ok(None),
            (Body::Inline(_), _) | (Body::Value(_), Kind::Shared | Kind::Str) => "string",
            (Body::Value(_), Kind::Boolean) => "boolean",
            (Body::Value(_), Kind::InlineStr) => "float",
            (Body::Value(_), Kind::ByFormat) => {
                match self.style_info(ruby, self.style(ruby, raw)?)?.1 {
                    ValueType::Number => "float",
                    ValueType::Date => "date",
                    ValueType::DateTime | ValueType::Time => return Ok(None),
                }
            }
        };
        Ok(Some(if raw.formula.is_some() {
            "formula"
        } else {
            kind
        }))
    }

    /// `context` is a `Roo::Calamine::Context`.
    pub fn new(ruby: &Ruby, context: Value, stream: bool) -> Result<Self, Error> {
        let hyperlinks: RHash = context.funcall("hyperlinks", ())?;
        let base_timestamp: Value = context.funcall("base_timestamp", ())?;
        let mut link_index = (!stream).then(HashMap::new);
        hyperlinks.foreach(|key: Value, value: Value| {
            if let Some(index) = &mut link_index {
                let key = RArray::from_value(key).filter(|key| key.len() == 2);
                let coordinate = key.and_then(|key| {
                    let row = magnus::Integer::from_value(key.entry(0).ok()?)?;
                    let column = magnus::Integer::from_value(key.entry(1).ok()?)?;
                    Some((
                        i64::try_convert(row.as_value()).ok()?,
                        i64::try_convert(column.as_value()).ok()?,
                    ))
                });
                match coordinate {
                    Some(key) => {
                        index.insert(key, value.into());
                    }
                    None => link_index = None,
                }
            }
            Ok(magnus::r_hash::ForEach::Continue)
        })?;
        Ok(Self {
            mode: if stream { Mode::Stream } else { Mode::Document },
            empty_cell: context.funcall::<_, _, Value>("empty_cell", ())?.to_bool(),
            build_cells: context
                .funcall::<_, _, Value>("build_cells?", ())?
                .to_bool(),
            context: context.into(),
            hyperlinks: (!hyperlinks.is_empty()).then(|| hyperlinks.into()),
            link_index,
            base_date: context.funcall::<_, _, Value>("base_date", ())?.into(),
            base_timestamp: base_timestamp.into(),
            base_seconds: i64::try_convert(base_timestamp)
                .ok()
                .filter(|seconds| seconds.unsigned_abs() < 1 << 53),
            formats: ruby.ary_new().into(),
            styles: RefCell::new(Vec::new()),
            shared_plain: StdCell::new(None),
            shared_html: StdCell::new(None),
            error_values: ruby.eval("Roo::Excelx::ERROR_VALUES.to_a")?,
            picked: (!stream).then(|| ruby.hash_new().into()),
            picked_indices: StdCell::new(None),
        })
    }

    /// `Roo::Calamine.each_row_values` for a streamed row: the `value` of each
    /// cell that `Sheet#each_row(pad_cells: true)` yields, and `nil` for each
    /// cell it pads.
    pub fn row_values(&self, ruby: &Ruby, row: &RawRow) -> Result<RArray, Error> {
        let values = ruby.ary_new_capa(row.cells.len());
        let mut last_column = 0;
        for raw in &row.cells {
            let (column, value) = self.value(ruby, row, raw)?;
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
        let (_, format_type, number_format) = self.style_info(ruby, self.style(ruby, raw)?)?;
        let value = match (raw.kind, format_type) {
            (Kind::Shared, _) => self.shared_string(ruby, text)?,
            (Kind::Boolean, _) => {
                let one = i64::try_convert(ruby_integer(ruby, text, false)?).ok() == Some(1);
                one.into_value_with(ruby)
            }
            (Kind::Str, _) => ruby.str_new(text).as_value(),
            (Kind::InlineStr, _) | (Kind::ByFormat, ValueType::Number) => match number_format {
                NumberFormat::Float => self.number(ruby, text, true)?,
                NumberFormat::ByText => self.number(ruby, text, false)?,
                NumberFormat::Unmatchable => self.cell_value(ruby, row, raw)?,
            },
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
        if let Some(number) = create_numeric(text, float_format) {
            return Ok(number.into_value_with(ruby));
        }
        if float_format || text.contains('.') || is_exponent_number(text) {
            ruby_float(ruby, text)
        } else {
            ruby_integer(ruby, text, true)
        }
    }

    fn cell_value(&self, ruby: &Ruby, row: &RawRow, raw: &RawCell) -> Result<Value, Error> {
        match self.cell(ruby, &row.text, raw, 0, 0, None)? {
            Some((_, cell)) => cell.funcall("value", ()),
            None => Ok(ruby.qnil().as_value()),
        }
    }

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
            if let Some((_, cell)) = rb_self.cell(ruby, &row.raw().text, raw, 0, index + 1, None)? {
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
            if let Some((coordinate, cell)) =
                self.cell(ruby, &row.text, raw, row_number, index + 1, None)?
            {
                cells.aset(coordinate, cell)?;
            }
        }
        Ok(())
    }

    /// Whether [`Self::cell`] builds `raw` without raising, so it may be built
    /// later than roo would, or never. `false` also for anything this does
    /// not tell apart cheaply, and for a record without a valid `r`.
    pub fn builds_safely(&self, ruby: &Ruby, buffer: &str, raw: &RawCell) -> bool {
        let Reference::At { row, column } = raw.reference else {
            return false;
        };
        let Body::Value(text) = &raw.body else {
            return true;
        };
        let text = &buffer[text.clone()];
        let Ok(style) = self.style(ruby, raw) else {
            return false;
        };
        if cacheable(style).is_none() {
            return false;
        }
        let Ok((_, format_type, number_format)) = self.style_info(ruby, style) else {
            return false;
        };
        let number_builds =
            || number_format != NumberFormat::Unmatchable && self.number_builds(text);
        match raw.kind {
            Kind::Shared => self
                .shared_string(ruby, text)
                .is_ok_and(|value| RString::from_value(value).is_some()),
            Kind::Boolean | Kind::Str => true,
            Kind::InlineStr => number_builds(),
            Kind::ByFormat => match format_type {
                ValueType::Number => number_builds(),
                ValueType::Date => true,
                // roo raises for a hyperlink here: `Roo::Link.new` gets a Float,
                // and for a day count whose seconds overflow to infinity.
                ValueType::DateTime | ValueType::Time => {
                    let seconds_finite = text.is_empty()
                        || (plain_number(text)
                            && text
                                .parse::<f64>()
                                .is_ok_and(|days| (days * 86_400.0).is_finite()));
                    seconds_finite && !self.has_link(ruby, row, column)
                }
            },
        }
    }

    /// `Cell::Number#create_numeric` returns without raising.
    fn number_builds(&self, text: &str) -> bool {
        text.is_empty() || plain_number(text) || self.error_values.iter().any(|error| error == text)
    }

    pub fn has_link(&self, ruby: &Ruby, row: i64, column: i64) -> bool {
        !self.link_at(ruby, row, column).is_nil()
    }

    fn link_at(&self, ruby: &Ruby, row: i64, column: i64) -> Value {
        if let Some(index) = &self.link_index {
            return index
                .get(&(row, column))
                .map(|value| ruby.get_inner(*value))
                .unwrap_or_else(|| ruby.qnil().as_value());
        }
        let Some(hyperlinks) = self.hyperlinks else {
            return ruby.qnil().as_value();
        };
        let key =
            ruby.ary_new_from_values(&[row.into_value_with(ruby), column.into_value_with(ruby)]);
        ruby.get_inner(hyperlinks)
            .get(key)
            .unwrap_or_else(|| ruby.qnil().as_value())
    }

    /// `cell&.presence` for the cell [`Self::cell`] builds for `raw`, when it
    /// is known without building the cell.
    pub fn presence(
        &self,
        ruby: &Ruby,
        buffer: &str,
        raw: &RawCell,
    ) -> Result<Option<bool>, Error> {
        Ok(match (&raw.body, raw.kind) {
            (Body::Empty, _) => Some(false),
            (Body::Inline(_), _) => Some(true),
            (Body::Value(text), Kind::Shared) => {
                RString::from_value(self.shared_string(ruby, &buffer[text.clone()])?)
                    .map(|value| !value.is_empty())
            }
            (Body::Value(text), Kind::Str) => Some(!text.is_empty()),
            (Body::Value(_), _) => Some(true),
        })
    }

    /// Whether roo's `cells` holds a cell for a record with an empty body.
    pub fn empty_cell(&self) -> bool {
        self.empty_cell
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
        match (&raw.reference, self.mode) {
            (Reference::At { row, column }, _) => self.new_coordinate(ruby, *row, *column),
            (Reference::Invalid(reference), _) => self
                .context(ruby)
                .funcall("coordinate", (ruby.str_new(reference),)),
            (Reference::Missing, Mode::Document) => self.new_coordinate(ruby, row, column),
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

    /// The format string of a style index, its value type, and how
    /// `Cell::Number` reads text under it. roo reads the format again for
    /// every cell, so a stream reads a format changed in place again. A
    /// document keeps what each format first read: roo builds every cell
    /// before a caller can change one.
    fn style_info(
        &self,
        ruby: &Ruby,
        style: Value,
    ) -> Result<(Value, ValueType, NumberFormat), Error> {
        let formats = ruby.get_inner(self.formats);
        let cacheable = cacheable(style);
        if let Some(index) = cacheable {
            if let Some(Some(info)) = self.styles.borrow().get(index) {
                let format: Value = formats.entry(index as isize)?;
                if self.mode == Mode::Document || info.read_from(format) {
                    return Ok((format, info.value_type, info.number_format));
                }
            }
        }
        let info: RArray = self.context(ruby).funcall("style", (style,))?;
        let format: Value = info.entry(0)?;
        let type_symbol: Symbol = info.entry(1)?;
        let float_format: Option<bool> = info.entry(2)?;
        let value_type = match type_symbol.name()?.as_ref() {
            "date" => ValueType::Date,
            "datetime" => ValueType::DateTime,
            "time" => ValueType::Time,
            _ => ValueType::Number,
        };
        let number_format = match float_format {
            Some(true) => NumberFormat::Float,
            Some(false) => NumberFormat::ByText,
            None => NumberFormat::Unmatchable,
        };
        if let (Some(index), Some(text)) = (cacheable, RString::from_value(format)) {
            formats.store(index as isize, format)?;
            let mut styles = self.styles.borrow_mut();
            if styles.len() <= index {
                styles.resize(index + 1, None);
            }
            styles[index] = Some(StyleInfo {
                format_text: unsafe { text.as_slice() }.to_vec(),
                format_encoding: text.enc_get(),
                value_type,
                number_format,
            });
        }
        Ok((format, value_type, number_format))
    }

    /// The style a document read first for a style index.
    fn style_as_read(&self, style: Value) -> Option<StyleInfo> {
        let index = cacheable(style)?;
        self.styles.borrow().get(index).cloned().flatten()
    }

    /// `shared_strings.use_html?(i) ? shared_strings.to_html[i] : shared_strings[i]`.
    /// In stream mode it is decided again for every cell, as roo does, since
    /// a caller may change a shared string in place between rows.
    fn shared_string(&self, ruby: &Ruby, text: &str) -> Result<Value, Error> {
        let index = parse_index(text);
        if self.picked.is_some() && index.is_some() {
            self.load_shared_strings(ruby)?;
        }
        if let (Some(index), Some(indices)) = (index, self.picked_indices.get()) {
            let indices = ruby.get_inner(indices);
            if index < indices.len() {
                return indices.entry(index as isize);
            }
        }
        let Some(picked) = self.picked.map(|picked| ruby.get_inner(picked)) else {
            return self.pick_shared_string(ruby, index, text);
        };
        let key = match index {
            Some(index) => index.into_value_with(ruby),
            None => ruby.str_new(text).funcall("to_i", ())?,
        };
        if let Some(string) = picked.get(key) {
            return Ok(string);
        }
        let string = self.pick_shared_string(ruby, index, text)?;
        picked.aset(key, string)?;
        Ok(string)
    }

    fn load_shared_strings(&self, ruby: &Ruby) -> Result<(), Error> {
        if self.shared_plain.get().is_none() {
            let tables: RArray = self.context(ruby).funcall("shared_strings", ())?;
            let plain: RArray = tables.entry(0)?;
            let html: Value = tables.entry(1)?;
            self.shared_plain.set(Some(plain.into()));
            self.shared_html.set(Some(html.into()));
            if self.picked.is_some()
                && (html.is_nil()
                    || <&HtmlStrings>::try_convert(html)
                        .is_ok_and(|html| html.is_plain(plain.len())))
            {
                self.picked_indices.set(Some(plain.dup().into()));
            }
        }
        Ok(())
    }

    fn pick_shared_string(
        &self,
        ruby: &Ruby,
        index: Option<usize>,
        text: &str,
    ) -> Result<Value, Error> {
        let Some(index) = index else {
            return self
                .context(ruby)
                .funcall("shared_string", (ruby.str_new(text),));
        };
        self.load_shared_strings(ruby)?;
        let plain = ruby.get_inner(self.shared_plain.get().expect("fetched above"));
        let html = ruby.get_inner(self.shared_html.get().expect("fetched above"));
        let chosen = match RArray::from_value(html) {
            None if html.is_nil() => Some(plain),
            None => match <&HtmlStrings>::try_convert(html)?.select(ruby, index)? {
                HtmlSelection::Html(string) => return Ok(string),
                HtmlSelection::Plain => Some(plain),
                HtmlSelection::Invalid => None,
            },
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
        self.context(ruby)
            .funcall("shared_string", (ruby.str_new(text),))
    }
}

/// The style indexes [`Converter`] keeps a format and type for.
fn cacheable(style: Value) -> Option<usize> {
    usize::try_convert(style).ok().filter(|i| *i < 1 << 16)
}
