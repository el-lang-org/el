//! Recoverable v1 process and byte-I/O semantics behind a backend-neutral boundary.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ErrorKind {
    NotFound,
    PermissionDenied,
    AlreadyExists,
    InvalidInput,
    IsDirectory,
    NotDirectory,
    Closed,
    BrokenPipe,
    OutOfSpace,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum IoOperation {
    OpenRead,
    Create,
    Append,
    Read,
    Write,
    Flush,
    Close,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct FileError {
    operation: IoOperation,
    kind: ErrorKind,
    code: Option<i64>,
}

impl FileError {
    #[must_use]
    pub const fn operation(&self) -> IoOperation {
        self.operation
    }
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }
    #[must_use]
    pub const fn code(&self) -> Option<i64> {
        self.code
    }
    fn closed(operation: IoOperation) -> Self {
        Self {
            operation,
            kind: ErrorKind::Closed,
            code: None,
        }
    }
    fn from_io(operation: IoOperation, error: &io::Error) -> Self {
        Self {
            operation,
            kind: map_error_kind(error),
            code: error.raw_os_error().map(i64::from),
        }
    }
}

impl fmt::Display for FileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "file {:?} failed: {:?}",
            self.operation, self.kind
        )?;
        if let Some(code) = self.code {
            write!(formatter, " (system code {code})")?;
        }
        Ok(())
    }
}

impl std::error::Error for FileError {}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct IoError {
    operation: IoOperation,
    kind: ErrorKind,
    code: Option<i64>,
}

impl IoError {
    #[must_use]
    pub const fn operation(&self) -> IoOperation {
        self.operation
    }
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }
    #[must_use]
    pub const fn code(&self) -> Option<i64> {
        self.code
    }
    fn from_io(operation: IoOperation, error: &io::Error) -> Self {
        Self {
            operation,
            kind: map_error_kind(error),
            code: error.raw_os_error().map(i64::from),
        }
    }
}

impl fmt::Display for IoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "I/O {:?} failed: {:?}",
            self.operation, self.kind
        )
    }
}

impl std::error::Error for IoError {}

