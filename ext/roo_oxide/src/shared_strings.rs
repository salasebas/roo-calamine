//! Reads `sharedStrings.xml` into the two arrays roo 4.0.0's
//! `Roo::Excelx::SharedStrings` builds from it: `#to_a`, the plain text of
//! each `/sst/si`, and `#to_html`, the same text with rich-text runs wrapped
//! in HTML tags.
//!
//! roo reads the file with Nokogiri and removes its namespaces, so elements
//! match by local name. [`SheetReader`] first checks that Nokogiri reads the
//! file as quick-xml does; anything it rejects is left to roo.

use std::cell::RefCell;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::panic::resume_unwind;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use magnus::gc::Marker;
use magnus::value::{Opaque, ReprValue};
use magnus::{prelude::*, DataTypeFunctions, Error, RArray, RString, Ruby, TypedData, Value};
use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::XmlVersion;

use crate::reader::{Mode, SheetReader, Unsupported};
use crate::{unsupported, without_gvl};

type Result<T> = std::result::Result<T, Unsupported>;

/// The text `SharedStrings#fix_invalid_shared_strings` replaces with a line
/// feed in the document behind `#to_a`, but not in the one behind `#to_html`.
const CARRIAGE_RETURN: &str = "_x000D_";

/// A list of strings kept in one buffer.
#[derive(Default)]
struct Strings {
    text: String,
    ends: Vec<u32>,
}

impl Strings {
    fn push(&mut self, string: &str) -> Result<()> {
        let end = string_end(self.text.len(), string.len())?;
        self.text.push_str(string);
        self.ends.push(end);
        Ok(())
    }

    fn len(&self) -> usize {
        self.ends.len()
    }

    fn iter(&self) -> impl Iterator<Item = &str> {
        let starts = std::iter::once(0).chain(self.ends.iter().copied());
        starts
            .zip(self.ends.iter().copied())
            .map(|(start, end)| &self.text[start as usize..end as usize])
    }

    fn get(&self, index: usize) -> &str {
        let start = index.checked_sub(1).map_or(0, |i| self.ends[i]);
        &self.text[start as usize..self.ends[index] as usize]
    }

    fn into_array(self, ruby: &Ruby) -> std::result::Result<RArray, Error> {
        let out = ruby.ary_new_capa(self.len());
        for string in self.iter() {
            out.push(ruby.str_new(string))?;
        }
        Ok(out)
    }
}

fn string_end(start: usize, len: usize) -> Result<u32> {
    start
        .checked_add(len)
        .and_then(|end| u32::try_from(end).ok())
        .ok_or_else(|| Unsupported("shared strings exceed 32-bit offsets".into()))
}

/// `string[/<([biu]|sup|sub)>/]`, the test in `SharedStrings#use_html?`.
pub(crate) fn has_html_tag(string: &[u8]) -> bool {
    string.windows(3).enumerate().any(|(i, w)| {
        w[0] == b'<'
            && (matches!(w, [_, b'b' | b'i' | b'u', b'>'])
                || string[i..].starts_with(b"<sup>")
                || string[i..].starts_with(b"<sub>"))
    })
}

pub(crate) enum HtmlSelection {
    Plain,
    Html(Value),
    Invalid,
}

#[derive(TypedData)]
#[magnus(
    class = "Roo::Oxide::Native::HtmlStrings",
    mark,
    free_immediately,
    size
)]
pub(crate) struct HtmlStrings {
    strings: RefCell<Option<Strings>>,
    cached: Opaque<RArray>,
}

impl DataTypeFunctions for HtmlStrings {
    fn mark(&self, marker: &Marker) {
        marker.mark(self.cached);
    }

    fn size(&self) -> usize {
        size_of::<Self>()
            + self.strings.borrow().as_ref().map_or(0, |strings| {
                strings.text.capacity() + strings.ends.capacity() * size_of::<u32>()
            })
    }
}

impl HtmlStrings {
    pub(crate) fn is_plain(&self, len: usize) -> bool {
        self.strings.borrow().as_ref().is_some_and(|strings| {
            strings.len() >= len
                && strings
                    .iter()
                    .all(|string| !has_html_tag(string.as_bytes()))
        })
    }

