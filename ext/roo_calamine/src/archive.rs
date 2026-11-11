//! Worksheet XML read straight from the workbook's zip, inflated on a thread
//! of its own, so roo does not have to extract it to a file first.
//!
//! rubyzip has already read the central directory: Ruby passes each entry's
//! local header offset, sizes, and compression method, so this reads the
//! same entry rubyzip would extract, and only the zip format's local header
//! and the entry's data are parsed here.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};

use flate2::read::DeflateDecoder;
use magnus::{function, method, prelude::*, Error, RModule, Ruby, TryConvert, Value};

use crate::reader::{Mode, SheetReader, Unsupported};
use crate::{unsupported, without_gvl};

const LOCAL_HEADER_SIGNATURE: u32 = 0x0403_4b50;
const LOCAL_HEADER_LENGTH: usize = 30;
const STORED: u16 = 0;
const DEFLATED: u16 = 8;
/// The general purpose flag of an encrypted entry.
const ENCRYPTED: u16 = 1;
const ZIP64_EXTRA: u16 = 1;
/// Bytes per inflated chunk handed from the inflating thread to the parser.
const CHUNK: usize = 256 * 1024;
/// Chunks the inflating thread may run ahead of the parser.
const CHUNKS_AHEAD: usize = 4;

/// One file in the zip at `zip`, which each read opens.
#[derive(Clone)]
#[magnus::wrap(class = "Roo::Calamine::Native::ZipEntry", free_immediately, size)]
pub struct ZipEntry {
    zip: PathBuf,
    offset: u64,
    compressed_size: u64,
    size: u64,
    deflated: bool,
}

impl ZipEntry {
    /// `Native::ZipEntry.new(zip, offset, compressed_size, size, method)`:
    /// the entry rubyzip describes as `local_header_offset`,
    /// `compressed_size`, `size`, and `compression_method`.
    fn new(
        ruby: &Ruby,
        zip: PathBuf,
        offset: u64,
        compressed_size: u64,
        size: u64,
        method: u16,
    ) -> Result<Self, Error> {
        if method != STORED && method != DEFLATED {
            return Err(Error::new(
                ruby.exception_arg_error(),
                format!("unsupported compression method {method}"),
            ));
        }
        Ok(Self {
            zip,
            offset,
            compressed_size,
            size,
            deflated: method == DEFLATED,
        })
    }

    /// The entry's data as stored, after its local header. Fails where the
    /// local header disagrees with the central directory on what rubyzip
    /// reads from it: the encryption flag, the compression method, and a
    /// stored entry's size.
    fn data(&self) -> io::Result<impl Read> {
        let mut file = File::open(&self.zip)?;
        file.seek(SeekFrom::Start(self.offset))?;
        let mut header = [0; LOCAL_HEADER_LENGTH];
        file.read_exact(&mut header)?;
        let field = |at: usize| u16::from_le_bytes([header[at], header[at + 1]]);
        let long = |at: usize| {
            u32::from_le_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]])
        };
        if long(0) != LOCAL_HEADER_SIGNATURE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "no local header at the entry's offset",
            ));
        }
        file.seek(SeekFrom::Current(i64::from(field(26))))?;
        let mut extra = vec![0; usize::from(field(28))];
        file.read_exact(&mut extra)?;
        let method = if self.deflated { DEFLATED } else { STORED };
        if field(6) & ENCRYPTED != 0
            || field(8) != method
            || (!self.deflated && local_size(long(22), &extra) != Some(self.size))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the local header disagrees with the central directory",
            ));
        }
        Ok(file.take(self.compressed_size))
    }

    /// The entry's inflated bytes. Fails once more than `size` bytes come
    /// out, where rubyzip raises or warns (`Zip.validate_entry_sizes`).
    fn inflated(&self) -> io::Result<impl Read> {
        let data = self.data()?;
        let bytes: Box<dyn Read + Send> = if self.deflated {
            Box::new(DeflateDecoder::new(data))
        } else {
            Box::new(data)
        };
        Ok(SizeLimited {
            inner: bytes,
            left: self.size,
        })
    }

    /// The inflated bytes, inflated on a thread of its own ahead of the
    /// reader.
    fn inflating(&self) -> Inflating {
        let (sender, chunks) = mpsc::sync_channel(CHUNKS_AHEAD);
        let (spent, reusable) = mpsc::channel::<Vec<u8>>();
        let entry = self.clone();
        let spawned = std::thread::Builder::new()
            .name("roo-calamine-inflate".into())
            .spawn(move || {
                let mut bytes = match entry.inflated() {
                    Ok(bytes) => bytes,
                    Err(e) => return drop(sender.send(Err(e))),
                };
                loop {
                    let mut chunk = reusable.try_recv().unwrap_or_default();
                    chunk.resize(CHUNK, 0);
                    let read = read_full(&mut bytes, &mut chunk).map(|n| {
                        chunk.truncate(n);
                        chunk
                    });
                    let last = !matches!(&read, Ok(chunk) if chunk.len() == CHUNK);
                    let reader_dropped = sender.send(read).is_err();
                    if reader_dropped || last {
                        return;
                    }
                }
            });
        Inflating {
            chunks: spawned.ok().map(|_| chunks),
            spent,
            chunk: Vec::new(),
            at: 0,
            ended: false,
        }
    }

    /// Writes the inflated entry to `path`, as rubyzip's `Zip::Entry#extract`
    /// does, with the GVL released. Fails without touching a file already at
    /// `path`; a file that fails part way is removed, so rubyzip can extract
    /// it instead.
    fn extract(ruby: &Ruby, rb_self: &Self, path: PathBuf) -> Result<(), Error> {
        let entry = rb_self.clone();
        let written = without_gvl(move |interrupted| -> io::Result<()> {
            let mut bytes = entry.inflated()?;
            let mut file = File::create_new(&path)?;
            let written = (|| {
                let mut chunk = vec![0; CHUNK];
                loop {
                    if interrupted.load(Ordering::Acquire) {
                        return Err(io::Error::from(io::ErrorKind::Interrupted));
                    }
                    let n = read_full(&mut bytes, &mut chunk)?;
                    file.write_all(&chunk[..n])?;
                    if n < CHUNK {
                        return Ok(());
                    }
                }
            })();
            if written.is_err() {
                let _ = std::fs::remove_file(&path);
            }
            written
        })?;
        written.map_err(|e| Error::new(ruby.exception_io_error(), e.to_string()))
    }
}

