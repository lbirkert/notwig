use std::sync::Arc;

use notwig_core::ids::{DeviceChain, Track};
use notwig_core::protocol::{Command, CommandPayload, EngineMsg, PlanHandle, PlaybackSchedule};
use notwig_core::*;
use notwig_dsp::EngineCaps;
use notwig_engine::{Engine, EngineConfig};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn doc_at_tempo(tempo_bpm: f64) -> ProjectDoc {
    let meta = Meta {
        name: "Transport".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    ProjectDoc::new_template(meta, id::<Track>(1), id::<DeviceChain>(100)).unwrap()
}

fn apply_plan(engine: &mut Engine, doc: &ProjectDoc) {
    let caps = EngineCaps {
        sample_rate: 48_000,
        max_block_size: 128,
        channels: 2,
    };
    let plan = notwig_dsp::compile(doc, &notwig_dsp::stock_registry(), caps).unwrap();
    engine
        .apply(EngineMsg::UpdateSchedule {
            schedule: Arc::new(PlaybackSchedule::from_doc(doc)),
        })
        .unwrap();
    engine
        .apply(EngineMsg::ApplyPlan {
            plan: PlanHandle::new(plan),
        })
        .unwrap();
}

fn prepare_engine(doc: &ProjectDoc) -> Engine {
    let mut engine = Engine::new(EngineConfig::new(48_000, 128));
    apply_plan(&mut engine, doc);
    engine
}

fn set_transport(
    engine: &mut Engine,
    playing: bool,
    playhead_samples: u64,
    loop_start_samples: Option<u64>,
    loop_end_samples: Option<u64>,
) {
    engine
        .apply(EngineMsg::SetTransport {
            playing,
            playhead_samples,
            loop_enabled: loop_start_samples.is_some() && loop_end_samples.is_some(),
            loop_start_samples,
            loop_end_samples,
            stop: false,
        })
        .unwrap();
}

fn process(engine: &mut Engine, frames: usize) {
    let mut output = vec![0.0_f32; frames * 2];
    engine.process(&mut output, frames).unwrap();
}

#[test]
fn loop_does_not_wrap_a_playhead_started_after_the_loop_end() {
    let doc = doc_at_tempo(120.0);
    let mut engine = prepare_engine(&doc);
    set_transport(&mut engine, true, 10_000, Some(0), Some(4_800));

    process(&mut engine, 256);

    assert_eq!(engine.playhead_samples(), 10_256);
}

#[test]
fn loop_does_not_wrap_a_playhead_started_at_the_loop_end() {
    let doc = doc_at_tempo(120.0);
    let mut engine = prepare_engine(&doc);
    set_transport(&mut engine, true, 4_800, Some(0), Some(4_800));

    process(&mut engine, 128);

    assert_eq!(engine.playhead_samples(), 4_928);
}

#[test]
fn loop_wraps_when_the_playhead_crosses_the_loop_end() {
    let doc = doc_at_tempo(120.0);
    let mut engine = prepare_engine(&doc);
    set_transport(&mut engine, true, 4_600, Some(0), Some(4_800));

    process(&mut engine, 128);
    assert_eq!(engine.playhead_samples(), 4_728);

    process(&mut engine, 128);
    assert_eq!(engine.playhead_samples(), 56);
}

#[test]
fn tempo_change_is_reflected_in_engine_plan_ticks() {
    let mut doc = doc_at_tempo(120.0);
    let mut engine = prepare_engine(&doc);
    set_transport(&mut engine, true, 48_000, None, None);
    process(&mut engine, 128);

    let mut events = Vec::new();
    engine.drain_events(&mut events);
    let old_tick = events
        .iter()
        .find_map(|event| match event {
            RtEvent::TransportTick { playhead_mus, .. } => Some(*playhead_mus),
            _ => None,
        })
        .expect("transport tick");
    assert!((old_tick.as_f64() - 2.005_333).abs() < 1e-3);

    let result = apply(
        &doc,
        &Command::new(CommandPayload::SetTempo { tempo_bpm: 60.0 }),
    )
    .unwrap();
    doc = result.doc;
    apply_plan(&mut engine, &doc);
    set_transport(&mut engine, true, 48_000, None, None);
    process(&mut engine, 128);

    let mut events = Vec::new();
    engine.drain_events(&mut events);
    let new_tick = events
        .iter()
        .find_map(|event| match event {
            RtEvent::TransportTick { playhead_mus, .. } => Some(*playhead_mus),
            _ => None,
        })
        .expect("transport tick");
    assert!(
        (new_tick.as_f64() - 1.002_667).abs() < 1e-3,
        "expected about one beat at 60 BPM, got {}",
        new_tick.as_f64()
    );
}
