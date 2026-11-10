//! Streaming reader for worksheet XML that yields the raw cell records roo
//! builds its cells from.
//!
//! roo reads a worksheet in two ways, and each one selects rows and cells
//! differently:
//!
//! * [`Mode::Document`] mirrors `Roo::Excelx::SheetDoc#extract_cells`: the
//!   XPath `/worksheet/sheetData/row` and then `c` children, on a document
//!   whose namespaces were removed, so names compare by local name.
//! * [`Mode::Stream`] mirrors `Roo::Utils.each_element(path, "row")` plus
//!   `SheetDoc#each_cell`: every element whose qualified name is `row`, at any
//!   depth, and every element child of that row.
//!
//! Inside a cell both modes follow `SheetDoc#cell_from_xml`: the first `v`
//! child, or the first `is` child with non-empty text, decides the value, and
//! an `f` child seen before it is the formula.

use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::ops::Range;
use std::path::Path;

use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::XmlVersion;

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

type Result<T> = std::result::Result<T, Unsupported>;

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

pub struct SheetReader<R: BufRead> {
    xml: quick_xml::Reader<R>,
    mode: Mode,
    buf: Vec<u8>,
    depth: usize,
    /// Document mode: depth of the open `sheetData` element under the
    /// `worksheet` root, if any.
    sheet_data_depth: Option<usize>,
    root_is_worksheet: bool,
    done: bool,
    /// An error met after some rows of a batch, returned by the next call.
    pending_error: Option<Unsupported>,
}

impl SheetReader<BufReader<File>> {
    pub fn open(path: &Path, mode: Mode) -> Result<Self> {
        let file = File::open(path).map_err(|e| Unsupported(e.to_string()))?;
        Ok(Self::new(BufReader::with_capacity(64 * 1024, file), mode))
    }
}

#[derive(Clone, Copy)]
enum Child {
    Is,
    F,
    V,
    Other,
}

fn child(e: &BytesStart<'_>) -> Child {
    match e.local_name().as_ref() {
        "is" => Child::Is,
        "f" => Child::F,
        "v" => Child::V,
        _ => Child::Other,
    }
}

impl<R: BufRead> SheetReader<R> {
    pub fn new(input: R, mode: Mode) -> Self {
        let mut xml = quick_xml::Reader::from_reader(input);
        xml.config_mut().check_end_names = true;
        Self {
            xml,
            mode,
            buf: Vec::with_capacity(16 * 1024),
            depth: 0,
            sheet_data_depth: None,
            root_is_worksheet: false,
            done: false,
            pending_error: None,
        }
    }

    /// Reads up to `limit` rows. An empty vector means the sheet is done. When
    /// an error stops a batch, the rows before it are returned first and the
    /// error on the next call.
    pub fn next_rows(&mut self, limit: usize) -> Result<Vec<RawRow>> {
        if let Some(error) = self.pending_error.take() {
            return Err(error);
        }
        let mut rows = Vec::with_capacity(limit.min(1024));
        while rows.len() < limit {
            match self.next_row() {
                Ok(Some(row)) => rows.push(row),
                Ok(None) => break,
                Err(error) if rows.is_empty() => return Err(error),
                Err(error) => {
                    self.pending_error = Some(error);
                    break;
                }
            }
        }
        Ok(rows)
    }

    pub fn next_row(&mut self) -> Result<Option<RawRow>> {
        let mut buf = std::mem::take(&mut self.buf);
        let row = self.find_row(&mut buf);
        self.buf = buf;
        row
    }

    fn find_row(&mut self, buf: &mut Vec<u8>) -> Result<Option<RawRow>> {
        if self.done {
            return Ok(None);
        }
        loop {
            buf.clear();
            match self.xml.read_event_into(buf)? {
                Event::Start(e) => {
                    self.depth += 1;
                    if self.is_row(&e) {
                        let row = self.read_row(buf)?;
                        self.depth -= 1;
                        return Ok(Some(row));
                    }
                    self.enter(&e);
                }
                Event::Empty(e) => {
                    self.depth += 1;
                    let is_row = self.is_row(&e);
                    self.depth -= 1;
                    if is_row {
                        return Ok(Some(RawRow::default()));
                    }
                }
                Event::End(_) => {
                    if self.sheet_data_depth == Some(self.depth) {
                        self.sheet_data_depth = None;
                    }
                    self.depth -= 1;
                }
                Event::Decl(decl) => check_encoding(&decl)?,
                Event::Eof => {
                    self.done = true;
                    return Ok(None);
                }
                _ => {}
            }
        }
    }

