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
use crate::store::Store;

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

/// Text that `Float()` and `Integer(text, 10)` both read as a finite number,
/// and `String#to_f` reads whole: `-?digits(.digits)?([eE][-+]?digits)?`.
fn plain_number(text: &str) -> bool {
    fn digits(bytes: &[u8]) -> usize {
        bytes.iter().take_while(|b| b.is_ascii_digit()).count()
    }
    let bytes = text.as_bytes();
    let mut at = usize::from(bytes.first() == Some(&b'-'));
    let whole = digits(&bytes[at..]);
    if whole == 0 {
        return false;
    }
    at += whole;
    if bytes.get(at) == Some(&b'.') {
        let fraction = digits(&bytes[at + 1..]);
        if fraction == 0 {
            return false;
        }
        at += 1 + fraction;
    }
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        at += 1;
        if matches!(bytes.get(at), Some(b'-' | b'+')) {
            at += 1;
        }
        let exponent = digits(&bytes[at..]);
        if exponent == 0 {
            return false;
        }
        at += exponent;
    }
    at == bytes.len() && text.parse::<f64>().is_ok_and(f64::is_finite)
}

/// `string[/<([biu]|sup|sub)>/]`, the test in `SharedStrings#use_html?`.
fn has_html_tag(string: &[u8]) -> bool {
    string.windows(3).enumerate().any(|(i, w)| {
        w[0] == b'<'
            && (matches!(w, [_, b'b' | b'i' | b'u', b'>'])
                || string[i..].starts_with(b"<sup>")
                || string[i..].starts_with(b"<sub>"))
    })
}

