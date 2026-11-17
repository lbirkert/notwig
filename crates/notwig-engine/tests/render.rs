use std::sync::Arc;

use notwig_core::ids::{
    Asset as AssetMarker, Clip as ClipMarker, Device as DeviceMarker, DeviceChain,
    Note as NoteMarker, Track,
};
use notwig_core::protocol::{Command, CommandPayload, EngineMsg, PlanHandle, PlaybackSchedule};
use notwig_core::sequencer::{
    AudioClipData, ClipKind, ClipPayload, FadeShape, MidiClipData, NoteEntity,
};
use notwig_core::tracks::TrackKind;
use notwig_core::*;
use notwig_dsp::devices::{ProcessContext, ProcessStatus};
use notwig_dsp::{
    CompiledPlan, DeviceProcessor, EngineCaps, NoteEvent, NoteKind, PlanDevice,
    instantiate_stock_params,
};
use notwig_engine::{DeviceFactory, Engine, EngineConfig, StockDeviceFactory};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn meta() -> Meta {
    Meta {
        name: "Render".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    }
}

fn base_doc() -> ProjectDoc {
    ProjectDoc::new_template(meta(), id::<Track>(1), id::<DeviceChain>(100)).unwrap()
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

fn add_asset(doc: &mut ProjectDoc, n: u128, frames: u64) -> AssetId {
    let asset = id::<AssetMarker>(n);
    let result = apply(
        doc,
        &Command::new(CommandPayload::RegisterAsset {
            asset_id: asset,
            entry: AssetEntry {
                id: asset,
                kind: AssetKind::Audio,
                uri: format!("assets/{asset}.wav"),
                hash: None,
                format: "wav".into(),
                sample_rate: Some(48_000),
                channels: Some(2),
                duration_samples: Some(frames),
                original_sample_rate: Some(48_000),
                original_channels: Some(2),
            },
        }),
    )
    .unwrap();
    *doc = result.doc;
    asset
}

fn add_audio_clip(doc: &mut ProjectDoc, track: TrackId, asset: AssetId, clip_n: u128, beats: i64) {
    let samples = (beats as u64) * 24_000;
    let result = apply(
        doc,
        &Command::new(CommandPayload::CreateClip {
            id: id::<ClipMarker>(clip_n),
            track_id: track,
            kind: ClipKind::Audio,
            range: RangeMus::new(Rational::zero(), Rational::new(beats, 1)).unwrap(),
            payload: ClipPayload::Audio(AudioClipData {
                asset_id: asset,
                source_start_sample: 0,
                source_end_sample: samples,
                gain_db: Db(0.0),
                fade_in_samples: 0,
                fade_out_samples: 0,
                fade_shape: FadeShape::Linear,
            }),
        }),
    )
    .unwrap();
    *doc = result.doc;
}

fn audio_buffer(frames: u64, value: f32) -> AudioAssetBuffer {
    AudioAssetBuffer {
        sample_rate: 48_000,
        channels: 2,
        samples: vec![value; frames as usize * 2],
    }
}

fn prepare_engine(
    doc: &ProjectDoc,
    assets: Vec<(AssetId, AudioAssetBuffer)>,
    factory: Arc<dyn DeviceFactory>,
    config: EngineConfig,
) -> Engine {
    let caps = EngineCaps {
        sample_rate: config.sample_rate,
        max_block_size: config.max_block_size,
        channels: 2,
    };
    let plan = notwig_dsp::compile(doc, &notwig_dsp::stock_registry(), caps).unwrap();
    let mut engine = Engine::with_factory(config, factory);
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
    for (asset_id, buffer) in assets {
        engine
            .apply(EngineMsg::PrepareAudioAsset {
                asset_id,
                buffer: Arc::new(buffer),
            })
            .unwrap();
    }
    engine
        .apply(EngineMsg::SetTransport {
            playing: true,
            playhead_samples: 0,
            loop_enabled: false,
            loop_start_samples: None,
            loop_end_samples: None,
            stop: false,
        })
        .unwrap();
    engine
}

#[test]
fn renders_audio_clip_through_master() {
    let mut doc = base_doc();
    let (track, _) = add_track(&mut doc, 2, TrackKind::Audio);
    let asset = add_asset(&mut doc, 500, 24_000);
    add_audio_clip(&mut doc, track, asset, 20, 1);

    let mut engine = prepare_engine(
        &doc,
        vec![(asset, audio_buffer(24_000, 0.5))],
        Arc::new(StockDeviceFactory),
        EngineConfig::new(48_000, 256),
    );
    let output = engine.render_offline(0, 25_000).unwrap();
    assert_eq!(output.len(), 50_000);
    let peak = output
        .iter()
        .fold(0.0_f32, |acc, value| acc.max(value.abs()));
    assert!(peak > 0.2, "peak was {peak}");
    let tail = &output[48_000..];
    assert!(tail.iter().all(|sample| sample.abs() < 1e-6));
}

#[test]
fn stock_device_changes_audio() {
    let mut doc = base_doc();
    let (track, chain) = add_track(&mut doc, 2, TrackKind::Audio);
    let asset = add_asset(&mut doc, 500, 24_000);
    add_audio_clip(&mut doc, track, asset, 20, 1);
    let result = apply(
        &doc,
        &Command::new(CommandPayload::InsertDevice {
            id: id::<DeviceMarker>(300),
            type_id: notwig_dsp::stock::TOOL_TYPE_ID.into(),
            version: Some(SchemaVersion::V0),
            kind: DeviceKind::AudioFx,
            target_chain: chain,
            index: None,
            child_chain_ids: vec![],
            name: None,
        }),
    )
    .unwrap();
    let mut doc = result.doc;
    let result = apply(
        &doc,
        &Command::new(CommandPayload::SetParam {
            device_id: id::<DeviceMarker>(300),
            param_id: notwig_dsp::stock::p_output_gain(),
            value: ParamValue::F64(-60.0),
            ramp: None,
        }),
    )
    .unwrap();
    doc = result.doc;

    let mut engine = prepare_engine(
        &doc,
        vec![(asset, audio_buffer(24_000, 0.5))],
        Arc::new(StockDeviceFactory),
        EngineConfig::new(48_000, 256),
    );
    let output = engine.render_offline(0, 24_000).unwrap();
    let peak = output
        .iter()
        .fold(0.0_f32, |acc, value| acc.max(value.abs()));
    assert!(peak < 0.01, "device should attenuate output, peak={peak}");
}

#[test]
fn live_param_updates_change_processing() {
    let mut doc = base_doc();
    let (track, chain) = add_track(&mut doc, 2, TrackKind::Audio);
    let asset = add_asset(&mut doc, 500, 24_000);
    add_audio_clip(&mut doc, track, asset, 20, 1);
    let result = apply(
        &doc,
        &Command::new(CommandPayload::InsertDevice {
            id: id::<DeviceMarker>(300),
            type_id: notwig_dsp::stock::TOOL_TYPE_ID.into(),
            version: Some(SchemaVersion::V0),
            kind: DeviceKind::AudioFx,
            target_chain: chain,
            index: None,
            child_chain_ids: vec![],
            name: None,
        }),
    )
    .unwrap();
    let doc = result.doc;
    let mut engine = prepare_engine(
        &doc,
        vec![(asset, audio_buffer(24_000, 0.5))],
        Arc::new(StockDeviceFactory),
        EngineConfig::new(48_000, 256),
    );
    engine
        .apply(EngineMsg::SetParam {
            device_id: id::<DeviceMarker>(300),
            param_id: notwig_dsp::stock::p_output_gain(),
            value: ParamValue::F64(-60.0),
            ramp: None,
        })
        .unwrap();
    let output = engine.render_offline(0, 24_000).unwrap();
    let peak = output
        .iter()
        .fold(0.0_f32, |acc, value| acc.max(value.abs()));
    assert!(peak < 0.01, "live param should attenuate, peak={peak}");
}

#[test]
fn nested_chain_processes_child_devices() {
    let mut doc = base_doc();
    let (track, chain) = add_track(&mut doc, 2, TrackKind::Audio);
    let asset = add_asset(&mut doc, 500, 24_000);
    add_audio_clip(&mut doc, track, asset, 20, 1);
    let child_chain = id::<DeviceChain>(400);
    let result = apply(
        &doc,
        &Command::new(CommandPayload::InsertDevice {
            id: id::<DeviceMarker>(300),
            type_id: notwig_dsp::stock::CHAIN_TYPE_ID.into(),
            version: Some(SchemaVersion::V0),
            kind: DeviceKind::Container,
            target_chain: chain,
            index: None,
            child_chain_ids: vec![child_chain],
            name: None,
        }),
    )
    .unwrap();
    let result = apply(
        &result.doc,
        &Command::new(CommandPayload::InsertDevice {
            id: id::<DeviceMarker>(301),
            type_id: notwig_dsp::stock::TOOL_TYPE_ID.into(),
            version: Some(SchemaVersion::V0),
            kind: DeviceKind::AudioFx,
            target_chain: child_chain,
            index: None,
            child_chain_ids: vec![],
            name: None,
        }),
    )
    .unwrap();
    let doc = result.doc;

    let mut engine = prepare_engine(
        &doc,
        vec![(asset, audio_buffer(24_000, 0.5))],
        Arc::new(StockDeviceFactory),
        EngineConfig::new(48_000, 256),
    );
    engine
        .apply(EngineMsg::SetParam {
            device_id: id::<DeviceMarker>(301),
            param_id: notwig_dsp::stock::p_output_gain(),
            value: ParamValue::F64(-60.0),
            ramp: None,
        })
        .unwrap();
    engine
        .apply(EngineMsg::UpdateSchedule {
            schedule: Arc::new(PlaybackSchedule::from_doc(&doc)),
        })
        .unwrap();
    let output = engine.render_offline(0, 24_000).unwrap();
    let peak = output
        .iter()
        .fold(0.0_f32, |acc, value| acc.max(value.abs()));
    assert!(peak < 0.01, "nested device should attenuate, peak={peak}");
}

#[test]
fn block_size_does_not_change_stock_audio() {
    let mut doc = base_doc();
    let (track, _) = add_track(&mut doc, 2, TrackKind::Audio);
    let asset = add_asset(&mut doc, 500, 24_000);
    add_audio_clip(&mut doc, track, asset, 20, 1);

    let mut small = prepare_engine(
        &doc,
        vec![(asset, audio_buffer(24_000, 0.25))],
        Arc::new(StockDeviceFactory),
        EngineConfig::new(48_000, 64),
    );
    let mut large = prepare_engine(
        &doc,
        vec![(asset, audio_buffer(24_000, 0.25))],
        Arc::new(StockDeviceFactory),
        EngineConfig::new(48_000, 512),
    );
    let a = small.render_offline(0, 24_000).unwrap();
    let b = large.render_offline(0, 24_000).unwrap();
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b.iter()) {
        assert!((x - y).abs() < 1e-6);
    }
}

