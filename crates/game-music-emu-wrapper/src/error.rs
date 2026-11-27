/// Contains an error message passed by Game Music Emu
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
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
#[derive(Debug, thiserror::Error)]
pub enum GmeOrIoError {
    #[error("IO error: {0}")]
    IoError(#[source] std::io::Error),
    #[error("GME error: {0}")]
    Gme(#[source] GmeError),
}

impl From<std::io::Error> for GmeOrIoError {
    fn from(err: std::io::Error) -> Self {
        GmeOrIoError::IoError(err)
    }
}

impl From<GmeError> for GmeOrIoError {
    fn from(err: GmeError) -> Self {
        GmeOrIoError::Gme(err)
    }
}

// pub(crate) type GmeOrIoResult<T> = Result<T, GmeOrIoError>;