/// The uncompressed size a local header gives: its 32-bit field, or, when
/// that is `u32::MAX`, the first value of its Zip64 extra field.
fn local_size(field: u32, extra: &[u8]) -> Option<u64> {
    if field != u32::MAX {
        return Some(u64::from(field));
    }
    let mut rest = extra;
    while let [a, b, c, d, data @ ..] = rest {
        let (id, length) = (
            u16::from_le_bytes([*a, *b]),
            usize::from(u16::from_le_bytes([*c, *d])),
        );
        let data = data.get(..length)?;
        if id == ZIP64_EXTRA {
            return Some(u64::from_le_bytes(data.get(..8)?.try_into().ok()?));
        }
        rest = &rest[4 + length..];
    }
    None
}

/// Reads until `buf` is full or the input ends.
fn read_full(input: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match input.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

struct SizeLimited<R> {
    inner: R,
    left: u64,
}

impl<R: Read> Read for SizeLimited<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.left = self.left.checked_sub(n as u64).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "the entry inflates to more bytes than its size",
            )
        })?;
        Ok(n)
    }
}

/// The chunks [`ZipEntry::inflating`] inflates, as one input.
pub struct Inflating {
    /// `None` when the thread could not start.
    chunks: Option<Receiver<io::Result<Vec<u8>>>>,
    /// Read chunks, back to the thread to fill again.
    spent: Sender<Vec<u8>>,
    chunk: Vec<u8>,
    at: usize,
    ended: bool,
}

impl Read for Inflating {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let available = self.fill_buf()?;
        let n = available.len().min(buf.len());
        buf[..n].copy_from_slice(&available[..n]);
        self.consume(n);
        Ok(n)
    }
}

impl BufRead for Inflating {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.at == self.chunk.len() && !self.ended {
            let chunks = self
                .chunks
                .as_ref()
                .ok_or_else(|| io::Error::other("the inflating thread did not start"))?;
            let chunk = chunks
                .recv()
                .map_err(|_| io::Error::other("the inflating thread stopped"))??;
            self.ended = chunk.len() < CHUNK;
            let _thread_done = self.spent.send(std::mem::replace(&mut self.chunk, chunk));
            self.at = 0;
        }
        Ok(&self.chunk[self.at..])
    }

    fn consume(&mut self, amount: usize) {
        self.at += amount;
    }
}

/// What a worksheet's XML is read from.
pub enum Input {
    File(BufReader<File>),
    Zip(Inflating),
}

impl Read for Input {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::File(file) => file.read(buf),
            Self::Zip(zip) => zip.read(buf),
        }
    }
}

impl BufRead for Input {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        match self {
            Self::File(file) => file.fill_buf(),
            Self::Zip(zip) => zip.fill_buf(),
        }
    }

    fn consume(&mut self, amount: usize) {
        match self {
            Self::File(file) => file.consume(amount),
            Self::Zip(zip) => zip.consume(amount),
        }
    }
}

/// A worksheet: the path of its extracted file, or a `Native::ZipEntry`.
pub enum Source {
    Path(PathBuf),
    Entry(ZipEntry),
}

impl TryConvert for Source {
    fn try_convert(value: Value) -> Result<Self, Error> {
        match <&ZipEntry>::try_convert(value) {
            Ok(entry) => Ok(Self::Entry(entry.clone())),
            Err(_) => Ok(Self::Path(PathBuf::try_convert(value)?)),
        }
    }
}

impl Source {
    pub fn input(&self, ruby: &Ruby) -> Result<Input, Error> {
        Ok(match self {
            Self::Path(path) => Input::File(BufReader::with_capacity(
                64 * 1024,
                File::open(path).map_err(|e| unsupported(ruby, Unsupported(e.to_string())))?,
            )),
            Self::Entry(entry) => Input::Zip(entry.inflating()),
        })
    }

    pub fn open(&self, ruby: &Ruby, mode: Mode) -> Result<SheetReader<Input>, Error> {
        Ok(SheetReader::new(self.input(ruby)?, mode))
    }
}

pub fn define(ruby: &Ruby, native: RModule) -> Result<(), Error> {
    let entry = native.define_class("ZipEntry", ruby.class_object())?;
    entry.define_singleton_method("new", function!(ZipEntry::new, 5))?;
    entry.define_method("extract", method!(ZipEntry::extract, 1))?;
    Ok(())
}