struct SineInstrument {
    phase: f64,
    freq: f64,
    active: bool,
    velocity: f32,
    sample_rate: f64,
}

impl SineInstrument {
    fn new() -> Self {
        Self {
            phase: 0.0,
            freq: 440.0,
            active: false,
            velocity: 0.0,
            sample_rate: 48_000.0,
        }
    }
}

impl DeviceProcessor for SineInstrument {
    fn prepare(&mut self, config: notwig_dsp::RenderConfig) -> Result<(), notwig_dsp::GraphError> {
        self.sample_rate = f64::from(config.sample_rate);
        Ok(())
    }

    fn reset(&mut self) {
        self.active = false;
        self.phase = 0.0;
    }

    fn set_param(&mut self, _param_id: ParamId, _value: &ParamValue) {}

    fn process(
        &mut self,
        _ctx: &ProcessContext<'_>,
        audio: &mut [f32],
        events: &[NoteEvent],
    ) -> ProcessStatus {
        let mut event_index = 0usize;
        for frame in 0..audio.len() / 2 {
            while event_index < events.len() && events[event_index].sample_offset as usize == frame
            {
                let event = events[event_index];
                match event.kind {
                    NoteKind::On => {
                        self.active = true;
                        self.velocity = event.velocity;
                        self.freq = 440.0 * 2.0_f64.powf((f64::from(event.pitch) - 69.0) / 12.0);
                        self.phase = 0.0;
                    }
                    NoteKind::Off => {
                        self.active = false;
                    }
                }
                event_index += 1;
            }
            if self.active {
                let sample = (self.phase.sin() * f64::from(self.velocity) * 0.25) as f32;
                audio[frame * 2] += sample;
                audio[frame * 2 + 1] += sample;
                self.phase += 2.0 * std::f64::consts::PI * self.freq / self.sample_rate;
                if self.phase > std::f64::consts::TAU {
                    self.phase -= std::f64::consts::TAU;
                }
            }
        }
        ProcessStatus::default()
    }
}

