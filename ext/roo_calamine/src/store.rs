//! Keeps a worksheet's raw records and builds the roo cell for one only when
//! it is read, for `Roo::Excelx#cell` and the other single-cell lookups.

use std::ops::Range;
use std::path::Path;
use std::sync::atomic::Ordering;

use magnus::gc::Marker;
use magnus::rb_sys::AsRawValue;
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Content {
    Empty,
    Inline,
    Value,
}

/// A record roo's `cells` keeps a cell for, as [`RawCell`] holds it but
/// without its heap parts, in 20 bytes: the coordinate is always known, the
/// style is an index, `text` is the span of an `Inline` or `Value` content in
/// [`Store::text`], and a record with a formula has its span in
/// [`Store::formulas`].
#[derive(Clone, Copy)]
struct Record {
    row: u32,
    text: Span,
    column: u16,
    style: u16,
    kind: Kind,
    content: Content,
    formula: bool,
}

const _: () = assert!(size_of::<Record>() == 20);

impl Record {
    /// The record and the span of its formula, or `None` for a record that
    /// needs a heap part, or Ruby, to read, or does not fit.
    fn new(raw: RawCell, row: i64, column: i64, offset: usize) -> Option<(Self, Option<Span>)> {
        let span = |range: Range<usize>| -> Option<Span> {
            Some((
                u32::try_from(range.start + offset).ok()?,
                u32::try_from(range.end + offset).ok()?,
            ))
        };
        let (content, text) = match raw.body {
            Body::Empty => (Content::Empty, (0, 0)),
            Body::Inline(text) => (Content::Inline, span(text)?),
            Body::Value(text) => (Content::Value, span(text)?),
        };
        let formula = match raw.formula {
            Some(formula) => Some(span(formula)?),
            None => None,
        };
        let record = Self {
            row: u32::try_from(row).ok()?,
            text,
            column: u16::try_from(column).ok()?,
            style: match raw.style {
                Style::Index(index) => u16::try_from(index).ok()?,
                Style::Other(_) => return None,
            },
            kind: raw.kind,
            content,
            formula: formula.is_some(),
        };
        Some((record, formula))
    }

    /// The cell as [`RawCell`] holds it, with `formula` the span of its
    /// formula when it has one.
    fn raw(&self, formula: Option<Span>) -> RawCell {
        RawCell {
            reference: Reference::At {
                row: self.row.into(),
                column: self.column.into(),
            },
            style: Style::Index(self.style.into()),
            kind: self.kind,
            formula: formula.map(range),
            body: match self.content {
                Content::Empty => Body::Empty,
                Content::Inline => Body::Inline(range(self.text)),
                Content::Value => Body::Value(range(self.text)),
            },
        }
    }

    fn key(&self) -> (u32, u32) {
        (self.row, self.column.into())
    }

    fn built_key(&self) -> u64 {
        u64::from(self.row) << 32 | u64::from(self.column)
    }

    fn shared_string(&self) -> bool {
        self.kind == Kind::Shared && self.content == Content::Value
    }
}

/// The span of the formula of each record that has one, by record, sorted.
#[derive(Default)]
struct Formulas(Vec<(u32, Span)>);

impl Formulas {
    /// [`Record::raw`] for the record at `index` of `records`.
    fn raw(&self, records: &[Record], index: u32) -> RawCell {
        let record = &records[index as usize];
        record.raw(record.formula.then(|| {
            let i = self
                .0
                .binary_search_by_key(&index, |&(i, _)| i)
                .expect("a record with a formula has its span");
            self.0[i].1
        }))
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
    formulas: Formulas,
    /// For each key of roo's `cells` hash, the record whose cell it holds
    /// (the last one with that key), sorted by key.
    index: Vec<u32>,
    /// The cells built so far, by [`key`].
    built: Opaque<RHash>,
    /// By record, the values [`Store::value`] handed out before their cell
    /// was built, which that cell then holds: `nil`, or an object that is
    /// not a shared string (the converter picks the same one again).
    handed_out: Opaque<RArray>,
    /// The keys more than one record has, sorted.
    repeated: Vec<u64>,
}

impl DataTypeFunctions for Store {
    fn mark(&self, marker: &Marker) {
        marker.mark(self.converter);
        marker.mark(self.built);
        marker.mark(self.handed_out);
    }

