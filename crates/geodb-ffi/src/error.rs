
use std::fmt;
use std::io;

#[derive(Debug, uniffi::Error)]
pub enum FfiError {
    Init { message: String },
    Io { message: String },
    Bincode { message: String },
}

impl fmt::Display for FfiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Init { message } => write!(f, "Failed to initialize GeoDB: {message}"),
            Self::Io { message } => write!(f, "IO Error: {message}"),
            Self::Bincode { message } => write!(f, "Bincode Error: {message}"),
        }
    }
}

impl From<io::Error> for FfiError {
    fn from(err: io::Error) -> Self {
        Self::Io {
            message: err.to_string(),
        }
    }
}

impl From<Box<bincode::ErrorKind>> for FfiError {
    fn from(err: Box<bincode::ErrorKind>) -> Self {
        Self::Bincode {
            message: err.to_string(),
        }
    }
}
