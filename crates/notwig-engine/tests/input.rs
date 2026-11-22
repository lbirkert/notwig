use std::sync::Arc;

use notwig_core::apply::create_track;
use notwig_core::dsp::EngineCaps;
use notwig_core::ids::{DeviceChain, Track};
use notwig_core::protocol::{EngineMsg, PlanHandle, PlaybackSchedule};
use notwig_core::*;
use notwig_engine::{Engine, EngineConfig, EngineControl, InputBus, InputRoute, TrackStateUpdate};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn doc_with_audio_track() -> (ProjectDoc, TrackId) {
    let meta = Meta {
        name: "Input".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    let mut doc = ProjectDoc::new_template(meta, id::<Track>(1), id::<DeviceChain>(100)).unwrap();
    let track = id::<Track>(2);
    create_track(
        &mut doc,
        track,
        id::<DeviceChain>(102),
        tracks::TrackKind::Audio,
        "Input".into(),
        None,
        None,
        None,
    )
    .unwrap();
    (doc, track)
}

#[test]
fn monitor_input_is_mixed_through_the_track_chain() {
    let (doc, track) = doc_with_audio_track();
    let config = EngineConfig::new(48_000, 128);
    let mut control = EngineControl::new();
    let bus = Arc::new(InputBus::new(48_000));
    let mut engine = Engine::new(config);
    engine.attach_control(control.handle());
    engine.attach_input_bus(Arc::clone(&bus));
    engine.set_control_sync_frames(1);
    let plan = notwig_core::dsp::compile(
        &doc,
        &notwig_core::dsp::stock_registry(),
        EngineCaps {
            sample_rate: 48_000,
            max_block_size: 128,
            channels: 2,
        },
    )
    .unwrap();
    engine
        .apply(EngineMsg::UpdateSchedule {
            schedule: Arc::new(PlaybackSchedule::from_doc(&doc)),
        })
        .unwrap();
    engine
        .apply(EngineMsg::ApplyPlan {
            plan: PlanHandle::new(plan),
        })
        .unwrap();
    control.data_mut().set_input_routes(vec![InputRoute {
        track_id: track,
        slot: 0,
    }]);
    control.flush();
    for _ in 0..128 {
        bus.push(0, 0.5);
    }

    let mut output = vec![0.0_f32; 128 * 2];
    engine.process(&mut output, 128).unwrap();

    assert!(
        output.iter().any(|sample| sample.abs() > 0.1),
        "monitored input should reach the master output: {output:?}"
    );
}

#[test]
fn recording_captures_interleaved_stereo_input() {
    let bus = InputBus::new(48_000);
    bus.set_recording(0, true);
    bus.push_stereo(0, 0.25, -0.5);
    bus.push(0, 0.75);
    let mut recorded = Vec::new();
    assert!(bus.drain_recording_into(0, &mut recorded));
    assert_eq!(recorded, vec![0.25, -0.5, 0.75, 0.75]);
}

#[test]
fn disabled_monitoring_discards_input_without_reaching_the_master() {
    let (doc, track) = doc_with_audio_track();
    let config = EngineConfig::new(48_000, 128);
    let mut control = EngineControl::new();
    let bus = Arc::new(InputBus::new(48_000));
    let mut engine = Engine::new(config);
    engine.attach_control(control.handle());
    engine.attach_input_bus(Arc::clone(&bus));
    engine.set_control_sync_frames(1);
    let plan = notwig_core::dsp::compile(
        &doc,
        &notwig_core::dsp::stock_registry(),
        EngineCaps {
            sample_rate: 48_000,
            max_block_size: 128,
            channels: 2,
        },
    )
    .unwrap();
    engine
        .apply(EngineMsg::UpdateSchedule {
            schedule: Arc::new(PlaybackSchedule::from_doc(&doc)),
        })
        .unwrap();
    engine
        .apply(EngineMsg::ApplyPlan {
            plan: PlanHandle::new(plan),
        })
        .unwrap();
    control.data_mut().set_input_routes(vec![InputRoute {
        track_id: track,
        slot: 0,
    }]);
    control.data_mut().push_track_state(TrackStateUpdate {
        track_id: track,
        mute: false,
        solo_effective: true,
        gain_db: Db(0.0),
        pan: Pan(0.0),
        monitor: MonitorMode::Disabled,
    });
    control.flush();
    for _ in 0..128 {
        bus.push(0, 0.5);
    }

    let mut output = vec![0.0_f32; 128 * 2];
    engine.process(&mut output, 128).unwrap();

    assert!(
        output.iter().all(|sample| sample.abs() < 0.001),
        "disabled monitoring should stay silent: {output:?}"
    );
}

#[test]
fn low_latency_monitoring_uses_the_track_chain_path() {
    let (mut doc, track) = doc_with_audio_track();
    doc.tracks.get_mut(&track).unwrap().io.monitor = MonitorMode::LowLatency;
    let config = EngineConfig::new(48_000, 128);
    let mut control = EngineControl::new();
    let bus = Arc::new(InputBus::new(48_000));
    let mut engine = Engine::new(config);
    engine.attach_control(control.handle());
    engine.attach_input_bus(Arc::clone(&bus));
    engine.set_control_sync_frames(1);
    let plan = notwig_core::dsp::compile(
        &doc,
        &notwig_core::dsp::stock_registry(),
        EngineCaps {
            sample_rate: 48_000,
            max_block_size: 128,
            channels: 2,
        },
    )
    .unwrap();
    engine
        .apply(EngineMsg::UpdateSchedule {
            schedule: Arc::new(PlaybackSchedule::from_doc(&doc)),
        })
        .unwrap();
    engine
        .apply(EngineMsg::ApplyPlan {
            plan: PlanHandle::new(plan),
        })
        .unwrap();
    control.data_mut().set_input_routes(vec![InputRoute {
        track_id: track,
        slot: 0,
    }]);
    control.flush();
    for _ in 0..128 {
        bus.push(0, 0.5);
    }

    let mut high = vec![0.0_f32; 128 * 2];
    engine.process(&mut high, 128).unwrap();
    assert!(
        high.iter().all(|sample| sample.abs() < 0.001),
        "low-latency input must not also enter the high-latency project path"
    );
    let mut low = vec![0.0_f32; 128 * 2];
    engine.process_low_latency_monitor(&mut low, 128).unwrap();

    assert!(
        low.iter().any(|sample| sample.abs() > 0.1),
        "low-latency monitor should render input: {low:?}"
    );
}

#[test]
fn disabling_low_latency_monitoring_falls_back_to_the_enabled_path() {
    let (mut doc, track) = doc_with_audio_track();
    doc.tracks.get_mut(&track).unwrap().io.monitor = MonitorMode::LowLatency;
    let config = EngineConfig::new(48_000, 128);
    let mut control = EngineControl::new();
    let bus = Arc::new(InputBus::new(48_000));
    let mut engine = Engine::new(config);
    engine.attach_control(control.handle());
    engine.attach_input_bus(Arc::clone(&bus));
    engine.set_control_sync_frames(1);
    let plan = notwig_core::dsp::compile(
        &doc,
        &notwig_core::dsp::stock_registry(),
        EngineCaps {
            sample_rate: 48_000,
            max_block_size: 128,
            channels: 2,
        },
    )
    .unwrap();
    engine
        .apply(EngineMsg::UpdateSchedule {
            schedule: Arc::new(PlaybackSchedule::from_doc(&doc)),
        })
        .unwrap();
    engine
        .apply(EngineMsg::ApplyPlan {
            plan: PlanHandle::new(plan),
        })
        .unwrap();
    control.data_mut().set_input_routes(vec![InputRoute {
        track_id: track,
        slot: 0,
    }]);
    control.data_mut().set_low_latency_monitoring(false);
    control.flush();
    for _ in 0..128 {
        bus.push(0, 0.5);
    }

    let mut high = vec![0.0_f32; 128 * 2];
    engine.process(&mut high, 128).unwrap();
    let mut low = vec![1.0_f32; 128 * 2];
    engine.process_low_latency_monitor(&mut low, 128).unwrap();

    assert!(high.iter().any(|sample| sample.abs() > 0.1));
    assert!(low.iter().all(|sample| sample.abs() < 0.001));
}
