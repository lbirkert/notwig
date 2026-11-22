use std::collections::BTreeMap;
use std::sync::Arc;

use notwig_core::ids::{DeviceChain, Track};
use notwig_core::protocol::MetronomeState;
use notwig_core::protocol::PlaybackSchedule;
use notwig_core::{Id, Meta, ProjectDoc, SampleRate};
use notwig_dsp::EngineCaps;
use notwig_engine::{Engine, EngineConfig, EngineControl, StockDeviceFactory, TransportUpdate};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn empty_doc() -> ProjectDoc {
    let meta = Meta {
        name: "Control".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    ProjectDoc::new_template(meta, id::<Track>(1), id::<DeviceChain>(100)).unwrap()
}

fn prepare(doc: &ProjectDoc) -> Box<notwig_engine::PreparedRuntime> {
    let caps = EngineCaps {
        sample_rate: 48_000,
        max_block_size: 128,
        channels: 2,
    };
    Engine::prepare_runtime(
        EngineConfig::new(48_000, 128),
        Arc::new(StockDeviceFactory),
        Arc::new(notwig_dsp::compile(doc, &notwig_dsp::stock_registry(), caps).unwrap()),
        Arc::new(PlaybackSchedule::from_doc(doc)),
        Arc::new(BTreeMap::new()),
    )
    .unwrap()
}

fn transport(playhead_samples: u64) -> TransportUpdate {
    TransportUpdate {
        playing: true,
        playhead_samples,
        loop_enabled: false,
        loop_start_samples: None,
        loop_end_samples: None,
        stop: false,
        metronome: None,
    }
}

#[test]
fn control_sync_installs_a_prepared_runtime_and_tracks_the_playhead() {
    let doc = empty_doc();
    let mut engine = Engine::new(EngineConfig::new(48_000, 128));
    engine.set_control_sync_frames(1);
    let mut producer = EngineControl::new();
    engine.attach_control(producer.handle());

    producer.data_mut().set_prepared_runtime(prepare(&doc));
    producer.data_mut().set_transport(transport(0));
    producer.flush();

    let mut output = vec![0.0_f32; 128 * 2];
    engine.process(&mut output, 128).unwrap();
    assert!(engine.transport().playing);
    assert_eq!(engine.transport().playhead_samples, 128.0);
    assert_eq!(producer.handle().playhead_samples(), 128);

    engine.process(&mut output, 128).unwrap();
    assert_eq!(engine.transport().playhead_samples, 256.0);
    assert_eq!(producer.handle().playhead_samples(), 256);
}

#[test]
fn control_transport_updates_enable_the_metronome() {
    let doc = empty_doc();
    let mut engine = Engine::new(EngineConfig::new(48_000, 128));
    engine.set_control_sync_frames(1);
    let mut producer = EngineControl::new();
    engine.attach_control(producer.handle());

    let mut update = transport(0);
    update.metronome = Some(MetronomeState {
        enabled: true,
        beats_per_bar: 4,
    });
    producer.data_mut().set_prepared_runtime(prepare(&doc));
    producer.data_mut().set_transport(update);
    producer.flush();

    let mut output = vec![0.0_f32; 128 * 2];
    engine.process(&mut output, 128).unwrap();
    assert!(
        output.iter().any(|sample| sample.abs() > 0.01),
        "metronome click should be audible"
    );
}

#[test]
fn transport_updates_take_effect_at_the_sync_interval() {
    let doc = empty_doc();
    let mut engine = Engine::new(EngineConfig::new(48_000, 128));
    engine.set_control_sync_frames(2);
    let mut producer = EngineControl::new();
    engine.attach_control(producer.handle());

    producer.data_mut().set_prepared_runtime(prepare(&doc));
    producer.data_mut().set_transport(transport(0));
    producer.flush();

    let mut output = vec![0.0_f32; 128 * 2];
    engine.process(&mut output, 128).unwrap();
    engine.process(&mut output, 128).unwrap();
    assert!(engine.transport().playing);
    assert_eq!(engine.transport().playhead_samples, 256.0);

    producer.data_mut().set_transport(transport(48_000));
    producer.flush();
    engine.process(&mut output, 128).unwrap();
    engine.process(&mut output, 128).unwrap();
    assert!(engine.transport().playhead_samples > 48_000.0);
    assert!(producer.handle().playhead_samples() >= 48_128);
}

#[test]
fn flushing_twice_without_new_data_does_not_reapply_old_updates() {
    let doc = empty_doc();
    let mut engine = Engine::new(EngineConfig::new(48_000, 128));
    engine.set_control_sync_frames(1);
    let mut producer = EngineControl::new();
    engine.attach_control(producer.handle());

    producer.data_mut().set_prepared_runtime(prepare(&doc));
    producer.data_mut().set_transport(transport(0));
    producer.flush();

    let mut output = vec![0.0_f32; 128 * 2];
    engine.process(&mut output, 128).unwrap();
    engine.process(&mut output, 128).unwrap();
    assert_eq!(engine.transport().playhead_samples, 256.0);

    // No new transport was staged; reuse of the returned box must not rewind
    // the playhead to zero.
    producer.flush();
    engine.process(&mut output, 128).unwrap();
    assert_eq!(engine.transport().playhead_samples, 384.0);
}

#[test]
fn remote_sink_receives_serializable_control_updates() {
    let (sender, receiver) = std::sync::mpsc::channel();
    let mut producer = EngineControl::new();
    producer.set_remote_sink(Some(Box::new(move |update| {
        let _ = sender.send(update);
    })));

    producer.data_mut().set_transport(TransportUpdate {
        playing: true,
        playhead_samples: 512,
        loop_enabled: false,
        loop_start_samples: None,
        loop_end_samples: None,
        stop: false,
        metronome: None,
    });
    producer.flush();

    let update = receiver.recv().expect("remote control update");
    let transport = update.transport.expect("transport update");
    assert!(transport.playing);
    assert_eq!(transport.playhead_samples, 512);
}
