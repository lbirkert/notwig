//! ARA (Audio Random Access) host support for the notwig-rack framework.
//!
//! ARA is a companion extension to CLAP, VST3, and Audio Unit v2. This crate
//! owns the format-agnostic host side:
//!
//! - [`AraDocument`] builds the host services, initializes a plugin factory,
//!   owns the checked ARA document graph, and attaches a bound extension.
//! - [`MemoryAudioAccess`] and [`MemoryArchiving`] are the default in-memory
//!   audio-source and document-archive providers.
//! - [`AraPlugin`] is implemented by `notwig-rack-clap`, `notwig-rack-vst3`,
//!   and `notwig-rack-au` to expose their companion binding entry point.
//!
//! # Lifecycle
//!
//! 1. Load the plugin through its normal format wrapper.
//! 2. Create an [`AraDocument`] from the plugin's [`AraFactory`].
//! 3. Bind the plugin before activation, state load, or editor creation.
//! 4. Build the graph through [`AraDocument::session_mut`].
//! 5. Keep the plugin alive until the document is dropped.
//!
//! # Threading
//!
//! Model graph operations, factory initialization, binding, and persistence run
//! on the model (UI) thread. Audio readers may be called concurrently from
//! plugin threads; the providers in this crate use thread-safe storage.
//!
//! # Real-time rules
//!
//! [`AudioSourceBuffer`] readers are allocation-free after construction. Do not
//! mutate the document graph from the audio callback.

#![warn(missing_docs)]

mod archive;
mod audio;
mod document;
mod factory;
mod roles;

pub use archive::{MemoryArchive, MemoryArchiving};
pub use audio::{AudioSourceBuffer, MemoryAudioAccess};
pub use document::{AraDocument, AraDocumentBuilder, RawAraFactory};
pub use factory::{AraFactory, AraPlugin, AraPluginExtension};
pub use roles::AraRoles;

pub use ara2_bridge_core::{
    ApiGeneration, AraError, AudioModificationProperties, AudioSourceProperties, ContentGrade,
    ContentTimeRange, DocumentProperties, MusicalContextProperties, PlaybackRegionProperties,
    RegionSequenceProperties, RestoreFilter, StoreFilter,
};
pub use ara2_bridge_host::{
    ArchiveReaderId, ArchiveWriterId, ArchivingProvider, AudioAccessProvider,
    AudioModificationHandle, AudioModificationId, AudioSourceHandle, AudioSourceId, CloseError,
    ContentAccessProvider, DocumentSession, EditSession, ExtensionRoles, FactoryMetadata,
    HostAudioReader, HostContentReaderSnapshot, HostServices, HostServicesBuilder, LoadedFactory,
    ModelUpdateProvider, MusicalContextHandle, MusicalContextId, PlaybackProvider,
    PlaybackRegionHandle, PlaybackRegionId, RegionSequenceHandle, RendererRole,
    StoredAudioFileChunk,
};
pub use ara2_bridge_sys::{
    ARAAssertCategory, ARAAssertFunction, ARADocumentControllerRef, ARAFactory,
    ARAPlugInExtensionInstance, ARAPlugInInstanceRoleFlags,
};