    /// Tracks the `/worksheet/sheetData` path for document mode. `self.depth`
    /// is already the depth of `e`.
    fn enter(&mut self, e: &BytesStart<'_>) {
        if self.mode != Mode::Document {
            return;
        }
        let name = e.local_name();
        if self.depth == 1 {
            self.root_is_worksheet = name.as_ref() == "worksheet";
        } else if self.depth == 2 && self.root_is_worksheet && name.as_ref() == "sheetData" {
            self.sheet_data_depth = Some(2);
        }
    }

    /// `self.depth` is already the depth of `e`.
    fn is_row(&self, e: &BytesStart<'_>) -> bool {
        match self.mode {
            Mode::Document => {
                self.sheet_data_depth == Some(self.depth - 1) && e.local_name().as_ref() == "row"
            }
            Mode::Stream => e.name().as_ref() == "row",
        }
    }

    fn is_cell(&self, e: &BytesStart<'_>) -> bool {
        match self.mode {
            Mode::Document => e.local_name().as_ref() == "c",
            Mode::Stream => true,
        }
    }

    /// Reads the children of an open `row` up to its end tag.
    fn read_row(&mut self, buf: &mut Vec<u8>) -> Result<RawRow> {
        let mut row = RawRow::default();
        loop {
            buf.clear();
            match self.xml.read_event_into(buf)? {
                Event::Start(e) => {
                    if self.is_cell(&e) {
                        let cell = cell_attributes(&e)?;
                        self.read_cell(buf, &mut row, cell)?;
                    } else {
                        self.skip(buf)?;
                    }
                }
                Event::Empty(e) => {
                    if self.is_cell(&e) {
                        row.cells.push(cell_attributes(&e)?);
                    }
                }
                Event::End(_) => return Ok(row),
                Event::Eof => return Err(Unsupported("unexpected end of row".into())),
                _ => {}
            }
        }
    }

    /// Reads the children of an open cell up to its end tag.
    fn read_cell(&mut self, buf: &mut Vec<u8>, row: &mut RawRow, mut cell: RawCell) -> Result<()> {
        let mut decided = false;
        loop {
            buf.clear();
            let (child, empty) = match self.xml.read_event_into(buf)? {
                Event::Start(e) => (child(&e), false),
                Event::Empty(e) => (child(&e), true),
                Event::End(_) => break,
                Event::Eof => return Err(Unsupported("unexpected end of cell".into())),
                _ => continue,
            };
            if decided {
                if !empty {
                    self.skip(buf)?;
                }
                continue;
            }
            let start = row.text.len();
            match (child, empty) {
                (Child::Is, false) => {
                    self.text_of_t_descendants(buf, &mut row.text)?;
                    if row.text.len() > start {
                        cell.body = Body::Inline(start..row.text.len());
                        decided = true;
                    }
                }
                (Child::F, false) => {
                    self.text_content(buf, &mut row.text)?;
                    cell.formula = Some(start..row.text.len());
                }
                (Child::F, true) => cell.formula = Some(start..start),
                (Child::V, false) => {
                    self.text_content(buf, &mut row.text)?;
                    cell.body = Body::Value(start..row.text.len());
                    decided = true;
                }
                (Child::V, true) => {
                    cell.body = Body::Value(start..start);
                    decided = true;
                }
                (_, false) => self.skip(buf)?,
                (_, true) => {}
            }
        }
        if !decided {
            cell.formula = None;
        }
        row.cells.push(cell);
        Ok(())
    }

    /// Nokogiri's `Node#content` of an open element: every text and CDATA
    /// descendant, with references resolved.
    fn text_content(&mut self, buf: &mut Vec<u8>, out: &mut String) -> Result<()> {
        let mut depth = 0usize;
        loop {
            buf.clear();
            match self.xml.read_event_into(buf)? {
                Event::Start(_) => depth += 1,
                Event::End(_) if depth == 0 => return Ok(()),
                Event::End(_) => depth -= 1,
                Event::Text(t) => out.push_str(&t.xml10_content()),
                Event::CData(t) => out.push_str(&t.xml10_content()),
                Event::GeneralRef(r) => push_reference(out, &r)?,
                Event::Eof => return Err(Unsupported("unexpected end of element".into())),
                _ => {}
            }
        }
    }

    /// `is_element.search("t").map(&:content).join` for an open `is`.
    fn text_of_t_descendants(&mut self, buf: &mut Vec<u8>, out: &mut String) -> Result<()> {
        let mut depth = 0usize;
        let mut t_depth: Option<usize> = None;
        loop {
            buf.clear();
            match self.xml.read_event_into(buf)? {
                Event::Start(e) => {
                    depth += 1;
                    if t_depth.is_none() && e.local_name().as_ref() == "t" {
                        t_depth = Some(depth);
                    }
                }
                Event::End(_) if depth == 0 => return Ok(()),
                Event::End(_) => {
                    if t_depth == Some(depth) {
                        t_depth = None;
                    }
                    depth -= 1;
                }
                Event::Text(t) if t_depth.is_some() => out.push_str(&t.xml10_content()),
                Event::CData(t) if t_depth.is_some() => out.push_str(&t.xml10_content()),
                Event::GeneralRef(r) if t_depth.is_some() => push_reference(out, &r)?,
                Event::Eof => return Err(Unsupported("unexpected end of element".into())),
                _ => {}
            }
        }
    }

