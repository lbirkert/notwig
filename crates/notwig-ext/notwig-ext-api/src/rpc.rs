//! Shared wire types for generated typed extension interfaces.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Recoverable extension/RPC failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct ExtensionError {
    /// Human-readable failure reason.
    pub message: String,
}

impl ExtensionError {
    /// Creates an error from a message.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl From<String> for ExtensionError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<&str> for ExtensionError {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

/// Stable target for a generated implementation handle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteTarget {
    /// Stable implementation id.
    pub implementation_id: String,
    /// Registry generation captured by the handle.
    pub generation: u64,
}

impl RemoteTarget {
    /// Creates a remote target.
    #[must_use]
    pub fn new(implementation_id: impl Into<String>, generation: u64) -> Self {
        Self {
            implementation_id: implementation_id.into(),
            generation,
        }
    }
}

/// Transport used by generated handles to invoke an implementation.
pub trait RpcTransport: Send + Sync {
    /// Invokes one interface method with a postcard payload.
    fn invoke(
        &self,
        target: &RemoteTarget,
        interface_id: &'static str,
        interface_version: u32,
        method: &'static str,
        payload: &[u8],
    ) -> Result<Vec<u8>, ExtensionError>;
}

/// Convenience alias used by generated handles.
pub type SharedRpcTransport = Arc<dyn RpcTransport>;
