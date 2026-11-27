//! Host-owned ARA document lifecycle.
//!
//! [`AraDocument`] pairs the host services, the plugin factory, and the
//! checked document graph. It deliberately keeps the plugin lifetime contract
//! in documentation rather than a borrow: a host creates the document from a
//! loaded plugin, may then keep using that plugin for audio processing, and
//! must drop the document before unloading the plugin.

use crate::archive::{MemoryArchive, MemoryArchiving};
use crate::audio::{AudioSourceBuffer, MemoryAudioAccess};
use crate::factory::{AraFactory, AraPlugin};
use crate::roles::AraRoles;
use ara2_bridge_core::{
    ApiGeneration, AraError, AudioSourceProperties, DocumentProperties, RestoreFilter, StoreFilter,
};
use ara2_bridge_host::{
    ArchivingProvider, AudioAccessProvider, AudioSourceHandle, AudioSourceId,
    ContentAccessProvider, DocumentSession, ExtensionController, FactoryMetadata, HostServices,
    HostServicesBuilder, LoadedFactory, ModelUpdateProvider, PlaybackProvider,
};
use ara2_bridge_sys::{ARAAssertCategory, ARAAssertFunction, ARADocumentControllerRef, ARAFactory};
use std::ffi::{CStr, c_char, c_void};
use std::sync::Arc;

unsafe extern "C" fn ara_assert(
    category: ARAAssertCategory,
    _problematic_argument: *const c_void,
    diagnosis: *const c_char,
) {
    let diagnosis = if diagnosis.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(diagnosis) }
            .to_string_lossy()
            .into_owned()
    };
    tracing::error!(category, diagnosis = %diagnosis, "ARA assertion");
}

const ARA_ASSERT: ARAAssertFunction = Some(ara_assert);

struct SharedAudio(Arc<dyn AudioAccessProvider>);
impl AudioAccessProvider for SharedAudio {
    fn create_reader(
        &self,
        source: AudioSourceId,
        use_64_bit_samples: bool,
    ) -> Result<Box<dyn ara2_bridge_host::HostAudioReader>, AraError> {
        self.0.create_reader(source, use_64_bit_samples)
    }
}

struct SharedArchiving(Arc<dyn ArchivingProvider>);
impl ArchivingProvider for SharedArchiving {
    fn len(&self, reader: ara2_bridge_host::ArchiveReaderId) -> Result<usize, AraError> {
        self.0.len(reader)
    }

    fn read_at(
        &self,
        reader: ara2_bridge_host::ArchiveReaderId,
        position: usize,
        buffer: &mut [u8],
    ) -> Result<(), AraError> {
        self.0.read_at(reader, position, buffer)
    }

    fn write_at(
        &self,
        writer: ara2_bridge_host::ArchiveWriterId,
        position: usize,
        buffer: &[u8],
    ) -> Result<(), AraError> {
        self.0.write_at(writer, position, buffer)
    }

    fn archiving_progress(&self, value: f32) -> Result<(), AraError> {
        self.0.archiving_progress(value)
    }

    fn unarchiving_progress(&self, value: f32) -> Result<(), AraError> {
        self.0.unarchiving_progress(value)
    }

    fn document_archive_id(
        &self,
        reader: ara2_bridge_host::ArchiveReaderId,
    ) -> Result<Option<String>, AraError> {
        self.0.document_archive_id(reader)
    }
}

struct SharedContent(Arc<dyn ContentAccessProvider>);
impl ContentAccessProvider for SharedContent {
    fn musical_context_grade(
        &self,
        context: ara2_bridge_host::MusicalContextId,
        content_type: i32,
    ) -> Result<Option<ara2_bridge_core::ContentGrade>, AraError> {
        self.0.musical_context_grade(context, content_type)
    }

    fn musical_context_reader(
        &self,
        context: ara2_bridge_host::MusicalContextId,
        content_type: i32,
        range: Option<ara2_bridge_core::ContentTimeRange>,
    ) -> Result<Option<ara2_bridge_host::HostContentReaderSnapshot>, AraError> {
        self.0.musical_context_reader(context, content_type, range)
    }

    fn audio_source_grade(
        &self,
        source: AudioSourceId,
        content_type: i32,
    ) -> Result<Option<ara2_bridge_core::ContentGrade>, AraError> {
        self.0.audio_source_grade(source, content_type)
    }

