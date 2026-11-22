use std::sync::Arc;

use notwig_core::dsp::devices::{ProcessContext, ProcessStatus};
use notwig_core::dsp::{
    CompiledPlan, DeviceProcessor, EngineCaps, NoteEvent, NoteKind, PlanDevice,
    instantiate_stock_params,
};
use notwig_core::ids::{
    Asset as AssetMarker, Clip as ClipMarker, Device as DeviceMarker, DeviceChain,
    Note as NoteMarker, Slice as SliceMarker, Track,
};
use notwig_core::protocol::{
    Command, CommandPayload, EngineMsg, MetronomeState, PlanHandle, PlaybackSchedule,
};
use notwig_core::sequencer::{
    AudioClipData, AudioSlice, ClipKind, ClipPayload, FadeShape, MidiClipData, NoteEntity,
};
use notwig_core::tracks::TrackKind;
use notwig_core::*;
use notwig_engine::{
    DeviceFactory, Engine, EngineConfig, EngineControl, StockDeviceFactory, TransportUpdate,
};
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
                original_name: None,
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
    add_audio_clip_with_gain(doc, track, asset, clip_n, beats, Db(0.0));
}

fn add_audio_clip_with_gain(
    doc: &mut ProjectDoc,
    track: TrackId,
    asset: AssetId,
    clip_n: u128,
    beats: i64,
    gain_db: Db,
) {
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
                gain_db,
                fade_in_samples: 0,
                fade_out_samples: 0,
                fade_shape: FadeShape::Linear,
                slices: Vec::new(),
                slices_enabled: false,
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
    let plan = notwig_core::dsp::compile(doc, &notwig_core::dsp::stock_registry(), caps).unwrap();
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
            metronome: None,
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
            type_id: notwig_core::dsp::stock::TOOL_TYPE_ID.into(),
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
            param_id: notwig_core::dsp::stock::p_output_gain(),
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

    let device_id = id::<DeviceMarker>(300);
    let meter = engine
        .device_meter_snapshot()
        .into_iter()
        .find_map(|(id, meter)| (id == device_id).then_some(meter))
        .expect("device meter snapshot");
    assert!(
        meter.input_peak_l > 0.4,
        "input peak was {}",
        meter.input_peak_l
    );
    assert!(
        meter.output_peak_l < meter.input_peak_l * 0.01,
        "output peak {} was not attenuated below input peak {}",
        meter.output_peak_l,
        meter.input_peak_l
    );
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
            type_id: notwig_core::dsp::stock::TOOL_TYPE_ID.into(),
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
            param_id: notwig_core::dsp::stock::p_output_gain(),
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
            type_id: notwig_core::dsp::stock::CHAIN_TYPE_ID.into(),
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
            type_id: notwig_core::dsp::stock::TOOL_TYPE_ID.into(),
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
            param_id: notwig_core::dsp::stock::p_output_gain(),
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
    fn prepare(
        &mut self,
        config: notwig_core::dsp::RenderConfig,
    ) -> Result<(), notwig_core::dsp::GraphError> {
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
fn sustained_note_survives_a_runtime_swap_at_the_same_playhead() {
    let doc = note_doc_with_sine();
    let config = EngineConfig::new(48_000, 128);
    let mut engine = prepare_engine(&doc, vec![], Arc::new(TestFactory), config);
    let mut producer = EngineControl::new();
    engine.attach_control(producer.handle());
    engine.set_control_sync_frames(1);

    let mut first = vec![0.0_f32; 128 * 2];
    engine.process(&mut first, 128).unwrap();
    assert!(first.iter().any(|sample| sample.abs() > 1e-4));

    let factory: Arc<dyn DeviceFactory> = Arc::new(TestFactory);
    let caps = EngineCaps {
        sample_rate: config.sample_rate,
        max_block_size: config.max_block_size,
        channels: 2,
    };
    let plan = Arc::new(
        notwig_core::dsp::compile(&doc, &notwig_core::dsp::stock_registry(), caps).unwrap(),
    );
    let schedule = Arc::new(PlaybackSchedule::from_doc(&doc));
    let replacement = Engine::prepare_runtime(
        config,
        Arc::clone(&factory),
        plan,
        schedule,
        Arc::new(std::collections::BTreeMap::new()),
    )
    .unwrap();

    let playhead = engine.playhead_samples();
    producer.data_mut().set_prepared_runtime(replacement);
    producer.data_mut().set_transport(TransportUpdate {
        playing: true,
        playhead_samples: playhead,
        loop_enabled: false,
        loop_start_samples: None,
        loop_end_samples: None,
        stop: false,
        metronome: None,
    });
    producer.flush();

    let mut second = vec![0.0_f32; 128 * 2];
    engine.process(&mut second, 128).unwrap();
    assert!(
        second.iter().any(|sample| sample.abs() > 1e-4),
        "the sustained note must be re-primed after a runtime swap"
    );
}

#[test]
fn seeking_into_a_sustained_note_reprimes_it() {
    let doc = note_doc_with_sine();
    let mut engine = prepare_engine(
        &doc,
        vec![],
        Arc::new(TestFactory),
        EngineConfig::new(48_000, 128),
    );

    let mut first = vec![0.0_f32; 128 * 2];
    engine.process(&mut first, 128).unwrap();
    assert!(first.iter().any(|sample| sample.abs() > 1e-4));

    engine
        .apply(EngineMsg::SetTransport {
            playing: true,
            playhead_samples: 6_000,
            loop_enabled: false,
            loop_start_samples: None,
            loop_end_samples: None,
            stop: false,
            metronome: None,
        })
        .unwrap();

    let mut second = vec![0.0_f32; 128 * 2];
    engine.process(&mut second, 128).unwrap();
    assert!(
        second.iter().any(|sample| sample.abs() > 1e-4),
        "seeking inside a sustained note must re-prime it"
    );
}

#[test]
fn metronome_only_update_does_not_cut_a_sustained_note() {
    let doc = note_doc_with_sine();
    let mut engine = prepare_engine(
        &doc,
        vec![],
        Arc::new(TestFactory),
        EngineConfig::new(48_000, 128),
    );
    let mut producer = EngineControl::new();
    engine.attach_control(producer.handle());
    engine.set_control_sync_frames(1);

    let mut first = vec![0.0_f32; 128 * 2];
    engine.process(&mut first, 128).unwrap();
    assert!(first.iter().any(|sample| sample.abs() > 1e-4));

    producer.data_mut().set_metronome(MetronomeState {
        enabled: true,
        beats_per_bar: 4,
        beat_length: TimeMus::new(1, 1),
        bar_length: TimeMus::new(4, 1),
    });
    producer.flush();

    let mut second = vec![0.0_f32; 128 * 2];
    engine.process(&mut second, 128).unwrap();
    assert!(
        second.iter().any(|sample| sample.abs() > 1e-4),
        "toggling the metronome must not emit all-notes-off"
    );
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
            metronome: None,
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

#[test]
fn solo_mutes_other_tracks() {
    let mut doc = base_doc();
    let (first, _) = add_track(&mut doc, 2, TrackKind::Audio);
    let (second, _) = add_track(&mut doc, 3, TrackKind::Audio);
    let loud = add_asset(&mut doc, 501, 24_000);
    let quiet = add_asset(&mut doc, 502, 24_000);
    add_audio_clip(&mut doc, first, loud, 30, 1);
    add_audio_clip(&mut doc, second, quiet, 31, 1);
    let result = apply(
        &doc,
        &Command::new(CommandPayload::SetTrackMixer {
            id: second,
            volume_db: None,
            pan: None,
            mute: None,
            solo: Some(true),
        }),
    )
    .unwrap();
    doc = result.doc;
    let mut engine = prepare_engine(
        &doc,
        vec![
            (loud, audio_buffer(24_000, 0.5)),
            (quiet, audio_buffer(24_000, 0.25)),
        ],
        Arc::new(StockDeviceFactory),
        EngineConfig::new(48_000, 256),
    );
    let output = engine.render_offline(0, 24_000).unwrap();
    let peak = output
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    let pan = std::f32::consts::FRAC_1_SQRT_2;
    assert!(
        (peak - 0.25 * pan).abs() < 0.01,
        "soloed track peak was {peak}"
    );
}
#[test]
fn empty_slice_set_renders_silence() {
    let mut doc = base_doc();
    let (track, _) = add_track(&mut doc, 2, TrackKind::Audio);
    let asset = add_asset(&mut doc, 551, 48_000);
    add_audio_clip(&mut doc, track, asset, 41, 2);
    let result = apply(
        &doc,
        &Command::new(CommandPayload::SetClipSlices {
            clip_id: id::<ClipMarker>(41),
            slices: Vec::new(),
        }),
    )
    .unwrap();
    doc = result.doc;
    let mut engine = prepare_engine(
        &doc,
        vec![(asset, audio_buffer(48_000, 0.5))],
        Arc::new(StockDeviceFactory),
        EngineConfig::new(48_000, 256),
    );
    let output = engine.render_offline(0, 48_000).unwrap();
    let peak = output
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    assert!(peak < 1e-4, "empty slice set should be silent, peak {peak}");
}

#[test]
fn metronome_clicks_on_beats_and_accents_bar_starts() {
    let doc = base_doc();
    let mut engine = prepare_engine(
        &doc,
        Vec::new(),
        Arc::new(StockDeviceFactory),
        EngineConfig::new(48_000, 256),
    );
    engine
        .apply(EngineMsg::SetTransport {
            playing: true,
            playhead_samples: 0,
            loop_enabled: false,
            loop_start_samples: None,
            loop_end_samples: None,
            stop: false,
            metronome: Some(notwig_core::protocol::MetronomeState {
                enabled: true,
                beats_per_bar: 4,
                beat_length: TimeMus::new(1, 1),
                bar_length: TimeMus::new(4, 1),
            }),
        })
        .unwrap();
    let output = engine.render_offline(0, 48_000).unwrap();
    assert!(output[100 * 2].abs() > 0.01, "bar click should be audible");
    assert!(
        output[10_000 * 2].abs() < 1e-4,
        "between beats should be silent"
    );
    assert!(
        output[24_000 * 2 + 100].abs() > 0.01,
        "second beat click should be audible"
    );
}

#[test]
fn metronome_uses_the_time_signature_denominator() {
    let doc = base_doc();
    let mut engine = prepare_engine(
        &doc,
        Vec::new(),
        Arc::new(StockDeviceFactory),
        EngineConfig::new(48_000, 256),
    );
    engine
        .apply(EngineMsg::SetTransport {
            playing: true,
            playhead_samples: 0,
            loop_enabled: false,
            loop_start_samples: None,
            loop_end_samples: None,
            stop: false,
            metronome: Some(notwig_core::protocol::MetronomeState {
                enabled: true,
                beats_per_bar: 6,
                beat_length: TimeMus::new(1, 2),
                bar_length: TimeMus::new(3, 1),
            }),
        })
        .unwrap();
    let output = engine.render_offline(0, 80_000).unwrap();
    assert!(
        output[12_100 * 2].abs() > 0.01,
        "6/8 should click on the second eighth note"
    );
    assert!(
        output[73_100 * 2].abs() > 0.01,
        "the next 6/8 bar should be accented"
    );
}

#[test]
fn runtime_chunks_blocks_larger_than_its_prepared_buffer() {
    let doc = base_doc();
    let small = EngineConfig::new(48_000, 512);
    let caps = EngineCaps {
        sample_rate: small.sample_rate,
        max_block_size: small.max_block_size,
        channels: 2,
    };
    let plan = Arc::new(
        notwig_core::dsp::compile(&doc, &notwig_core::dsp::stock_registry(), caps).unwrap(),
    );
    let schedule = Arc::new(PlaybackSchedule::from_doc(&doc));
    let factory: Arc<dyn DeviceFactory> = Arc::new(StockDeviceFactory);
    let prepared = Engine::prepare_runtime(
        small,
        Arc::clone(&factory),
        plan,
        schedule,
        Arc::new(std::collections::BTreeMap::new()),
    )
    .unwrap();
    let mut engine = Engine::from_prepared(
        EngineConfig::new(48_000, 2048),
        factory,
        prepared,
        notwig_engine::TransportRuntime::default(),
        false,
    );
    let mut output = vec![0.0_f32; 2048 * 2];
    engine.process(&mut output, 2048).unwrap();
}

#[test]
fn audio_slices_render_in_their_clip_local_regions() {
    let mut doc = base_doc();
    let (track, _) = add_track(&mut doc, 2, TrackKind::Audio);
    let asset = add_asset(&mut doc, 550, 48_000);
    add_audio_clip(&mut doc, track, asset, 40, 2);
    let result = apply(
        &doc,
        &Command::new(CommandPayload::SetClipSlices {
            clip_id: id::<ClipMarker>(40),
            slices: vec![
                AudioSlice {
                    id: id::<SliceMarker>(1),
                    start: Rational::zero(),
                    source_start_sample: 0,
                    source_end_sample: 8_000,
                    silent_prefix_samples: 0,
                    gain_db: Db(0.0),
                    asset_id: None,
                },
                AudioSlice {
                    id: id::<SliceMarker>(2),
                    start: Rational::new(1, 1),
                    source_start_sample: 40_000,
                    source_end_sample: 48_000,
                    silent_prefix_samples: 0,
                    gain_db: Db(0.0),
                    asset_id: None,
                },
            ],
        }),
    )
    .unwrap();
    doc = result.doc;
    let mut engine = prepare_engine(
        &doc,
        vec![(asset, audio_buffer(48_000, 0.5))],
        Arc::new(StockDeviceFactory),
        EngineConfig::new(48_000, 256),
    );
    let output = engine.render_offline(0, 48_000).unwrap();
    assert!(output[1_000 * 2].abs() > 0.05, "first slice should render");
    assert!(output[16_000 * 2].abs() < 1e-4, "gap should be silent");
    assert!(
        output[30_000 * 2].abs() > 0.05,
        "second slice should render"
    );
}

#[test]
fn audio_event_silent_prefix_renders_before_the_source_audio() {
    let mut doc = base_doc();
    let (track, _) = add_track(&mut doc, 2, TrackKind::Audio);
    let asset = add_asset(&mut doc, 554, 48_000);
    add_audio_clip(&mut doc, track, asset, 44, 2);
    let result = apply(
        &doc,
        &Command::new(CommandPayload::SetClipSlices {
            clip_id: id::<ClipMarker>(44),
            slices: vec![AudioSlice {
                id: id::<SliceMarker>(1),
                start: Rational::zero(),
                source_start_sample: 0,
                source_end_sample: 8_000,
                silent_prefix_samples: 24_000,
                gain_db: Db(0.0),
                asset_id: None,
            }],
        }),
    )
    .unwrap();
    doc = result.doc;
    let mut engine = prepare_engine(
        &doc,
        vec![(asset, audio_buffer(48_000, 0.5))],
        Arc::new(StockDeviceFactory),
        EngineConfig::new(48_000, 256),
    );
    let output = engine.render_offline(0, 48_000).unwrap();
    assert!(
        output[1_000 * 2].abs() < 1e-4,
        "silent prefix should render nothing"
    );
    assert!(
        output[26_000 * 2].abs() > 0.05,
        "audio after the silent prefix should render"
    );
    assert!(
        output[40_000 * 2].abs() < 1e-4,
        "event should end after its source window"
    );
}

#[test]
fn audio_event_starting_before_the_clip_plays_from_the_visible_source_offset() {
    let mut doc = base_doc();
    let (track, _) = add_track(&mut doc, 2, TrackKind::Audio);
    let asset = add_asset(&mut doc, 552, 48_000);
    add_audio_clip(&mut doc, track, asset, 42, 2);
    let result = apply(
        &doc,
        &Command::new(CommandPayload::SetClipSlices {
            clip_id: id::<ClipMarker>(42),
            slices: vec![AudioSlice {
                id: id::<SliceMarker>(1),
                start: Rational::new(-1, 1),
                source_start_sample: 0,
                source_end_sample: 48_000,
                silent_prefix_samples: 0,
                gain_db: Db(0.0),
                asset_id: None,
            }],
        }),
    )
    .unwrap();
    doc = result.doc;
    let mut samples = vec![0.0; 48_000 * 2];
    for frame in 0..48_000 {
        let value = frame as f32 / 48_000.0;
        samples[frame * 2] = value;
        samples[frame * 2 + 1] = value;
    }
    let mut engine = prepare_engine(
        &doc,
        vec![(
            asset,
            AudioAssetBuffer {
                sample_rate: 48_000,
                channels: 2,
                samples,
            },
        )],
        Arc::new(StockDeviceFactory),
        EngineConfig::new(48_000, 256),
    );
    let output = engine.render_offline(0, 48_000).unwrap();
    // The event starts one beat (24_000 samples) before the clip. The hidden
    // part is skipped, so the first audible sample comes from source 24_000 and
    // the ramp is at 0.5; 12_000 samples later it must be at 0.75.
    assert!(
        output[0] > 0.1,
        "expected the visible source offset to be audible, got {}",
        output[0]
    );
    let ratio = output[12_000 * 2] / output[0];
    assert!(
        (ratio - 1.5).abs() < 0.02,
        "expected source samples 24_000 then 36_000, ratio {ratio}"
    );
}

#[test]
fn audio_event_starting_after_the_clip_renders_silence() {
    let mut doc = base_doc();
    let (track, _) = add_track(&mut doc, 2, TrackKind::Audio);
    let asset = add_asset(&mut doc, 553, 48_000);
    add_audio_clip(&mut doc, track, asset, 43, 2);
    let result = apply(
        &doc,
        &Command::new(CommandPayload::SetClipSlices {
            clip_id: id::<ClipMarker>(43),
            slices: vec![AudioSlice {
                id: id::<SliceMarker>(1),
                start: Rational::new(3, 1),
                source_start_sample: 0,
                source_end_sample: 48_000,
                silent_prefix_samples: 0,
                gain_db: Db(0.0),
                asset_id: None,
            }],
        }),
    )
    .unwrap();
    doc = result.doc;
    let mut engine = prepare_engine(
        &doc,
        vec![(asset, audio_buffer(48_000, 0.5))],
        Arc::new(StockDeviceFactory),
        EngineConfig::new(48_000, 256),
    );
    let output = engine.render_offline(0, 48_000).unwrap();
    let peak = output
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    assert!(
        peak < 1e-4,
        "event beyond the clip should be silent, {peak}"
    );
}

fn note_doc_with_sine() -> ProjectDoc {
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
    apply(
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
    .unwrap()
    .doc
}

#[test]
fn process_chunks_oversized_device_blocks() {
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
            loop_enabled: false,
            loop_start_samples: None,
            loop_end_samples: None,
            stop: false,
            metronome: None,
        })
        .unwrap();
    let mut output = vec![0.0_f32; 512 * 2];
    engine.process(&mut output, 512).unwrap();
    assert_eq!(engine.playhead_samples(), 512);
}

#[test]
fn stopping_transport_sends_note_off() {
    let doc = note_doc_with_sine();
    let mut engine = prepare_engine(
        &doc,
        vec![],
        Arc::new(TestFactory),
        EngineConfig::new(48_000, 128),
    );
    engine
        .apply(EngineMsg::SetTransport {
            playing: true,
            playhead_samples: 0,
            loop_enabled: false,
            loop_start_samples: None,
            loop_end_samples: None,
            stop: false,
            metronome: None,
        })
        .unwrap();
    let mut first = vec![0.0_f32; 128 * 2];
    engine.process(&mut first, 128).unwrap();
    assert!(first.iter().any(|sample| sample.abs() > 1e-4));
    engine
        .apply(EngineMsg::SetTransport {
            playing: false,
            playhead_samples: engine.playhead_samples(),
            loop_enabled: false,
            loop_start_samples: None,
            loop_end_samples: None,
            stop: false,
            metronome: None,
        })
        .unwrap();
    let mut stopped = vec![0.0_f32; 128 * 2];
    engine.process(&mut stopped, 128).unwrap();
    assert!(stopped.iter().all(|sample| sample.abs() < 1e-4));
}

#[test]
fn safety_clipper_caps_runaway_audio_at_ten_db() {
    let mut doc = base_doc();
    let (track, _) = add_track(&mut doc, 2, TrackKind::Audio);
    let asset = add_asset(&mut doc, 560, 24_000);
    add_audio_clip_with_gain(&mut doc, track, asset, 60, 1, Db(40.0));

    let mut engine = prepare_engine(
        &doc,
        vec![(asset, audio_buffer(24_000, 1.0))],
        Arc::new(StockDeviceFactory),
        EngineConfig::new(48_000, 256),
    );
    let output = engine.render_offline(0, 24_000).unwrap();
    let ceiling = 10.0_f32.powf(10.0 / 20.0);

    assert!(
        output.iter().all(|sample| sample.abs() <= ceiling + 1.0e-5),
        "output must stay below the +10 dB safety ceiling"
    );
    assert!(
        output
            .iter()
            .any(|sample| (sample.abs() - ceiling).abs() < 1.0e-3),
        "a +40 dB clip must reach the safety ceiling"
    );
}
