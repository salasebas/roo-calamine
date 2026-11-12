//! Keeps a worksheet's raw records and builds the roo cell for one only when
//! it is read, for `Roo::Excelx#cell` and the other single-cell lookups.

use std::ops::Range;

mod formatters;

use magnus::gc::Marker;
use magnus::rb_sys::AsRawValue;
use magnus::typed_data::Obj;
use magnus::value::{Opaque, ReprValue};
use magnus::{
    prelude::*, DataTypeFunctions, Error, Integer, RArray, RHash, RString, Ruby, TypedData, Value,
};

use crate::archive::Source;
use crate::convert::{Converter, CsvOutput};
use crate::reader::{Body, Kind, RawCell, RawRow, Reference, Style};
use crate::Reader;

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
    /// Sanitized values, separate from the original values roo's cells retain.
    cleaned: Opaque<RArray>,
    /// The keys more than one record has, sorted.
    repeated: Vec<u64>,
}

impl DataTypeFunctions for Store {
    fn mark(&self, marker: &Marker) {
        marker.mark(self.converter);
        marker.mark(self.built);
        marker.mark(self.handed_out);
        marker.mark(self.cleaned);
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

/// Cells parsed ahead while Ruby-dependent checks hold the GVL.
const BATCH_CELLS: usize = 16_384;

/// Reads every row with the GVL released, in batches. `None` as soon as a
/// record does not fit in a [`Record`], or `converter` might not build it
/// safely.
fn read(ruby: &Ruby, source: Source, converter: &Converter) -> Result<Option<Records>, Error> {
    let reader = Reader::new(ruby, source, false, BATCH_CELLS)?;
    let mut records = Records {
        text: String::new(),
        records: Vec::new(),
        formulas: Formulas::default(),
    };
    let empty_cell = converter.empty_cell();
    let mut rows = 0;
    let mut checked: u32 = 0;
    loop {
        let batch = reader.read_batch(ruby)?;
        if batch.is_empty() {
            break;
        }
        for row in batch {
            rows += 1;
            if records.add(row, rows, empty_cell).is_none() {
                return Ok(None);
            }
        }
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
    }
    records.text.shrink_to_fit();
    records.records.shrink_to_fit();
    records.formulas.0.shrink_to_fit();
    Ok(Some(records))
}

impl Store {
    /// The store for the worksheet `source`, or `nil` when its cells have to
    /// be built all at once, in order. `context` is a `Roo::Calamine::Context`.
    fn read(ruby: &Ruby, source: Source, context: Value) -> Result<Option<Self>, Error> {
        let converter = ruby.obj_wrap(Converter::new(ruby, context, false)?);
        let Some(Records {
            text,
            records,
            formulas,
        }) = read(ruby, source, &converter)?
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
            cleaned: ruby.ary_new().into(),
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
        let key = self.records[record as usize].built_key();
        if let Some(cell) = built.get(key) {
            return Ok(cell);
        }
        let handed_out: Value = ruby.get_inner(self.handed_out).entry(record as isize)?;
        let known = (!handed_out.is_nil()).then_some(handed_out);
        let raw = self.formulas.raw(&self.records, record);
        let (_, cell) = ruby
            .get_inner(self.converter)
            .cell(ruby, &self.text, &raw, 0, 0, known)?
            .expect("the store holds only records roo keeps");
        self.apply_cleaned_value(ruby, record, cell)?;
        built.aset(key, cell)?;
        Ok(cell)
    }

    /// `cells[key]&.value` for the key of `record`, from its cell when it is
    /// built, else as that cell will hold it.
    fn value(&self, ruby: &Ruby, record: u32) -> Result<Value, Error> {
        let raw = &self.records[record as usize];
        let built = ruby.get_inner(self.built);
        if !built.is_empty() {
            if let Some(cell) = built.get(raw.built_key()) {
                return cell.funcall("value", ());
            }
        }
        let cleaned: Value = ruby.get_inner(self.cleaned).entry(record as isize)?;
        if !cleaned.is_nil() {
            return Ok(cleaned);
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

    fn apply_cleaned_value(&self, ruby: &Ruby, record: u32, cell: Value) -> Result<(), Error> {
        let value: Value = ruby.get_inner(self.cleaned).entry(record as isize)?;
        if !value.is_nil() {
            cell.funcall::<_, _, Value>("value=", (value,))?;
        }
        Ok(())
    }

    fn clean(ruby: &Ruby, rb_self: &Self, book: Value) -> Result<(), Error> {
        for (record, raw) in (0..).zip(&rb_self.records) {
            if rb_self.repeated.binary_search(&raw.built_key()).is_ok()
                && rb_self.position(raw.key()).map(|i| rb_self.index[i]) != Ok(record)
            {
                continue;
            }
            let value = rb_self.value(ruby, record)?;
            if RString::from_value(value).is_none() {
                continue;
            }
            let value: Value = book.funcall("sanitize_value", (value,))?;
            if let Some(cell) = ruby.get_inner(rb_self.built).get(raw.built_key()) {
                cell.funcall::<_, _, Value>("value=", (value,))?;
            } else {
                ruby.get_inner(rb_self.cleaned)
                    .store(record as isize, value)?;
            }
        }
        Ok(())
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
            if kept {
                rb_self.apply_cleaned_value(ruby, record, cell)?;
            }
            cells.aset(coordinate, cell)?;
        }
        Ok(cells)
    }

    /// `cells[key]`.
    fn get(ruby: &Ruby, rb_self: &Self, key: Value) -> Result<Option<Value>, Error> {
        let Some(key) = RArray::from_value(key).filter(|key| key.len() == 2) else {
            return Ok(None);
        };
        Self::cell_at(ruby, rb_self, key.entry(0)?, key.entry(1)?)
    }

    /// `cells[[row, column]]` without allocating the key.
    fn cell_at(
        ruby: &Ruby,
        rb_self: &Self,
        row: Value,
        column: Value,
    ) -> Result<Option<Value>, Error> {
        match rb_self.record_at(row, column) {
            Some(record) => rb_self.build(ruby, record).map(Some),
            None => Ok(None),
        }
    }

    fn record_at(&self, row: Value, column: Value) -> Option<u32> {
        let part = |value| Integer::from_value(value).and_then(|n| n.to_u32().ok());
        let (Some(row), Some(column)) = (part(row), part(column)) else {
            return None;
        };
        self.position((row, column)).ok().map(|i| self.index[i])
    }

    fn value_at(
        ruby: &Ruby,
        rb_self: &Self,
        row: Value,
        column: Value,
    ) -> Result<Option<Value>, Error> {
        let Some(record) = rb_self.record_at(row, column) else {
            return Ok(None);
        };
        let raw = &rb_self.records[record as usize];
        let built = ruby.get_inner(rb_self.built);
        if !built.is_empty() {
            if let Some(cell) = built.get(raw.built_key()) {
                return cell.funcall("value", ()).map(Some);
            }
        }
        let cell = rb_self.formulas.raw(&rb_self.records, record);
        if let Some(value) =
            ruby.get_inner(rb_self.converter)
                .known_value(ruby, &rb_self.text, &cell)?
        {
            if rb_sys::SPECIAL_CONST_P(value.as_raw()) || raw.shared_string() {
                return Ok(Some(value));
            }
        }
        rb_self.build(ruby, record)?.funcall("value", ()).map(Some)
    }

    fn type_at(
        ruby: &Ruby,
        rb_self: &Self,
        row: Value,
        column: Value,
    ) -> Result<Option<Value>, Error> {
        let Some(record) = rb_self.record_at(row, column) else {
            return Ok(None);
        };
        let built = ruby.get_inner(rb_self.built);
        if !built.is_empty() {
            if let Some(cell) = built.get(rb_self.records[record as usize].built_key()) {
                return cell.funcall("type", ()).map(Some);
            }
        }
        let raw = rb_self.formulas.raw(&rb_self.records, record);
        match ruby
            .get_inner(rb_self.converter)
            .formatter_type(ruby, &raw)?
        {
            Some(kind) => Ok(Some(ruby.sym_new(kind).as_value())),
            None => rb_self.build(ruby, record)?.funcall("type", ()).map(Some),
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
        let row = Integer::from_value(row).and_then(|n| n.to_u32().ok());
        let from = u32::try_from(first_column.max(0)).unwrap_or(u32::MAX);
        let mut start = row.map_or(0, |row| {
            rb_self.position((row, from)).unwrap_or_else(|start| start)
        });
        rb_self.row_values(ruby, row, first_column, last_column, &mut start)
    }

    fn row_values(
        &self,
        ruby: &Ruby,
        row: Option<u32>,
        first_column: i64,
        last_column: i64,
        start: &mut usize,
    ) -> Result<RArray, Error> {
        let width = usize::try_from(last_column - first_column + 1).unwrap_or(0);
        let values = ruby.ary_new_capa(width);
        let mut column = first_column;
        if let Some(row) = row {
            while let Some(&record) = self.index.get(*start) {
                let (record_row, record_column) = self.records[record as usize].key();
                if record_row != row || i64::from(record_column) > last_column {
                    break;
                }
                for _ in column..i64::from(record_column) {
                    values.push(ruby.qnil())?;
                }
                values.push(self.value(ruby, record)?)?;
                column = i64::from(record_column) + 1;
                *start += 1;
            }
        }
        for _ in column..=last_column {
            values.push(ruby.qnil())?;
        }
        Ok(values)
    }

    /// Yields rows until the block returns false, then returns the next row number.
    fn each_row(
        ruby: &Ruby,
        rb_self: &Self,
        last_row: u32,
        first_column: i64,
        last_column: i64,
    ) -> Result<u64, Error> {
        let mut start = 0;
        let from = u32::try_from(first_column.max(0)).unwrap_or(u32::MAX);
        for row in 1..=last_row {
            start = rb_self.position_from(start, (row, from));
            let values =
                rb_self.row_values(ruby, Some(row), first_column, last_column, &mut start)?;
            if !ruby.yield_value::<_, bool>(values)? {
                return Ok(u64::from(row) + 1);
            }
        }
        Ok(u64::from(last_row) + 1)
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
        let row = Integer::from_value(row).and_then(|n| n.to_u32().ok());
        let start = row.map(|row| {
            rb_self
                .position((row, u32::try_from(first_column.max(0)).unwrap_or(u32::MAX)))
                .unwrap_or_else(|start| start)
        });
        let width = (last_column - first_column + 1).max(0);
        let mut position = start.unwrap_or(0);
        let mut previous_column = first_column;
        let hash = ruby.hash_new_capa(fields.len());
        for field in fields {
            let field = RArray::try_convert(field)?;
            let key: Value = field.entry(0)?;
            let slot: i64 = field.entry(1)?;
            let slot = if slot < 0 { width + slot } else { slot };
            let record = match (row, start) {
                (Some(row), Some(start)) if (0..width).contains(&slot) => {
                    let column = first_column + slot;
                    u32::try_from(column).ok().and_then(|column| {
                        if i64::from(column) < previous_column {
                            position = start;
                        }
                        previous_column = i64::from(column);
                        position = rb_self.position_from(position, (row, column));
                        rb_self
                            .index
                            .get(position)
                            .copied()
                            .filter(|&i| rb_self.records[i as usize].key() == (row, column))
                    })
                }
                _ => None,
            };
            let value = match record {
                Some(record) => rb_self.value(ruby, record)?,
                None => ruby.qnil().as_value(),
            };
            hash.aset(key, value)?;
        }
        Ok(hash)
    }

    /// What `Roo::Excelx::Sheet#first_last_row_col` computes from `cells`.
    fn bounds(ruby: &Ruby, rb_self: &Self) -> Result<RHash, Error> {
        let presence = |cell: Value| -> Result<bool, Error> {
            Ok(cell.funcall::<_, _, Value>("presence", ())?.to_bool())
        };
        let built = ruby.get_inner(rb_self.built);
        let has_built = !built.is_empty();
        let handed_out = ruby.get_inner(rb_self.handed_out);
        let has_handed_out = !handed_out.is_empty();
        let converter = ruby.get_inner(rb_self.converter);
        let mut bounds: Option<(u32, u32, u32, u32)> = None;
        for &i in &rb_self.index {
            let record = &rb_self.records[i as usize];
            // A cell already handed out may have been changed in place.
            let cell = if has_built {
                built.get(record.built_key())
            } else {
                None
            };
            let cleaned: Value = ruby.get_inner(rb_self.cleaned).entry(i as isize)?;
            let present = match cell {
                Some(cell) => presence(cell)?,
                None if !cleaned.is_nil() => !RString::try_convert(cleaned)?.is_empty(),
                None => match (record.content, record.kind) {
                    (Content::Empty, _) => false,
                    (Content::Value, Kind::Shared) => match converter.presence(
                        ruby,
                        &rb_self.text,
                        &rb_self.formulas.raw(&rb_self.records, i),
                    )? {
                        Some(present) => present,
                        None => presence(rb_self.build(ruby, i)?)?,
                    },
                    (Content::Inline, _) | (Content::Value, Kind::Str) => {
                        let value: Value = if has_handed_out {
                            handed_out.entry(i as isize)?
                        } else {
                            ruby.qnil().as_value()
                        };
                        match RString::from_value(value) {
                            Some(value) => !value.is_empty(),
                            None => {
                                record.content == Content::Inline || record.text.0 != record.text.1
                            }
                        }
                    }
                    (Content::Value, _) => true,
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

/// What [`Store::write_csv`] writes: `rows` lines of `width` fields, with
/// the fields outside the sheet's first and last row and column left empty.
struct CsvArea {
    rows: i64,
    width: i64,
    first_row: i64,
    last_row: i64,
    first_column: i64,
    last_column: i64,
}

impl Store {
    /// `Roo::Calamine::CSV#write_csv_lines` for the cells of this store, with
    /// `book` the `Roo::Excelx` whose CSV helpers format the fields Ruby
    /// formats, and `area` `[rows, width, first_row, last_row, first_column,
    /// last_column]`. When a field raises, `io` holds the text up to it, as
    /// in roo.
    fn write_csv(
        ruby: &Ruby,
        rb_self: &Self,
        io: Value,
        book: Value,
        separator: RString,
        area: RArray,
    ) -> Result<(), Error> {
        let area = CsvArea {
            rows: area.entry(0)?,
            width: area.entry(1)?,
            first_row: area.entry(2)?,
            last_row: area.entry(3)?,
            first_column: area.entry(4)?,
            last_column: area.entry(5)?,
        };
        let mut output = CsvOutput::new(io, unsafe { separator.as_slice() }.to_vec());
        let written = rb_self.csv_lines(ruby, book, &mut output, &area);
        output.print(ruby).and(written)
    }

    fn csv_lines(
        &self,
        ruby: &Ruby,
        book: Value,
        output: &mut CsvOutput,
        area: &CsvArea,
    ) -> Result<(), Error> {
        let CsvArea { rows, width, .. } = *area;
        let mut line_row = 0;
        let mut written = 0;
        for &record in &self.index {
            let (row, column) = self.records[record as usize].key();
            let (row, column) = (i64::from(row), i64::from(column));
            if row > rows {
                break;
            }
            if row < 1 || column < 1 || column > width {
                continue;
            }
            if row != line_row {
                if line_row > 0 {
                    output.end_line(ruby, width - 1 - written)?;
                }
                for _ in line_row + 1..row {
                    output.end_line(ruby, width - 1)?;
                }
                line_row = row;
                written = 0;
            }
            output.separators(column - 1 - written);
            written = column - 1;
            let in_bounds = (area.first_row..=area.last_row).contains(&row)
                && (area.first_column..=area.last_column).contains(&column);
            self.csv_field(ruby, book, record, in_bounds, output)?;
        }
        if line_row > 0 {
            output.end_line(ruby, width - 1 - written)?;
        }
        for _ in line_row + 1..=rows {
            output.end_line(ruby, width - 1)?;
        }
        Ok(())
    }

    /// Appends the field of `record`, empty when its cell is empty or
    /// outside the sheet's bounds. A cell already built may have been changed
    /// since, so it is asked, as is one whose field needs the cell.
    fn csv_field(
        &self,
        ruby: &Ruby,
        book: Value,
        record: u32,
        in_bounds: bool,
        output: &mut CsvOutput,
    ) -> Result<(), Error> {
        let key = self.records[record as usize].built_key();
        let cell = match ruby.get_inner(self.built).get(key) {
            Some(cell) => cell,
            None if !in_bounds => return Ok(()),
            None => {
                let cleaned: Value = ruby.get_inner(self.cleaned).entry(record as isize)?;
                let kept: Value = if cleaned.is_nil() {
                    ruby.get_inner(self.handed_out).entry(record as isize)?
                } else {
                    cleaned
                };
                let kept = (!kept.is_nil()).then_some(kept);
                let raw = self.formulas.raw(&self.records, record);
                let converter = ruby.get_inner(self.converter);
                if converter.csv_field(ruby, book, &self.text, &raw, kept, output)? {
                    return Ok(());
                }
                self.build(ruby, record)?
            }
        };
        if cell.funcall::<_, _, Value>("empty?", ())?.to_bool() || !in_bounds {
            return Ok(());
        }
        output.field(ruby, book.funcall("csv_field", (cell,))?)
    }
}

pub fn define(ruby: &Ruby, native: magnus::RModule) -> Result<(), Error> {
    let store = native.define_class("Store", ruby.class_object())?;
    store.define_singleton_method("read", magnus::function!(Store::read, 2))?;
    store.define_method("[]", magnus::method!(Store::get, 1))?;
    store.define_method("cell_at", magnus::method!(Store::cell_at, 2))?;
    store.define_method("value_at", magnus::method!(Store::value_at, 2))?;
    store.define_method("type_at", magnus::method!(Store::type_at, 2))?;
    store.define_method("bounds", magnus::method!(Store::bounds, 0))?;
    store.define_method("row", magnus::method!(Store::row, 3))?;
    store.define_method("each_row", magnus::method!(Store::each_row, 3))?;
    store.define_method("column", magnus::method!(Store::column, 3))?;
    store.define_method("hash_row", magnus::method!(Store::hash_row, 4))?;
    store.define_method("cells", magnus::method!(Store::cells, 0))?;
    store.define_method("clean", magnus::method!(Store::clean, 1))?;
    store.define_method("write_csv", magnus::method!(Store::write_csv, 4))?;
    store.define_method("to_yaml", magnus::method!(Store::to_yaml, 3))?;
    store.define_method("xml_cells", magnus::method!(Store::xml_cells, 2))?;
    store.define_method("matrix_rows", magnus::method!(Store::matrix_rows, 1))?;
    Ok(())
}
