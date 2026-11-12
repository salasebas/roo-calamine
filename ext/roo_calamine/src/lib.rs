//! Native fast path for `roo-calamine`.
//!
//! [`reader`] turns worksheet XML into raw cell records without touching Ruby.
//! [`convert`] turns those records into roo's cells,
//! and [`store`] keeps them to build one cell at a time. [`shared_strings`]
//! reads a workbook's shared strings the way roo does.

pub mod archive;
pub mod convert;
pub mod numeric;
pub use calamine::roo as reader;
pub mod shared_strings;
pub mod sheet_parts;
pub mod store;

use std::cell::{Cell as StdCell, RefCell};
use std::ffi::c_void;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::time::Duration;

use magnus::value::Lazy;
use magnus::{function, method, prelude::*, Error, ExceptionClass, RArray, RHash, Ruby};

use archive::Source;
use convert::Converter;
use reader::{Mode, RawRow, Unsupported};

static UNSUPPORTED: Lazy<ExceptionClass> = Lazy::new(|r| {
    r.eval("Roo::Calamine::UnsupportedError")
        .expect("roo/calamine defines its error before the extension loads")
});

pub(crate) fn unsupported(ruby: &Ruby, error: Unsupported) -> Error {
    Error::new(ruby.get_inner(&UNSUPPORTED), error.0)
}

