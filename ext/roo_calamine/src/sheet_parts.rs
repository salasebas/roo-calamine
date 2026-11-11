//! The parts of a worksheet besides its cells that roo reads from the whole
//! document, read in one pass without keeping the rest:
//!
//! * `SheetDoc#extract_hyperlinks` selects `/worksheet/hyperlinks/hyperlink`
//!   and reads its `ref`, `id` (written `r:id`), and `location`.
//! * `SheetDoc#expand_merged_ranges` selects
//!   `/worksheet/mergeCells/mergeCell` and reads its `ref`.
//!
//! roo removes the document's namespaces first, so names compare by local
//! name. Nokogiri recovers from XML that is not well-formed into a tree this
//! reader does not reproduce, so a [`SheetReader`] checks on another thread
//! that Nokogiri reads the sheet as quick-xml does; anything it rejects is
//! left to roo.

use std::io::{self, BufRead, Read};
use std::panic::resume_unwind;
use std::sync::atomic::{AtomicBool, Ordering};

use magnus::{prelude::*, Error, RString, Ruby};
use quick_xml::events::{BytesStart, Event};
use quick_xml::XmlVersion;

use crate::archive::{Input, Source, ZipEntry};
use crate::reader::{Mode, Unsupported};
use crate::{unsupported, without_gvl};

type Result<T> = std::result::Result<T, Unsupported>;

/// A `hyperlink` element's `ref`, `id`, and `location`, `None` for one it
/// does not have.
pub type Hyperlink = (Option<String>, Option<String>, Option<String>);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Hyperlinks,
    MergeCells,
    Other,
}

pub struct SheetPartsReader<R: BufRead> {
    xml: quick_xml::Reader<R>,
    buf: Vec<u8>,
    depth: usize,
    root_is_worksheet: bool,
    section: Section,
    pub hyperlinks: Vec<Hyperlink>,
    /// Each `mergeCell`'s `ref`.
    pub merge_cells: Vec<Option<String>>,
}

impl<R: BufRead> SheetPartsReader<R> {
    pub fn new(input: R) -> Self {
        Self {
            xml: quick_xml::Reader::from_reader(input),
            buf: Vec::new(),
            depth: 0,
            root_is_worksheet: false,
            section: Section::Other,
            hyperlinks: Vec::new(),
            merge_cells: Vec::new(),
        }
    }

    /// Reads the next event; `false` at the end of the document.
    pub fn next_event(&mut self) -> Result<bool> {
        let mut buf = std::mem::take(&mut self.buf);
        buf.clear();
        let more = self.read_event(&mut buf);
        self.buf = buf;
        more
    }

    fn read_event(&mut self, buf: &mut Vec<u8>) -> Result<bool> {
        match self.xml.read_event_into(buf)? {
            Event::Start(e) => {
                self.depth += 1;
                let (depth, root_is_worksheet, section) =
                    (self.depth, self.root_is_worksheet, self.section);
                match (depth, e.local_name().as_ref()) {
                    (1, name) => self.root_is_worksheet = name == "worksheet",
                    (2, "hyperlinks") if root_is_worksheet => self.section = Section::Hyperlinks,
                    (2, "mergeCells") if root_is_worksheet => self.section = Section::MergeCells,
                    (3, _) if section != Section::Other => self.take(&e)?,
                    _ => {}
                }
            }
            Event::Empty(e) if self.depth == 2 && self.section != Section::Other => {
                self.take(&e)?;
            }
            Event::End(_) => {
                if self.depth == 2 {
                    self.section = Section::Other;
                }
                self.depth -= 1;
            }
            Event::Eof => return Ok(false),
            _ => {}
        }
        Ok(true)
    }

    /// Keeps `e`, a child of the open section, when it is a part.
    fn take(&mut self, e: &BytesStart<'_>) -> Result<()> {
        match (self.section, e.local_name().as_ref()) {
            (Section::Hyperlinks, "hyperlink") => {
                let [reference, id, location] = attributes(e, ["ref", "id", "location"])?;
                self.hyperlinks.push((reference, id, location));
            }
            (Section::MergeCells, "mergeCell") => {
                let [reference] = attributes(e, ["ref"])?;
                self.merge_cells.push(reference);
            }
            _ => {}
        }
        Ok(())
    }
}

