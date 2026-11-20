#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("io error: {0}")]
    Io(String),
    #[error("schema error: {0}")]
    Schema(String),
    #[error("unsupported version: {0}")]
    UnsupportedVersion(String),
    #[error("invalid project: {0}")]
    Invalid(String),
    #[error("missing asset: {0}")]
    MissingAsset(String),
    #[error("unsupported audio format: {0}")]
    UnsupportedAudio(String),
    #[error("audio decode error: {0}")]
    Decode(String),
    #[error("resample error: {0}")]
    Resample(String),
    #[error("audio encode error: {0}")]
    Encode(String),
}

impl From<std::io::Error> for ProjectError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

impl From<serde_json::Error> for ProjectError {
    fn from(value: serde_json::Error) -> Self {
        Self::Schema(value.to_string())
    }
}
