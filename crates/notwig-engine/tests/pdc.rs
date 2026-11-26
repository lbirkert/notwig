use std::sync::Arc;

use notwig_core::dsp::devices::{ProcessContext, ProcessStatus};
use notwig_core::dsp::{
    CompiledPlan, DeviceProcessor, DeviceRegistry, EngineCaps, GraphError, NoteEvent, PlanDevice,
    RenderConfig, instantiate_stock_params,
};
use notwig_core::ids::{
    Asset as AssetMarker, Clip as ClipMarker, Device as DeviceMarker, DeviceChain,
    Send as SendMarker, Track,
};
use notwig_core::protocol::{Command, CommandPayload, EngineMsg, PlanHandle, PlaybackSchedule};
use notwig_core::sequencer::{AudioClipData, ClipKind, ClipPayload, FadeShape};
use notwig_core::tracks::{Send, TrackKind, TrackOutput};
use notwig_core::*;
use notwig_engine::{DeviceFactory, Engine, EngineConfig};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn meta() -> Meta {
    Meta {
        name: "PDC".into(),
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

fn apply_ok(doc: &ProjectDoc, payload: CommandPayload) -> ProjectDoc {
    apply(doc, &Command::new(payload)).unwrap().doc
}

fn add_track(
    doc: &ProjectDoc,
    n: u128,
    kind: TrackKind,
    parent: Option<TrackId>,
) -> (ProjectDoc, TrackId, DeviceChainId) {
    let track = id::<Track>(n);
    let chain = id::<DeviceChain>(n + 1000);
    let doc = apply_ok(
        doc,
        CommandPayload::CreateTrack {
            id: track,
            chain_id: chain,
            kind,
            name: format!("Track {n}"),
            parent,
            index: None,
            color: None,
        },
    );
    (doc, track, chain)
}

fn insert_latent(doc: &ProjectDoc, n: u128, type_id: &str, chain: DeviceChainId) -> ProjectDoc {
    apply_ok(
        doc,
        CommandPayload::InsertDevice {
            id: id::<DeviceMarker>(n),
            type_id: type_id.into(),
            version: None,
            kind: DeviceKind::AudioFx,
            target_chain: chain,
            index: None,
            child_chain_ids: vec![],
            name: None,
        },
    )
}

fn add_impulse_asset(doc: &ProjectDoc, n: u128, frames: u64) -> (ProjectDoc, AssetId) {
    let asset = id::<AssetMarker>(n);
    let doc = apply_ok(
        doc,
        CommandPayload::RegisterAsset {
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
        },
    );
    (doc, asset)
}

fn add_impulse_clip(
    doc: &ProjectDoc,
    track: TrackId,
    asset: AssetId,
    clip_n: u128,
    frames: u64,
) -> ProjectDoc {
    let source_end = frames;
    apply_ok(
        doc,
        CommandPayload::CreateClip {
            id: id::<ClipMarker>(clip_n),
            track_id: track,
            kind: ClipKind::Audio,
            range: RangeMus::new(Rational::zero(), Rational::new(1, 1)).unwrap(),
            payload: ClipPayload::Audio(AudioClipData {
                asset_id: asset,
                source_start_sample: 0,
                source_end_sample: source_end,
                gain_db: Db(0.0),
                gain_curve: None,
                fade_in_samples: 0,
                fade_out_samples: 0,
                fade_shape: FadeShape::Linear,
                slices: Vec::new(),
                slices_enabled: false,
            }),
        },
    )
}

fn impulse_buffer(frames: u64, at: u64, value: f32) -> AudioAssetBuffer {
    let mut samples = vec![0.0; frames as usize * 2];
    samples[at as usize * 2] = value;
    samples[at as usize * 2 + 1] = value;
    AudioAssetBuffer {
        sample_rate: 48_000,
        channels: 2,
        samples,
    }
}

fn pdc_registry() -> DeviceRegistry {
    let mut registry = notwig_core::dsp::stock_registry();
    for (type_id, latency) in [("test.latent32", 32), ("test.latent64", 64)] {
        let mut spec = notwig_core::dsp::stock::tool_spec();
        spec.type_id = type_id.into();
        spec.latency_samples = latency;
        registry.register(spec);
    }
    registry
}

struct LatentProcessor {
    delay: Vec<f32>,
    pos: usize,
}

impl LatentProcessor {
    fn new(samples: u32) -> Self {
        Self {
            delay: vec![0.0; samples as usize * 2],
            pos: 0,
        }
    }
}

impl DeviceProcessor for LatentProcessor {
    fn prepare(&mut self, _config: RenderConfig) -> Result<(), GraphError> {
        Ok(())
    }

    fn reset(&mut self) {
        self.delay.fill(0.0);
        self.pos = 0;
    }

    fn set_param(&mut self, _param_id: ParamId, _value: &ParamValue) {}

    fn process(
        &mut self,
        _ctx: &ProcessContext<'_>,
        audio: &mut [f32],
        _events: &[NoteEvent],
    ) -> ProcessStatus {
        let len = self.delay.len() / 2;
        if len == 0 {
            return ProcessStatus::default();
        }
        let frames = audio.len() / 2;
        let mut pos = self.pos % len;
        for frame in 0..frames {
            for channel in 0..2 {
                let index = pos * 2 + channel;
                std::mem::swap(&mut audio[frame * 2 + channel], &mut self.delay[index]);
            }
            pos = (pos + 1) % len;
        }
        self.pos = pos;
        ProcessStatus::default()
    }
}

struct PdcFactory;

impl DeviceFactory for PdcFactory {
    fn create(
        &self,
        _plan: &CompiledPlan,
        device: &PlanDevice,
    ) -> Option<Box<dyn DeviceProcessor>> {
        if device.type_id.starts_with("test.latent") {
            return Some(Box::new(LatentProcessor::new(device.spec.latency_samples)));
        }
        instantiate_stock_params(&device.type_id, &device.spec, &device.param_values)
    }
}

fn prepare_engine(
    doc: &ProjectDoc,
    assets: Vec<(AssetId, AudioAssetBuffer)>,
    config: EngineConfig,
) -> Engine {
    let caps = EngineCaps {
        sample_rate: config.sample_rate,
        max_block_size: config.max_block_size,
        channels: 2,
    };
    let plan = notwig_core::dsp::compile(doc, &pdc_registry(), caps).unwrap();
    let mut engine = Engine::with_factory(config, Arc::new(PdcFactory));
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
fn pdc_aligns_latent_track_with_direct_track() {
    let (doc, direct, _) = add_track(&base_doc(), 2, TrackKind::Audio, None);
    let (doc, latent, latent_chain) = add_track(&doc, 3, TrackKind::Audio, None);
    let doc = insert_latent(&doc, 300, "test.latent64", latent_chain);
    let (doc, asset) = add_impulse_asset(&doc, 500, 24_000);
    let doc = add_impulse_clip(&doc, direct, asset, 20, 24_000);
    let doc = add_impulse_clip(&doc, latent, asset, 21, 24_000);

    let mut engine = prepare_engine(
        &doc,
        vec![(asset, impulse_buffer(24_000, 100, 0.5))],
        EngineConfig::new(48_000, 256),
    );
    let output = engine.render_offline(0, 1_000).unwrap();

    assert!(
        output[100 * 2].abs() < 1e-6,
        "direct impulse was not delayed"
    );
    let pan = std::f32::consts::FRAC_1_SQRT_2;
    let aligned = output[164 * 2].abs();
    assert!(
        (aligned - 0.5 * pan * 2.0).abs() < 1e-5,
        "impulses should sum in phase at sample 164, got {aligned}"
    );
}

#[test]
fn pdc_aligns_nested_group_latency() {
    let (doc, group, group_chain) = add_track(&base_doc(), 2, TrackKind::Group, None);
    let doc = insert_latent(&doc, 300, "test.latent32", group_chain);
    let (doc, child, child_chain) = add_track(&doc, 3, TrackKind::Audio, Some(group));
    let doc = insert_latent(&doc, 301, "test.latent64", child_chain);
    let (doc, reference, _) = add_track(&doc, 4, TrackKind::Audio, None);

    let (doc, asset) = add_impulse_asset(&doc, 500, 24_000);
    let doc = add_impulse_clip(&doc, child, asset, 20, 24_000);
    let doc = add_impulse_clip(&doc, reference, asset, 21, 24_000);

    let mut engine = prepare_engine(
        &doc,
        vec![(asset, impulse_buffer(24_000, 100, 0.5))],
        EngineConfig::new(48_000, 256),
    );
    let output = engine.render_offline(0, 1_000).unwrap();

    assert!(output[100 * 2].abs() < 1e-6);
    assert!(output[164 * 2].abs() < 1e-6);
    let pan = std::f32::consts::FRAC_1_SQRT_2;
    let expected = f32::midpoint(pan, pan * pan);
    let aligned = output[(100 + 64 + 32) * 2].abs();
    assert!(
        (aligned - expected).abs() < 1e-5,
        "nested group and child should stay aligned, got {aligned}"
    );
}

#[test]
fn pdc_aligns_send_with_direct_path() {
    let doc = base_doc();
    let master = doc.master_track().unwrap();
    let (doc, group, group_chain) = add_track(&doc, 2, TrackKind::Group, None);
    let doc = insert_latent(&doc, 300, "test.latent64", group_chain);
    let (doc, source, _) = add_track(&doc, 3, TrackKind::Audio, None);
    let doc = apply_ok(
        &doc,
        CommandPayload::SetTrackRoute {
            id: source,
            audio_out: TrackOutput::Track(group),
        },
    );
    let doc = apply_ok(
        &doc,
        CommandPayload::SetSend {
            track_id: source,
            send: Send {
                id: id::<SendMarker>(700),
                target: master,
                amount_db: Db(0.0),
                pre_fader: false,
                enabled: true,
            },
        },
    );
    let (doc, asset) = add_impulse_asset(&doc, 500, 24_000);
    let doc = add_impulse_clip(&doc, source, asset, 20, 24_000);

    let mut engine = prepare_engine(
        &doc,
        vec![(asset, impulse_buffer(24_000, 100, 0.5))],
        EngineConfig::new(48_000, 256),
    );
    let output = engine.render_offline(0, 1_000).unwrap();

    assert!(output[100 * 2].abs() < 1e-6, "send arrived early");
    let pan = std::f32::consts::FRAC_1_SQRT_2;
    let expected = f32::midpoint(pan, pan * pan);
    let aligned = output[164 * 2].abs();
    assert!(
        (aligned - expected).abs() < 1e-5,
        "send and direct path should sum in phase, got {aligned}"
    );
}