    fn audio_source_reader(
        &self,
        source: AudioSourceId,
        content_type: i32,
        range: Option<ara2_bridge_core::ContentTimeRange>,
    ) -> Result<Option<ara2_bridge_host::HostContentReaderSnapshot>, AraError> {
        self.0.audio_source_reader(source, content_type, range)
    }
}

struct SharedModelUpdates(Arc<dyn ModelUpdateProvider>);
impl ModelUpdateProvider for SharedModelUpdates {
    fn audio_source_analysis_progress(
        &self,
        source: AudioSourceId,
        state: i32,
        value: f32,
    ) -> Result<(), AraError> {
        self.0.audio_source_analysis_progress(source, state, value)
    }

    fn audio_source_content_changed(
        &self,
        source: AudioSourceId,
        range: Option<ara2_bridge_core::ContentTimeRange>,
        flags: i32,
    ) -> Result<(), AraError> {
        self.0.audio_source_content_changed(source, range, flags)
    }

    fn audio_modification_content_changed(
        &self,
        modification: ara2_bridge_host::AudioModificationId,
        range: Option<ara2_bridge_core::ContentTimeRange>,
        flags: i32,
    ) -> Result<(), AraError> {
        self.0
            .audio_modification_content_changed(modification, range, flags)
    }

    fn playback_region_content_changed(
        &self,
        region: ara2_bridge_host::PlaybackRegionId,
        range: Option<ara2_bridge_core::ContentTimeRange>,
        flags: i32,
    ) -> Result<(), AraError> {
        self.0.playback_region_content_changed(region, range, flags)
    }

    fn document_data_changed(&self) -> Result<(), AraError> {
        self.0.document_data_changed()
    }
}

struct SharedPlayback(Arc<dyn PlaybackProvider>);
impl PlaybackProvider for SharedPlayback {
    fn start(&self) -> Result<(), AraError> {
        self.0.start()
    }

    fn stop(&self) -> Result<(), AraError> {
        self.0.stop()
    }

    fn set_position(&self, position: f64) -> Result<(), AraError> {
        self.0.set_position(position)
    }

    fn set_cycle_range(&self, start: f64, duration: f64) -> Result<(), AraError> {
        self.0.set_cycle_range(start, duration)
    }

    fn enable_cycle(&self, enable: bool) -> Result<(), AraError> {
        self.0.enable_cycle(enable)
    }
}

/// Builder for one host ARA document.
pub struct AraDocumentBuilder {
    generation: ApiGeneration,
    document_name: Option<String>,
    audio: Arc<dyn AudioAccessProvider>,
    archiving: Arc<dyn ArchivingProvider>,
    content: Option<Arc<dyn ContentAccessProvider>>,
    model_updates: Option<Arc<dyn ModelUpdateProvider>>,
    playback: Option<Arc<dyn PlaybackProvider>>,
    memory_audio: Option<Arc<MemoryAudioAccess>>,
    memory_archiving: Option<Arc<MemoryArchiving>>,
    known_roles: AraRoles,
    assigned_roles: AraRoles,
}

impl Default for AraDocumentBuilder {
    fn default() -> Self {
        Self::new(ApiGeneration::V23Final)
    }
}

impl AraDocumentBuilder {
    /// Creates a builder for `generation` with in-memory audio and archive
    /// providers.
    #[must_use]
    pub fn new(generation: ApiGeneration) -> Self {
        let audio = Arc::new(MemoryAudioAccess::new());
        let archiving = Arc::new(MemoryArchiving::new());
        Self {
            generation,
            document_name: None,
            audio: audio.clone(),
            archiving: archiving.clone(),
            content: None,
            model_updates: None,
            playback: None,
            memory_audio: Some(audio),
            memory_archiving: Some(archiving),
            known_roles: AraRoles::KNOWN,
            assigned_roles: AraRoles::ASSIGNED,
        }
    }

    /// Sets the document display name.
    #[must_use]
    pub fn document_name(mut self, name: impl Into<String>) -> Self {
        self.document_name = Some(name.into());
        self
    }

