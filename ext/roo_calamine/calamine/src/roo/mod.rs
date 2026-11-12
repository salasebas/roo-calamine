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

mod attributes;

use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::ops::Range;
use std::path::Path;

use quick_xml::events::{BytesStart, Event};

pub use crate::utils::unescape_entity_to_buffer as push_reference;
use attributes::TagAttributes;

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
    /// Whether any event was read yet; an XML declaration must come first.
    started: bool,
    seen_root: bool,
    /// Depth of the open elements, for namespace scopes.
    open_elements: usize,
    /// Namespace prefixes declared on open elements, with the depth they were
    /// declared at.
    prefixes: Vec<(usize, String)>,
    /// `xml:space` values on open elements (`true` for `preserve`), with the
    /// depth they were set at.
    space: Vec<(usize, bool)>,
    /// Stream mode: whether a row is being read.
    in_row: bool,
    /// The attributes of the last start tag read.
    attributes: TagAttributes,
    /// The text length and cell count of the last row read, which the next
    /// row likely has too.
    row_size: (usize, usize),
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
        let config = xml.config_mut();
        config.check_end_names = true;
        config.check_comments = true;
        Self {
            xml,
            mode,
            buf: Vec::with_capacity(16 * 1024),
            depth: 0,
            sheet_data_depth: None,
            root_is_worksheet: false,
            done: false,
            started: false,
            seen_root: false,
            open_elements: 0,
            prefixes: Vec::new(),
            space: Vec::new(),
            in_row: false,
            attributes: TagAttributes::default(),
            row_size: (0, 0),
            pending_error: None,
        }
    }

    /// Reads rows until they hold about `cells` cells (an empty row counts as
    /// one). An empty vector means the sheet is done. When an error stops a
    /// batch, the rows before it are returned first and the error on the next
    /// call.
    pub fn next_rows(&mut self, cells: usize) -> Result<Vec<RawRow>> {
        if let Some(error) = self.pending_error.take() {
            return Err(error);
        }
        let mut rows = Vec::new();
        let mut read = 0;
        while read < cells {
            match self.next_row() {
                Ok(Some(row)) => {
                    read += row.cells.len().max(1);
                    rows.push(row);
                }
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

    /// Reads a validated XML event without building worksheet rows.
    pub fn next_xml_event<'b>(&mut self, buf: &'b mut Vec<u8>) -> Result<Event<'b>> {
        let depth = self.open_elements;
        let event = self.read_event(buf)?;
        if depth == 0 {
            self.check_top_level(&event)?;
        }
        match &event {
            Event::Decl(decl) => check_encoding(decl)?,
            Event::Eof if depth != 0 => {
                return Err(Unsupported("unexpected end of the document".into()));
            }
            _ => {}
        }
        Ok(event)
    }

    fn find_row(&mut self, buf: &mut Vec<u8>) -> Result<Option<RawRow>> {
        if self.done {
            return Ok(None);
        }
        loop {
            buf.clear();
            let event = self.read_event(buf)?;
            if self.depth == 0 {
                self.check_top_level(&event)?;
            }
            match event {
                Event::Start(e) => {
                    self.depth += 1;
                    if self.is_row(&e) {
                        self.in_row = true;
                        let row = self.read_row(buf);
                        self.in_row = false;
                        self.depth -= 1;
                        return row.map(Some);
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
                Event::Eof if self.depth != 0 => {
                    return Err(Unsupported("unexpected end of the sheet".into()));
                }
                Event::Eof => {
                    self.done = true;
                    return Ok(None);
                }
                _ => {}
            }
        }
    }

    /// An element name and its attributes, as far as quick-xml leaves them
    /// unchecked: names and their namespace prefixes, the attribute list, and
    /// the references in values.
    fn check_start(&mut self, e: &BytesStart<'_>) -> Result<()> {
        let name = e.name();
        check_name(name.as_ref())?;
        let raw = e.attributes_raw();
        if self.mode == Mode::Stream && self.in_row {
            // roo's streaming reader yields a nested `row` as a row of its own,
            // and matches prefixed names in a row by namespace.
            if name.as_ref() == "row" || name.as_ref().contains(':') {
                return Err(Unsupported(format!(
                    "<{}> inside a streamed row",
                    name.as_ref()
                )));
            }
        }
        self.attributes.read(raw)?;
        let prefixed = self.attributes.prefixed();
        if !prefixed && !self.attributes.references() {
            return self.check_prefix(name.as_ref());
        }
        let depth = self.open_elements + 1;
        for attribute in self.attributes.iter(raw) {
            let key = attribute.key;
            if attribute.value.contains('&') {
                check_chars(&attribute.normalized_value()?)?;
            }
            if let Some(prefix) = key.strip_prefix("xmlns:") {
                if attribute.value.is_empty() {
                    return Err(Unsupported(format!("an empty namespace for {prefix}")));
                }
                self.prefixes.push((depth, prefix.to_owned()));
            }
            if key == "xml:space" {
                self.space.push((depth, attribute.value == "preserve"));
            }
        }
        // A prefix may be declared after the attribute that uses it.
        if prefixed {
            for attribute in self.attributes.iter(raw) {
                self.check_prefix(attribute.key)?;
            }
        }
        self.check_prefix(name.as_ref())
    }

    /// Ends the namespace and `xml:space` scopes of an element at `depth`.
    fn leave(&mut self, depth: usize) {
        while self.prefixes.last().is_some_and(|(at, _)| *at == depth) {
            self.prefixes.pop();
        }
        while self.space.last().is_some_and(|(at, _)| *at == depth) {
            self.space.pop();
        }
    }

    fn preserves_space(&self) -> bool {
        self.space.last().is_some_and(|(_, preserve)| *preserve)
    }

    /// libxml2 keeps an undeclared prefix as part of the name, so roo would
    /// not see `x:row` as `row`.
    fn check_prefix(&self, name: &str) -> Result<()> {
        match name.split_once(':') {
            Some(("xml" | "xmlns", _)) | None => Ok(()),
            Some((prefix, _)) if self.prefixes.iter().any(|(_, declared)| declared == prefix) => {
                Ok(())
            }
            Some(_) => Err(Unsupported(format!("the undeclared prefix in {name}"))),
        }
    }

    /// Outside the root element, XML allows only one root, whitespace,
    /// comments, and processing instructions.
    fn check_top_level(&mut self, event: &Event<'_>) -> Result<()> {
        match event {
            Event::Start(_) | Event::Empty(_) if self.seen_root => {
                Err(Unsupported("a second root element".into()))
            }
            Event::Start(_) | Event::Empty(_) => {
                self.seen_root = true;
                Ok(())
            }
            Event::Text(t) if !t.trim_ascii().is_empty() => {
                Err(Unsupported("text outside the root element".into()))
            }
            Event::CData(_) | Event::GeneralRef(_) => {
                Err(Unsupported("text outside the root element".into()))
            }
            Event::Eof if !self.seen_root => Err(Unsupported("no root element".into())),
            _ => Ok(()),
        }
    }

    /// The next event, rejecting characters XML does not allow, which
    /// Nokogiri does not accept and quick-xml does not check.
    fn read_event<'b>(&mut self, buf: &'b mut Vec<u8>) -> Result<Event<'b>> {
        if !self.started {
            loop {
                match self.xml.get_mut().fill_buf() {
                    Ok(bytes) if matches!(bytes.first(), Some(0xFF | 0xFE)) => {
                        return Err(Unsupported("a UTF-16 byte-order mark".into()));
                    }
                    Ok(_) => break,
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(Unsupported(error.to_string())),
                }
            }
        }
        let event = self.xml.read_event_into(buf)?;
        let first = !std::mem::replace(&mut self.started, true);
        match &event {
            Event::Start(e) => {
                self.check_start(e)?;
                self.open_elements += 1;
            }
            Event::Empty(e) => {
                self.check_start(e)?;
                self.leave(self.open_elements + 1);
            }
            Event::End(_) => {
                self.leave(self.open_elements);
                self.open_elements = self.open_elements.saturating_sub(1);
            }
            Event::Text(t) if t.contains("]]>") => {
                return Err(Unsupported("`]]>` in text".into()));
            }
            Event::Text(t) | Event::Comment(t) => check_chars(t)?,
            Event::CData(t) => check_chars(&t.xml10_content())?,
            Event::PI(pi) => {
                check_name(pi.target())?;
                check_chars(pi)?;
            }
            Event::GeneralRef(r) => match r.resolve_char_ref()? {
                Some(c) => check_chars(c.encode_utf8(&mut [0; 4]))?,
                None => {
                    if !matches!(
                        r.xml10_content().as_ref(),
                        "lt" | "gt" | "amp" | "apos" | "quot"
                    ) {
                        return Err(Unsupported(
                            "an entity reference XML does not define".into(),
                        ));
                    }
                }
            },
            Event::Decl(_) if !first => {
                return Err(Unsupported("an XML declaration after the start".into()));
            }
            // A DTD can define entities and attribute defaults, which Nokogiri
            // applies and this reader does not.
            Event::DocType(_) => return Err(Unsupported("a document type declaration".into())),
            _ => {}
        }
        Ok(event)
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
        let (text, cells) = self.row_size;
        let mut row = RawRow {
            text: String::with_capacity(text),
            cells: Vec::with_capacity(cells),
        };
        // quick-xml expects markup after a text event; bypass it only after tags.
        // https://docs.rs/quick-xml/0.42.0/src/quick_xml/reader/mod.rs.html#350-392
        let mut after_tag = true;
        loop {
            if after_tag && self.read_plain_cell(&mut row)? {
                continue;
            }
            buf.clear();
            after_tag = true;
            match self.read_event(buf)? {
                Event::Start(e) => {
                    if self.is_cell(&e) {
                        let cell =
                            Self::cell_attributes(&self.attributes, e.attributes_raw(), self.mode)?;
                        self.read_cell(buf, &mut row, cell)?;
                    } else {
                        self.skip(buf)?;
                    }
                }
                Event::Empty(e) => {
                    if self.is_cell(&e) {
                        row.cells.push(Self::cell_attributes(
                            &self.attributes,
                            e.attributes_raw(),
                            self.mode,
                        )?);
                    }
                }
                Event::End(_) => {
                    self.row_size = (row.text.len(), row.cells.len());
                    return Ok(row);
                }
                Event::Eof => return Err(Unsupported("unexpected end of row".into())),
                // roo's streaming takes every child node of a row as a cell and
                // fails on one that is not an element.
                Event::Text(t)
                    if self.mode == Mode::Stream
                        && (self.preserves_space() || !t.trim_ascii().is_empty()) =>
                {
                    return Err(Unsupported("text in a streamed row".into()));
                }
                Event::Comment(_) | Event::PI(_) | Event::CData(_) | Event::GeneralRef(_)
                    if self.mode == Mode::Stream =>
                {
                    return Err(Unsupported("a non-element child of a streamed row".into()));
                }
                _ => after_tag = false,
            }
        }
    }

    /// Reads a whole cell written the plain way straight from the input,
    /// without the event parser: `<c` with only `r`, `s`, and `t` attributes,
    /// then `<f>`, `<v>`, or `<is><t>` children that hold only text and the
    /// five references XML defines, then `</c>`. Reads nothing and returns
    /// `false` for any other bytes, and for a cell that the input buffer cuts
    /// short.
    fn read_plain_cell(&mut self, row: &mut RawRow) -> Result<bool> {
        let Ok(input) = self.xml.get_mut().fill_buf() else {
            return Ok(false);
        };
        let Some(tag) = input.strip_prefix(b"<c") else {
            return Ok(false);
        };
        let Some(tag_end) = tag.iter().position(|&b| b == b'>') else {
            return Ok(false);
        };
        let Ok(raw) = std::str::from_utf8(&tag[..tag_end]) else {
            return Ok(false);
        };
        // XML allows whitespace before the `>` of a start tag.
        let raw = raw.trim_end_matches(' ');
        if !self.attributes.read_common(raw) {
            return Ok(false);
        }
        let mut cell = Self::cell_attributes(&self.attributes, raw, self.mode)?;
        let text_start = row.text.len();
        let mut rest = &tag[tag_end + 1..];
        let mut decided = false;
        loop {
            if let Some(after) = rest.strip_prefix(b"</c>") {
                rest = after;
                break;
            }
            let (child, open, close): (Child, &[u8], &[u8]) = match rest {
                [b'<', b'v', b'>', ..] => (Child::V, b"<v>", b"</v>"),
                [b'<', b'f', b'>', ..] => (Child::F, b"<f>", b"</f>"),
                _ if rest.starts_with(b"<is><t>") => (Child::Is, b"<is><t>", b"</t></is>"),
                _ => return Self::not_plain(row, text_start),
            };
            let start = row.text.len();
            let mut content = &rest[open.len()..];
            rest = loop {
                let length = content
                    .iter()
                    .position(|&b| b < b' ' || matches!(b, b'<' | b'>' | b'&'))
                    .unwrap_or(content.len());
                let Ok(text) = std::str::from_utf8(&content[..length]) else {
                    return Self::not_plain(row, text_start);
                };
                if check_chars(text).is_err() {
                    return Self::not_plain(row, text_start);
                }
                row.text.push_str(text);
                content = &content[length..];
                if let Some(after) = content.strip_prefix(close) {
                    break after;
                }
                let references: [(&[u8], char); 5] = [
                    (b"&amp;", '&'),
                    (b"&lt;", '<'),
                    (b"&gt;", '>'),
                    (b"&quot;", '"'),
                    (b"&apos;", '\''),
                ];
                let Some((reference, resolved)) = references
                    .into_iter()
                    .find(|(reference, _)| content.starts_with(reference))
                else {
                    return Self::not_plain(row, text_start);
                };
                row.text.push(resolved);
                content = &content[reference.len()..];
            };
            if decided {
                row.text.truncate(start);
                continue;
            }
            match child {
                Child::V => {
                    cell.body = Body::Value(start..row.text.len());
                    decided = true;
                }
                Child::Is if row.text.len() > start => {
                    cell.body = Body::Inline(start..row.text.len());
                    decided = true;
                }
                Child::F => cell.formula = Some(start..row.text.len()),
                Child::Is | Child::Other => {}
            }
        }
        if !decided {
            cell.formula = None;
        }
        let used = input.len() - rest.len();
        row.cells.push(cell);
        self.xml.get_mut().consume(used);
        Ok(true)
    }

    fn not_plain(row: &mut RawRow, text_start: usize) -> Result<bool> {
        row.text.truncate(text_start);
        Ok(false)
    }

    /// Reads the children of an open cell up to its end tag.
    fn read_cell(&mut self, buf: &mut Vec<u8>, row: &mut RawRow, mut cell: RawCell) -> Result<()> {
        let mut decided = false;
        loop {
            buf.clear();
            let (child, empty) = match self.read_event(buf)? {
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
            match self.read_event(buf)? {
                Event::Start(_) => depth += 1,
                Event::End(_) if depth == 0 => return Ok(()),
                Event::End(_) => depth -= 1,
                Event::Text(t) => out.push_str(&t.xml10_content()),
                Event::CData(t) => out.push_str(&t.xml10_content()),
                Event::GeneralRef(r) => push_reference(&r, out)?,
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
            match self.read_event(buf)? {
                Event::Start(e) => {
                    depth += 1;
                    if e.local_name().as_ref() == "t" {
                        if t_depth.is_some() {
                            // roo joins the text of every `t`, the nested one twice.
                            return Err(Unsupported("a `t` inside a `t`".into()));
                        }
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
                Event::GeneralRef(r) if t_depth.is_some() => push_reference(&r, out)?,
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
            match self.read_event(buf)? {
                Event::Start(_) => depth += 1,
                Event::End(_) if depth == 0 => return Ok(()),
                Event::End(_) => depth -= 1,
                Event::Eof => return Err(Unsupported("unexpected end of element".into())),
                _ => {}
            }
        }
    }

    fn cell_attributes(attributes: &TagAttributes, raw: &str, mode: Mode) -> Result<RawCell> {
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

#[cfg(test)]
mod tests;
