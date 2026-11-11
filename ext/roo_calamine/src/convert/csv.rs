//! The text `Roo::Formatters::CSV#cell_to_csv` writes for a cell, from the
//! value the cell holds or would hold, without building the cell when its
//! value is enough. Formatting that depends on Ruby (numbers, dates, errors)
//! stays the `Roo::Calamine::CSV` helpers of the workbook.

use magnus::encoding::EncodingCapable;
use magnus::value::ReprValue;
use magnus::{Error, RString, Ruby, Value};

use super::{Converter, ValueType};
use crate::reader::{Body, Kind, RawCell, Reference};

/// The roo cell class a record becomes, as far as its CSV text tells them
/// apart.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    String,
    Boolean,
    Number,
    Date,
}

/// `%("#{text.gsub('"', '""')}")`.
fn quote(out: &mut Vec<u8>, text: &[u8]) {
    out.push(b'"');
    let mut from = 0;
    for (i, &byte) in text.iter().enumerate() {
        if byte == b'"' {
            out.extend_from_slice(&text[from..=i]);
            out.push(b'"');
            from = i + 1;
        }
    }
    out.extend_from_slice(&text[from..]);
    out.push(b'"');
}

/// Appends a String a Ruby call returned.
fn append(out: &mut Vec<u8>, text: RString) {
    out.extend_from_slice(unsafe { text.as_slice() });
}

impl Converter {
    /// Appends to `out` what `csv_field` writes for the cell [`Self::cell`]
    /// builds for `raw` when `cell.empty?` is false, and nothing when it is
    /// true. `kept` is the value that cell will hold, when one was handed out
    /// before it was built. `false`, with nothing appended, when the cell has
    /// to be built and asked.
    pub fn csv_field(
        &self,
        ruby: &Ruby,
        book: Value,
        buffer: &str,
        raw: &RawCell,
        kept: Option<Value>,
        out: &mut Vec<u8>,
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
                    quote(out, buffer[text.clone()].as_bytes());
                }
                return Ok(true);
            }
            (Body::Inline(_), _) | (Body::Value(_), Kind::Shared | Kind::Str) => Class::String,
            (Body::Value(_), Kind::Boolean) => Class::Boolean,
            (Body::Value(_), Kind::InlineStr) => Class::Number,
            (Body::Value(_), Kind::ByFormat) => {
                match self.style_info(ruby, self.style(ruby, raw)?)?.1 {
                    ValueType::Number => Class::Number,
                    ValueType::Date => Class::Date,
                    ValueType::DateTime | ValueType::Time => return Ok(false),
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
            (Class::String, Some(text)) => quote_string(ruby, book, text, out)?,
            (Class::String, None) => return Ok(false),
            (_, Some(text)) if formula => quote_string(ruby, book, text, out)?,
            (Class::Boolean, _) => {
                let text = match (value.to_bool(), formula) {
                    (true, false) => "\"true\"",
                    (false, false) => "\"false\"",
                    (true, true) => "true",
                    (false, true) => "false",
                };
                out.extend_from_slice(text.as_bytes());
            }
            (_, _) if formula => append(out, book.funcall("csv_formula_value", (value,))?),
            (Class::Number, _) => append(out, book.funcall("csv_number", (value,))?),
            (Class::Date, _) => append(out, value.funcall("to_s", ())?),
        }
        Ok(true)
    }
}

/// `csv_quote(text) unless text.empty?`, quoting UTF-8 text here, where
/// `gsub` doubles each `"` byte even in broken text, and leaving any other
/// encoding to Ruby.
fn quote_string(ruby: &Ruby, book: Value, text: RString, out: &mut Vec<u8>) -> Result<(), Error> {
    if text.is_empty() {
        return Ok(());
    }
    let index = text.enc_get();
    let utf8 = index == ruby.utf8_encindex() || index == ruby.usascii_encindex();
    if utf8 {
        quote(out, unsafe { text.as_slice() });
    } else {
        append(out, book.funcall("csv_quote", (text,))?);
    }
    Ok(())
}
