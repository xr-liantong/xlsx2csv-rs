//! Error types. Messages mirror upstream where upstream prints them.

use std::fmt;
use std::io;

#[derive(Debug)]
pub enum PortError {
    /// zip container could not be opened / entry missing.
    Io(io::Error),
    /// zip archive invalid.
    Zip(zip::result::ZipError),
    /// XML not well formed.
    Xml(String),
    /// upstream `XlsxValueError("Error: potential invalid date format.")`
    Value,
    /// upstream `XlsxValueError("unknown format %s at %d")`
    UnknownFormat(String),
    /// upstream `Sheet '<name>' not found`
    SheetNameNotFound(String),
    /// upstream `Sheet %i not found`
    SheetIndexNotFound(u32),
    /// upstream `Sheet with index %i not found or can't be handled`
    SheetIndexUnhandled(u32),
    /// upstream `Invalid xlsx file: <path>`
    InvalidXlsx(String),
    /// upstream `File <path> already exists!`
    OutFileExists(String),
    /// upstream `Input file not found!`
    InputNotFound,
    /// a feature this build does not implement yet.
    Unsupported(&'static str),
    /// a `-I` / `-E` pattern outside the supported `re` subset.
    Pattern(String),
    /// a merged follower that precedes its top-left cell (upstream: KeyError).
    Merge(String),
}

impl PortError {
    pub fn value_error() -> Self {
        PortError::Value
    }

    /// Process exit code. Upstream exits 1 for every raised `XlsxException`
    /// (argparse usage errors are 2 and are produced in `cli`).
    pub fn exit_code(&self) -> i32 {
        1
    }
}

impl fmt::Display for PortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PortError::Io(e) => write!(f, "{e}"),
            PortError::Zip(e) => write!(f, "{e}"),
            PortError::Xml(m) => write!(f, "{m}"),
            PortError::Value => write!(f, "Error: potential invalid date format."),
            PortError::UnknownFormat(m) => write!(f, "{m}"),
            PortError::SheetNameNotFound(n) => write!(f, "Sheet '{n}' not found"),
            PortError::SheetIndexNotFound(i) => write!(f, "Sheet {i} not found"),
            PortError::SheetIndexUnhandled(i) => {
                write!(f, "Sheet with index {i} not found or can't be handled")
            }
            PortError::InvalidXlsx(p) => write!(f, "Invalid xlsx file: {p}"),
            PortError::OutFileExists(p) => write!(f, "File {p} already exists!"),
            PortError::InputNotFound => write!(f, "Input file not found!"),
            PortError::Unsupported(m) => write!(f, "unsupported: {m}"),
            PortError::Pattern(m) => write!(f, "{m}"),
            PortError::Merge(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for PortError {}

impl From<io::Error> for PortError {
    fn from(e: io::Error) -> Self {
        PortError::Io(e)
    }
}

impl From<zip::result::ZipError> for PortError {
    fn from(e: zip::result::ZipError) -> Self {
        PortError::Zip(e)
    }
}
