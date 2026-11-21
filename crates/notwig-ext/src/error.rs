#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ExtensionError {
    #[error("wasm error: {0}")]
    Wasm(String),
    #[error("native extension error: {0}")]
    Native(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("manifest error: {0}")]
    Manifest(String),
    #[error("abi error: {0}")]
    Abi(String),
    #[error("extension is disabled: {0}")]
    Disabled(String),
    #[error("unknown extension type: {0}")]
    UnknownType(String),
    #[error("device handle is invalid")]
    InvalidHandle,
    #[error("extension exceeded its resource budget")]
    BudgetExceeded,
    #[error("missing wasm export: {0}")]
    MissingExport(String),
    #[error("archive error: {0}")]
    Archive(String),
    #[error("extension archive is too large")]
    ArchiveTooLarge,
    #[error("extension archive path is unsafe: {0}")]
    UnsafeArchivePath(String),
    #[error("extension archive is missing extension.json")]
    MissingManifest,
    #[error("extension id mismatch: expected {expected}, found {found}")]
    ExtensionIdMismatch { expected: String, found: String },
    #[error("invalid extension id: {0}")]
    InvalidExtensionId(String),
}

#[cfg(feature = "native")]
pub(crate) fn wasm_err(error: impl std::fmt::Display) -> ExtensionError {
    ExtensionError::Wasm(error.to_string())
}
