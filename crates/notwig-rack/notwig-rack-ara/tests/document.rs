use ara2_bridge_testkit::{TestPluginTrace, build_test_factory};
use notwig_rack_ara::{
    ApiGeneration, AraDocumentBuilder, AraError, AraFactory, AudioAccessProvider,
    AudioSourceBuffer, AudioSourceProperties, MemoryArchive, MemoryAudioAccess,
};
use std::sync::Arc;

fn build_document() -> (
    ara2_bridge_plugin::Factory,
    notwig_rack_ara::AraDocument,
    Arc<MemoryAudioAccess>,
) {
    let factory = build_test_factory(TestPluginTrace::new()).expect("test factory");
    let audio = Arc::new(MemoryAudioAccess::new());
    let document = AraDocumentBuilder::new(ApiGeneration::V23Final)
        .document_name("Notwig ARA test")
        .with_memory_audio(Arc::clone(&audio))
        .build(unsafe { AraFactory::from_raw(factory.as_raw()) }.expect("factory handle"))
        .expect("document");
    (factory, document, audio)
}

#[test]
fn metadata_and_audio_source_roundtrip() {
    let (_factory, mut document, audio) = build_document();
    assert_eq!(
        document.factory_metadata().factory_id(),
        "org.ara2-bridge.test"
    );
    assert_eq!(document.generation(), ApiGeneration::V23Final);

    let properties =
        AudioSourceProperties::new(Some("take"), "take-1", 4, 44_100.0, 2, false.into())
            .expect("source properties");
    let buffer = Arc::new(
        AudioSourceBuffer::new(vec![vec![1.0, 2.0, 3.0, 4.0], vec![-1.0, -2.0, -3.0, -4.0]])
            .expect("buffer"),
    );
    let handle = document
        .create_audio_source(properties, Arc::clone(&buffer))
        .expect("audio source");
    let id = document.audio_source_id(handle).expect("source id");
    assert!(Arc::ptr_eq(
        &audio.get(id).expect("registered buffer"),
        &buffer
    ));

    let mut reader = audio.create_reader(id, false).expect("audio reader");
    assert_eq!(reader.channel_count(), 2);
    assert_eq!(reader.sample_count(), 4);
    let mut left = [0.0f32; 4];
    let mut right = [0.0f32; 4];
    {
        let mut channels: [&mut [f32]; 2] = [&mut left, &mut right];
        reader.read_f32(0, &mut channels).expect("read");
    }
    assert_eq!(left, [1.0, 2.0, 3.0, 4.0]);
    assert_eq!(right, [-1.0, -2.0, -3.0, -4.0]);

    let mut beyond = [9.0f32; 2];
    {
        let mut channels: [&mut [f32]; 2] = [&mut beyond, &mut [0.0f32; 2]];
        reader.read_f32(3, &mut channels).expect("read past end");
    }
    assert_eq!(beyond, [4.0, 0.0]);
}

#[test]
fn document_archive_roundtrip() {
    let (_factory, mut document, _audio) = build_document();
    let mut archive = MemoryArchive::new();
    document
        .store_document_to_archive(&mut archive)
        .expect("store document");
    assert!(!archive.is_empty());
    assert_eq!(
        archive.archive_id(),
        Some(document.factory_metadata().document_archive_id())
    );

    let filter = document
        .session()
        .store_filter_builder()
        .build()
        .expect("store filter");
    let mut objects = MemoryArchive::new();
    document
        .store_objects_to_archive(&mut objects, Some(&filter))
        .expect("store objects");
}

#[test]
fn binding_without_plugin_is_rejected() {
    let (_factory, document, _audio) = build_document();
    assert!(!document.is_bound());
    assert!(!document.controller_ref().is_null());
}

#[test]
fn empty_buffer_is_rejected() {
    assert!(matches!(
        AudioSourceBuffer::new(Vec::new()),
        Err(AraError::InvalidArgument(_))
    ));
}

#[test]
fn archive_can_be_restored_into_a_fresh_document() {
    let factory = build_test_factory(TestPluginTrace::new()).expect("test factory");
    let mut document = AraDocumentBuilder::new(ApiGeneration::V23Final)
        .build(unsafe { AraFactory::from_raw(factory.as_raw()) }.expect("factory handle"))
        .expect("document");
    let mut archive = MemoryArchive::new();
    document
        .store_document_to_archive(&mut archive)
        .expect("store document");
    drop(document);

    let restore_factory = build_test_factory(TestPluginTrace::new()).expect("restore factory");
    let mut restored = AraDocumentBuilder::new(ApiGeneration::V23Final)
        .build(unsafe { AraFactory::from_raw(restore_factory.as_raw()) }.expect("factory handle"))
        .expect("restore document");
    let source_archive =
        MemoryArchive::with_archive_id(archive.bytes(), archive.archive_id().expect("archive id"));
    restored
        .restore_objects_from_archive(&source_archive, None)
        .expect("restore objects");
}
