//! Raw worksheet records and XML compatibility checks for roo consumers.
mod attributes;
mod xml;
pub use crate::utils::unescape_entity_to_buffer as push_reference;
use attributes::TagAttributes;
use std::fmt;
use std::ops::Range;
pub use xml::XmlReader;
/// Calamine's shared XLSX stream configured through `for_roo`.
pub type SheetReader<R> = crate::xlsx::cells_reader::XlsxCellStream<'static, R>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Document,
    Stream,
}

/// The `r` attribute.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reference {
    Missing,
    /// What `Roo::Utils.extract_coordinate` returns for it.
    At {
        row: i64,
        column: i64,
    },
    /// A value `Roo::Utils.extract_coordinate` raises for.
    Invalid(String),
}

/// The `s` attribute.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Style {
    /// Absent (index 0) or plain decimal digits.
    Index(i64),
    /// Anything else, for Ruby's `String#to_i` to read.
    Other(String),
}

/// The `t` attribute, as far as `SheetDoc#cell_value_type` tells types apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Absent or any other value: the style's number format decides.
    ByFormat,
    Shared,
    Boolean,
    Str,
    InlineStr,
}

/// A byte range of [`RawRow::text`].
pub type Span = Range<usize>;

/// The value-bearing part of a cell, as `cell_from_xml` sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    /// No `v` child and no `is` child with text.
    Empty,
    /// Text of every `t` element inside the first non-empty `is` child.
    Inline(Span),
    /// Text content of the first `v` child.
    Value(Span),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawCell {
    pub reference: Reference,
    pub style: Style,
    pub kind: Kind,
    pub formula: Option<Span>,
    pub body: Body,
}

impl RawCell {
    /// The `[row, column]` key of roo's document-mode `cells` hash for this
    /// cell, the `index`-th of the `row_number`-th row (both 1-based), or
    /// `None` when roo raises for its `r`.
    pub fn position(&self, row_number: usize, index: usize) -> Option<(i64, i64)> {
        match self.reference {
            Reference::At { row, column } => Some((row, column)),
            Reference::Missing => Some((row_number as i64, index as i64)),
            Reference::Invalid(_) => None,
        }
    }
}

/// A row's cells. Their text lives in one buffer, so a row costs a few
/// allocations rather than a few per cell.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RawRow {
    pub text: String,
    pub cells: Vec<RawCell>,
}

impl RawRow {
    pub fn text(&self, span: &Span) -> &str {
        &self.text[span.clone()]
    }
}

/// The input uses XML that this reader does not reproduce exactly the way
/// Nokogiri does, so the caller falls back to roo's own parser.
#[derive(Debug, PartialEq, Eq)]
pub struct Unsupported(pub String);

impl fmt::Display for Unsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Unsupported {}

impl From<quick_xml::Error> for Unsupported {
    fn from(error: quick_xml::Error) -> Self {
        Unsupported(error.to_string())
    }
}

impl From<quick_xml::events::attributes::AttrError> for Unsupported {
    fn from(error: quick_xml::events::attributes::AttrError) -> Self {
        Unsupported(error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Unsupported>;

/// `Roo::Utils.extract_coordinate` for well-formed references. `None` means
/// roo raises.
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

/// `String#to_i` for plain decimal digits; `None` leaves it to Ruby.
pub fn parse_index(text: &str) -> Option<usize> {
    if text.is_empty() || text.len() > 18 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

pub(crate) fn cell_attributes(
    attributes: &TagAttributes,
    raw: &str,
    mode: Mode,
) -> Result<RawCell> {
    let mut cell = RawCell {
        reference: Reference::Missing,
        style: Style::Index(0),
        kind: Kind::ByFormat,
        formula: None,
        body: Body::Empty,
    };
    for attribute in attributes.iter(raw) {
        let value = || attribute.normalized_value();
        match attribute.key {
            "r" => {
                let value = value()?;
                cell.reference = match parse_reference(&value) {
                    Some((row, column)) => Reference::At { row, column },
                    None => Reference::Invalid(value.into_owned()),
                };
            }
            "s" => {
                let value = value()?;
                cell.style = match parse_index(&value).filter(|i| *i < 1 << 30) {
                    Some(index) => Style::Index(index as i64),
                    None => Style::Other(value.into_owned()),
                };
            }
            "t" => {
                cell.kind = match value()?.as_ref() {
                    "s" => Kind::Shared,
                    "b" => Kind::Boolean,
                    "str" => Kind::Str,
                    "inlineStr" => Kind::InlineStr,
                    _ => Kind::ByFormat,
                };
            }
            // roo's document read removes namespaces first, so `x:t` would
            // count as `t` there.
            key if mode == Mode::Document
                && key.split_once(':').is_some_and(|(prefix, local)| {
                    prefix != "xmlns" && matches!(local, "r" | "s" | "t")
                }) =>
            {
                return Err(Unsupported(format!("the attribute {key} on a cell")));
            }
            _ => {}
        }
    }
    Ok(cell)
}
/// XML 1.0 `Char`: tab, line feed, carriage return, and U+0020 up, except
/// U+FFFE and U+FFFF (Rust strings hold no surrogates).
fn check_chars(text: &str) -> Result<()> {
    let bytes = text.as_bytes();
    // U+FFFE and U+FFFF are EF BF BE and EF BF BF in UTF-8.
    let suspect = bytes
        .iter()
        .fold(false, |suspect, &b| suspect | (b < 0x20) | (b == 0xEF));
    if !suspect {
        return Ok(());
    }
    let control = bytes
        .iter()
        .any(|&b| b < 0x20 && !matches!(b, b'\t' | b'\n' | b'\r'));
    let noncharacter = bytes
        .windows(3)
        .any(|w| matches!(w, [0xEF, 0xBF, 0xBE | 0xBF]));
    if control || noncharacter {
        return Err(Unsupported("a character XML does not allow".into()));
    }
    Ok(())
}

/// An ASCII XML name: a letter or `_` first, then those, digits, `-`, and
/// `.`, with at most one `:` between two non-empty parts. Non-ASCII names are
/// valid XML but never appear in worksheets, so they are left to roo rather
/// than checked character by character.
fn check_name(name: &str) -> Result<()> {
    let mut part_start = true;
    let mut prefixed = false;
    for &b in name.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => part_start = false,
            b'0'..=b'9' | b'-' | b'.' if !part_start => {}
            b':' if !part_start && !prefixed => {
                part_start = true;
                prefixed = true;
            }
            _ => return Err(Unsupported(format!("the name {name:?}"))),
        }
    }
    if part_start {
        return Err(Unsupported(format!("the name {name:?}")));
    }
    Ok(())
}

fn check_encoding(decl: &quick_xml::events::BytesDecl<'_>) -> Result<()> {
    if let Some(encoding) = decl.encoding() {
        let encoding = encoding?;
        if !encoding.eq_ignore_ascii_case("utf-8") {
            return Err(Unsupported(format!("XML encoding {encoding}")));
        }
    }
    Ok(())
}

impl From<crate::XlsxError> for Unsupported {
    fn from(error: crate::XlsxError) -> Self {
        Self(error.to_string())
    }
}
impl From<Unsupported> for crate::XlsxError {
    fn from(error: Unsupported) -> Self {
        Self::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    }
}
#[cfg(test)]
mod tests;
