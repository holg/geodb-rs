use std::fmt;
use std::io;

// Note: field is named `msg` instead of `message` to avoid conflict
// with Kotlin's Exception.message property (UniFFI bug workaround)
#[derive(Debug, uniffi::Error)]
pub enum FfiError {
    Init { msg: String },
    Io { msg: String },
    Bincode { msg: String },
}

impl fmt::Display for FfiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Init { msg } => write!(f, "Failed to initialize GeoDB: {msg}"),
            Self::Io { msg } => write!(f, "IO Error: {msg}"),
            Self::Bincode { msg } => write!(f, "Bincode Error: {msg}"),
        }
    }
}

impl From<io::Error> for FfiError {
    fn from(err: io::Error) -> Self {
        Self::Io {
            msg: err.to_string(),
        }
    }
}

impl From<Box<bincode::ErrorKind>> for FfiError {
    fn from(err: Box<bincode::ErrorKind>) -> Self {
        Self::Bincode {
            msg: err.to_string(),
        }
    }
}
