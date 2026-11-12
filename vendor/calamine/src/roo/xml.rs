use super::{
    attributes::TagAttributes, check_chars, check_encoding, check_name, Mode, RawCell, Result,
    Unsupported,
};
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::QName;
use std::io::BufRead;
use std::ops::{Deref, DerefMut};

/// XML events with optional roo/Nokogiri validation, shared by Calamine's XLSX paths.
pub struct XmlReader<R> {
    inner: quick_xml::Reader<R>,
    checks: Option<Checks>,
}
struct Checks {
    mode: Mode,
    started: bool,
    seen_root: bool,
    open_elements: usize,
    prefixes: Vec<(usize, String)>,
    space: Vec<(usize, bool)>,
    in_row: bool,
    attributes: TagAttributes,
}
impl<R> Deref for XmlReader<R> {
    type Target = quick_xml::Reader<R>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl<R> DerefMut for XmlReader<R> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
impl<R: BufRead> XmlReader<R> {
    pub(crate) fn from_reader(input: R) -> Self {
        Self {
            inner: quick_xml::Reader::from_reader(input),
            checks: None,
        }
    }
    pub(crate) fn for_roo(input: R, mode: Mode) -> Self {
        let mut reader = Self::from_reader(input);
        reader.inner.config_mut().check_end_names = true;
        reader.inner.config_mut().check_comments = true;
        reader.checks = Some(Checks {
            mode,
            started: false,
            seen_root: false,
            open_elements: 0,
            prefixes: Vec::new(),
            space: Vec::new(),
            in_row: false,
            attributes: TagAttributes::default(),
        });
        reader
    }
    pub(crate) fn read_event_into<'b>(
        &mut self,
        buf: &'b mut Vec<u8>,
    ) -> quick_xml::Result<Event<'b>> {
        if self.checks.as_ref().is_some_and(|checks| !checks.started) {
            loop {
                match self.inner.get_mut().fill_buf() {
                    Ok(bytes) if matches!(bytes.first(), Some(0xFF | 0xFE)) => {
                        return Err(xml_error(Unsupported("a UTF-16 byte-order mark".into())))
                    }
                    Ok(_) => break,
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error.into()),
                }
            }
        }
        let event = self.inner.read_event_into(buf)?;
        if let Some(checks) = self.checks.as_mut() {
            let depth = checks.open_elements;
            checks.check_event(&event).map_err(xml_error)?;
            if depth == 0 {
                checks.check_top_level(&event).map_err(xml_error)?;
            }
            match &event {
                Event::Decl(decl) => check_encoding(decl).map_err(xml_error)?,
                Event::Eof if depth != 0 => {
                    return Err(xml_error(Unsupported(
                        "unexpected end of the document".into(),
                    )))
                }
                _ => {}
            }
        }
        Ok(event)
    }
    pub(crate) fn read_to_end_into(
        &mut self,
        end: QName<'_>,
        buf: &mut Vec<u8>,
    ) -> quick_xml::Result<std::ops::Range<u64>> {
        if self.checks.is_none() {
            return self.inner.read_to_end_into(end, buf);
        }
        let start = self.inner.buffer_position();
        let mut depth = 0;
        loop {
            buf.clear();
            match self.read_event_into(buf)? {
                Event::Start(_) => depth += 1,
                Event::End(e) if depth == 0 && e.name() == end => {
                    return Ok(start..self.inner.buffer_position())
                }
                Event::End(_) if depth > 0 => depth -= 1,
                Event::Eof => {
                    return Err(xml_error(Unsupported("unexpected end of element".into())))
                }
                _ => {}
            }
        }
    }
    /// Returns one validated XML event without interpreting worksheet cells.
    pub fn next_xml_event<'b>(&mut self, buf: &'b mut Vec<u8>) -> Result<Event<'b>> {
        self.read_event_into(buf).map_err(Into::into)
    }
    pub(crate) fn depth(&self) -> usize {
        self.checks
            .as_ref()
            .map_or(0, |checks| checks.open_elements)
    }
    pub(crate) fn in_row(&mut self, value: bool) {
        if let Some(checks) = self.checks.as_mut() {
            checks.in_row = value;
        }
    }
    pub(crate) fn preserves_space(&self) -> bool {
        self.checks.as_ref().is_some_and(Checks::preserves_space)
    }
    pub(crate) fn cell_attributes(&self, e: &BytesStart<'_>) -> Result<RawCell> {
        let checks = self.checks.as_ref().expect("roo reader");
        super::cell_attributes(&checks.attributes, e.attributes_raw(), checks.mode)
    }
}
fn xml_error(error: Unsupported) -> quick_xml::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, error).into()
}
impl Checks {
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

    fn check_event(&mut self, event: &Event<'_>) -> Result<()> {
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
        Ok(())
    }
}
