//! Keeps a worksheet's raw records and builds the roo cell for one only when
//! it is read, for `Roo::Excelx#cell` and the other single-cell lookups.

use std::ops::Range;
use std::path::Path;
use std::sync::atomic::Ordering;

use magnus::gc::Marker;
use magnus::typed_data::Obj;
use magnus::value::{Opaque, ReprValue};
use magnus::{
    prelude::*, DataTypeFunctions, Error, Integer, RArray, RHash, Ruby, TypedData, Value,
};

use crate::convert::Converter;
use crate::reader::{Body, Kind, Mode, RawCell, RawRow, Reference, SheetReader, Style};
use crate::{unsupported, without_gvl};

type Span = (u32, u32);

fn range((start, end): Span) -> Range<usize> {
    start as usize..end as usize
}

#[derive(Clone, Copy)]
enum Content {
    Empty,
    Inline(Span),
    Value(Span),
}

/// A record roo's `cells` keeps a cell for, as [`RawCell`] holds it but
/// without its heap parts: the coordinate is always known, the style is an
/// index, and spans index [`Store::text`].
#[derive(Clone, Copy)]
struct Record {
    row: u32,
    column: u32,
    style: u32,
    kind: Kind,
    formula: Option<Span>,
    content: Content,
}

impl Record {
    /// `None` for a record that needs a heap part, or Ruby, to read.
    fn new(raw: RawCell, row: i64, column: i64, offset: usize) -> Option<Self> {
        let span = |range: Range<usize>| -> Option<Span> {
            Some((
                u32::try_from(range.start + offset).ok()?,
                u32::try_from(range.end + offset).ok()?,
            ))
        };
        Some(Self {
            row: u32::try_from(row).ok()?,
            column: u32::try_from(column).ok()?,
            style: match raw.style {
                Style::Index(index) => u32::try_from(index).ok()?,
                Style::Other(_) => return None,
            },
            kind: raw.kind,
            formula: match raw.formula {
                Some(formula) => Some(span(formula)?),
                None => None,
            },
            content: match raw.body {
                Body::Empty => Content::Empty,
                Body::Inline(text) => Content::Inline(span(text)?),
                Body::Value(text) => Content::Value(span(text)?),
            },
        })
    }

    fn raw(&self) -> RawCell {
        RawCell {
            reference: Reference::At {
                row: self.row.into(),
                column: self.column.into(),
            },
            style: Style::Index(self.style.into()),
            kind: self.kind,
            formula: self.formula.map(range),
            body: match self.content {
                Content::Empty => Body::Empty,
                Content::Inline(text) => Body::Inline(range(text)),
                Content::Value(text) => Body::Value(range(text)),
            },
        }
    }

    fn key(&self) -> (u32, u32) {
        (self.row, self.column)
    }

    fn built_key(&self) -> u64 {
        u64::from(self.row) << 32 | u64::from(self.column)
    }
}

/// The records of a worksheet read in document mode, the way
/// `Roo::Excelx::SheetDoc#extract_cells` reads it, when every cell can be
/// built on its own: none raises, so roo would not raise for the sheet.
#[derive(TypedData)]
#[magnus(class = "Roo::Calamine::Native::Store", mark, free_immediately, size)]
pub struct Store {
    converter: Opaque<Obj<Converter>>,
    text: String,
    /// In document order.
    records: Vec<Record>,
    /// For each key of roo's `cells` hash, the record whose cell it holds
    /// (the last one with that key), sorted by key.
    index: Vec<u32>,
    /// The cells built so far, by [`key`].
    built: Opaque<RHash>,
    /// The keys more than one record has, sorted.
    repeated: Vec<u64>,
}

impl DataTypeFunctions for Store {
    fn mark(&self, marker: &Marker) {
        marker.mark(self.converter);
        marker.mark(self.built);
    }
}

struct Records {
    text: String,
    records: Vec<Record>,
}

