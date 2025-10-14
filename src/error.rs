/// Contains an error message passed by Game Music Emu
#[derive(Debug, err_derive::Error)]
#[error(display = "_0")]
pub struct GmeError(String);

impl GmeError {
    pub fn new(message: String) -> Self {
        Self(message)
    }

    pub fn message(&self) -> &str {
        &self.0
    }
}

pub(crate) type GmeResult<T> = Result<T, GmeError>;

/// Either an IO error or a GME error
#[derive(Debug, err_derive::Error)]
pub enum GmeOrIoError {
    #[error(display = "IO error: _0")]
    IoError(#[source] std::io::Error),
    #[error(display = "GME error: _0")]
    Gme(#[source] GmeError),
}

// pub(crate) type GmeOrIoResult<T> = Result<T, GmeOrIoError>;