    /// Replaces the audio-access provider.
    ///
    /// The default in-memory provider handle is dropped, so
    /// [`AraDocument::memory_audio`] returns `None` after this call.
    #[must_use]
    pub fn with_audio_provider<P: AudioAccessProvider + 'static>(mut self, provider: P) -> Self {
        self.audio = Arc::new(provider);
        self.memory_audio = None;
        self
    }

    /// Replaces the archiving provider.
    #[must_use]
    pub fn with_archiving_provider<P: ArchivingProvider + 'static>(mut self, provider: P) -> Self {
        self.archiving = Arc::new(provider);
        self.memory_archiving = None;
        self
    }

    /// Installs an optional content-access provider.
    #[must_use]
    pub fn with_content_provider<P: ContentAccessProvider + 'static>(
        mut self,
        provider: P,
    ) -> Self {
        self.content = Some(Arc::new(provider));
        self
    }

    /// Installs an optional model-update provider.
    #[must_use]
    pub fn with_model_updates<P: ModelUpdateProvider + 'static>(mut self, provider: P) -> Self {
        self.model_updates = Some(Arc::new(provider));
        self
    }

    /// Installs an optional playback-control provider.
    #[must_use]
    pub fn with_playback<P: PlaybackProvider + 'static>(mut self, provider: P) -> Self {
        self.playback = Some(Arc::new(provider));
        self
    }

    /// Reuses an existing in-memory audio provider.
    #[must_use]
    pub fn with_memory_audio(mut self, audio: Arc<MemoryAudioAccess>) -> Self {
        self.audio = audio.clone();
        self.memory_audio = Some(audio);
        self
    }

    /// Reuses an existing in-memory archiving provider.
    #[must_use]
    pub fn with_memory_archiving(mut self, archiving: Arc<MemoryArchiving>) -> Self {
        self.archiving = archiving.clone();
        self.memory_archiving = Some(archiving);
        self
    }

    /// Sets the known and assigned ARA roles used when binding a plugin.
    #[must_use]
    pub fn with_roles(mut self, known: AraRoles, assigned: AraRoles) -> Self {
        self.known_roles = known;
        self.assigned_roles = assigned;
        self
    }

    /// Creates the document controller and host graph for `factory`.
    ///
    /// The factory is initialized exactly once per document. The plugin that
    /// owns `factory` must stay loaded until the document is dropped.
    ///
    /// # Errors
    ///
    /// Returns an ARA error when the factory is incompatible with the selected
    /// generation or the host services cannot be built.
    pub fn build(self, factory: AraFactory) -> Result<AraDocument, AraError> {
        let mut services = HostServicesBuilder::new()
            .audio(SharedAudio(self.audio))
            .archiving(SharedArchiving(self.archiving));
        if let Some(content) = self.content {
            services = services.content(SharedContent(content));
        }
        if let Some(model_updates) = self.model_updates {
            services = services.model_updates(SharedModelUpdates(model_updates));
        }
        if let Some(playback) = self.playback {
            services = services.playback(SharedPlayback(playback));
        }
        let services = Box::new(services.build(self.generation)?);

        let factory: Box<LoadedFactory<'static>> = Box::new(unsafe {
            LoadedFactory::load(factory.as_raw(), self.generation, ARA_ASSERT)?
        });
        let properties = DocumentProperties::new(self.document_name.as_deref())?;
        let session = DocumentSession::new(&factory, &services, properties)?;
        // SAFETY: `factory` and `services` live in this struct, are never moved
        // after boxing, and are declared after `session` so they drop last. The
        // session stores only raw pointers and lifetimes used for checking.
        let session: DocumentSession<'static, 'static> = unsafe { std::mem::transmute(session) };

        Ok(AraDocument {
            session: Some(session),
            extension: None,
            factory,
            services,
            generation: self.generation,
            known_roles: self.known_roles,
            assigned_roles: self.assigned_roles,
            memory_audio: self.memory_audio,
            memory_archiving: self.memory_archiving,
        })
    }
}

/// One host-owned ARA document and its plugin binding.
pub struct AraDocument {
    // Field order is intentional: the session and extension drop before the
    // factory/services they point into.
    session: Option<DocumentSession<'static, 'static>>,
    extension: Option<ExtensionController<'static>>,
    factory: Box<LoadedFactory<'static>>,
    services: Box<HostServices>,
    generation: ApiGeneration,
    known_roles: AraRoles,
    assigned_roles: AraRoles,
    memory_audio: Option<Arc<MemoryAudioAccess>>,
    memory_archiving: Option<Arc<MemoryArchiving>>,
}

impl std::fmt::Debug for AraDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AraDocument")
            .field("generation", &self.generation)
            .field("bound", &self.extension.is_some())
            .field("factory", &self.factory.metadata().factory_id())
            .finish_non_exhaustive()
    }
}