struct TestFactory;

impl DeviceFactory for TestFactory {
    fn create(
        &self,
        _plan: &CompiledPlan,
        device: &PlanDevice,
    ) -> Option<Box<dyn DeviceProcessor>> {
        if device.type_id == "test.sine" {
            return Some(Box::new(SineInstrument::new()));
        }
        instantiate_stock_params(&device.type_id, &device.spec, &device.param_values)
    }
}

#[test]
fn instrument_receives_notes_and_stops_at_note_off() {
    let mut doc = base_doc();
    let (track, chain) = add_track(&mut doc, 2, TrackKind::Instrument);
    let result = apply(
        &doc,
        &Command::new(CommandPayload::InsertDevice {
            id: id::<DeviceMarker>(300),
            type_id: "test.sine".into(),
            version: None,
            kind: DeviceKind::Instrument,
            target_chain: chain,
            index: None,
            child_chain_ids: vec![],
            name: None,
        }),
    )
    .unwrap();
    doc = result.doc;
    let result = apply(
        &doc,
        &Command::new(CommandPayload::CreateClip {
            id: id::<ClipMarker>(20),
            track_id: track,
            kind: ClipKind::Midi,
            range: RangeMus::new(Rational::zero(), Rational::new(1, 1)).unwrap(),
            payload: ClipPayload::Midi(MidiClipData {
                length: Rational::new(1, 1),
                content_start: Rational::zero(),
                notes: vec![NoteEntity {
                    id: id::<NoteMarker>(30),
                    pitch: 69,
                    start: Rational::zero(),
                    length: Rational::new(1, 2),
                    velocity: Norm01::clamped(1.0),
                    release_velocity: None,
                    expression: std::collections::BTreeMap::default(),
                }],
            }),
        }),
    )
    .unwrap();
    doc = result.doc;

    let mut engine = prepare_engine(
        &doc,
        vec![],
        Arc::new(TestFactory),
        EngineConfig::new(48_000, 128),
    );
    let output = engine.render_offline(0, 24_000).unwrap();
    let first_half_energy: f32 = output[..24_000].iter().map(|s| s.abs()).sum();
    let second_half_energy: f32 = output[24_000..].iter().map(|s| s.abs()).sum();
    assert!(first_half_energy > 10.0, "first {first_half_energy}");
    assert!(second_half_energy < 1e-3, "second {second_half_energy}");
}