    fn size(&self) -> usize {
        size_of::<Self>()
            + self.text.capacity()
            + self.records.capacity() * size_of::<Record>()
            + self.formulas.0.capacity() * size_of::<(u32, Span)>()
            + self.index.capacity() * size_of::<u32>()
            + self.repeated.capacity() * size_of::<u64>()
    }
}

struct Records {
    text: String,
    records: Vec<Record>,
    formulas: Formulas,
}

impl Records {
    /// `None` once a record does not fit in a [`Record`].
    fn add(&mut self, row: RawRow, row_number: usize, empty_cell: bool) -> Option<()> {
        let offset = self.text.len();
        self.text.push_str(&row.text);
        for (index, raw) in row.cells.into_iter().enumerate() {
            let (row, column) = raw.position(row_number, index + 1)?;
            if raw.body != Body::Empty || empty_cell {
                let (record, formula) = Record::new(raw, row, column, offset)?;
                if let Some(formula) = formula {
                    let index = u32::try_from(self.records.len()).ok()?;
                    self.formulas.0.push((index, formula));
                }
                self.records.push(record);
            }
        }
        Some(())
    }
}

/// The records [`read`] reads with the GVL released before it checks them.
const BATCH_RECORDS: usize = 16_384;

/// Reads every row with the GVL released, in batches. `None` as soon as a
/// record does not fit in a [`Record`], or `converter` might not build it
/// safely.
fn read(ruby: &Ruby, path: &str, converter: &Converter) -> Result<Option<Records>, Error> {
    let mut sheet =
        SheetReader::open(Path::new(path), Mode::Document).map_err(|e| unsupported(ruby, e))?;
    let mut records = Records {
        text: String::new(),
        records: Vec::new(),
        formulas: Formulas::default(),
    };
    let empty_cell = converter.empty_cell();
    let mut rows = 0;
    let mut checked: u32 = 0;
    loop {
        let batch_end = checked as usize + BATCH_RECORDS;
        let read = without_gvl(|interrupted| {
            while !interrupted.load(Ordering::Acquire) && records.records.len() < batch_end {
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
        let Some(done) = read.map_err(|e| unsupported(ruby, e))? else {
            return Ok(None);
        };
        let Ok(end) = u32::try_from(records.records.len()) else {
            return Ok(None);
        };
        if !(checked..end).all(|i| {
            let raw = records.formulas.raw(&records.records, i);
            converter.builds_safely(ruby, &records.text, &raw)
        }) {
            return Ok(None);
        }
        checked = end;
        if done {
            break;
        }
    }
    records.text.shrink_to_fit();
    records.records.shrink_to_fit();
    records.formulas.0.shrink_to_fit();
    Ok(Some(records))
}

impl Store {
    /// The store for the worksheet at `path`, or `nil` when its cells have to
    /// be built all at once, in order. `context` is a `Roo::Calamine::Context`.
    fn read(ruby: &Ruby, path: String, context: Value) -> Result<Option<Self>, Error> {
        let converter = ruby.obj_wrap(Converter::new(ruby, context, false)?);
        let Some(Records {
            text,
            records,
            formulas,
        }) = read(ruby, &path, &converter)?
        else {
            return Ok(None);
        };
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
            formulas,
            index,
            built: ruby.hash_new().into(),
            handed_out: ruby.ary_new().into(),
            repeated,
        };
        // `Roo::Link.new` copies a linked cell's shared string, which roo does
        // before a caller can change that string in place.
        for &i in &store.index {
            let record = &store.records[i as usize];
            if record.shared_string()
                && converter.has_link(ruby, record.row.into(), record.column.into())
            {
                store.build(ruby, i)?;
            }
        }
        Ok(Some(store))
    }

    fn build(&self, ruby: &Ruby, record: u32) -> Result<Value, Error> {
        let built = ruby.get_inner(self.built);
        let handed_out: Value = ruby.get_inner(self.handed_out).entry(record as isize)?;
        let key = self.records[record as usize].built_key();
        if let Some(cell) = built.get(key) {
            return Ok(cell);
        }
        let known = (!handed_out.is_nil()).then_some(handed_out);
        let raw = self.formulas.raw(&self.records, record);
        let (_, cell) = ruby
            .get_inner(self.converter)
            .cell(ruby, &self.text, &raw, 0, 0, known)?
            .expect("the store holds only records roo keeps");
        built.aset(key, cell)?;
        Ok(cell)
    }

    /// `cells[key]&.value` for the key of `record`, from its cell when it is
    /// built, else as that cell will hold it.
    fn value(&self, ruby: &Ruby, record: u32) -> Result<Value, Error> {
        let raw = &self.records[record as usize];
        if let Some(cell) = ruby.get_inner(self.built).get(raw.built_key()) {
            return cell.funcall("value", ());
        }
        let handed_out = ruby.get_inner(self.handed_out);
        let kept: Value = handed_out.entry(record as isize)?;
        if !kept.is_nil() {
            return Ok(kept);
        }
        let converter = ruby.get_inner(self.converter);
        let cell = self.formulas.raw(&self.records, record);
        let Some(value) = converter.known_value(ruby, &self.text, &cell)? else {
            return self.build(ruby, record)?.funcall("value", ());
        };
        if !rb_sys::SPECIAL_CONST_P(value.as_raw()) && !raw.shared_string() {
            handed_out.store(record as isize, value)?;
        }
        Ok(value)
    }

    /// The position in [`Store::index`] of the record for `key`.
    fn position(&self, key: (u32, u32)) -> Result<usize, usize> {
        self.index
            .binary_search_by_key(&key, |&i| self.records[i as usize].key())
    }

    /// The first position from `start` on in [`Store::index`] whose key is
    /// `key` or after it, searched in windows that double from `start`.
    fn position_from(&self, start: usize, key: (u32, u32)) -> usize {
        let before = |&i: &u32| self.records[i as usize].key() < key;
        let rest = &self.index[start..];
        let mut bound = 1;
        while bound < rest.len() && before(&rest[bound]) {
            bound *= 2;
        }
        let from = bound / 2;
        start + from + rest[from..rest.len().min(bound)].partition_point(before)
    }

    /// `cells` as `SheetDoc#extract_cells` builds it, holding the cells
    /// built so far and the values handed out. Of the records with a
    /// repeated key, all but the one [`Store::index`] keeps get a cell of
    /// their own, as roo builds one for each.
    fn cells(ruby: &Ruby, rb_self: &Self) -> Result<RHash, Error> {
        let cells = ruby.hash_new_capa(rb_self.index.len());
        let built = ruby.get_inner(rb_self.built);
        let handed_out = ruby.get_inner(rb_self.handed_out);
        let converter = ruby.get_inner(rb_self.converter);
        for (record, raw) in (0..).zip(&rb_self.records) {
            let key = raw.built_key();
            let kept = rb_self.repeated.binary_search(&key).is_err()
                || rb_self.position(raw.key()).map(|i| rb_self.index[i]) == Ok(record);
            if let Some(cell) = built.get(key).filter(|_| kept) {
                cells.aset(cell.funcall::<_, _, Value>("coordinate", ())?, cell)?;
                continue;
            }
            let value: Value = handed_out.entry(record as isize)?;
            let known = (kept && !value.is_nil()).then_some(value);
            let (coordinate, cell) = converter
                .cell(
                    ruby,
                    &rb_self.text,
                    &rb_self.formulas.raw(&rb_self.records, record),
                    0,
                    0,
                    known,
                )?
                .expect("the store holds only records roo keeps");
            cells.aset(coordinate, cell)?;
        }
        Ok(cells)
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
        match rb_self.position((row, column)) {
            Ok(i) => rb_self.build(ruby, rb_self.index[i]).map(Some),
            Err(_) => Ok(None),
        }
    }

    /// `first_column.upto(last_column).map { |column| cells[[row, column]]&.value }`,
    /// building only the cells whose value needs one.
    fn row(
        ruby: &Ruby,
        rb_self: &Self,
        row: Value,
        first_column: i64,
        last_column: i64,
    ) -> Result<RArray, Error> {
        let width = usize::try_from(last_column - first_column + 1).unwrap_or(0);
        let values = ruby.ary_new_capa(width);
        let mut column = first_column;
        if let Some(row) = Integer::from_value(row).and_then(|n| n.to_u32().ok()) {
            let from = u32::try_from(first_column.max(0)).unwrap_or(u32::MAX);
            let start = rb_self.position((row, from)).unwrap_or_else(|start| start);
            for &record in &rb_self.index[start..] {
                let (record_row, record_column) = rb_self.records[record as usize].key();
                if record_row != row || i64::from(record_column) > last_column {
                    break;
                }
                for _ in column..i64::from(record_column) {
                    values.push(ruby.qnil())?;
                }
                values.push(rb_self.value(ruby, record)?)?;
                column = i64::from(record_column) + 1;
            }
        }
        for _ in column..=last_column {
            values.push(ruby.qnil())?;
        }
        Ok(values)
    }

    /// `first_row.upto(last_row).map { |row| cells[[row, column]]&.value }`,
    /// building only the cells whose value needs one.
    fn column(
        ruby: &Ruby,
        rb_self: &Self,
        column: Value,
        first_row: i64,
        last_row: i64,
    ) -> Result<RArray, Error> {
        let height = usize::try_from(last_row - first_row + 1).unwrap_or(0);
        let values = ruby.ary_new_capa(height);
        let column = Integer::from_value(column).and_then(|n| n.to_u32().ok());
        let mut start = 0;
        for row in first_row..=last_row {
            let record = match (u32::try_from(row), column) {
                (Ok(row), Some(column)) => {
                    start = rb_self.position_from(start, (row, column));
                    let record = rb_self.index.get(start).copied();
                    record.filter(|&i| rb_self.records[i as usize].key() == (row, column))
                }
                _ => None,
            };
            match record {
                Some(record) => values.push(rb_self.value(ruby, record)?)?,
                None => values.push(ruby.qnil())?,
            }
        }
        Ok(values)
    }

    /// `fields.each_with_object({}) { |(key, slot), hash| hash[key] = values[slot] }`
    /// for the `values` of [`Store::row`].
    fn hash_row(
        ruby: &Ruby,
        rb_self: &Self,
        row: Value,
        first_column: i64,
        last_column: i64,
        fields: RArray,
    ) -> Result<RHash, Error> {
        let values = Self::row(ruby, rb_self, row, first_column, last_column)?;
        let hash = ruby.hash_new_capa(fields.len());
        for field in fields {
            let field = RArray::try_convert(field)?;
            let key: Value = field.entry(0)?;
            let slot: isize = field.entry(1)?;
            hash.aset(key, values.entry::<Value>(slot)?)?;
        }
        Ok(hash)
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
                    &rb_self.formulas.raw(&rb_self.records, i),
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
    store.define_method("row", magnus::method!(Store::row, 3))?;
    store.define_method("column", magnus::method!(Store::column, 3))?;
    store.define_method("hash_row", magnus::method!(Store::hash_row, 4))?;
    store.define_method("cells", magnus::method!(Store::cells, 0))?;
    Ok(())
}
