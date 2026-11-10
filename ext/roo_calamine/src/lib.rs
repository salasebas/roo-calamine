//! Native fast path for `roo-calamine`.
//!
//! [`reader`] turns worksheet XML into raw cell records without touching Ruby,
//! with the GVL released. [`convert`] turns those records into roo's cells.

pub mod convert;
pub mod reader;

use std::cell::{Cell as StdCell, RefCell};
use std::ffi::c_void;
use std::fs::File;
use std::io::BufReader;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;

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

#[magnus::wrap(class = "Roo::Calamine::Native::Reader", free_immediately, size)]
struct Reader {
    sheet: RefCell<SheetReader<BufReader<File>>>,
    rows_read: StdCell<usize>,
}

impl Reader {
    fn new(ruby: &Ruby, path: String, stream: bool) -> Result<Self, Error> {
        let mode = if stream { Mode::Stream } else { Mode::Document };
        let sheet = SheetReader::open(Path::new(&path), mode).map_err(|e| unsupported(ruby, e))?;
        Ok(Self {
            sheet: RefCell::new(sheet),
            rows_read: StdCell::new(0),
        })
    }

    fn read_batch(&self, ruby: &Ruby, cells: usize) -> Result<Vec<RawRow>, Error> {
        let mut sheet = self
            .sheet
            .try_borrow_mut()
            .map_err(|_| Error::new(ruby.exception_runtime_error(), "reader is already in use"))?;
        without_gvl(|| sheet.next_rows(cells)).map_err(|e| unsupported(ruby, e))
    }

    /// Document mode: adds the next rows, about `batch` cells, to `cells`.
    /// Returns `false` once the sheet is exhausted.
    fn fill_cells(
        ruby: &Ruby,
        rb_self: &Self,
        cells: RHash,
        converter: &Converter,
        batch: usize,
    ) -> Result<bool, Error> {
        let rows = rb_self.read_batch(ruby, batch)?;
        for row in &rows {
            let row_number = rb_self.rows_read.get() + 1;
            rb_self.rows_read.set(row_number);
            converter.add_row(ruby, cells, row, row_number)?;
        }
        Ok(!rows.is_empty())
    }

    /// Stream mode: the next rows, about `batch` cells, or `nil` at the end.
    fn next_rows(ruby: &Ruby, rb_self: &Self, batch: usize) -> Result<Option<RArray>, Error> {
        let rows = rb_self.read_batch(ruby, batch)?;
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
    reader.define_singleton_method("new", function!(Reader::new, 2))?;
    reader.define_method("fill_cells", method!(Reader::fill_cells, 3))?;
    reader.define_method("next_rows", method!(Reader::next_rows, 1))?;

    native.define_class("Row", ruby.class_object())?;

    let converter = native.define_class("Converter", ruby.class_object())?;
    converter.define_singleton_method("new", function!(Converter::new, 2))?;
    converter.define_method("row_cells", method!(Converter::row_cells, 1))?;
    Ok(())
}
