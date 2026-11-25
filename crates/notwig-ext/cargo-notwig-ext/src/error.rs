use std::path::{Path, PathBuf};

/// Error type for CLI build and scaffold operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A user-facing message without an underlying source.
    #[error("{0}")]
    Message(String),
    /// An I/O failure attributed to a path.
    #[error("{path}: {source}")]
    Io {
        /// Path involved in the failed operation.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// A ZIP read or write failure.
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    /// A JSON read or write failure.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl Error {
    /// Creates a message-only error.
    pub fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }

    /// Creates an I/O error attributed to `path`.
    pub fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }
}

/// Result alias using the CLI [`Error`] type.
pub type Result<T> = std::result::Result<T, Error>;