impl Drop for AraDocument {
    fn drop(&mut self) {
        if let Some(session) = self.session.take() {
            let _ = session.close();
        }
    }
}

impl AraDocument {
    /// Returns the selected ARA API generation.
    #[must_use]
    pub fn generation(&self) -> ApiGeneration {
        self.generation
    }

    /// Returns copied factory metadata.
    #[must_use]
    pub fn factory_metadata(&self) -> &FactoryMetadata {
        self.factory.metadata()
    }

    /// Returns the host services backing this document.
    #[must_use]
    pub fn services(&self) -> &HostServices {
        &self.services
    }

    /// Returns the checked document graph.
    ///
    /// # Panics
    ///
    /// Panics only if called after teardown, which cannot happen while `self`
    /// is alive because teardown consumes the session only in `Drop`.
    #[must_use]
    pub fn session(&self) -> &DocumentSession<'static, 'static> {
        self.session
            .as_ref()
            .expect("ARA session is live until drop")
    }

    /// Returns the checked document graph mutably.
    ///
    /// # Panics
    ///
    /// Panics only if called after teardown, which cannot happen while `self`
    /// is alive because teardown consumes the session only in `Drop`.
    pub fn session_mut(&mut self) -> &mut DocumentSession<'static, 'static> {
        self.session
            .as_mut()
            .expect("ARA session is live until drop")
    }

    /// Returns the document-controller reference used for companion binding.
    #[must_use]
    pub fn controller_ref(&self) -> ARADocumentControllerRef {
        self.session().controller_ref()
    }

    /// Returns the stable token for a host audio source.
    ///
    /// # Errors
    ///
    /// Returns an ARA error when the source is provisional or from another
    /// document.
    pub fn audio_source_id(&self, handle: AudioSourceHandle) -> Result<AudioSourceId, AraError> {
        self.session().audio_source_id(handle)
    }

    /// Returns the default in-memory audio provider, when one is installed.
    #[must_use]
    pub fn memory_audio(&self) -> Option<&Arc<MemoryAudioAccess>> {
        self.memory_audio.as_ref()
    }

    /// Returns the default in-memory archiving provider, when one is installed.
    #[must_use]
    pub fn memory_archiving(&self) -> Option<&Arc<MemoryArchiving>> {
        self.memory_archiving.as_ref()
    }

    /// Returns `true` after a plugin has been bound to this document.
    #[must_use]
    pub fn is_bound(&self) -> bool {
        self.extension.is_some()
    }

    /// Binds a loaded plugin instance to this document.
    ///
    /// The plugin's factory must match the factory used to build the document.
    /// Binding must happen before activation, state load, or editor creation.
    ///
    /// # Errors
    ///
    /// Returns an ARA error when the factory does not match, the document is
    /// already bound, or the companion API rejects the binding.
    pub fn bind<P: AraPlugin + ?Sized>(&mut self, plugin: &mut P) -> Result<(), AraError> {
        if self.extension.is_some() {
            return Err(AraError::InvalidState("ARA document is already bound"));
        }
        let reported = plugin
            .ara_factory()
            .ok_or(AraError::Unsupported("plugin has no ARA factory"))?;
        if !std::ptr::eq(reported.as_raw(), self.factory.as_raw()) {
            return Err(AraError::InvalidArgument(
                "plugin factory does not match the document factory",
            ));
        }
        let session = self
            .session
            .as_mut()
            .ok_or(AraError::InvalidState("ARA document is closed"))?;
        let controller = session.controller_ref();
        let extension =
            unsafe { plugin.bind_ara(controller, self.known_roles, self.assigned_roles)? };
        let controller = unsafe {
            session.bind_extension(
                extension.as_raw(),
                self.known_roles.into(),
                self.assigned_roles.into(),
            )?
        };
        // SAFETY: the extension backing is owned by the plugin and must outlive
        // the document; the controller stores only raw references.
        let controller: ExtensionController<'static> = unsafe { std::mem::transmute(controller) };
        self.extension = Some(controller);
        Ok(())
    }

