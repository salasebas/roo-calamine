//! Native fast path for `roo-calamine`.
//!
//! [`reader`] turns worksheet XML into raw cell records without touching Ruby,
//! on a thread of its own. [`convert`] turns those records into roo's cells,
//! and [`store`] keeps them to build one cell at a time.

pub mod convert;
pub mod numeric;
pub mod reader;
pub mod store;

use std::cell::{Cell as StdCell, RefCell};
use std::ffi::c_void;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::time::Duration;

use magnus::value::Lazy;
use magnus::{function, method, prelude::*, Error, ExceptionClass, RArray, RHash, Ruby};

use convert::Converter;
use reader::{Mode, RawRow, SheetReader, Unsupported};
use store::Store;

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

/// Reads a worksheet on its own thread, one batch ahead of Ruby: while Ruby
/// turns one batch into cells, the thread parses the next, without the GVL.
/// Ruby releases the GVL only when it has to wait for a batch, so a busy Ruby
/// thread elsewhere does not slow every batch down.
#[magnus::wrap(class = "Roo::Calamine::Native::Reader", free_immediately, size)]
struct Reader {
    /// `None` once closed.
    batches: RefCell<Option<Receiver<Batch>>>,
    /// The rows of the last batch that [`Self::next_values`] has not
    /// converted yet.
    unconverted: RefCell<std::vec::IntoIter<RawRow>>,
    /// Set while a Ruby thread waits on `batches` without the GVL; the
    /// receiver must not be used from two threads at once.
    in_use: StdCell<bool>,
    done: StdCell<bool>,
    rows_read: StdCell<usize>,
}

impl Reader {
    /// `batch` is the number of cells parsed per batch.
    fn new(ruby: &Ruby, path: String, stream: bool, batch: usize) -> Result<Self, Error> {
        let mode = if stream { Mode::Stream } else { Mode::Document };
        let mut sheet =
            SheetReader::open(Path::new(&path), mode).map_err(|e| unsupported(ruby, e))?;
        let (sender, batches) = mpsc::sync_channel::<Batch>(1);
        std::thread::Builder::new()
            .name("roo-calamine".into())
            .spawn(move || loop {
                let rows = sheet.next_rows(batch);
                let last = !matches!(&rows, Ok(rows) if !rows.is_empty());
                // A send fails once the Reader is garbage collected.
                if sender.send(rows).is_err() || last {
                    return;
                }
            })
            .map_err(|e| Error::new(ruby.exception_runtime_error(), e.to_string()))?;
        Ok(Self {
            batches: RefCell::new(Some(batches)),
            unconverted: RefCell::new(Vec::new().into_iter()),
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
        let received = match self.batches.borrow().as_ref() {
            None => Ok(Wait::Batch(Ok(Vec::new()))),
            Some(batches) => match batches.try_recv() {
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
        rb_self.unconverted.replace(Vec::new().into_iter());
        rb_self.done.set(true);
        Ok(())
    }

    /// Document mode: adds the next batch of rows to `cells`, reusing the
    /// cells `store` (a `Native::Store`, or `nil`) built. Returns `false`
    /// once the sheet is exhausted.
    fn fill_cells(
        ruby: &Ruby,
        rb_self: &Self,
        cells: RHash,
        converter: &Converter,
        store: Option<&Store>,
    ) -> Result<bool, Error> {
        let rows = rb_self.read_batch(ruby)?;
        for row in &rows {
            let row_number = rb_self.rows_read.get() + 1;
            rb_self.rows_read.set(row_number);
            converter.add_row(ruby, cells, row, row_number, store)?;
        }
        Ok(!rows.is_empty())
    }

    /// Stream mode: the next batch of rows, or `nil` at the end of the sheet.
    fn next_rows(ruby: &Ruby, rb_self: &Self) -> Result<Option<RArray>, Error> {
        let rows = rb_self.read_batch(ruby)?;
        if rows.is_empty() {
            return Ok(None);
        }
        let out = ruby.ary_new_capa(rows.len());
        for row in rows {
            out.push(Row(row))?;
        }
        Ok(Some(out))
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
        let next = rb_self.unconverted.borrow_mut().next();
        let row = match next {
            Some(row) => row,
            None => {
                let mut rows = rb_self.read_batch(ruby)?.into_iter();
                let Some(row) = rows.next() else {
                    return Ok(None);
                };
                rb_self.unconverted.replace(rows);
                row
            }
        };
        converter.row_values(ruby, &row).map(Some)
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
    reader.define_method("fill_cells", method!(Reader::fill_cells, 3))?;
    reader.define_method("next_rows", method!(Reader::next_rows, 0))?;
    reader.define_method("next_values", method!(Reader::next_values, 1))?;
    reader.define_method("close", method!(Reader::close, 0))?;

    native.define_class("Row", ruby.class_object())?;

    let converter = native.define_class("Converter", ruby.class_object())?;
    converter.define_singleton_method("new", function!(Converter::new, 2))?;
    converter.define_method("row_cells", method!(Converter::row_cells, 1))?;

    store::define(ruby, native)?;
    Ok(())
}
