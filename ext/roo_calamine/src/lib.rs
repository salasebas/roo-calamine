//! Native fast path for `roo-calamine`.
//!
//! [`reader`] turns worksheet XML into raw cell records without touching Ruby.
//! This module turns those records into the same `Roo::Excelx::Cell` objects
//! that `Roo::Excelx::SheetDoc#cell_from_xml` builds, by calling roo's own
//! constructors with the same arguments.

pub mod reader;

use std::cell::{Cell as StdCell, RefCell};
use std::ffi::c_void;
use std::fs::File;
use std::io::BufReader;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;

use magnus::value::{Lazy, ReprValue};
use magnus::{
    function, method, prelude::*, Error, ExceptionClass, IntoValue, RArray, RClass, RHash, Ruby,
    Symbol, Value,
};

use reader::{Body, Mode, RawCell, RawRow, SheetReader, Unsupported};

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
static UNSUPPORTED: Lazy<ExceptionClass> =
    Lazy::new(|r| constant(r, "Roo::Calamine::UnsupportedError"));

fn unsupported(ruby: &Ruby, error: Unsupported) -> Error {
    Error::new(ruby.get_inner(&UNSUPPORTED), error.0)
}

/// Runs `work` with the GVL released. `work` must not touch any Ruby object.
fn without_gvl<F: FnOnce() -> R, R>(work: F) -> R {
    struct Call<F, R> {
        work: Option<F>,
        result: Option<std::thread::Result<R>>,
    }

    unsafe extern "C" fn trampoline<F: FnOnce() -> R, R>(data: *mut c_void) -> *mut c_void {
        let call = unsafe { &mut *(data as *mut Call<F, R>) };
        let work = call.work.take().expect("called once");
        call.result = Some(panic::catch_unwind(AssertUnwindSafe(work)));
        std::ptr::null_mut()
    }

    let mut call = Call {
        work: Some(work),
        result: None,
    };
    unsafe {
        rb_sys::rb_thread_call_without_gvl(
            Some(trampoline::<F, R>),
            &mut call as *mut _ as *mut c_void,
            None,
            std::ptr::null_mut(),
        );
    }
    match call.result.expect("trampoline ran") {
        Ok(result) => result,
        Err(payload) => panic::resume_unwind(payload),
    }
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

/// `Roo::Utils.extract_coordinate` for well-formed references. `None` means
/// roo raises, so the caller lets roo's own method produce the error.
fn parse_reference(reference: &str) -> Option<(i64, i64)> {
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
fn parse_index(text: &str) -> Option<usize> {
    if text.is_empty() || text.len() > 18 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// Per-call conversion state. It lives on the stack, so every Ruby object it
/// holds stays visible to the garbage collector.
struct Converter<'r> {
    ruby: &'r Ruby,
    context: Value,
    mode: Mode,
    empty_cell: bool,
    hyperlinks: Option<RHash>,
    base_date: Value,
    base_timestamp: Value,
    /// Format string per style index, filled on first use.
    formats: RArray,
    types: Vec<Option<ValueType>>,
    shared_strings: Option<RArray>,
    numeric_or_formula: Symbol,
    sym_date: Symbol,
    sym_datetime: Symbol,
    sym_time: Symbol,
}

impl<'r> Converter<'r> {
    fn new(ruby: &'r Ruby, context: Value, mode: Mode) -> Result<Self, Error> {
        let hyperlinks: RHash = context.funcall("hyperlinks", ())?;
        Ok(Self {
            ruby,
            context,
            mode,
            empty_cell: context.funcall::<_, _, Value>("empty_cell", ())?.to_bool(),
            hyperlinks: (!hyperlinks.is_empty()).then_some(hyperlinks),
            base_date: context.funcall("base_date", ())?,
            base_timestamp: context.funcall("base_timestamp", ())?,
            formats: ruby.ary_new(),
            types: Vec::new(),
            shared_strings: None,
            numeric_or_formula: ruby.to_symbol("numeric_or_formula"),
            sym_date: ruby.to_symbol("date"),
            sym_datetime: ruby.to_symbol("datetime"),
            sym_time: ruby.to_symbol("time"),
        })
    }

    fn coordinate(&self, raw: &RawCell, row: usize, column: usize) -> Result<Value, Error> {
        let coordinate = self.ruby.get_inner(&COORDINATE);
        match (&raw.r, self.mode) {
            (Some(reference), _) => match parse_reference(reference) {
                Some((row, column)) => coordinate.new_instance((row, column)),
                None => self
                    .context
                    .funcall("coordinate", (self.ruby.str_new(reference),)),
            },
            (None, Mode::Document) => coordinate.new_instance((row, column)),
            (None, Mode::Stream) => self.context.funcall("coordinate", (self.ruby.qnil(),)),
        }
    }

    fn style(&self, raw: &RawCell) -> Result<Value, Error> {
        match raw.s.as_deref() {
            None => Ok(0i64.into_value_with(self.ruby)),
            Some(text) => match parse_index(text).filter(|i| *i < 1 << 30) {
                Some(index) => Ok((index as i64).into_value_with(self.ruby)),
                None => self
                    .context
                    .funcall("style_index", (self.ruby.str_new(text),)),
            },
        }
    }

    /// `[format.to_s, Excelx::Format.to_type(format)]` for a style index.
    fn style_info(&mut self, style: Value) -> Result<(Value, ValueType), Error> {
        let cacheable = usize::try_convert(style).ok().filter(|i| *i < 1 << 16);
        if let Some(index) = cacheable {
            if let Some(Some(value_type)) = self.types.get(index) {
                return Ok((self.formats.entry(index as isize)?, *value_type));
            }
        }
        let info: RArray = self.context.funcall("style", (style,))?;
        let format: Value = info.entry(0)?;
        let type_symbol: Symbol = info.entry(1)?;
        let value_type = if type_symbol.equal(self.sym_date)? {
            ValueType::Date
        } else if type_symbol.equal(self.sym_datetime)? {
            ValueType::DateTime
        } else if type_symbol.equal(self.sym_time)? {
            ValueType::Time
        } else {
            ValueType::Number
        };
        if let Some(index) = cacheable {
            if self.types.len() <= index {
                self.types.resize(index + 1, None);
            }
            self.types[index] = Some(value_type);
            self.formats.store(index as isize, format)?;
        }
        Ok((format, value_type))
    }

    fn shared_string(&mut self, text: &str) -> Result<Value, Error> {
        if let Some(index) = parse_index(text) {
            let table = match self.shared_strings {
                Some(table) => table,
                None => {
                    let table: RArray = self.context.funcall("shared_strings", ())?;
                    self.shared_strings = Some(table);
                    table
                }
            };
            if index < table.len() {
                return table.entry(index as isize);
            }
        }
        self.context
            .funcall("shared_string", (self.ruby.str_new(text),))
    }

    /// Builds the cell `SheetDoc#cell_from_xml` builds for `raw`. `None` means
    /// roo stores no cell (an empty cell without the `empty_cell` option).
    fn cell(
        &mut self,
        raw: RawCell,
        row: usize,
        column: usize,
    ) -> Result<Option<(Value, Value)>, Error> {
        let ruby = self.ruby;
        let coordinate = self.coordinate(&raw, row, column)?;
        let style = self.style(&raw)?;
        let link = match self.hyperlinks {
            Some(hyperlinks) => hyperlinks
                .get(coordinate)
                .unwrap_or_else(|| ruby.qnil().as_value()),
            None => ruby.qnil().as_value(),
        };
        let formula = raw.formula.as_deref().map(|f| ruby.str_new(f));
        let cell: Value =
            match raw.body {
                Body::Empty => {
                    if !self.empty_cell {
                        return Ok(None);
                    }
                    ruby.get_inner(&CELL_EMPTY).new_instance((coordinate,))?
                }
                Body::Inline(text) => ruby.get_inner(&CELL_STRING).new_instance((
                    ruby.str_new(&text),
                    formula,
                    style,
                    link,
                    coordinate,
                ))?,
                // roo looks up the style format for every `v` cell, which raises when
                // the workbook has no styles part, so this does too.
                Body::Value(text) => {
                    match (raw.t.as_deref(), self.style_info(style)?) {
                        (Some("s"), _) => {
                            let value = self.shared_string(&text)?;
                            ruby.get_inner(&CELL_STRING)
                                .new_instance((value, formula, style, link, coordinate))?
                        }
                        (Some("b"), _) => ruby.get_inner(&CELL_BOOLEAN).new_instance((
                            ruby.str_new(&text),
                            formula,
                            style,
                            link,
                            coordinate,
                        ))?,
                        (Some("str"), _) => ruby.get_inner(&CELL_STRING).new_instance((
                            ruby.str_new(&text),
                            formula,
                            style,
                            link,
                            coordinate,
                        ))?,
                        (other, (format, format_type)) => {
                            let value_type = if other == Some("inlineStr") {
                                ValueType::Number
                            } else {
                                format_type
                            };
                            let excelx_type = ruby
                                .ary_new_from_values(&[self.numeric_or_formula.as_value(), format]);
                            let content = ruby.str_new(&text);
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
                                    self.base_date,
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
                                        base,
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

#[magnus::wrap(class = "Roo::Calamine::Native::Reader", free_immediately, size)]
struct Reader {
    sheet: RefCell<SheetReader<BufReader<File>>>,
    mode: Mode,
    rows_read: StdCell<usize>,
}

impl Reader {
    fn new(ruby: &Ruby, path: String, stream: bool) -> Result<Self, Error> {
        let mode = if stream { Mode::Stream } else { Mode::Document };
        let sheet = SheetReader::open(Path::new(&path), mode).map_err(|e| unsupported(ruby, e))?;
        Ok(Self {
            sheet: RefCell::new(sheet),
            mode,
            rows_read: StdCell::new(0),
        })
    }

    fn read_batch(&self, ruby: &Ruby, limit: usize) -> Result<Vec<RawRow>, Error> {
        let mut sheet = self
            .sheet
            .try_borrow_mut()
            .map_err(|_| Error::new(ruby.exception_runtime_error(), "reader is already in use"))?;
        without_gvl(|| sheet.next_rows(limit)).map_err(|e| unsupported(ruby, e))
    }

    /// Document mode: adds the cells of up to `limit` rows to `cells`.
    /// Returns `false` once the sheet is exhausted.
    fn fill_cells(
        ruby: &Ruby,
        rb_self: &Self,
        cells: RHash,
        context: Value,
        limit: usize,
    ) -> Result<bool, Error> {
        let rows = rb_self.read_batch(ruby, limit)?;
        if rows.is_empty() {
            return Ok(false);
        }
        let mut converter = Converter::new(ruby, context, rb_self.mode)?;
        for row in rows {
            let row_number = rb_self.rows_read.get() + 1;
            rb_self.rows_read.set(row_number);
            for (index, raw) in row.cells.into_iter().enumerate() {
                if let Some((coordinate, cell)) = converter.cell(raw, row_number, index + 1)? {
                    cells.aset(coordinate, cell)?;
                }
            }
        }
        Ok(true)
    }

    /// Stream mode: the next `limit` rows as arrays of cells, or `nil`.
    fn next_rows(
        ruby: &Ruby,
        rb_self: &Self,
        context: Value,
        limit: usize,
    ) -> Result<Option<RArray>, Error> {
        let rows = rb_self.read_batch(ruby, limit)?;
        if rows.is_empty() {
            return Ok(None);
        }
        let mut converter = Converter::new(ruby, context, rb_self.mode)?;
        let out = ruby.ary_new_capa(rows.len());
        for row in rows {
            let cells = ruby.ary_new_capa(row.cells.len());
            for (index, raw) in row.cells.into_iter().enumerate() {
                if let Some((_, cell)) = converter.cell(raw, 0, index + 1)? {
                    cells.push(cell)?;
                }
            }
            out.push(cells)?;
        }
        Ok(Some(out))
    }
}

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let calamine = ruby.define_module("Roo")?.define_module("Calamine")?;
    let native = calamine.define_module("Native")?;
    let reader = native.define_class("Reader", ruby.class_object())?;
    reader.define_singleton_method("new", function!(Reader::new, 2))?;
    reader.define_method("fill_cells", method!(Reader::fill_cells, 3))?;
    reader.define_method("next_rows", method!(Reader::next_rows, 2))?;
    Ok(())
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
