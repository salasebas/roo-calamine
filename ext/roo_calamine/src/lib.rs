//! Native fast path for `roo-calamine`.
//!
//! [`reader`] turns worksheet XML into raw cell records without touching Ruby,
//! on a thread of its own. [`convert`] turns those records into roo's cells.

pub mod convert;
pub mod reader;

use std::cell::Cell as StdCell;
use std::ffi::c_void;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use magnus::value::Lazy;
use magnus::{function, method, prelude::*, Error, ExceptionClass, RArray, RHash, Ruby};

use convert::Converter;
use reader::{Mode, RawRow, SheetReader, Unsupported};

static UNSUPPORTED: Lazy<ExceptionClass> = Lazy::new(|r| {
    r.eval("Roo::Calamine::UnsupportedError")
        .expect("roo/calamine defines its error before the extension loads")
});

fn unsupported(ruby: &Ruby, error: Unsupported) -> Error {
    Error::new(ruby.get_inner(&UNSUPPORTED), error.0)
}

/// Runs `work` with the GVL released. `work` must not touch any Ruby object.
fn without_gvl<F: FnOnce() -> R, R>(work: F) -> R {
    struct Call<F, R> {
        work: Option<F>,
        result: Option<std::thread::Result<R>>,
    }

    unsafe extern "C" fn trampoline<F: FnOnce() -> R, R>(data: *mut c_void) -> *mut c_void {
        let call = unsafe { &mut *(data as *mut Call<F, R>) };
        let work = call.work.take().expect("called once");
        call.result = Some(panic::catch_unwind(AssertUnwindSafe(work)));
        std::ptr::null_mut()
    }

    let mut call = Call {
        work: Some(work),
        result: None,
    };
    unsafe {
        rb_sys::rb_thread_call_without_gvl(
            Some(trampoline::<F, R>),
            &mut call as *mut _ as *mut c_void,
            None,
            std::ptr::null_mut(),
        );
    }
    match call.result.expect("trampoline ran") {
        Ok(result) => result,
        Err(payload) => panic::resume_unwind(payload),
    }
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
    batches: Receiver<Batch>,
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
            batches,
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
        let received = match self.batches.try_recv() {
            Ok(batch) => Ok(batch),
            Err(TryRecvError::Empty) => {
                without_gvl(|| self.batches.recv()).map_err(|_| TryRecvError::Disconnected)
            }
            Err(error) => Err(error),
        };
        self.in_use.set(false);
        let rows = received
            .map_err(|_| unsupported(ruby, Unsupported("the reader thread stopped".into())))?
            .map_err(|e| unsupported(ruby, e))?;
        if rows.is_empty() {
            self.done.set(true);
        }
        Ok(rows)
    }

    /// Document mode: adds the next batch of rows to `cells`. Returns `false`
    /// once the sheet is exhausted.
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
    reader.define_method("next_rows", method!(Reader::next_rows, 0))?;

    native.define_class("Row", ruby.class_object())?;

    let converter = native.define_class("Converter", ruby.class_object())?;
    converter.define_singleton_method("new", function!(Converter::new, 2))?;
    converter.define_method("row_cells", method!(Converter::row_cells, 1))?;
    Ok(())
}
