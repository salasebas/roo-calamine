//! The text `Roo::Formatters::CSV#cell_to_csv` writes for a cell, from the
//! value the cell holds or would hold, without building the cell when its
//! value is enough. Formatting that depends on Ruby (numbers, dates, errors)
//! stays the `Roo::Oxide::CSV` helpers of the workbook.

use std::io::Write;

use magnus::encoding::EncodingCapable;
use magnus::value::ReprValue;
use magnus::{Error, Integer, RString, Ruby, TryConvert, Value};

use super::{Converter, ValueType};
use crate::reader::{Body, Kind, RawCell, Reference};

/// The output prints its text at the end of the line that takes it past this
/// many bytes.
const CHUNK: usize = 64 * 1024;

/// CSV text on its way to an IO, printed in chunks of UTF-8 text.
pub struct CsvOutput {
    io: Value,
    bytes: Vec<u8>,
    separator: Vec<u8>,
}

impl CsvOutput {
    pub fn new(io: Value, separator: Vec<u8>) -> Self {
        Self {
            io,
            bytes: Vec::with_capacity(CHUNK * 2),
            separator,
        }
    }

    pub fn separators(&mut self, count: i64) {
        for _ in 0..count {
            self.bytes.extend_from_slice(&self.separator);
        }
    }

    /// Ends the line with `separators` separators and a newline.
    pub fn end_line(&mut self, ruby: &Ruby, separators: i64) -> Result<(), Error> {
        self.separators(separators);
        self.bytes.push(b'\n');
        if self.bytes.len() >= CHUNK {
            self.print(ruby)?;
        }
        Ok(())
    }

    /// Prints the text not printed yet.
    pub fn print(&mut self, ruby: &Ruby) -> Result<(), Error> {
        if self.bytes.is_empty() {
            return Ok(());
        }
        let text = ruby.enc_str_new(&self.bytes, ruby.utf8_encoding());
        self.bytes.clear();
        self.io.funcall::<_, _, Value>("print", (text,))?;
        Ok(())
    }

    /// Writes a field a Ruby call returned. roo prints each field on its
    /// own, and the IO may convert one in another encoding, so such a field
    /// is printed on its own too.
    pub fn field(&mut self, ruby: &Ruby, text: RString) -> Result<(), Error> {
        if utf8(ruby, text) {
            self.bytes.extend_from_slice(unsafe { text.as_slice() });
            return Ok(());
        }
        self.print(ruby)?;
        self.io.funcall::<_, _, Value>("print", (text,))?;
        Ok(())
    }

    /// `%("#{text.gsub('"', '""')}")`.
    fn quote(&mut self, text: &[u8]) {
        self.bytes.push(b'"');
        let mut from = 0;
        for (i, &byte) in text.iter().enumerate() {
            if byte == b'"' {
                self.bytes.extend_from_slice(&text[from..=i]);
                self.bytes.push(b'"');
                from = i + 1;
            }
        }
        self.bytes.extend_from_slice(&text[from..]);
        self.bytes.push(b'"');
    }
}

fn utf8(ruby: &Ruby, text: RString) -> bool {
    let index = text.enc_get();
    index == ruby.utf8_encindex() || index == ruby.usascii_encindex()
}

/// The roo cell class a record becomes, as far as its CSV text tells them
/// apart.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    String,
    Boolean,
    Number,
    Temporal,
}

impl Converter {
    /// Writes what `csv_field` writes for the cell [`Self::cell`] builds for
    /// `raw` when `cell.empty?` is false, and nothing when it is true. `kept`
    /// is the value that cell will hold, when one was handed out before it
    /// was built. `false`, with nothing written, when the cell has to be
    /// built and asked.
    pub fn csv_field(
        &self,
        ruby: &Ruby,
        book: Value,
        buffer: &str,
        raw: &RawCell,
        kept: Option<Value>,
        output: &mut CsvOutput,
    ) -> Result<bool, Error> {
        let Reference::At { row, column } = raw.reference else {
            return Ok(false);
        };
        if !self.build_cells || self.has_link(ruby, row, column) {
            return Ok(false);
        }
        let class = match (&raw.body, raw.kind) {
            (Body::Empty, _) => return Ok(true),
            (Body::Inline(text), _) | (Body::Value(text), Kind::Str) if kept.is_none() => {
                if !text.is_empty() {
                    output.quote(buffer[text.clone()].as_bytes());
                }
                return Ok(true);
            }
            (Body::Inline(_), _) | (Body::Value(_), Kind::Shared | Kind::Str) => Class::String,
            (Body::Value(_), Kind::Boolean) => Class::Boolean,
            (Body::Value(_), Kind::InlineStr) => Class::Number,
            (Body::Value(_), Kind::ByFormat) => {
                match self.style_info(ruby, self.style(ruby, raw)?)?.1 {
                    ValueType::Number => Class::Number,
                    ValueType::Date | ValueType::DateTime | ValueType::Time => Class::Temporal,
                }
            }
        };
        let value = match kept {
            Some(value) => value,
            None => match self.known_value(ruby, buffer, raw)? {
                Some(value) => value,
                None => return Ok(false),
            },
        };
        let formula = raw.formula.is_some();
        match (class, RString::from_value(value)) {
            // `:string`, and a `:formula` String: quoted unless empty.
            (Class::String, Some(text)) => quote_string(ruby, book, text, output)?,
            (Class::String, None) => return Ok(false),
            (_, Some(text)) if formula => quote_string(ruby, book, text, output)?,
            (Class::Boolean, _) => {
                let text = match (value.to_bool(), formula) {
                    (true, false) => "\"true\"",
                    (false, false) => "\"false\"",
                    (true, true) => "true",
                    (false, true) => "false",
                };
                output.bytes.extend_from_slice(text.as_bytes());
            }
            (_, _) if formula => {
                output.field(ruby, book.funcall("csv_formula_value", (value,))?)?
            }
            (Class::Number, _) => output.field(ruby, book.funcall("csv_number", (value,))?)?,
            (Class::Temporal, _) if Integer::from_value(value).is_some() => {
                if let Ok(seconds) = i32::try_convert(value) {
                    write!(
                        output.bytes,
                        "{:02}:{:02}:{:02}",
                        seconds.div_euclid(3600),
                        seconds.rem_euclid(3600) / 60,
                        seconds.rem_euclid(60)
                    )
                    .expect("writing to a Vec cannot fail");
                } else {
                    output.field(ruby, book.funcall("integer_to_timestring", (value,))?)?;
                }
            }
            (Class::Temporal, _) => output.field(ruby, value.funcall("to_s", ())?)?,
        }
        Ok(true)
    }
}

/// `csv_quote(text) unless text.empty?`, quoting UTF-8 text here, where
/// `gsub` doubles each `"` byte even in broken text, and leaving any other
/// encoding to Ruby.
fn quote_string(
    ruby: &Ruby,
    book: Value,
    text: RString,
    output: &mut CsvOutput,
) -> Result<(), Error> {
    if text.is_empty() {
        return Ok(());
    }
    if utf8(ruby, text) {
        output.quote(unsafe { text.as_slice() });
        Ok(())
    } else {
        output.field(ruby, book.funcall("csv_quote", (text,))?)
    }
}
