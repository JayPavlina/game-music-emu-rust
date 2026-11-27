/// Error type used throughout the library, replacing C++ `blargg_err_t` (const char*).
/// NULL in C++ maps to Ok(()) in Rust.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GmeError {
    Eof,
    WrongFileType,
    OutOfMemory,
    ReadError,
    OpenFailed,
    NotAnM3uPlaylist,
    CorruptFile,
    Internal(&'static str),
}

impl GmeError {
    pub fn as_str(&self) -> &'static str {
        match self {
            GmeError::Eof => "Unexpected end of file",
            GmeError::WrongFileType => "Wrong file type for this emulator",
            GmeError::OutOfMemory => "Out of memory",
            GmeError::ReadError => "Read error",
            GmeError::OpenFailed => "Couldn't open file",
            GmeError::NotAnM3uPlaylist => "Not an m3u playlist",
            GmeError::CorruptFile => "Corrupt file",
            GmeError::Internal(s) => s,
        }
    }
}

impl std::fmt::Display for GmeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::error::Error for GmeError {}

impl From<&'static str> for GmeError {
    fn from(s: &'static str) -> Self {
        GmeError::Internal(s)
    }
}

/// Result type used throughout the library.
pub type GmeResult<T = ()> = Result<T, GmeError>;