    /// Creates an audio source and registers its samples with the default
    /// in-memory provider.
    ///
    /// # Errors
    ///
    /// Returns an ARA error when the document uses a custom audio provider, the
    /// edit cannot be started, or the plugin rejects the source.
    pub fn create_audio_source(
        &mut self,
        properties: AudioSourceProperties,
        buffer: Arc<AudioSourceBuffer>,
    ) -> Result<AudioSourceHandle, AraError> {
        let memory = self
            .memory_audio
            .clone()
            .ok_or(AraError::Unsupported("custom ARA audio provider"))?;
        let handle = {
            let session = self
                .session
                .as_mut()
                .ok_or(AraError::InvalidState("ARA document is closed"))?;
            let mut edit = session.edit()?;
            let handle = edit.create_audio_source(properties)?;
            edit.finish()?;
            handle
        };
        let source = self.session().audio_source_id(handle)?;
        memory.insert(source, buffer);
        self.session_mut()
            .set_audio_source_samples_access(handle, true)?;
        Ok(handle)
    }

    /// Stores the complete document into an in-memory archive.
    ///
    /// On success, the archive ID is set from the plugin factory metadata.
    ///
    /// # Errors
    ///
    /// Returns an ARA error when no in-memory archiving provider is installed,
    /// the document is editing, or the plugin rejects the store.
    pub fn store_document_to_archive(
        &mut self,
        archive: &mut MemoryArchive,
    ) -> Result<(), AraError> {
        let memory = self.archiving()?;
        let key = std::ptr::from_ref(archive).cast::<()>() as usize;
        memory.register_writer(key, archive);
        let result = self
            .session
            .as_mut()
            .ok_or(AraError::InvalidState("ARA document is closed"))?
            .store_document_to_archive(archive);
        memory.unregister(key);
        if result.is_ok() {
            archive.set_archive_id(self.factory.metadata().document_archive_id());
        }
        result
    }

    /// Stores selected ARA 2 objects into an in-memory archive.
    ///
    /// # Errors
    ///
    /// Returns an ARA error when no in-memory archiving provider is installed,
    /// the filter belongs to another document, or the plugin rejects the store.
    pub fn store_objects_to_archive(
        &mut self,
        archive: &mut MemoryArchive,
        filter: Option<&StoreFilter>,
    ) -> Result<(), AraError> {
        let memory = self.archiving()?;
        let key = std::ptr::from_ref(archive).cast::<()>() as usize;
        memory.register_writer(key, archive);
        let result = self
            .session
            .as_mut()
            .ok_or(AraError::InvalidState("ARA document is closed"))?
            .store_objects_to_archive(archive, filter);
        memory.unregister(key);
        result
    }

    /// Restores the complete document from an in-memory archive.
    ///
    /// # Errors
    ///
    /// Returns an ARA error when no in-memory archiving provider is installed,
    /// the archive ID is incompatible, or the plugin rejects the restore.
    pub fn restore_document_from_archive(
        &mut self,
        archive: &MemoryArchive,
    ) -> Result<(), AraError> {
        let memory = self.archiving()?;
        let key = std::ptr::from_ref(archive).cast::<()>() as usize;
        memory.register_reader(key, archive);
        let result = (|| {
            let session = self
                .session
                .as_mut()
                .ok_or(AraError::InvalidState("ARA document is closed"))?;
            let edit = session.restore_document_from_archive(archive)?;
            edit.finish()
        })();
        memory.unregister(key);
        result
    }

    /// Restores selected ARA 2 objects from an in-memory archive.
    ///
    /// # Errors
    ///
    /// Returns an ARA error when no in-memory archiving provider is installed,
    /// the archive/filter is incompatible, or the plugin rejects the restore.
    pub fn restore_objects_from_archive(
        &mut self,
        archive: &MemoryArchive,
        filter: Option<&RestoreFilter>,
    ) -> Result<(), AraError> {
        let memory = self.archiving()?;
        let key = std::ptr::from_ref(archive).cast::<()>() as usize;
        memory.register_reader(key, archive);
        let result = (|| {
            let session = self
                .session
                .as_mut()
                .ok_or(AraError::InvalidState("ARA document is closed"))?;
            let mut edit = session.edit()?;
            edit.restore_objects_from_archive(archive, filter)?;
            edit.finish()
        })();
        memory.unregister(key);
        result
    }

    fn archiving(&self) -> Result<Arc<MemoryArchiving>, AraError> {
        self.memory_archiving
            .clone()
            .ok_or(AraError::Unsupported("custom ARA archiving provider"))
    }
}

/// The raw ARA factory type exposed by companion wrappers.
pub type RawAraFactory = ARAFactory;
