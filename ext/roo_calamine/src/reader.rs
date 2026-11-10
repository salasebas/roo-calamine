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
use std::path::Path;

use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::name::QName;
use quick_xml::XmlVersion;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Document,
    Stream,
}

/// The value-bearing part of a cell, as `cell_from_xml` sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    /// No `v` child and no `is` child with text.
    Empty,
    /// Text of every `t` element inside the first non-empty `is` child.
    Inline(String),
    /// Text content of the first `v` child.
    Value(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawCell {
    pub r: Option<String>,
    pub s: Option<String>,
    pub t: Option<String>,
    pub formula: Option<String>,
    pub body: Body,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RawRow {
    pub cells: Vec<RawCell>,
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

impl<R: BufRead> SheetReader<R> {
    pub fn new(input: R, mode: Mode) -> Self {
        let mut xml = quick_xml::Reader::from_reader(input);
        let config = xml.config_mut();
        config.check_end_names = true;
        config.expand_empty_elements = false;
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
        if self.done {
            return Ok(None);
        }
        loop {
            self.buf.clear();
            let event = self.xml.read_event_into(&mut self.buf)?.into_owned();
            match event {
                Event::Start(e) => {
                    self.depth += 1;
                    if self.is_row(&e) {
                        let e = e.into_owned();
                        let row = self.read_row(&e)?;
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

    fn read_row(&mut self, row: &BytesStart<'static>) -> Result<RawRow> {
        let mut cells = Vec::new();
        let mut buf = Vec::with_capacity(1024);
        loop {
            buf.clear();
            match self.xml.read_event_into(&mut buf)? {
                Event::Start(e) => {
                    let e = e.into_owned();
                    if self.is_cell(&e) {
                        cells.push(self.read_cell(&e, false)?);
                    } else {
                        self.skip(&e)?;
                    }
                }
                Event::Empty(e) => {
                    if self.is_cell(&e) {
                        cells.push(self.read_cell(&e, true)?);
                    }
                }
                Event::End(e) if e.name() == row.name() => return Ok(RawRow { cells }),
                Event::Eof => return Err(Unsupported("unexpected end of row".into())),
                _ => {}
            }
        }
    }

    fn is_cell(&self, e: &BytesStart<'_>) -> bool {
        match self.mode {
            Mode::Document => e.local_name().as_ref() == "c",
            Mode::Stream => true,
        }
    }

    fn read_cell(&mut self, c: &BytesStart<'_>, empty: bool) -> Result<RawCell> {
        let mut cell = RawCell {
            r: attribute(c, "r")?,
            s: attribute(c, "s")?,
            t: attribute(c, "t")?,
            formula: None,
            body: Body::Empty,
        };
        if empty {
            return Ok(cell);
        }
        let mut decided = false;
        let mut buf = Vec::with_capacity(256);
        loop {
            buf.clear();
            match self.xml.read_event_into(&mut buf)? {
                Event::Start(e) => {
                    let e = e.into_owned();
                    if decided {
                        self.skip(&e)?;
                        continue;
                    }
                    match e.local_name().as_ref() {
                        "is" => {
                            let text = self.text_of_t_descendants(&e)?;
                            if !text.is_empty() {
                                cell.body = Body::Inline(text);
                                decided = true;
                            }
                        }
                        "f" => cell.formula = Some(self.text_content(&e)?),
                        "v" => {
                            cell.body = Body::Value(self.text_content(&e)?);
                            decided = true;
                        }
                        _ => self.skip(&e)?,
                    }
                }
                Event::Empty(e) => {
                    if decided {
                        continue;
                    }
                    match e.local_name().as_ref() {
                        "f" => cell.formula = Some(String::new()),
                        "v" => {
                            cell.body = Body::Value(String::new());
                            decided = true;
                        }
                        _ => {}
                    }
                }
                Event::End(e) if e.name() == c.name() => {
                    if !decided {
                        cell.formula = None;
                    }
                    return Ok(cell);
                }
                Event::Eof => return Err(Unsupported("unexpected end of cell".into())),
                _ => {}
            }
        }
    }

    /// Nokogiri's `Node#content`: every text and CDATA descendant, with
    /// references resolved.
    fn text_content(&mut self, start: &BytesStart<'_>) -> Result<String> {
        let mut text = String::new();
        let mut depth = 0usize;
        let mut buf = Vec::with_capacity(128);
        loop {
            buf.clear();
            match self.xml.read_event_into(&mut buf)? {
                Event::Start(_) => depth += 1,
                Event::End(e) => {
                    if depth == 0 {
                        debug_assert_eq!(e.name(), start.name());
                        return Ok(text);
                    }
                    depth -= 1;
                }
                Event::Text(t) => text.push_str(&t.xml10_content()),
                Event::CData(t) => text.push_str(&t.xml10_content()),
                Event::GeneralRef(r) => push_reference(&mut text, &r)?,
                Event::Eof => return Err(Unsupported("unexpected end of element".into())),
                _ => {}
            }
        }
    }

    /// `is_element.search("t").map(&:content).join`.
    fn text_of_t_descendants(&mut self, start: &BytesStart<'_>) -> Result<String> {
        let mut text = String::new();
        let mut depth = 0usize;
        let mut t_depth: Option<usize> = None;
        let mut buf = Vec::with_capacity(128);
        loop {
            buf.clear();
            match self.xml.read_event_into(&mut buf)? {
                Event::Start(e) => {
                    depth += 1;
                    if t_depth.is_none() && e.local_name().as_ref() == "t" {
                        t_depth = Some(depth);
                    }
                }
                Event::End(e) => {
                    if depth == 0 {
                        debug_assert_eq!(e.name(), start.name());
                        return Ok(text);
                    }
                    if t_depth == Some(depth) {
                        t_depth = None;
                    }
                    depth -= 1;
                }
                Event::Text(t) if t_depth.is_some() => text.push_str(&t.xml10_content()),
                Event::CData(t) if t_depth.is_some() => text.push_str(&t.xml10_content()),
                Event::GeneralRef(r) if t_depth.is_some() => push_reference(&mut text, &r)?,
                Event::Eof => return Err(Unsupported("unexpected end of element".into())),
                _ => {}
            }
        }
    }

    fn skip(&mut self, start: &BytesStart<'_>) -> Result<()> {
        let name = start.name().as_ref().to_owned();
        let mut buf = Vec::new();
        self.xml.read_to_end_into(QName(&name), &mut buf)?;
        Ok(())
    }
}

fn attribute(e: &BytesStart<'_>, key: &str) -> Result<Option<String>> {
    match e.try_get_attribute(key)? {
        Some(attr) => Ok(Some(
            attr.normalized_value(XmlVersion::Implicit1_0)?.into_owned(),
        )),
        None => Ok(None),
    }
}

fn push_reference(text: &mut String, reference: &BytesRef<'_>) -> Result<()> {
    if reference.is_char_ref() {
        match reference.resolve_char_ref()? {
            Some(c) => text.push(c),
            None => return Err(Unsupported("invalid character reference".into())),
        }
        return Ok(());
    }
    let name = reference.xml10_content();
    let resolved = match name.as_ref() {
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

    fn cell(r: &str, body: Body) -> RawCell {
        RawCell {
            r: Some(r.into()),
            s: None,
            t: None,
            formula: None,
            body,
        }
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
        assert_eq!(
            got[0].cells[0],
            RawCell {
                r: Some("A1".into()),
                s: Some("2".into()),
                t: Some("n".into()),
                formula: Some("B1+1".into()),
                body: Body::Value("1.0".into()),
            }
        );
        assert_eq!(got[0].cells[1].body, Body::Inline("ab".into()));
    }

    #[test]
    fn resolves_references_and_normalizes_line_ends() {
        let xml = format!(
            "<worksheet {NS}><sheetData><row><c r=\"A1\" t=\"str\"><v>a&amp;b&#13;\r\nc<![CDATA[<d>]]></v></c></row></sheetData></worksheet>"
        );
        let got = rows(&xml, Mode::Document);
        assert_eq!(got[0].cells[0].body, Body::Value("a&b\r\nc<d>".into()));
    }

    #[test]
    fn first_value_wins_and_formula_after_it_is_ignored() {
        let xml = format!(
            "<worksheet {NS}><sheetData><row><c r=\"A1\"><v>1</v><f>X</f><v>2</v></c><c r=\"B1\"><f>Y</f></c></row></sheetData></worksheet>"
        );
        let got = rows(&xml, Mode::Document);
        assert_eq!(got[0].cells[0], cell("A1", Body::Value("1".into())));
        assert_eq!(got[0].cells[1], cell("B1", Body::Empty));
    }

    #[test]
    fn empty_inline_string_does_not_decide_the_value() {
        let xml = format!(
            "<worksheet {NS}><sheetData><row><c r=\"A1\" t=\"inlineStr\"><is><t></t></is><v>7</v></c></row></sheetData></worksheet>"
        );
        let got = rows(&xml, Mode::Document);
        assert_eq!(got[0].cells[0].body, Body::Value("7".into()));
    }

    #[test]
    fn document_mode_reads_only_sheet_data_rows_and_c_children() {
        let xml = format!(
            "<worksheet {NS}><sheetPr><row/></sheetPr><sheetData><row r=\"1\"/><row r=\"2\"><x>1</x><c r=\"A2\"/></row></sheetData></worksheet>"
        );
        let got = rows(&xml, Mode::Document);
        assert_eq!(got.len(), 2);
        assert!(got[0].cells.is_empty());
        assert_eq!(got[1].cells, vec![cell("A2", Body::Empty)]);
    }

    #[test]
    fn document_mode_matches_prefixed_names_by_local_name() {
        let xml = r#"<x:worksheet xmlns:x="urn:x"><x:sheetData><x:row><x:c r="A1"><x:v>3</x:v></x:c></x:row></x:sheetData></x:worksheet>"#;
        let got = rows(xml, Mode::Document);
        assert_eq!(got[0].cells[0].body, Body::Value("3".into()));
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
            got[0].cells,
            vec![cell("A1", Body::Empty), cell("B1", Body::Value("1".into()))]
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
}
