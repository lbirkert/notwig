use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use notwig_core::dsp::devices::{ProcessContext, ProcessStatus};
use notwig_core::dsp::{
    CompiledPlan, DeviceFactory, DeviceProcessor, EngineCaps, NoteEvent, PlanDevice,
    instantiate_stock_params,
};
use notwig_core::ids::{Device as DeviceMarker, DeviceChain, Track};
use notwig_core::protocol::{Command, CommandPayload, PlanHandle, PlaybackSchedule, RtEvent};
use notwig_core::tracks::TrackKind;
use notwig_core::{DeviceChainId, DeviceKind, Id, Meta, ProjectDoc, SampleRate, TrackId, apply};
use notwig_engine::{Engine, EngineConfig};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn base_doc() -> ProjectDoc {
    let meta = Meta {
        name: "Status".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    ProjectDoc::new_template(meta, id::<Track>(1), id::<DeviceChain>(100)).unwrap()
}

fn add_track(doc: &mut ProjectDoc, n: u128, kind: TrackKind) -> (TrackId, DeviceChainId) {
    let track = id::<Track>(n);
    let chain = id::<DeviceChain>(n + 1000);
    let result = apply(
        doc,
        &Command::new(CommandPayload::CreateTrack {
            id: track,
            chain_id: chain,
            kind,
            name: format!("Track {n}"),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap();
    *doc = result.doc;
    (track, chain)
}

fn insert_device(doc: &mut ProjectDoc, chain: DeviceChainId, n: u128, type_id: &str) {
    let result = apply(
        doc,
        &Command::new(CommandPayload::InsertDevice {
            id: id::<DeviceMarker>(n),
            type_id: type_id.into(),
            version: None,
            kind: DeviceKind::AudioFx,
            target_chain: chain,
            index: None,
            child_chain_ids: vec![],
            name: None,
        }),
    )
    .unwrap();
    *doc = result.doc;
}

fn prepare_engine(
    doc: &ProjectDoc,
    factory: Arc<dyn DeviceFactory>,
    config: EngineConfig,
) -> Engine {
    let caps = EngineCaps {
        sample_rate: config.sample_rate,
        max_block_size: config.max_block_size,
        channels: 2,
    };
    let plan = notwig_core::dsp::compile(doc, &notwig_core::dsp::stock_registry(), caps).unwrap();
    let mut engine = Engine::with_factory(config, factory);
    engine
        .apply(notwig_core::protocol::EngineMsg::UpdateSchedule {
            schedule: Arc::new(PlaybackSchedule::from_doc(doc)),
        })
        .unwrap();
    engine
        .apply(notwig_core::protocol::EngineMsg::ApplyPlan {
            plan: PlanHandle::new(plan),
        })
        .unwrap();
    engine
        .apply(notwig_core::protocol::EngineMsg::SetTransport {
            playing: true,
            playhead_samples: 0,
            loop_enabled: false,
            loop_start_samples: None,
            loop_end_samples: None,
            stop: false,
            metronome: None,
        })
        .unwrap();
    engine
}

enum BadKind {
    NonFiniteOverload,
    ClaimsSilence,
}

struct BadProcessor {
    kind: BadKind,
}

impl DeviceProcessor for BadProcessor {
    fn prepare(
        &mut self,
        _config: notwig_core::dsp::RenderConfig,
    ) -> Result<(), notwig_core::dsp::GraphError> {
        Ok(())
    }

    fn reset(&mut self) {}

    fn set_param(&mut self, _param_id: notwig_core::ParamId, _value: &notwig_core::ParamValue) {}

    fn process(
        &mut self,
        _ctx: &ProcessContext<'_>,
        audio: &mut [f32],
        _events: &[NoteEvent],
    ) -> ProcessStatus {
        match self.kind {
            BadKind::NonFiniteOverload => {
                audio.fill(f32::NAN);
                ProcessStatus {
                    overloaded: true,
                    produced_silence: false,
                }
            }
            BadKind::ClaimsSilence => {
                audio.fill(0.5);
                ProcessStatus {
                    overloaded: false,
                    produced_silence: true,
                }
            }
        }
    }
}

struct BadFactory;

impl DeviceFactory for BadFactory {
    fn create(
        &self,
        _plan: &CompiledPlan,
        device: &PlanDevice,
    ) -> Option<Box<dyn DeviceProcessor>> {
        let kind = match device.type_id.as_str() {
            "test.nonfinite" => BadKind::NonFiniteOverload,
            "test.silence" => BadKind::ClaimsSilence,
            _ => {
                return instantiate_stock_params(
                    &device.type_id,
                    &device.spec,
                    &device.param_values,
                );
            }
        };
        Some(Box::new(BadProcessor { kind }))
    }
}

#[test]
fn overloaded_output_is_sanitized_and_reported() {
    let mut doc = base_doc();
    let (_track, chain) = add_track(&mut doc, 2, TrackKind::Audio);
    insert_device(&mut doc, chain, 300, "test.nonfinite");

    let mut engine = prepare_engine(&doc, Arc::new(BadFactory), EngineConfig::new(48_000, 128));
    let output = engine.render_offline(0, 128).unwrap();
    assert!(
        output.iter().all(|sample| sample.is_finite()),
        "non-finite device output leaked through"
    );
    assert!(output.iter().all(|sample| sample.abs() < 1e-9));

    let mut events = Vec::new();
    engine.drain_events(&mut events);
    assert!(
        events.iter().any(|event| matches!(
            event,
            RtEvent::Error { code, .. } if code == "overloaded"
        )),
        "overload was not reported: {events:?}"
    );
}

#[test]
fn silence_status_clears_stale_device_output() {
    let mut doc = base_doc();
    let (_track, chain) = add_track(&mut doc, 2, TrackKind::Audio);
    insert_device(&mut doc, chain, 300, "test.silence");

    let mut engine = prepare_engine(&doc, Arc::new(BadFactory), EngineConfig::new(48_000, 128));
    let output = engine.render_offline(0, 128).unwrap();
    assert!(
        output.iter().all(|sample| sample.abs() < 1e-9),
        "device output was not silenced despite ProcessStatus::produced_silence"
    );
}

struct MonitorPrepareFailFactory {
    creates: AtomicUsize,
}

struct MonitorPrepareProcessor {
    fail_prepare: bool,
}

impl DeviceProcessor for MonitorPrepareProcessor {
    fn prepare(
        &mut self,
        _config: notwig_core::dsp::RenderConfig,
    ) -> Result<(), notwig_core::dsp::GraphError> {
        if self.fail_prepare {
            Err(notwig_core::dsp::GraphError::InvalidDocument {
                reason: "monitor copy cannot prepare".into(),
            })
        } else {
            Ok(())
        }
    }

    fn reset(&mut self) {}

    fn set_param(&mut self, _param_id: notwig_core::ParamId, _value: &notwig_core::ParamValue) {}

    fn process(
        &mut self,
        _ctx: &ProcessContext<'_>,
        _audio: &mut [f32],
        _events: &[NoteEvent],
    ) -> ProcessStatus {
        ProcessStatus::default()
    }
}

impl DeviceFactory for MonitorPrepareFailFactory {
    fn create(
        &self,
        _plan: &CompiledPlan,
        device: &PlanDevice,
    ) -> Option<Box<dyn DeviceProcessor>> {
        if device.type_id == "test.monitor_fail" {
            let index = self.creates.fetch_add(1, Ordering::Relaxed);
            return Some(Box::new(MonitorPrepareProcessor {
                fail_prepare: index > 0,
            }));
        }
        instantiate_stock_params(&device.type_id, &device.spec, &device.param_values)
    }
}

#[test]
fn low_latency_monitor_prepare_failure_is_reported() {
    let mut doc = base_doc();
    let (track, chain) = add_track(&mut doc, 2, TrackKind::Audio);
    insert_device(&mut doc, chain, 300, "test.monitor_fail");
    let result = apply(
        &doc,
        &Command::new(CommandPayload::SetTrackMonitorMode {
            id: track,
            mode: notwig_core::MonitorMode::LowLatency,
        }),
    )
    .unwrap();
    doc = result.doc;

    let factory = Arc::new(MonitorPrepareFailFactory {
        creates: AtomicUsize::new(0),
    });
    let mut engine = prepare_engine(&doc, factory, EngineConfig::new(48_000, 128));

    let mut events = Vec::new();
    engine.drain_events(&mut events);
    assert!(
        events.iter().any(|event| matches!(
            event,
            RtEvent::Error { scope, .. } if scope == "low-latency-monitor"
        )),
        "low-latency monitor prepare failure was not reported: {events:?}"
    );
}
