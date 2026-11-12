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
//! reader does not reproduce, so each event passes through [`SheetReader`]'s
//! XML checks. Anything it rejects is left to roo.

use std::collections::HashMap;
use std::io::{self, BufRead, Read};
use std::sync::atomic::{AtomicBool, Ordering};

use magnus::{prelude::*, Error, RArray, RHash, RString, Ruby, Value};
use quick_xml::events::{BytesStart, Event};
use quick_xml::XmlVersion;

use crate::archive::{Input, Source, ZipEntry};
use crate::reader::{parse_reference, Mode, SheetReader, Unsupported};
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
    xml: SheetReader<R>,
    buf: Vec<u8>,
    depth: usize,
    root_is_worksheet: bool,
    section: Section,
    pub hyperlinks: Vec<Hyperlink>,
    /// Each `mergeCell`'s `ref`.
    pub merge_cells: Vec<Option<String>>,
    pub relationships: HashMap<Option<String>, (Option<String>, Option<String>)>,
    root_is_relationships: bool,
}

impl<R: BufRead> SheetPartsReader<R> {
    pub fn new(input: R) -> Self {
        Self {
            xml: SheetReader::for_roo(input, Mode::Document),
            buf: Vec::new(),
            depth: 0,
            root_is_worksheet: false,
            section: Section::Other,
            hyperlinks: Vec::new(),
            merge_cells: Vec::new(),
            relationships: HashMap::new(),
            root_is_relationships: false,
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
        match self.xml.next_xml_event(buf)? {
            Event::Start(e) => {
                self.depth += 1;
                let (depth, root_is_worksheet, section) =
                    (self.depth, self.root_is_worksheet, self.section);
                match (depth, e.local_name().as_ref()) {
                    (1, name) => {
                        self.root_is_worksheet = name == "worksheet";
                        self.root_is_relationships = name == "Relationships";
                    }
                    (2, "Relationship") if self.root_is_relationships => self.relationship(&e)?,
                    (2, "hyperlinks") if root_is_worksheet => self.section = Section::Hyperlinks,
                    (2, "mergeCells") if root_is_worksheet => self.section = Section::MergeCells,
                    (3, _) if section != Section::Other => self.take(&e)?,
                    _ => {}
                }
            }
            Event::Empty(e) if self.depth == 2 && self.section != Section::Other => {
                self.take(&e)?;
            }
            Event::Empty(e) if self.depth == 1 && self.root_is_relationships => {
                if e.local_name().as_ref() == "Relationship" {
                    self.relationship(&e)?;
                }
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

    fn relationship(&mut self, e: &BytesStart<'_>) -> Result<()> {
        let [id, kind, target] = attributes(e, ["Id", "Type", "Target"])?;
        self.relationships.insert(id, (kind, target));
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

/// Reads and validates worksheet or relationship parts with the GVL released.
fn read(ruby: &Ruby, source: &Source) -> std::result::Result<SheetPartsReader<Input>, Error> {
    let mut parts = SheetPartsReader::new(source.input(ruby)?);
    while !without_gvl(|interrupted| until(interrupted, || parts.next_event()))?
        .map_err(|e| unsupported(ruby, e))?
    {}
    Ok(parts)
}

/// `Native.sheet_parts(source, targets)`, for a worksheet's path or
/// `Native::ZipEntry`: `[hyperlinks, merge_cells]`, with resolved targets
/// keyed by roo coordinates and merged references in document order. Raises
/// `Roo::Calamine::UnsupportedError` for a sheet roo reads differently.
fn sheet_parts(
    ruby: &Ruby,
    source: Source,
    targets: RHash,
) -> std::result::Result<(RHash, Vec<Option<String>>), Error> {
    let parts = read(ruby, &source)?;
    let hyperlinks = ruby.hash_new();
    let coordinate_class: magnus::RClass = ruby.eval("Roo::Excelx::Coordinate")?;
    let utils: Value = ruby.eval("Roo::Utils")?;
    for (reference, id, location) in parts.hyperlinks {
        let Some(mut target) = targets.get(id) else {
            continue;
        };
        if let Some(location) = location {
            target = target.funcall("+", (ruby.str_new(&format!("#{location}")),))?;
        } else {
            target = target.funcall("dup", ())?;
        }
        match reference.as_deref().and_then(parse_reference) {
            Some((row, column)) => {
                let coordinate = RArray::from_value(coordinate_class.obj_alloc()?)
                    .expect("Coordinate is an Array");
                coordinate.push(row)?;
                coordinate.push(column)?;
                coordinate.freeze();
                hyperlinks.aset(coordinate, target)?;
            }
            None => {
                let coordinates: Value =
                    utils.funcall("coordinates_in_range", (reference.unwrap_or_default(),))?;
                let coordinates: RArray = coordinates.funcall("to_a", ())?;
                for coordinate in coordinates.into_iter() {
                    hyperlinks.aset(coordinate, target)?;
                }
            }
        }
    }
    Ok((hyperlinks, parts.merge_cells))
}

pub fn define(native: magnus::RModule) -> std::result::Result<(), Error> {
    native.define_singleton_method("sheet_parts", magnus::function!(sheet_parts, 2))?;
    native.define_singleton_method("metadata_bytes", magnus::function!(metadata_bytes, 1))?;
    native.define_singleton_method("hyperlink_targets", magnus::function!(hyperlink_targets, 1))?;
    Ok(())
}

fn hyperlink_targets(ruby: &Ruby, source: Source) -> std::result::Result<Option<RHash>, Error> {
    let parts = read(ruby, &source)?;
    if !parts.relationships.values().any(|(kind, _)| {
        kind.as_deref()
            .is_some_and(|kind| kind.contains("hyperlink"))
    }) {
        return Ok(None);
    }
    let targets = ruby.hash_new();
    for (id, (_, target)) in parts.relationships {
        targets.aset(id, target)?;
    }
    Ok(Some(targets))
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