#[test]
fn looping_wraps_and_emits_transport_ticks() {
    let mut doc = base_doc();
    let (track, _) = add_track(&mut doc, 2, TrackKind::Audio);
    let asset = add_asset(&mut doc, 500, 24_000);
    add_audio_clip(&mut doc, track, asset, 20, 1);
    let mut engine = prepare_engine(
        &doc,
        vec![(asset, audio_buffer(24_000, 0.5))],
        Arc::new(StockDeviceFactory),
        EngineConfig::new(48_000, 128),
    );
    engine
        .apply(EngineMsg::SetTransport {
            playing: true,
            playhead_samples: 0,
            loop_enabled: true,
            loop_start_samples: Some(0),
            loop_end_samples: Some(4_800),
            stop: false,
        })
        .unwrap();
    let mut output = vec![0.0_f32; 9_600 * 2];
    let mut offset = 0usize;
    while offset < 9_600 {
        let frames = 128.min(9_600 - offset);
        engine
            .process(&mut output[offset * 2..(offset + frames) * 2], frames)
            .unwrap();
        offset += frames;
    }
    assert!(engine.playhead_samples() <= 4_800);
    let first = &output[..4_800];
    let second = &output[4_800 * 2..4_800 * 2 + 4_800];
    assert!(first.iter().any(|s| s.abs() > 0.1));
    assert!(second.iter().any(|s| s.abs() > 0.1));
}

#[test]
fn mute_silences_track_but_not_master() {
    let mut doc = base_doc();
    let (track, _) = add_track(&mut doc, 2, TrackKind::Audio);
    let asset = add_asset(&mut doc, 500, 24_000);
    add_audio_clip(&mut doc, track, asset, 20, 1);
    let result = apply(
        &doc,
        &Command::new(CommandPayload::SetTrackMixer {
            id: track,
            volume_db: None,
            pan: None,
            mute: Some(true),
            solo: None,
        }),
    )
    .unwrap();
    doc = result.doc;
    let mut engine = prepare_engine(
        &doc,
        vec![(asset, audio_buffer(24_000, 0.5))],
        Arc::new(StockDeviceFactory),
        EngineConfig::new(48_000, 256),
    );
    let output = engine.render_offline(0, 24_000).unwrap();
    assert!(output.iter().all(|sample| sample.abs() < 1e-6));
}
