//! Host archive persistence for ARA documents.
//!
//! [`MemoryArchive`] is a byte buffer token passed to the document
//! store/restore helpers. [`MemoryArchiving`] is the default
//! [`ArchivingProvider`] that reads and writes those buffers when the plugin
//! asks the host for archive I/O.

use ara2_bridge_core::AraError;
use ara2_bridge_host::{ArchiveReaderId, ArchiveWriterId, ArchivingProvider};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// One in-memory ARA document archive.
///
/// The same token type is used for storing and restoring. When restoring an
/// ARA 2 archive, set the compatible [`MemoryArchive::with_archive_id`] value
/// reported by the plugin factory.
#[derive(Debug, Clone, Default)]
pub struct MemoryArchive {
    bytes: Arc<Mutex<Vec<u8>>>,
    archive_id: Option<String>,
}

impl MemoryArchive {
    /// Creates an empty archive.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an archive with existing bytes.
    #[must_use]
    pub fn with_bytes(bytes: Vec<u8>) -> Self {
        Self {
            bytes: Arc::new(Mutex::new(bytes)),
            archive_id: None,
        }
    }

    /// Creates an archive with existing bytes and a document archive ID.
    #[must_use]
    pub fn with_archive_id(bytes: Vec<u8>, archive_id: impl Into<String>) -> Self {
        Self {
            bytes: Arc::new(Mutex::new(bytes)),
            archive_id: Some(archive_id.into()),
        }
    }

    /// Returns the archive bytes.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        lock(&self.bytes).clone()
    }

    /// Returns the current archive length.
    #[must_use]
    pub fn len(&self) -> usize {
        lock(&self.bytes).len()
    }

    /// Returns `true` when no bytes have been stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the document archive ID, if known.
    #[must_use]
    pub fn archive_id(&self) -> Option<&str> {
        self.archive_id.as_deref()
    }

    /// Sets the document archive ID.
    pub fn set_archive_id(&mut self, archive_id: impl Into<String>) {
        self.archive_id = Some(archive_id.into());
    }

    fn shared_bytes(&self) -> Arc<Mutex<Vec<u8>>> {
        Arc::clone(&self.bytes)
    }
}

#[derive(Clone, Debug)]
struct RegisteredArchive {
    bytes: Arc<Mutex<Vec<u8>>>,
    archive_id: Option<String>,
}

/// Default in-memory archiving provider.
///
/// The document helpers in [`crate::AraDocument`] register and unregister the
/// borrowed [`MemoryArchive`] token around each store/restore call. Plugins see
/// ordinary position-based archive I/O.
#[derive(Debug, Default)]
pub struct MemoryArchiving {
    archives: Mutex<BTreeMap<usize, RegisteredArchive>>,
}

impl MemoryArchiving {
    /// Creates an empty provider.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn register_writer(&self, key: usize, archive: &MemoryArchive) {
        lock(&self.archives).insert(
            key,
            RegisteredArchive {
                bytes: archive.shared_bytes(),
                archive_id: archive.archive_id.clone(),
            },
        );
    }

    pub(crate) fn register_reader(&self, key: usize, archive: &MemoryArchive) {
        lock(&self.archives).insert(
            key,
            RegisteredArchive {
                bytes: archive.shared_bytes(),
                archive_id: archive.archive_id.clone(),
            },
        );
    }

    pub(crate) fn unregister(&self, key: usize) -> Option<Arc<Mutex<Vec<u8>>>> {
        lock(&self.archives).remove(&key).map(|entry| entry.bytes)
    }

    fn entry(&self, key: usize) -> Result<RegisteredArchive, AraError> {
        lock(&self.archives)
            .get(&key)
            .cloned()
            .ok_or(AraError::InvalidArgument("unknown ARA archive token"))
    }
}

impl ArchivingProvider for MemoryArchiving {
    fn len(&self, reader: ArchiveReaderId) -> Result<usize, AraError> {
        Ok(lock(&self.entry(reader.address())?.bytes).len())
    }

    fn read_at(
        &self,
        reader: ArchiveReaderId,
        position: usize,
        buffer: &mut [u8],
    ) -> Result<(), AraError> {
        let entry = self.entry(reader.address())?;
        let bytes = lock(&entry.bytes);
        let end = position
            .checked_add(buffer.len())
            .ok_or(AraError::InvalidArgument("archive read range overflow"))?;
        let source = bytes
            .get(position..end)
            .ok_or(AraError::InvalidArgument("archive read beyond end"))?;
        buffer.copy_from_slice(source);
        Ok(())
    }

    fn write_at(
        &self,
        writer: ArchiveWriterId,
        position: usize,
        buffer: &[u8],
    ) -> Result<(), AraError> {
        let entry = self.entry(writer.address())?;
        let mut bytes = lock(&entry.bytes);
        let end = position
            .checked_add(buffer.len())
            .ok_or(AraError::InvalidArgument("archive write range overflow"))?;
        if bytes.len() < end {
            bytes.resize(end, 0);
        }
        bytes[position..end].copy_from_slice(buffer);
        Ok(())
    }

    fn document_archive_id(&self, reader: ArchiveReaderId) -> Result<Option<String>, AraError> {
        Ok(self.entry(reader.address())?.archive_id)
    }
}