/// `Coordinate.new(row, column)`: a frozen `[row, column]` of roo's Array
/// subclass, built without running its Ruby `initialize`.
fn new_coordinate<T: IntoValue>(ruby: &Ruby, row: T, column: T) -> Result<Value, Error> {
    let coordinate = RArray::from_value(ruby.get_inner(&COORDINATE).obj_alloc()?)
        .expect("Roo::Excelx::Coordinate is an Array");
    coordinate.push(row)?;
    coordinate.push(column)?;
    coordinate.freeze();
    Ok(coordinate.as_value())
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
    /// A frozen copy of each of those as first used.
    formats_read: Opaque<RArray>,
    types: RefCell<Vec<Option<ValueType>>>,
    /// roo's own shared-string arrays: plain text, and the HTML version
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
        marker.mark(self.formats_read);
        if let Some(plain) = self.shared_plain.get() {
            marker.mark(plain);
        }
        if let Some(html) = self.shared_html.get() {
            marker.mark(html);
        }
        if let Some(picked) = self.picked {
            marker.mark(picked);
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
            formats_read: ruby.ary_new().into(),
            types: RefCell::new(Vec::new()),
            shared_plain: StdCell::new(None),
            shared_html: StdCell::new(None),
            error_values: ruby.eval("Roo::Excelx::ERROR_VALUES.to_a")?,
            picked: (!stream).then(|| ruby.hash_new().into()),
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

    fn cell_value(&self, ruby: &Ruby, row: &RawRow, raw: &RawCell) -> Result<Value, Error> {
        match self.cell(ruby, &row.text, raw, 0, 0)? {
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
            if let Some((_, cell)) = rb_self.cell(ruby, &row.raw().text, raw, 0, index + 1)? {
                cells.push(cell)?;
            }
        }
        Ok(cells)
    }

    /// `SheetDoc#extract_cells` for one row, adding to `cells`. A cell
    /// `store` built goes in its place instead of a new one.
    pub fn add_row(
        &self,
        ruby: &Ruby,
        cells: RHash,
        row: &RawRow,
        row_number: usize,
        store: Option<&Store>,
    ) -> Result<(), Error> {
        for (index, raw) in row.cells.iter().enumerate() {
            let column = index + 1;
            let reused = match store {
                Some(store) if self.empty_cell || raw.body != Body::Empty => {
                    match raw
                        .position(row_number, column)
                        .and_then(|(row, column)| crate::store::key(row, column))
                    {
                        Some(key) => store.reused(ruby, cells, key)?,
                        None => None,
                    }
                }
                _ => None,
            };
            if let Some(cell) = reused {
                cells.aset(cell.funcall::<_, _, Value>("coordinate", ())?, cell)?;
            } else if let Some((coordinate, cell)) =
                self.cell(ruby, &row.text, raw, row_number, column)?
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
        let Ok((_, format_type)) = self.style_info(ruby, style) else {
            return false;
        };
        match raw.kind {
            Kind::Shared => self
                .shared_string(ruby, text)
                .is_ok_and(|value| RString::from_value(value).is_some()),
            Kind::Boolean | Kind::Str => true,
            Kind::InlineStr => self.number_builds(text),
            Kind::ByFormat => match format_type {
                ValueType::Number => self.number_builds(text),
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
        let Some(hyperlinks) = self.hyperlinks else {
            return false;
        };
        let key =
            ruby.ary_new_from_values(&[row.into_value_with(ruby), column.into_value_with(ruby)]);
        ruby.get_inner(hyperlinks).get(key).is_some()
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
            (Reference::At { row, column }, _) => new_coordinate(ruby, *row, *column),
            (Reference::Invalid(reference), _) => self
                .context(ruby)
                .funcall("coordinate", (ruby.str_new(reference),)),
            (Reference::Missing, Mode::Document) => new_coordinate(ruby, row, column),
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
        let cacheable = cacheable(style);
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
            let read: Value = format.funcall("dup", ())?;
            ruby.get_inner(self.formats_read)
                .store(index as isize, read.funcall::<_, _, Value>("freeze", ())?)?;
            let mut types = self.types.borrow_mut();
            if types.len() <= index {
                types.resize(index + 1, None);
            }
            types[index] = Some(value_type);
        }
        Ok((format, value_type))
    }

    /// Document mode: roo builds every cell before a caller can change a
    /// format string in place, and `Cell::Number#create_numeric` reads it,
    /// so a cell built later gets the value the format first read gives.
    fn keep_number_as_read(
        &self,
        ruby: &Ruby,
        style: Value,
        format: Value,
        content: RString,
        cell: Value,
    ) -> Result<(), Error> {
        if self.mode != Mode::Document {
            return Ok(());
        }
        let Some(index) = cacheable(style) else {
            return Ok(());
        };
        let as_read: Value = ruby.get_inner(self.formats_read).entry(index as isize)?;
        let (Some(now), Some(then)) = (RString::from_value(format), RString::from_value(as_read))
        else {
            return Ok(());
        };
        if unsafe { now.as_slice() == then.as_slice() } {
            return Ok(());
        }
        let excelx_type =
            ruby.ary_new_from_values(&[ruby.get_inner(&NUMERIC_OR_FORMULA).as_value(), as_read]);
        let value: Value = ruby
            .get_inner(&CELL_NUMBER)
            .new_instance((
                content,
                ruby.qnil(),
                excelx_type,
                style,
                ruby.qnil(),
                ruby.qnil(),
            ))?
            .funcall("value", ())?;
        cell.funcall::<_, _, Value>("value=", (value,))?;
        Ok(())
    }

    /// `shared_strings.use_html?(i) ? shared_strings.to_html[i] : shared_strings[i]`.
    /// In stream mode it is decided again for every cell, as roo does, since
    /// a caller may change a shared string in place between rows.
    fn shared_string(&self, ruby: &Ruby, text: &str) -> Result<Value, Error> {
        let index = parse_index(text);
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
        self.context(ruby)
            .funcall("shared_string", (ruby.str_new(text),))
    }

    /// Builds the cell `SheetDoc#cell_from_xml` builds for `raw`. `None` means
    /// roo stores no cell (an empty cell without the `empty_cell` option).
    pub fn cell(
        &self,
        ruby: &Ruby,
        buffer: &str,
        raw: &RawCell,
        row_number: usize,
        column: usize,
    ) -> Result<Option<(Value, Value)>, Error> {
        let coordinate = self.coordinate(ruby, raw, row_number, column)?;
        let style = self.style(ruby, raw)?;
        let link = self.link(ruby, coordinate);
        let formula = raw
            .formula
            .as_ref()
            .map(|f| ruby.str_new(&buffer[f.clone()]));
        let cell: Value = match &raw.body {
            Body::Empty => {
                if !self.empty_cell {
                    return Ok(None);
                }
                ruby.get_inner(&CELL_EMPTY).new_instance((coordinate,))?
            }
            Body::Inline(text) => ruby.get_inner(&CELL_STRING).new_instance((
                ruby.str_new(&buffer[text.clone()]),
                formula,
                style,
                link,
                coordinate,
            ))?,
            // roo looks up the style format for every `v` cell, which raises when
            // the workbook has no styles part, so this does too.
            Body::Value(text) => {
                let text = &buffer[text.clone()];
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
                        let excelx_type = ruby.ary_new_from_values(&[numeric_or_formula, format]);
                        let content = ruby.str_new(text);
                        match value_type {
                            ValueType::Number => {
                                let cell = ruby.get_inner(&CELL_NUMBER).new_instance((
                                    content,
                                    formula,
                                    excelx_type,
                                    style,
                                    link,
                                    coordinate,
                                ))?;
                                if link.is_nil() {
                                    self.keep_number_as_read(ruby, style, format, content, cell)?;
                                }
                                cell
                            }
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

/// The style indexes [`Converter`] keeps a format and type for.
fn cacheable(style: Value) -> Option<usize> {
    usize::try_convert(style).ok().filter(|i| *i < 1 << 16)
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