impl Records {
    /// `None` once a record does not fit in a [`Record`].
    fn add(&mut self, row: RawRow, row_number: usize, empty_cell: bool) -> Option<()> {
        let offset = self.text.len();
        self.text.push_str(&row.text);
        for (index, raw) in row.cells.into_iter().enumerate() {
            let (row, column) = raw.position(row_number, index + 1)?;
            if raw.body != Body::Empty || empty_cell {
                self.records.push(Record::new(raw, row, column, offset)?);
            }
        }
        Some(())
    }
}

/// The key [`Store::built`] holds a cell under: `row << 32 | column`.
pub fn key(row: i64, column: i64) -> Option<u64> {
    Some(u64::from(u32::try_from(row).ok()?) << 32 | u64::from(u32::try_from(column).ok()?))
}

/// Reads every row with the GVL released. `None` when a record does not fit
/// in a [`Record`].
fn read(ruby: &Ruby, path: &str, empty_cell: bool) -> Result<Option<Records>, Error> {
    let mut sheet =
        SheetReader::open(Path::new(path), Mode::Document).map_err(|e| unsupported(ruby, e))?;
    let mut records = Records {
        text: String::new(),
        records: Vec::new(),
    };
    let mut rows = 0;
    loop {
        let read = without_gvl(|interrupted| {
            while !interrupted.load(Ordering::Acquire) {
                let Some(row) = sheet.next_row()? else {
                    return Ok(Some(true));
                };
                rows += 1;
                if records.add(row, rows, empty_cell).is_none() {
                    return Ok(None);
                }
            }
            Ok(Some(false))
        })?;
        match read.map_err(|e| unsupported(ruby, e))? {
            Some(true) => break,
            Some(false) => continue,
            None => return Ok(None),
        }
    }
    records.text.shrink_to_fit();
    records.records.shrink_to_fit();
    Ok(Some(records))
}

impl Store {
    /// The store for the worksheet at `path`, or `nil` when its cells have to
    /// be built all at once, in order. `context` is a `Roo::Calamine::Context`.
    fn read(ruby: &Ruby, path: String, context: Value) -> Result<Option<Self>, Error> {
        let converter = ruby.obj_wrap(Converter::new(ruby, context, false)?);
        let Some(Records { text, records }) = read(ruby, &path, converter.empty_cell())? else {
            return Ok(None);
        };
        if !records
            .iter()
            .all(|record| converter.builds_safely(ruby, &text, &record.raw()))
        {
            return Ok(None);
        }
        let Ok(count) = u32::try_from(records.len()) else {
            return Ok(None);
        };
        let mut index: Vec<u32> = (0..count).collect();
        let mut repeated = Vec::new();
        // Stable, so records with the same key stay in document order, and the
        // last one is kept, as a Hash keeps the last value stored for a key.
        index.sort_by_key(|&i| records[i as usize].key());
        index.dedup_by(|later, kept| {
            let same = records[*later as usize].key() == records[*kept as usize].key();
            if same {
                let key = records[*kept as usize].built_key();
                if repeated.last() != Some(&key) {
                    repeated.push(key);
                }
                *kept = *later;
            }
            same
        });
        index.shrink_to_fit();
        let store = Self {
            converter: converter.into(),
            text,
            records,
            index,
            built: ruby.hash_new().into(),
            repeated,
        };
        // `Roo::Link.new` copies a linked cell's shared string, which roo does
        // before a caller can change that string in place.
        for &i in &store.index {
            let record = &store.records[i as usize];
            let shared = record.kind == Kind::Shared && matches!(record.content, Content::Value(_));
            if shared && converter.has_link(ruby, record.row.into(), record.column.into()) {
                store.build(ruby, i)?;
            }
        }
        Ok(Some(store))
    }

    fn build(&self, ruby: &Ruby, record: u32) -> Result<Value, Error> {
        let built = ruby.get_inner(self.built);
        let record = &self.records[record as usize];
        let key = record.built_key();
        if let Some(cell) = built.get(key) {
            return Ok(cell);
        }
        let (_, cell) = ruby
            .get_inner(self.converter)
            .cell(ruby, &self.text, &record.raw(), 0, 0)?
            .expect("the store holds only records roo keeps");
        built.aset(key, cell)?;
        Ok(cell)
    }