fn map_error_kind(error: &io::Error) -> ErrorKind {
    match error.kind() {
        io::ErrorKind::NotFound => ErrorKind::NotFound,
        io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
        io::ErrorKind::AlreadyExists => ErrorKind::AlreadyExists,
        io::ErrorKind::InvalidInput | io::ErrorKind::InvalidFilename => ErrorKind::InvalidInput,
        io::ErrorKind::IsADirectory => ErrorKind::IsDirectory,
        io::ErrorKind::NotADirectory => ErrorKind::NotDirectory,
        io::ErrorKind::BrokenPipe => ErrorKind::BrokenPipe,
        io::ErrorKind::StorageFull => ErrorKind::OutOfSpace,
        _ => ErrorKind::Other,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReadResult<E> {
    Data(Vec<u8>),
    Eof,
    Error(E),
}

pub trait Reader {
    type Error;
    fn read(&self, max_bytes: usize) -> ReadResult<Self::Error>;
}
pub trait Writer {
    type Error;
    fn write(&self, data: &[u8]) -> Result<(), Self::Error>;
    fn flush(&self) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Stdin;
#[derive(Clone, Copy, Debug, Default)]
pub struct Stdout;
#[derive(Clone, Copy, Debug, Default)]
pub struct Stderr;

#[must_use]
pub const fn stdin() -> Stdin {
    Stdin
}
#[must_use]
pub const fn stdout() -> Stdout {
    Stdout
}
#[must_use]
pub const fn stderr() -> Stderr {
    Stderr
}

impl Reader for Stdin {
    type Error = IoError;
    fn read(&self, max_bytes: usize) -> ReadResult<Self::Error> {
        if max_bytes == 0 {
            return ReadResult::Data(Vec::new());
        }
        let mut buffer = vec![0; max_bytes];
        match retry_io(|| io::stdin().lock().read(&mut buffer)) {
            Ok(0) => ReadResult::Eof,
            Ok(length) => {
                buffer.truncate(length);
                ReadResult::Data(buffer)
            }
            Err(error) => ReadResult::Error(IoError::from_io(IoOperation::Read, &error)),
        }
    }
}

macro_rules! standard_writer {
    ($type:ty, $stream:expr) => {
        impl Writer for $type {
            type Error = IoError;
            fn write(&self, data: &[u8]) -> Result<(), Self::Error> {
                retry_io(|| $stream.lock().write_all(data))
                    .map_err(|error| IoError::from_io(IoOperation::Write, &error))
            }
            fn flush(&self) -> Result<(), Self::Error> {
                retry_io(|| $stream.lock().flush())
                    .map_err(|error| IoError::from_io(IoOperation::Flush, &error))
            }
        }
    };
}

standard_writer!(Stdout, io::stdout());
standard_writer!(Stderr, io::stderr());

#[derive(Debug)]
struct SharedFile(Mutex<Option<File>>);

#[derive(Clone, Debug)]
pub struct FileReader(Arc<SharedFile>);
#[derive(Clone, Debug)]
pub struct FileWriter(Arc<SharedFile>);

impl FileReader {
    pub fn close(&self) -> Result<(), FileError> {
        close_shared(&self.0)
    }
}
impl FileWriter {
    pub fn close(&self) -> Result<(), FileError> {
        close_shared(&self.0)
    }
}

pub enum FileStream<'a> {
    Reader(&'a FileReader),
    Writer(&'a FileWriter),
}
impl FileStream<'_> {
    pub fn close(self) -> Result<(), FileError> {
        match self {
            Self::Reader(value) => value.close(),
            Self::Writer(value) => value.close(),
        }
    }
}

pub fn open_file(path: &str) -> Result<FileReader, FileError> {
    validate_path(path, IoOperation::OpenRead)?;
    retry_io(|| File::open(Path::new(path)))
        .map(|file| FileReader(Arc::new(SharedFile(Mutex::new(Some(file))))))
        .map_err(|error| FileError::from_io(IoOperation::OpenRead, &error))
}

pub fn create_file(path: &str) -> Result<FileWriter, FileError> {
    validate_path(path, IoOperation::Create)?;
    retry_io(|| File::create(Path::new(path)))
        .map(|file| FileWriter(Arc::new(SharedFile(Mutex::new(Some(file))))))
        .map_err(|error| FileError::from_io(IoOperation::Create, &error))
}

pub fn append_file(path: &str) -> Result<FileWriter, FileError> {
    validate_path(path, IoOperation::Append)?;
    retry_io(|| {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(Path::new(path))
    })
    .map(|file| FileWriter(Arc::new(SharedFile(Mutex::new(Some(file))))))
    .map_err(|error| FileError::from_io(IoOperation::Append, &error))
}

fn validate_path(path: &str, operation: IoOperation) -> Result<(), FileError> {
    if path.contains('\0') {
        Err(FileError {
            operation,
            kind: ErrorKind::InvalidInput,
            code: None,
        })
    } else {
        Ok(())
    }
}

fn close_shared(shared: &Arc<SharedFile>) -> Result<(), FileError> {
    let mut guard = shared
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if guard.take().is_some() {
        Ok(())
    } else {
        Err(FileError::closed(IoOperation::Close))
    }
}

impl Reader for FileReader {
    type Error = FileError;
    fn read(&self, max_bytes: usize) -> ReadResult<Self::Error> {
        if max_bytes == 0 {
            return ReadResult::Data(Vec::new());
        }
        let mut guard = self
            .0
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(file) = guard.as_mut() else {
            return ReadResult::Error(FileError::closed(IoOperation::Read));
        };
        let mut buffer = vec![0; max_bytes];
        match retry_io(|| file.read(&mut buffer)) {
            Ok(0) => ReadResult::Eof,
            Ok(length) => {
                buffer.truncate(length);
                ReadResult::Data(buffer)
            }
            Err(error) => ReadResult::Error(FileError::from_io(IoOperation::Read, &error)),
        }
    }
}

impl Writer for FileWriter {
    type Error = FileError;
    fn write(&self, data: &[u8]) -> Result<(), Self::Error> {
        let mut guard = self
            .0
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(file) = guard.as_mut() else {
            return Err(FileError::closed(IoOperation::Write));
        };
        retry_io(|| file.write_all(data))
            .map_err(|error| FileError::from_io(IoOperation::Write, &error))
    }
    fn flush(&self) -> Result<(), Self::Error> {
        let mut guard = self
            .0
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(file) = guard.as_mut() else {
            return Err(FileError::closed(IoOperation::Flush));
        };
        retry_io(|| file.flush()).map_err(|error| FileError::from_io(IoOperation::Flush, &error))
    }
}

fn retry_io<T>(mut operation: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    loop {
        match operation() {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => return result,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessSnapshot {
    arguments: Result<Vec<String>, usize>,
    environment: Vec<(OsString, OsString)>,
}

impl ProcessSnapshot {
    #[must_use]
    pub fn capture() -> Self {
        Self::from_native(std::env::args_os().skip(1), std::env::vars_os())
    }

    pub fn from_native(
        arguments: impl IntoIterator<Item = OsString>,
        environment: impl IntoIterator<Item = (OsString, OsString)>,
    ) -> Self {
        let arguments = arguments
            .into_iter()
            .enumerate()
            .map(|(index, value)| strict_native_text(&value).map_err(|()| index))
            .collect();
        Self {
            arguments,
            environment: environment.into_iter().collect(),
        }
    }

    pub fn arguments(&self) -> Result<Vec<String>, usize> {
        self.arguments.clone()
    }

    pub fn get_env(&self, name: &str) -> Result<Option<String>, ProcessTextError> {
        if name.contains(['\0', '=']) {
            return Err(ProcessTextError::InvalidName);
        }
        let requested = OsStr::new(name);
        let value = self
            .environment
            .iter()
            .find(|(candidate, _)| environment_names_equal(candidate, requested))
            .map(|(_, value)| value);
        value
            .map(|value| strict_native_text(value.as_os_str()))
            .transpose()
            .map_err(|()| ProcessTextError::InvalidText)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessTextError {
    InvalidName,
    InvalidText,
}

fn strict_native_text(value: &OsStr) -> Result<String, ()> {
    value.to_str().map(str::to_owned).ok_or(())
}
#[cfg(not(windows))]
fn environment_names_equal(left: &OsStr, right: &OsStr) -> bool {
    left == right
}
#[cfg(windows)]
fn environment_names_equal(left: &OsStr, right: &OsStr) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn path() -> String {
        std::env::temp_dir()
            .join(format!(
                "el-io-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ))
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn file_aliases_share_position_and_close_state() {
        let path = path();
        let writer = create_file(&path).unwrap();
        let alias = writer.clone();
        writer.write(b"abc").unwrap();
        alias.write(b"def").unwrap();
        writer.close().unwrap();
        assert_eq!(alias.write(b"x").unwrap_err().kind(), ErrorKind::Closed);
        assert_eq!(alias.close().unwrap_err().operation(), IoOperation::Close);
        let reader = open_file(&path).unwrap();
        assert_eq!(reader.read(4), ReadResult::Data(b"abcd".to_vec()));
        assert_eq!(reader.read(4), ReadResult::Data(b"ef".to_vec()));
        assert_eq!(reader.read(4), ReadResult::Eof);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn embedded_nul_is_stable_invalid_input() {
        let error = open_file("bad\0path").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidInput);
        assert_eq!(error.code(), None);
    }
    #[test]
    fn process_snapshot_excludes_nothing_supplied_and_is_immutable() {
        let snapshot = ProcessSnapshot::from_native(
            [OsString::from("one"), OsString::from("two")],
            [(OsString::from("A"), OsString::from("before"))],
        );
        assert_eq!(snapshot.arguments().unwrap(), ["one", "two"]);
        assert_eq!(snapshot.get_env("A"), Ok(Some("before".to_owned())));
        assert_eq!(snapshot.get_env("A=B"), Err(ProcessTextError::InvalidName));
    }
}