    pub(crate) fn select(
        &self,
        ruby: &Ruby,
        index: usize,
    ) -> std::result::Result<HtmlSelection, Error> {
        let cached = ruby.get_inner(self.cached);
        if index < cached.len() {
            let value: Value = cached.entry(index as isize)?;
            if !value.is_nil() || self.strings.borrow().is_none() {
                return Ok(match RString::from_value(value) {
                    Some(string) if has_html_tag(unsafe { string.as_slice() }) => {
                        HtmlSelection::Html(value)
                    }
                    Some(_) => HtmlSelection::Plain,
                    None => HtmlSelection::Invalid,
                });
            }
        }
        {
            let strings = self.strings.borrow();
            let Some(strings) = strings.as_ref().filter(|strings| index < strings.len()) else {
                return Ok(HtmlSelection::Invalid);
            };
            if !has_html_tag(strings.get(index).as_bytes()) {
                return Ok(HtmlSelection::Plain);
            }
        }
        self.materialize(ruby, index).map(HtmlSelection::Html)
    }

    fn materialize(&self, ruby: &Ruby, index: usize) -> std::result::Result<Value, Error> {
        let cached = ruby.get_inner(self.cached);
        let value: Value = cached.entry(index as isize)?;
        if !value.is_nil() {
            return Ok(value);
        }
        let html = {
            let strings = self.strings.borrow();
            format!(
                "<html>{}</html>",
                strings.as_ref().expect("not materialized").get(index)
            )
        };
        let string = ruby.str_new(&html);
        cached.store(index as isize, string)?;
        Ok(string.as_value())
    }

    fn to_a(ruby: &Ruby, rb_self: &Self) -> std::result::Result<RArray, Error> {
        let len = rb_self.strings.borrow().as_ref().map(Strings::len);
        if let Some(len) = len {
            for index in 0..len {
                rb_self.materialize(ruby, index)?;
            }
            rb_self.strings.borrow_mut().take();
        }
        Ok(ruby.get_inner(rb_self.cached))
    }
}

/// HTML tags of a rich-text run, in the order roo nests them.
#[derive(Clone, Copy, Default)]
struct Format {
    sub: bool,
    sup: bool,
    b: bool,
    i: bool,
    u: bool,
}

impl Format {
    fn tags(&self) -> impl DoubleEndedIterator<Item = &'static str> {
        [
            (self.sub, "sub"),
            (self.sup, "sup"),
            (self.b, "b"),
            (self.i, "i"),
            (self.u, "u"),
        ]
        .into_iter()
        .filter_map(|(on, tag)| on.then_some(tag))
    }

    /// `SharedStrings#create_html`.
    fn wrap(&self, text: &str, out: &mut String) {
        for tag in self.tags() {
            out.push('<');
            out.push_str(tag);
            out.push('>');
        }
        out.push_str(text);
        for tag in self.tags().rev() {
            out.push_str("</");
            out.push_str(tag);
            out.push('>');
        }
    }
}

struct SharedStringsReader<R: BufRead> {
    xml: quick_xml::Reader<R>,
    buf: Vec<u8>,
    /// Whether to build `#to_html` too.
    html: bool,
    depth: usize,
    in_sst: bool,
    plain: Strings,
    html_strings: Strings,
    item_plain: String,
    item_html: String,
    /// The content of the `t` being read.
    content: String,
    /// Where each text or CDATA node of `content` after the first starts:
    /// Nokogiri keeps text and CDATA in separate nodes.
    node_starts: Vec<usize>,
}

