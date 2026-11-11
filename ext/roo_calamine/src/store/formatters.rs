use std::fmt::Write;

use magnus::encoding::EncodingCapable;
use magnus::value::ReprValue;
use magnus::{Error, RArray, RHash, RString, Ruby, Symbol, TryConvert, Value};

use super::{Content, Store};
use crate::reader::Kind;

impl Store {
    fn formatter_cell(&self, ruby: &Ruby, record: u32) -> Result<Option<(Value, Symbol)>, Error> {
        let raw = &self.records[record as usize];
        let built = ruby.get_inner(self.built).get(raw.built_key());
        if built.is_none() {
            if raw.content == Content::Empty {
                return Ok(None);
            }
            let converter = ruby.get_inner(self.converter);
            let cell = self.formulas.raw(&self.records, record);
            if let Some(kind) = converter.formatter_type(ruby, &cell)? {
                let value = self.value(ruby, record)?;
                let string =
                    raw.content == Content::Inline || matches!(raw.kind, Kind::Shared | Kind::Str);
                if string && RString::try_convert(value)?.is_empty() {
                    return Ok(None);
                }
                return Ok(Some((value, ruby.sym_new(kind).into())));
            }
        }
        let cell = match built {
            Some(cell) => cell,
            None => self.build(ruby, record)?,
        };
        if cell.funcall::<_, _, Value>("empty?", ())?.to_bool() {
            return Ok(None);
        }
        Ok(Some((
            cell.funcall("value", ())?,
            cell.funcall("type", ())?,
        )))
    }

    pub(super) fn matrix_rows(ruby: &Ruby, rb_self: &Self, area: RArray) -> Result<RArray, Error> {
        let first_row: i64 = area.entry(0)?;
        let first_column: i64 = area.entry(1)?;
        let last_row: i64 = area.entry(2)?;
        let last_column: i64 = area.entry(3)?;
        let rows = ruby.ary_new();
        for row in first_row..=last_row {
            rows.push(Self::row(
                ruby,
                rb_self,
                ruby.integer_from_i64(row).as_value(),
                first_column,
                last_column,
            )?)?;
        }
        Ok(rows)
    }

    pub(super) fn to_yaml(
        ruby: &Ruby,
        rb_self: &Self,
        book: Value,
        prefix: Value,
        area: RArray,
    ) -> Result<RString, Error> {
        let output = ruby.str_new("--- \n");
        let empty_prefix = magnus::RHash::from_value(prefix).is_some_and(|hash| hash.is_empty());
        rb_self.each_formatter_cell(ruby, area, |row, column, value, kind| {
            output.cat(format!("cell_{row}_{column}: \n"));
            if !empty_prefix {
                let prefix: RString = book.funcall("formatter_yaml_prefix", (prefix,))?;
                output.funcall::<_, _, Value>("<<", (prefix,))?;
            }
            output.cat(format!(
                "  row: {row} \n  col: {column} \n  celltype: {} \n  value: ",
                kind.name()?
            ));
            let value = if kind == ruby.sym_new("time") {
                book.funcall("integer_to_timestring", (value,))?
            } else {
                value
            };
            output.funcall::<_, _, Value>("<<", (value.to_r_string()?,))?;
            output.cat(" \n");
            Ok(())
        })?;
        Ok(output)
    }

    pub(super) fn xml_cells(
        ruby: &Ruby,
        rb_self: &Self,
        book: Value,
        area: RArray,
    ) -> Result<RString, Error> {
        let mut output = String::new();
        rb_self.each_formatter_cell(ruby, area, |row, column, value, kind| {
            let text = value.to_r_string()?;
            let bytes = unsafe { text.as_slice() };
            let utf8 =
                text.enc_get() == ruby.utf8_encindex() || text.enc_get() == ruby.usascii_encindex();
            if !utf8
                || std::str::from_utf8(bytes).is_err()
                || bytes.contains(&0)
                || RHash::from_value(value).is_some()
            {
                let cell: RString =
                    book.funcall("formatter_xml_cell", (value, row, column, kind))?;
                output.push_str("    ");
                output.push_str(&cell.to_string()?);
                output.push('\n');
                return Ok(());
            }
            write!(
                output,
                "    <cell row=\"{row}\" column=\"{column}\" type=\"{}\"",
                kind.name()?
            )
            .unwrap();
            if bytes.is_empty() {
                output.push_str("/>\n");
            } else {
                output.push('>');
                for character in std::str::from_utf8(bytes).expect("validated above").chars() {
                    match character {
                        '&' => output.push_str("&amp;"),
                        '<' => output.push_str("&lt;"),
                        '>' => output.push_str("&gt;"),
                        '\r' => output.push_str("&#xD;"),
                        '\u{1}'..='\u{8}'
                        | '\u{b}'
                        | '\u{c}'
                        | '\u{e}'..='\u{1f}'
                        | '\u{fffe}'
                        | '\u{ffff}' => output.push_str("&#xFFFD;"),
                        character if !character.is_ascii() => {
                            write!(output, "&#x{:X};", u32::from(character)).unwrap();
                        }
                        character => output.push(character),
                    }
                }
                output.push_str("</cell>\n");
            }
            Ok(())
        })?;
        Ok(ruby.str_new(&output))
    }

    fn each_formatter_cell(
        &self,
        ruby: &Ruby,
        area: RArray,
        mut visit: impl FnMut(i64, i64, Value, Symbol) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let first_row: i64 = area.entry(0)?;
        let first_column: i64 = area.entry(1)?;
        let last_row: i64 = area.entry(2)?;
        let last_column: i64 = area.entry(3)?;
        let (Some(top), Some(left), Some(bottom), Some(right)) = (
            area.entry::<Option<i64>>(4)?,
            area.entry::<Option<i64>>(5)?,
            area.entry::<Option<i64>>(6)?,
            area.entry::<Option<i64>>(7)?,
        ) else {
            return Ok(());
        };
        for &record in &self.index {
            let raw = &self.records[record as usize];
            let (row, column) = (i64::from(raw.row), i64::from(raw.column));
            if row > last_row.min(bottom) {
                break;
            }
            if row < first_row.max(top)
                || !(first_column.max(left)..=last_column.min(right)).contains(&column)
            {
                continue;
            }
            if let Some((value, kind)) = self.formatter_cell(ruby, record)? {
                visit(row, column, value, kind)?;
            }
        }
        Ok(())
    }
}