    /// The cell built for `key`, for `Reader#fill_cells` to put in `cells`
    /// in place of a new one. roo's `cells` keeps the key of the first record
    /// with a repeated coordinate, so that record gets a cell of its own.
    pub fn reused(&self, ruby: &Ruby, cells: RHash, key: u64) -> Result<Option<Value>, Error> {
        let Some(cell) = ruby.get_inner(self.built).get(key) else {
            return Ok(None);
        };
        if self.repeated.binary_search(&key).is_ok() {
            let coordinate: Value = cell.funcall("coordinate", ())?;
            if cells.get(coordinate).is_none() {
                return Ok(None);
            }
        }
        Ok(Some(cell))
    }

    /// The converter that built them, for `Reader#fill_cells` to build the
    /// rest with the same shared strings.
    fn converter(ruby: &Ruby, rb_self: &Self) -> Obj<Converter> {
        ruby.get_inner(rb_self.converter)
    }

    /// `cells[key]`.
    fn get(ruby: &Ruby, rb_self: &Self, key: Value) -> Result<Option<Value>, Error> {
        let Some(key) = RArray::from_value(key).filter(|key| key.len() == 2) else {
            return Ok(None);
        };
        let part = |i| {
            key.entry::<Value>(i)
                .ok()
                .and_then(Integer::from_value)
                .and_then(|n| n.to_u32().ok())
        };
        let (Some(row), Some(column)) = (part(0), part(1)) else {
            return Ok(None);
        };
        let found = rb_self
            .index
            .binary_search_by_key(&(row, column), |&i| rb_self.records[i as usize].key());
        match found {
            Ok(i) => rb_self.build(ruby, rb_self.index[i]).map(Some),
            Err(_) => Ok(None),
        }
    }

    /// What `Roo::Excelx::Sheet#first_last_row_col` computes from `cells`.
    fn bounds(ruby: &Ruby, rb_self: &Self) -> Result<RHash, Error> {
        let presence = |cell: Value| -> Result<bool, Error> {
            Ok(cell.funcall::<_, _, Value>("presence", ())?.to_bool())
        };
        let built = ruby.get_inner(rb_self.built);
        let mut bounds: Option<(u32, u32, u32, u32)> = None;
        for &i in &rb_self.index {
            let record = &rb_self.records[i as usize];
            // A cell already handed out may have been changed in place.
            let present = match built.get(record.built_key()) {
                Some(cell) => presence(cell)?,
                None => match ruby.get_inner(rb_self.converter).presence(
                    ruby,
                    &rb_self.text,
                    &record.raw(),
                )? {
                    Some(present) => present,
                    None => presence(rb_self.build(ruby, i)?)?,
                },
            };
            if present {
                let (row, column) = record.key();
                bounds = Some(match bounds {
                    None => (row, row, column, column),
                    Some((first_row, last_row, first_column, last_column)) => (
                        first_row.min(row),
                        last_row.max(row),
                        first_column.min(column),
                        last_column.max(column),
                    ),
                });
            }
        }
        let (first_row, last_row, first_column, last_column) = match bounds {
            Some((a, b, c, d)) => (Some(a), Some(b), Some(c), Some(d)),
            None => (None, None, None, None),
        };
        let hash = ruby.hash_new();
        hash.aset(ruby.sym_new("first_row"), first_row)?;
        hash.aset(ruby.sym_new("last_row"), last_row)?;
        hash.aset(ruby.sym_new("first_column"), first_column)?;
        hash.aset(ruby.sym_new("last_column"), last_column)?;
        Ok(hash)
    }
}

pub fn define(ruby: &Ruby, native: magnus::RModule) -> Result<(), Error> {
    let store = native.define_class("Store", ruby.class_object())?;
    store.define_singleton_method("read", magnus::function!(Store::read, 2))?;
    store.define_method("[]", magnus::method!(Store::get, 1))?;
    store.define_method("bounds", magnus::method!(Store::bounds, 0))?;
    store.define_method("converter", magnus::method!(Store::converter, 0))?;
    Ok(())
}