/// The values Nokogiri's `Node#[]` returns for `names` once the namespaces
/// are removed: the first attribute with that local name, whatever its
/// prefix.
fn attributes<const N: usize>(e: &BytesStart<'_>, names: [&str; N]) -> Result<[Option<String>; N]> {
    let mut values = [const { None }; N];
    for attribute in e.attributes() {
        let attribute = attribute?;
        let key = attribute.key;
        if key.as_namespace_binding().is_some() {
            continue;
        }
        let Some(at) = names
            .iter()
            .position(|name| *name == key.local_name().as_ref())
        else {
            continue;
        };
        if values[at].is_none() {
            let value = attribute.normalized_value(XmlVersion::Implicit1_0)?;
            values[at] = Some(value.into_owned());
        }
    }
    Ok(values)
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

/// Reads the parts of the worksheet `source` with the GVL released, while
/// another thread checks that Nokogiri reads the sheet the same way.
fn read(ruby: &Ruby, source: &Source) -> std::result::Result<SheetPartsReader<Input>, Error> {
    let mut check = source.open(ruby, Mode::Document)?;
    let mut parts = SheetPartsReader::new(source.input(ruby)?);
    let (mut checked, mut read) = (false, false);
    while !(checked && read) {
        let (checking, reading) = without_gvl(|interrupted| {
            std::thread::scope(|scope| {
                let checking = if checked {
                    None
                } else {
                    let thread = std::thread::Builder::new()
                        .name("roo-calamine".into())
                        .spawn_scoped(scope, || {
                            until(interrupted, || Ok(check.next_row()?.is_some()))
                        })
                        .map_err(|e| Unsupported(e.to_string()))?;
                    Some(thread)
                };
                let reading = read || until(interrupted, || parts.next_event())?;
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
    Ok(parts)
}

/// `Native.sheet_parts(source)`, for a worksheet's path or `Native::ZipEntry`:
/// `[hyperlinks, merge_cells]`, where each
/// hyperlink is `[ref, id, location]` and each merged range its `ref`, in
/// document order, with `nil` for a missing attribute. Raises
/// `Roo::Calamine::UnsupportedError` for a sheet roo reads differently.
fn sheet_parts(
    ruby: &Ruby,
    source: Source,
) -> std::result::Result<(Vec<Hyperlink>, Vec<Option<String>>), Error> {
    let parts = read(ruby, &source)?;
    Ok((parts.hyperlinks, parts.merge_cells))
}

pub fn define(native: magnus::RModule) -> std::result::Result<(), Error> {
    native.define_singleton_method("sheet_parts", magnus::function!(sheet_parts, 1))?;
    native.define_singleton_method("metadata_bytes", magnus::function!(metadata_bytes, 1))?;
    Ok(())
}

/// Inflates a workbook metadata entry without an intermediate file, using
/// the same local-header and size checks as native extraction.
fn metadata_bytes(ruby: &Ruby, entry: &ZipEntry) -> std::result::Result<RString, Error> {
    let entry = entry.clone();
    let bytes = without_gvl(move |interrupted| -> io::Result<Vec<u8>> {
        let mut input = entry.inflated()?;
        let mut bytes = Vec::new();
        let mut chunk = [0; 8192];
        loop {
            if interrupted.load(Ordering::Acquire) {
                return Err(io::Error::from(io::ErrorKind::Interrupted));
            }
            let read = input.read(&mut chunk)?;
            if read == 0 {
                return Ok(bytes);
            }
            bytes.extend_from_slice(&chunk[..read]);
        }
    })?
    .map_err(|error| Error::new(ruby.exception_io_error(), error.to_string()))?;
    Ok(ruby.str_from_slice(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(xml: &str) -> (Vec<Hyperlink>, Vec<Option<String>>) {
        let mut reader = SheetPartsReader::new(xml.as_bytes());
        while reader.next_event().unwrap() {}
        (reader.hyperlinks, reader.merge_cells)
    }

    fn some(text: &str) -> Option<String> {
        Some(text.to_owned())
    }

    #[test]
    fn reads_hyperlinks_and_merged_ranges_under_the_worksheet() {
        let (hyperlinks, merges) = parts(
            r#"<x:worksheet xmlns:x="s" xmlns:r="r"><x:sheetData><row><c r="A1"/></row></x:sheetData>
            <x:mergeCells><x:mergeCell ref="A1:B2"/><mergeCell/><other ref="C1"/></x:mergeCells>
            <hyperlinks><hyperlink ref="A1" r:id="rId1"><hyperlink ref="B9"/></hyperlink>
            <hyperlink location="a&amp;b&#10;c&#x9;d	e" ref="C3" id="first" r:id="second"/></hyperlinks>
            <sheetData><hyperlinks><hyperlink ref="Z9"/></hyperlinks></sheetData></x:worksheet>"#,
        );
        assert_eq!(
            hyperlinks,
            [
                (some("A1"), some("rId1"), None),
                (some("C3"), some("first"), some("a&b\nc\td e")),
            ]
        );
        assert_eq!(merges, [some("A1:B2"), None]);
    }

    #[test]
    fn reads_nothing_under_another_root() {
        let (hyperlinks, merges) = parts(
            r#"<chartsheet><hyperlinks><hyperlink ref="A1"/></hyperlinks>
            <mergeCells><mergeCell ref="A1:B2"/></mergeCells></chartsheet>"#,
        );
        assert!(hyperlinks.is_empty() && merges.is_empty());
    }
}