/// Runs `work` with the GVL released. `work` must not touch any Ruby object,
/// and should return soon after the flag it is given turns true: Ruby sets
/// it when it has something to deliver to this thread (`Thread#raise`,
/// `Timeout`, Ctrl-C). Such an exception comes back as `Err` instead of
/// unwinding through the caller's Rust frames.
pub(crate) fn without_gvl<F: FnOnce(&AtomicBool) -> R, R>(work: F) -> Result<R, Error> {
    struct Call<'a, F, R> {
        work: Option<F>,
        interrupted: &'a AtomicBool,
        result: Option<std::thread::Result<R>>,
    }

    unsafe extern "C" fn trampoline<F: FnOnce(&AtomicBool) -> R, R>(
        data: *mut c_void,
    ) -> *mut c_void {
        let call = unsafe { &mut *(data as *mut Call<'_, F, R>) };
        let work = call.work.take().expect("called once");
        let interrupted = call.interrupted;
        call.result = Some(panic::catch_unwind(AssertUnwindSafe(|| work(interrupted))));
        std::ptr::null_mut()
    }

    unsafe extern "C" fn unblock(data: *mut c_void) {
        unsafe { &*(data as *const AtomicBool) }.store(true, Ordering::Release);
    }

    let interrupted = AtomicBool::new(false);
    let mut call = Call {
        work: Some(work),
        interrupted: &interrupted,
        result: None,
    };
    let data = &mut call as *mut Call<'_, F, R> as *mut c_void;
    let flag = &interrupted as *const AtomicBool as *mut c_void;
    magnus::rb_sys::protect(|| unsafe {
        rb_sys::rb_thread_call_without_gvl(Some(trampoline::<F, R>), data, Some(unblock), flag);
        rb_sys::Qnil as rb_sys::VALUE
    })?;
    match call.result.expect("trampoline ran") {
        Ok(result) => Ok(result),
        Err(payload) => panic::resume_unwind(payload),
    }
}

enum Wait {
    Batch(Batch),
    /// Ruby interrupted the wait without raising; wait again.
    Interrupted,
    /// The reading thread ended without sending a last batch.
    Stopped,
}

/// Waits for the next batch with the GVL released.
fn wait_for(batches: &Receiver<Batch>) -> Result<Wait, Error> {
    without_gvl(|interrupted| loop {
        match batches.recv_timeout(Duration::from_millis(20)) {
            Ok(batch) => return Wait::Batch(batch),
            Err(RecvTimeoutError::Disconnected) => return Wait::Stopped,
            Err(RecvTimeoutError::Timeout) if interrupted.load(Ordering::Acquire) => {
                return Wait::Interrupted
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
    })
}

/// One streamed `<row>`, converted to cells only when roo asks for them.
#[magnus::wrap(class = "Roo::Calamine::Native::Row", free_immediately, size)]
pub struct Row(RawRow);

impl Row {
    pub fn raw(&self) -> &RawRow {
        &self.0
    }
}

type Batch = Result<Vec<RawRow>, Unsupported>;

enum Batches {
    Thread(Receiver<Batch>),
    Inline {
        sheet: Box<reader::SheetReader<archive::Input>>,
        batch: usize,
    },
}

/// Reads small ZIP entries on demand without the GVL. Large worksheets are
/// read one batch ahead on a worker, so a busy Ruby thread does not slow
/// every batch down.
#[magnus::wrap(class = "Roo::Calamine::Native::Reader", free_immediately, size)]
struct Reader {
    /// `None` once closed.
    batches: RefCell<Option<Batches>>,
    /// The rows of the last batch not handed to Ruby yet.
    pending: RefCell<std::vec::IntoIter<RawRow>>,
    /// Set while a Ruby thread waits on `batches` without the GVL; the
    /// receiver must not be used from two threads at once.
    in_use: StdCell<bool>,
    done: StdCell<bool>,
    rows_read: StdCell<usize>,
}

impl Reader {
    /// `batch` is the number of cells parsed per batch.
    fn new(ruby: &Ruby, source: Source, stream: bool, batch: usize) -> Result<Self, Error> {
        let mode = if stream { Mode::Stream } else { Mode::Document };
        let mut sheet = source.open(ruby, mode)?;
        let batches = if source.small() {
            Batches::Inline {
                sheet: Box::new(sheet),
                batch,
            }
        } else {
            let (sender, batches) = mpsc::sync_channel::<Batch>(1);
            std::thread::Builder::new()
                .name("roo-calamine".into())
                .spawn(move || loop {
                    let rows = sheet.next_rows(batch);
                    let last = !matches!(&rows, Ok(rows) if !rows.is_empty());
                    if sender.send(rows).is_err() || last {
                        return;
                    }
                })
                .map_err(|e| Error::new(ruby.exception_runtime_error(), e.to_string()))?;
            Batches::Thread(batches)
        };
        Ok(Self {
            batches: RefCell::new(Some(batches)),
            pending: RefCell::new(Vec::new().into_iter()),
            in_use: StdCell::new(false),
            done: StdCell::new(false),
            rows_read: StdCell::new(0),
        })
    }

    fn read_batch(&self, ruby: &Ruby) -> Result<Vec<RawRow>, Error> {
        if self.done.get() {
            return Ok(Vec::new());
        }
        if self.in_use.replace(true) {
            return Err(Error::new(
                ruby.exception_runtime_error(),
                "reader is already in use",
            ));
        }
        let received = match self.batches.borrow_mut().as_mut() {
            None => Ok(Wait::Batch(Ok(Vec::new()))),
            Some(Batches::Inline { sheet, batch }) => {
                without_gvl(|_| Wait::Batch(sheet.next_rows(*batch)))
            }
            Some(Batches::Thread(batches)) => match batches.try_recv() {
                Ok(batch) => Ok(Wait::Batch(batch)),
                Err(TryRecvError::Disconnected) => Ok(Wait::Stopped),
                Err(TryRecvError::Empty) => loop {
                    match wait_for(batches) {
                        Ok(Wait::Interrupted) => continue,
                        other => break other,
                    }
                },
            },
        };
        self.in_use.set(false);
        let rows = match received? {
            Wait::Batch(batch) => batch.map_err(|e| unsupported(ruby, e))?,
            Wait::Stopped | Wait::Interrupted => {
                return Err(unsupported(
                    ruby,
                    Unsupported("the reader thread stopped".into()),
                ));
            }
        };
        if rows.is_empty() {
            self.done.set(true);
        }
        Ok(rows)
    }

    /// Stops the reading thread, which closes the file and frees the rows it
    /// read ahead, without waiting for garbage collection.
    fn close(ruby: &Ruby, rb_self: &Self) -> Result<(), Error> {
        if rb_self.in_use.get() {
            return Err(Error::new(
                ruby.exception_runtime_error(),
                "reader is already in use",
            ));
        }
        rb_self.batches.borrow_mut().take();
        rb_self.pending.replace(Vec::new().into_iter());
        rb_self.done.set(true);
        Ok(())
    }

    /// Document mode: adds the next batch of rows to `cells`. Returns
    /// `false` once the sheet is exhausted.
    fn fill_cells(
        ruby: &Ruby,
        rb_self: &Self,
        cells: RHash,
        converter: &Converter,
    ) -> Result<bool, Error> {
        let rows = rb_self.read_batch(ruby)?;
        for row in &rows {
            let row_number = rb_self.rows_read.get() + 1;
            rb_self.rows_read.set(row_number);
            converter.add_row(ruby, cells, row, row_number)?;
        }
        Ok(!rows.is_empty())
    }

    /// Stream mode: the next row of the sheet, `None` at its end. Rows wait
    /// in their batch until asked for, so Ruby holds one at a time.
    fn next_raw_row(&self, ruby: &Ruby) -> Result<Option<RawRow>, Error> {
        if let Some(row) = self.pending.borrow_mut().next() {
            return Ok(Some(row));
        }
        let mut rows = self.read_batch(ruby)?.into_iter();
        let row = rows.next();
        self.pending.replace(rows);
        Ok(row)
    }

    /// Stream mode: the next row, or `nil` at the end of the sheet.
    fn next_row(ruby: &Ruby, rb_self: &Self) -> Result<Option<Row>, Error> {
        Ok(rb_self.next_raw_row(ruby)?.map(Row))
    }

    /// Stream mode: the values of the next row, as
    /// [`Converter::row_values`] gives them, or `nil` at the end of the
    /// sheet. Each row is converted only when asked for, as roo reads it,
    /// and its parsed record is freed right after.
    fn next_values(
        ruby: &Ruby,
        rb_self: &Self,
        converter: &Converter,
    ) -> Result<Option<RArray>, Error> {
        match rb_self.next_raw_row(ruby)? {
            Some(row) => converter.row_values(ruby, &row).map(Some),
            None => Ok(None),
        }
    }
}

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let native = ruby
        .define_module("Roo")?
        .define_module("Calamine")?
        .define_module("Native")?;

    let reader = native.define_class("Reader", ruby.class_object())?;
    reader.define_singleton_method("new", function!(Reader::new, 3))?;
    reader.define_method("fill_cells", method!(Reader::fill_cells, 2))?;
    reader.define_method("next_row", method!(Reader::next_row, 0))?;
    reader.define_method("next_values", method!(Reader::next_values, 1))?;
    reader.define_method("close", method!(Reader::close, 0))?;

    native.define_class("Row", ruby.class_object())?;

    let converter = native.define_class("Converter", ruby.class_object())?;
    converter.define_singleton_method("new", function!(Converter::new, 2))?;
    converter.define_method("row_cells", method!(Converter::row_cells, 2))?;

    store::define(ruby, native)?;
    shared_strings::define(native)?;
    sheet_parts::define(native)?;
    archive::define(ruby, native)?;
    Ok(())
}

#[cfg(test)]
mod reader_tests {
    use crate::reader::*;
    use std::io::BufReader;
    mod cases {
        include!("../calamine/src/roo/tests.rs");
    }
}
