use super::{Dimensions, ValueBufs, XlsxCellStream};
use crate::roo::{Mode, RawRow, Result, Span, Unsupported, XmlReader};
use quick_xml::events::BytesStart;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FormulaMode {
    Ignore,
    Metadata,
    Expanded,
}
pub(super) enum RooTag {
    Row,
    Cell,
    Skip,
    Other,
}
pub(super) struct RooState {
    mode: Mode,
    worksheet: bool,
    sheet_depth: Option<usize>,
    pub(super) row_depth: Option<usize>,
    pub(super) row: RawRow,
    row_size: (usize, usize),
    pub(super) done: bool,
    pending_error: Option<Unsupported>,
}
impl RooState {
    pub(super) fn start(&mut self, e: &BytesStart<'_>, depth: usize, empty: bool) -> RooTag {
        if let Some(row) = self.row_depth {
            return if depth == row + 1 {
                if self.mode == Mode::Document && e.local_name().as_ref() != "c" {
                    RooTag::Skip
                } else {
                    RooTag::Cell
                }
            } else {
                RooTag::Other
            };
        }
        if depth == 1 {
            self.worksheet = e.local_name().as_ref() == "worksheet";
        }
        if depth == 2 && self.worksheet && e.local_name().as_ref() == "sheetData" && !empty {
            self.sheet_depth = Some(depth);
        }
        let row = match self.mode {
            Mode::Document => {
                self.sheet_depth == Some(depth - 1) && e.local_name().as_ref() == "row"
            }
            Mode::Stream => e.name().as_ref() == "row",
        };
        if row {
            if !empty {
                self.row_depth = Some(depth);
            }
            RooTag::Row
        } else {
            RooTag::Other
        }
    }
    pub(super) fn end(&mut self, depth: usize) -> bool {
        if self.sheet_depth == Some(depth + 1) {
            self.sheet_depth = None;
        }
        if self.row_depth == Some(depth + 1) {
            self.row_depth = None;
            true
        } else {
            false
        }
    }
    pub(super) fn append(&mut self, text: &str) -> Span {
        let start = self.row.text.len();
        self.row.text.push_str(text);
        start..self.row.text.len()
    }
    pub(super) fn in_streamed_row(&self) -> bool {
        self.mode == Mode::Stream && self.row_depth.is_some()
    }
    fn take_row(&mut self) -> RawRow {
        let row = std::mem::take(&mut self.row);
        self.row_size = (row.text.len(), row.cells.len());
        self.row = RawRow {
            text: String::with_capacity(self.row_size.0),
            cells: Vec::with_capacity(self.row_size.1),
        };
        row
    }
}
impl<B: BufRead> XlsxCellStream<'static, B> {
    /// Creates a direct XLSX stream preserving roo's raw values and row selection.
    /// Workbook metadata remains with the caller; no ZIP, styles, or shared strings are loaded here.
    pub fn for_roo(input: B, mode: Mode) -> Self {
        Self {
            xml: XmlReader::for_roo(input, mode),
            strings: &[],
            formats: &[],
            styles: &[],
            is_1904: false,
            dimensions: Dimensions::default(),
            row_index: 0,
            col_index: 0,
            buf: Vec::with_capacity(16 * 1024),
            cell_buf: Vec::with_capacity(1024),
            value_bufs: ValueBufs::new(),
            formulas: Vec::new(),
            roo: Some(RooState {
                mode,
                worksheet: false,
                sheet_depth: None,
                row_depth: None,
                row: RawRow::default(),
                row_size: (0, 0),
                done: false,
                pending_error: None,
            }),
        }
    }
    /// Returns the next selected row, including empty rows, or None after validating EOF.
    pub fn next_row(&mut self) -> Result<Option<RawRow>> {
        if self.roo.as_ref().expect("for_roo reader").done {
            return Ok(None);
        }
        while self
            .next_cell_formula_record_impl(FormulaMode::Ignore)?
            .is_some()
        {}
        let roo = self.roo.as_mut().expect("for_roo reader");
        if roo.done {
            Ok(None)
        } else {
            Ok(Some(roo.take_row()))
        }
    }
    /// Reads bounded row batches. Rows before a failure are returned before the error.
    pub fn next_rows(&mut self, cells: usize) -> Result<Vec<RawRow>> {
        if let Some(error) = self
            .roo
            .as_mut()
            .expect("for_roo reader")
            .pending_error
            .take()
        {
            return Err(error);
        }
        let mut rows = Vec::new();
        let mut count = 0;
        while count < cells {
            match self.next_row() {
                Ok(Some(row)) => {
                    count += row.cells.len().max(1);
                    rows.push(row);
                }
                Ok(None) => break,
                Err(error) if rows.is_empty() => return Err(error),
                Err(error) => {
                    self.roo.as_mut().expect("for_roo reader").pending_error = Some(error);
                    break;
                }
            }
        }
        Ok(rows)
    }
    /// Returns a validated event without constructing worksheet rows.
    pub fn next_xml_event<'b>(
        &mut self,
        buf: &'b mut Vec<u8>,
    ) -> Result<quick_xml::events::Event<'b>> {
        self.xml.next_xml_event(buf)
    }
}
impl XlsxCellStream<'static, BufReader<File>> {
    /// Opens a worksheet or metadata part with the same compatibility checks as `for_roo`.
    pub fn open(path: &Path, mode: Mode) -> Result<Self> {
        let file = File::open(path).map_err(|error| Unsupported(error.to_string()))?;
        Ok(Self::for_roo(
            BufReader::with_capacity(64 * 1024, file),
            mode,
        ))
    }
}