    /// Skips the rest of an open element.
    fn skip(&mut self, buf: &mut Vec<u8>) -> Result<()> {
        let mut depth = 0usize;
        loop {
            buf.clear();
            match self.xml.read_event_into(buf)? {
                Event::Start(_) => depth += 1,
                Event::End(_) if depth == 0 => return Ok(()),
                Event::End(_) => depth -= 1,
                Event::Eof => return Err(Unsupported("unexpected end of element".into())),
                _ => {}
            }
        }
    }
}

fn cell_attributes(e: &BytesStart<'_>) -> Result<RawCell> {
    let mut cell = RawCell {
        reference: Reference::Missing,
        style: Style::Index(0),
        kind: Kind::ByFormat,
        formula: None,
        body: Body::Empty,
    };
    for attribute in e.attributes() {
        let attribute = attribute?;
        let value = || attribute.normalized_value(XmlVersion::Implicit1_0);
        match attribute.key.as_ref() {
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
            _ => {}
        }
    }
    Ok(cell)
}

fn push_reference(text: &mut String, reference: &BytesRef<'_>) -> Result<()> {
    if reference.is_char_ref() {
        match reference.resolve_char_ref()? {
            Some(c) => text.push(c),
            None => return Err(Unsupported("invalid character reference".into())),
        }
        return Ok(());
    }
    let resolved = match reference.xml10_content().as_ref() {
        "lt" => '<',
        "gt" => '>',
        "amp" => '&',
        "apos" => '\'',
        "quot" => '"',
        other => return Err(Unsupported(format!("entity reference &{other};"))),
    };
    text.push(resolved);
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

#[cfg(test)]
mod tests {
    use super::*;

    const NS: &str = r#"xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main""#;

    fn rows(xml: &str, mode: Mode) -> Vec<RawRow> {
        let mut reader = SheetReader::new(xml.as_bytes(), mode);
        let mut out = Vec::new();
        while let Some(row) = reader.next_row().unwrap() {
            out.push(row);
        }
        out
    }

    /// `(reference, formula, body)` of every cell, with text resolved.
    fn texts(row: &RawRow) -> Vec<(Reference, Option<&str>, Option<&str>)> {
        row.cells
            .iter()
            .map(|c| {
                let body = match &c.body {
                    Body::Empty => None,
                    Body::Inline(s) | Body::Value(s) => Some(row.text(s)),
                };
                (
                    c.reference.clone(),
                    c.formula.as_ref().map(|s| row.text(s)),
                    body,
                )
            })
            .collect()
    }

    fn at(row: i64, column: i64) -> Reference {
        Reference::At { row, column }
    }

    #[test]
    fn reads_values_formulas_and_inline_strings() {
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><worksheet {NS}><sheetData>
            <row r="1"><c r="A1" s="2" t="n"><f>B1+1</f><v>1.0</v></c><c r="B1" t="inlineStr"><is><r><t>a</t></r><rPh><t>b</t></rPh></is></c></row>
            </sheetData></worksheet>"#
        );
        let got = rows(&xml, Mode::Document);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].cells[0].style, Style::Index(2));
        assert_eq!(got[0].cells[0].kind, Kind::ByFormat);
        assert_eq!(got[0].cells[1].kind, Kind::InlineStr);
        assert!(matches!(got[0].cells[1].body, Body::Inline(_)));
        assert_eq!(
            texts(&got[0]),
            vec![
                (at(1, 1), Some("B1+1"), Some("1.0")),
                (at(1, 2), None, Some("ab"))
            ]
        );
    }

    #[test]
    fn keeps_attributes_ruby_has_to_read() {
        let xml = format!(
            r#"<worksheet {NS}><sheetData><row><c r="1A" s=" 3" t="e"/><c/></row></sheetData></worksheet>"#
        );
        let got = rows(&xml, Mode::Document);
        let cells = &got[0].cells;
        assert_eq!(cells[0].reference, Reference::Invalid("1A".into()));
        assert_eq!(cells[0].style, Style::Other(" 3".into()));
        assert_eq!(cells[0].kind, Kind::ByFormat);
        assert_eq!(cells[1].reference, Reference::Missing);
        assert_eq!(cells[1].style, Style::Index(0));
    }

    #[test]
    fn resolves_references_and_normalizes_line_ends() {
        let xml = format!(
            "<worksheet {NS}><sheetData><row><c r=\"A1\" t=\"str\"><v>a&amp;b&#13;\r\nc<![CDATA[<d>]]></v></c></row></sheetData></worksheet>"
        );
        let got = rows(&xml, Mode::Document);
        assert_eq!(texts(&got[0])[0].2, Some("a&b\r\nc<d>"));
    }

    #[test]
    fn first_value_wins_and_formula_after_it_is_ignored() {
        let xml = format!(
            "<worksheet {NS}><sheetData><row><c r=\"A1\"><v>1</v><f>X</f><v>2</v></c><c r=\"B1\"><f>Y</f></c></row></sheetData></worksheet>"
        );
        let got = rows(&xml, Mode::Document);
        assert_eq!(
            texts(&got[0]),
            vec![(at(1, 1), None, Some("1")), (at(1, 2), None, None)]
        );
    }

    #[test]
    fn empty_inline_string_does_not_decide_the_value() {
        let xml = format!(
            "<worksheet {NS}><sheetData><row><c r=\"A1\" t=\"inlineStr\"><is><t></t></is><v>7</v></c></row></sheetData></worksheet>"
        );
        let got = rows(&xml, Mode::Document);
        assert!(matches!(got[0].cells[0].body, Body::Value(_)));
        assert_eq!(texts(&got[0])[0].2, Some("7"));
    }

    #[test]
    fn document_mode_reads_only_sheet_data_rows_and_c_children() {
        let xml = format!(
            "<worksheet {NS}><sheetPr><row/></sheetPr><sheetData><row r=\"1\"/><row r=\"2\"><x>1</x><c r=\"A2\"/></row></sheetData></worksheet>"
        );
        let got = rows(&xml, Mode::Document);
        assert_eq!(got.len(), 2);
        assert!(got[0].cells.is_empty());
        assert_eq!(texts(&got[1]), vec![(at(2, 1), None, None)]);
    }

    #[test]
    fn document_mode_matches_prefixed_names_by_local_name() {
        let xml = r#"<x:worksheet xmlns:x="urn:x"><x:sheetData><x:row><x:c r="A1"><x:v>3</x:v></x:c></x:row></x:sheetData></x:worksheet>"#;
        let got = rows(xml, Mode::Document);
        assert_eq!(texts(&got[0])[0].2, Some("3"));
    }

    #[test]
    fn stream_mode_needs_an_unprefixed_row_and_takes_every_child_element() {
        let prefixed = r#"<x:worksheet xmlns:x="urn:x"><x:sheetData><x:row><x:c r="A1"/></x:row></x:sheetData></x:worksheet>"#;
        assert!(rows(prefixed, Mode::Stream).is_empty());

        let xml = format!(
            "<worksheet {NS}><sheetData><row><c r=\"A1\"/><extra r=\"B1\"><v>1</v></extra></row></sheetData></worksheet>"
        );
        let got = rows(&xml, Mode::Stream);
        assert_eq!(
            texts(&got[0]),
            vec![(at(1, 1), None, None), (at(1, 2), None, Some("1"))]
        );
    }

    #[test]
    fn rejects_inputs_it_cannot_reproduce() {
        let latin1 = r#"<?xml version="1.0" encoding="ISO-8859-1"?><worksheet/>"#;
        assert!(SheetReader::new(latin1.as_bytes(), Mode::Document)
            .next_row()
            .is_err());
        let entity = format!(
            "<worksheet {NS}><sheetData><row><c><v>&custom;</v></c></row></sheetData></worksheet>"
        );
        assert!(SheetReader::new(entity.as_bytes(), Mode::Document)
            .next_row()
            .is_err());
    }

    #[test]
    fn returns_the_rows_before_an_error_first() {
        let xml = format!(
            "<worksheet {NS}><sheetData><row/><row><c><v>&x;</v></c></row></sheetData></worksheet>"
        );
        let mut reader = SheetReader::new(xml.as_bytes(), Mode::Document);
        assert_eq!(reader.next_rows(5).unwrap().len(), 1);
        assert!(reader.next_rows(5).is_err());
    }

    #[test]
    fn batches_rows() {
        let xml = format!("<worksheet {NS}><sheetData><row/><row/><row/></sheetData></worksheet>");
        let mut reader = SheetReader::new(xml.as_bytes(), Mode::Document);
        assert_eq!(reader.next_rows(2).unwrap().len(), 2);
        assert_eq!(reader.next_rows(2).unwrap().len(), 1);
        assert!(reader.next_rows(2).unwrap().is_empty());
    }

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