impl SharedStringsReader<BufReader<File>> {
    fn open(path: &Path, html: bool) -> Result<Self> {
        let file = File::open(path).map_err(|e| Unsupported(e.to_string()))?;
        Ok(Self::new(BufReader::with_capacity(64 * 1024, file), html))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Name {
    T,
    R,
    RPr,
    B,
    I,
    U,
    VertAlign,
    Other,
}

fn name(e: &BytesStart<'_>) -> Name {
    match e.local_name().as_ref() {
        "t" => Name::T,
        "r" => Name::R,
        "rPr" => Name::RPr,
        "b" => Name::B,
        "i" => Name::I,
        "u" => Name::U,
        "vertAlign" => Name::VertAlign,
        _ => Name::Other,
    }
}

impl<R: BufRead> SharedStringsReader<R> {
    fn new(input: R, html: bool) -> Self {
        Self {
            xml: quick_xml::Reader::from_reader(input),
            buf: Vec::with_capacity(16 * 1024),
            html,
            depth: 0,
            in_sst: false,
            plain: Strings::default(),
            html_strings: Strings::default(),
            item_plain: String::new(),
            item_html: String::new(),
            content: String::new(),
            node_starts: Vec::new(),
        }
    }

    /// Reads up to the end of the next `/sst/si`. `false` at the end of the
    /// document.
    fn next_item(&mut self) -> Result<bool> {
        let mut buf = std::mem::take(&mut self.buf);
        let read = self.find_item(&mut buf);
        self.buf = buf;
        read
    }

    fn find_item(&mut self, buf: &mut Vec<u8>) -> Result<bool> {
        loop {
            buf.clear();
            match self.read_event(buf)? {
                Event::Start(e) => {
                    self.depth += 1;
                    match self.depth {
                        1 => self.in_sst = e.local_name().as_ref() == "sst",
                        2 if self.in_sst && e.local_name().as_ref() == "si" => {
                            self.read_si(buf)?;
                            self.depth -= 1;
                            return Ok(true);
                        }
                        _ => {
                            self.skip(buf)?;
                            self.depth -= 1;
                        }
                    }
                }
                Event::Empty(e) => {
                    if self.depth == 1 && self.in_sst && e.local_name().as_ref() == "si" {
                        self.plain.push("")?;
                        if self.html {
                            self.html_strings.push("")?;
                        }
                        return Ok(true);
                    }
                }
                Event::End(_) => self.depth = self.depth.saturating_sub(1),
                Event::Eof => return Ok(false),
                _ => {}
            }
        }
    }

    /// `SharedStrings#extract_shared_strings` and `#extract_html` for one
    /// open `si`.
    fn read_si(&mut self, buf: &mut Vec<u8>) -> Result<()> {
        self.item_plain.clear();
        self.item_html.clear();
        loop {
            buf.clear();
            let (child, empty) = match self.read_event(buf)? {
                Event::Start(e) => (name(&e), false),
                Event::Empty(e) => (name(&e), true),
                Event::End(_) => break,
                Event::Eof => return Err(Unsupported("unexpected end of a shared string".into())),
                _ => continue,
            };
            match (child, empty) {
                (Name::T, false) => {
                    self.read_t(buf)?;
                    self.item_plain.clear();
                    self.push_fixed_content();
                    if self.html {
                        self.item_html.push_str(&self.content);
                    }
                }
                (Name::T, true) => self.item_plain.clear(),
                (Name::R, false) => self.read_run(buf)?,
                (_, false) => self.skip(buf)?,
                (_, true) => {}
            }
        }
        self.plain.push(&self.item_plain)?;
        if self.html {
            self.html_strings.push(&self.item_html)?;
        }
        Ok(())
    }

    /// `SharedStrings#extract_html_r` for one open `r`, which also adds the
    /// text of its `t` children to the plain string.
    fn read_run(&mut self, buf: &mut Vec<u8>) -> Result<()> {
        let mut format = Format::default();
        loop {
            buf.clear();
            let (child, empty) = match self.read_event(buf)? {
                Event::Start(e) => (name(&e), false),
                Event::Empty(e) => (name(&e), true),
                Event::End(_) => return Ok(()),
                Event::Eof => return Err(Unsupported("unexpected end of a run".into())),
                _ => continue,
            };
            match (child, empty) {
                (Name::T, _) => {
                    if empty {
                        self.content.clear();
                        self.node_starts.clear();
                    } else {
                        self.read_t(buf)?;
                    }
                    self.push_fixed_content();
                    if self.html {
                        format.wrap(&self.content, &mut self.item_html);
                    }
                }
                (Name::RPr, false) => self.read_run_properties(buf, &mut format)?,
                (_, false) => self.skip(buf)?,
                (_, true) => {}
            }
        }
    }

    /// The children of an open `rPr` that turn on an HTML tag.
    fn read_run_properties(&mut self, buf: &mut Vec<u8>, format: &mut Format) -> Result<()> {
        loop {
            buf.clear();
            match self.read_event(buf)? {
                Event::Start(e) => {
                    self.run_property(&e, format)?;
                    self.skip(buf)?;
                }
                Event::Empty(e) => self.run_property(&e, format)?,
                Event::End(_) => return Ok(()),
                Event::Eof => return Err(Unsupported("unexpected end of run properties".into())),
                _ => {}
            }
        }
    }

    fn run_property(&self, e: &BytesStart<'_>, format: &mut Format) -> Result<()> {
        match name(e) {
            Name::B => format.b = true,
            Name::I => format.i = true,
            Name::U => format.u = true,
            Name::VertAlign if self.html => match vert_align(e)?.as_ref() {
                "subscript" => (format.sub, format.sup) = (true, false),
                "superscript" => (format.sub, format.sup) = (false, true),
                _ => {}
            },
            _ => {}
        }
        Ok(())
    }

    /// Nokogiri's `content` of an open `t`, into `self.content`. Any markup
    /// inside it is left to roo: roo's `_x000D_` fix serializes the document
    /// with indentation, which changes the content of such a `t`.
    fn read_t(&mut self, buf: &mut Vec<u8>) -> Result<()> {
        self.content.clear();
        self.node_starts.clear();
        let mut in_cdata = false;
        loop {
            buf.clear();
            match self.read_event(buf)? {
                Event::Text(t) => {
                    if in_cdata {
                        self.node_starts.push(self.content.len());
                        in_cdata = false;
                    }
                    self.content.push_str(&t.xml10_content());
                }
                Event::GeneralRef(r) => {
                    if in_cdata {
                        self.node_starts.push(self.content.len());
                        in_cdata = false;
                    }
                    push_reference(&mut self.content, &r)?;
                }
                Event::CData(t) => {
                    // libxml2 merges adjacent CDATA sections into one node.
                    if !in_cdata && !self.content.is_empty() {
                        self.node_starts.push(self.content.len());
                    }
                    in_cdata = true;
                    self.content.push_str(&t.xml10_content());
                }
                Event::End(_) => return Ok(()),
                Event::Eof => return Err(Unsupported("unexpected end of a `t`".into())),
                _ => return Err(Unsupported("markup inside a shared string's `t`".into())),
            }
        }
    }

    /// Appends `self.content` to the plain string as roo's
    /// `fix_invalid_shared_strings` leaves it: `_x000D_` becomes a line feed
    /// within each text or CDATA node, which the serialized document keeps
    /// apart.
    fn push_fixed_content(&mut self) {
        if !self.content.contains(CARRIAGE_RETURN) {
            self.item_plain.push_str(&self.content);
            return;
        }
        let starts = std::iter::once(0).chain(self.node_starts.iter().copied());
        let ends = self
            .node_starts
            .iter()
            .copied()
            .chain(std::iter::once(self.content.len()));
        for (start, end) in starts.zip(ends) {
            self.item_plain
                .push_str(&self.content[start..end].replace(CARRIAGE_RETURN, "\n"));
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

    fn read_event<'b>(&mut self, buf: &'b mut Vec<u8>) -> Result<Event<'b>> {
        let event = self.xml.read_event_into(buf)?;
        match &event {
            // roo's fix also replaces `_x000D_` in names, which breaks them.
            Event::Start(e) | Event::Empty(e) if e.contains(CARRIAGE_RETURN) => {
                Err(Unsupported(format!("{CARRIAGE_RETURN} in a tag")))
            }
            // Nokogiri names a processing instruction by its target, so roo
            // takes `<?t x?>` for a `t`.
            Event::PI(_) => Err(Unsupported("a processing instruction".into())),
            _ => Ok(event),
        }
    }
}

/// `vertAlign.xpath('@val').first.value`. roo raises without a `val`; a
/// prefixed one is left to roo, which removes namespaces from attributes too.
fn vert_align(e: &BytesStart<'_>) -> Result<String> {
    let mut value = None;
    for attribute in e.attributes() {
        let attribute = attribute?;
        let key = attribute.key;
        if key.local_name().as_ref() != "val" {
            continue;
        }
        if key.prefix().is_some() {
            return Err(Unsupported("a prefixed `val` on a `vertAlign`".into()));
        }
        value = Some(
            attribute
                .normalized_value(XmlVersion::Implicit1_0)?
                .into_owned(),
        );
    }
    value.ok_or_else(|| Unsupported("a `vertAlign` without a `val`".into()))
}

fn push_reference(text: &mut String, reference: &BytesRef<'_>) -> Result<()> {
    if let Some(c) = reference.resolve_char_ref()? {
        text.push(c);
        return Ok(());
    }
    text.push(match reference.xml10_content().as_ref() {
        "lt" => '<',
        "gt" => '>',
        "amp" => '&',
        "apos" => '\'',
        "quot" => '"',
        other => return Err(Unsupported(format!("entity reference &{other};"))),
    });
    Ok(())
}

/// Calls `step` until it returns `false` (`Ok(true)`) or `interrupted`
/// turns true (`Ok(false)`).
fn until(interrupted: &AtomicBool, mut step: impl FnMut() -> Result<bool>) -> Result<bool> {
    while !interrupted.load(Ordering::Acquire) {
        if !step()? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Reads the shared strings at `path` with the GVL released, while another
/// thread checks that Nokogiri reads the file the same way.
fn read(
    ruby: &Ruby,
    path: &Path,
    html: bool,
) -> std::result::Result<SharedStringsReader<BufReader<File>>, Error> {
    let mut check = SheetReader::open(path, Mode::Document).map_err(|e| unsupported(ruby, e))?;
    let mut reader = SharedStringsReader::open(path, html).map_err(|e| unsupported(ruby, e))?;
    let (mut checked, mut read) = (false, false);
    while !(checked && read) {
        let (checking, reading) = without_gvl(|interrupted| {
            std::thread::scope(|scope| {
                let checking = if checked {
                    None
                } else {
                    let thread = std::thread::Builder::new()
                        .name("roo-oxide".into())
                        .spawn_scoped(scope, || {
                            until(interrupted, || Ok(check.next_row()?.is_some()))
                        })
                        .map_err(|e| Unsupported(e.to_string()))?;
                    Some(thread)
                };
                let reading = read || until(interrupted, || reader.next_item())?;
                let checking = match checking {
                    Some(thread) => thread.join().unwrap_or_else(|panic| resume_unwind(panic))?,
                    None => true,
                };
                Ok((checking, reading))
            })
        })?
        .map_err(|e| unsupported(ruby, e))?;
        (checked, read) = (checking, reading);
    }
    Ok(reader)
}

/// `Native.shared_strings(path, html, plain)`: plain strings and deferred HTML
/// for roo's `SharedStrings`, with `nil` for an unrequested table. Raises
/// `Roo::Oxide::UnsupportedError` for a file roo reads differently.
fn shared_strings(
    ruby: &Ruby,
    path: String,
    html: bool,
    plain: bool,
) -> std::result::Result<(Option<RArray>, Option<HtmlStrings>), Error> {
    let build_plain = plain;
    let SharedStringsReader {
        plain,
        html_strings,
        ..
    } = read(ruby, Path::new(&path), html)?;
    let plain = if build_plain {
        Some(plain.into_array(ruby)?)
    } else {
        drop(plain);
        None
    };
    let html = if html {
        Some(HtmlStrings {
            cached: ruby.ary_new().into(),
            strings: RefCell::new(Some(html_strings)),
        })
    } else {
        None
    };
    Ok((plain, html))
}

pub fn define(native: magnus::RModule) -> std::result::Result<(), Error> {
    let ruby = Ruby::get().expect("GVL held");
    let html = native.define_class("HtmlStrings", ruby.class_object())?;
    html.define_method("to_a", magnus::method!(HtmlStrings::to_a, 0))?;
    native.define_singleton_method("shared_strings", magnus::function!(shared_strings, 3))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{has_html_tag, string_end};

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

    #[test]
    fn offsets_reject_overflow_instead_of_wrapping() {
        assert_eq!(string_end(u32::MAX as usize - 1, 1).unwrap(), u32::MAX);
        assert!(string_end(u32::MAX as usize, 1).is_err());
        assert!(string_end(usize::MAX, 1).is_err());
    }
}
