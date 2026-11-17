#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use notwig_core::ids::{Asset, DeviceChain, Track};
use notwig_core::protocol::{Command, CommandPayload, EngineMsg, PlanHandle, PlaybackSchedule};
use notwig_core::sequencer::{AudioClipData, ClipPayload, FadeShape};
use notwig_core::tracks::TrackKind;
use notwig_core::{
    AssetKind, AudioAssetBuffer, ClipKind, Db, Id, Meta, ProjectDoc, RangeMus, Rational,
    SampleRate, TrackId, apply,
};
use notwig_dsp::EngineCaps;
use notwig_engine::{Engine, EngineConfig};
use notwig_project::{ProjectError, import_asset, open, save, write_wav_f32};
use notwig_ui::scene_summary;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("usage: {0}")]
    Usage(String),
    #[error("project error: {0}")]
    Project(#[from] ProjectError),
    #[error("core error: {0}")]
    Core(#[from] notwig_core::CoreError),
    #[error("engine error: {0}")]
    Engine(#[from] notwig_engine::EngineError),
    #[error("graph error: {0}")]
    Graph(#[from] notwig_dsp::GraphError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid argument: {0}")]
    Argument(String),
}

fn main() {
    if let Err(err) = run(std::env::args().skip(1).collect()) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run(args: Vec<String>) -> Result<(), AppError> {
    let Some(command) = args.first().map(String::as_str) else {
        print_help();
        return Ok(());
    };
    match command {
        "new" => {
            let path = args
                .get(1)
                .ok_or_else(|| AppError::Usage("new <bundle.notwig>".into()))?;
            let bundle = PathBuf::from(path);
            let name = bundle
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("Untitled");
            let doc = create_project(name)?;
            let saved = save(&doc, &bundle)?;
            println!("created {}", saved.path.display());
            Ok(())
        }
        "demo" => {
            let dir = PathBuf::from(args.get(1).cloned().unwrap_or_else(|| "notwig-demo".into()));
            run_demo(&dir)
        }
        "render" => {
            let bundle = args
                .get(1)
                .ok_or_else(|| AppError::Usage("render <bundle.notwig> <out.wav>".into()))?;
            let out = args
                .get(2)
                .ok_or_else(|| AppError::Usage("render <bundle.notwig> <out.wav>".into()))?;
            let mut start_beat = 0.0;
            let mut end_beat = None;
            let mut index = 3;
            while index < args.len() {
                match args[index].as_str() {
                    "--start" => {
                        start_beat = parse_f64(args.get(index + 1))?;
                        index += 2;
                    }
                    "--end" => {
                        end_beat = Some(parse_f64(args.get(index + 1))?);
                        index += 2;
                    }
                    other => return Err(AppError::Argument(other.into())),
                }
            }
            render_bundle(Path::new(bundle), Path::new(out), start_beat, end_beat)
        }
        "info" => {
            let bundle = args
                .get(1)
                .ok_or_else(|| AppError::Usage("info <bundle.notwig>".into()))?;
            print_info(Path::new(bundle))
        }
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(AppError::Usage(format!("unknown command '{other}'"))),
    }
}

fn print_help() {
    println!("notwig {}", env!("CARGO_PKG_VERSION"));
    println!("usage:");
    println!("  notwig new <bundle.notwig>");
    println!("  notwig demo [dir]");
    println!("  notwig render <bundle.notwig> <out.wav> [--start beats] [--end beats]");
    println!("  notwig info <bundle.notwig>");
}

fn parse_f64(value: Option<&String>) -> Result<f64, AppError> {
    value
        .ok_or_else(|| AppError::Usage("expected a number".into()))?
        .parse::<f64>()
        .map_err(|err| AppError::Argument(err.to_string()))
}

fn create_project(name: &str) -> Result<ProjectDoc, AppError> {
    let meta = Meta {
        name: name.into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    let master: TrackId = Id::from_uuid(Uuid::new_v4());
    let master_chain = Id::<DeviceChain>::from_uuid(Uuid::new_v4());
    Ok(ProjectDoc::new_template(meta, master, master_chain)?)
}

fn run_demo(dir: &Path) -> Result<(), AppError> {
    std::fs::create_dir_all(dir)?;
    let bundle = dir.join("demo.notwig");
    let source = dir.join("demo-source.wav");
    let sample_rate = 48_000u32;
    let frames = 48_000usize;
    let samples: Vec<f32> = (0..frames)
        .map(|frame| {
            let t = frame as f32 / sample_rate as f32;
            (std::f32::consts::TAU * 220.0 * t).sin() * 0.25
        })
        .collect();
    write_wav_f32(&source, sample_rate, 1, &samples)?;

    let mut doc = create_project("Notwig Demo")?;
    let asset_id = Id::<Asset>::from_uuid(Uuid::new_v4());
    let imported = import_asset(&source, &bundle, asset_id, sample_rate)?;
    doc = apply(
        &doc,
        &Command::new(CommandPayload::RegisterAsset {
            asset_id,
            entry: imported.entry.clone(),
        }),
    )?
    .doc;

    let track_id = Id::<Track>::from_uuid(Uuid::new_v4());
    let chain_id = Id::<DeviceChain>::from_uuid(Uuid::new_v4());
    doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: track_id,
            chain_id,
            kind: TrackKind::Audio,
            name: "Demo Tone".into(),
            parent: None,
            index: None,
            color: None,
        }),
    )?
    .doc;
    let clip_id = Id::<notwig_core::ids::Clip>::from_uuid(Uuid::new_v4());
    doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateClip {
            id: clip_id,
            track_id,
            kind: ClipKind::Audio,
            range: RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
            payload: ClipPayload::Audio(AudioClipData {
                asset_id,
                source_start_sample: 0,
                source_end_sample: u64::from(sample_rate),
                gain_db: Db(0.0),
                fade_in_samples: 480,
                fade_out_samples: 480,
                fade_shape: FadeShape::Linear,
            }),
        }),
    )?
    .doc;
    save(&doc, &bundle)?;
    let out = dir.join("demo.wav");
    render_doc(&doc, &bundle, &out, 0.0, Some(2.0))?;
    println!("created {}", bundle.display());
    println!("rendered {}", out.display());
    println!("source {}", source.display());
    Ok(())
}

fn render_bundle(
    bundle: &Path,
    out: &Path,
    start_beat: f64,
    end_beat: Option<f64>,
) -> Result<(), AppError> {
    let loaded = open(bundle)?;
    for issue in &loaded.issues {
        eprintln!("warning: {}: {}", issue.entity, issue.message);
    }
    render_doc(&loaded.doc, bundle, out, start_beat, end_beat)
}

fn render_doc(
    doc: &ProjectDoc,
    bundle: &Path,
    out: &Path,
    start_beat: f64,
    end_beat: Option<f64>,
) -> Result<(), AppError> {
    let sample_rate = doc.meta.sample_rate.value();
    let map = doc.tempo_map();
    let end_beat = end_beat.unwrap_or_else(|| {
        doc.arranger
            .clips
            .values()
            .map(|clip| clip.range.end.as_f64())
            .fold(4.0_f64, f64::max)
    });
    if end_beat <= start_beat {
        return Err(AppError::Argument("end must be after start".into()));
    }
    let start_samples = (start_beat * map.samples_per_beat()).round() as u64;
    let end_samples = (end_beat * map.samples_per_beat()).round() as u64;

    let caps = EngineCaps {
        sample_rate,
        max_block_size: 512,
        channels: 2,
    };
    let plan = notwig_dsp::compile(doc, &notwig_dsp::stock_registry(), caps)?;
    let config = EngineConfig::new(sample_rate, 512);
    let mut engine = Engine::new(config);
    engine.apply(EngineMsg::UpdateSchedule {
        schedule: Arc::new(PlaybackSchedule::from_doc(doc)),
    })?;
    engine.apply(EngineMsg::ApplyPlan {
        plan: PlanHandle::new(plan),
    })?;
    for entry in doc.assets.assets.values() {
        if entry.kind != AssetKind::Audio {
            continue;
        }
        let path = bundle.join(&entry.uri);
        match notwig_project::decode_audio(&path) {
            Ok(buffer) => {
                engine.apply(EngineMsg::PrepareAudioAsset {
                    asset_id: entry.id,
                    buffer: Arc::new(buffer),
                })?;
            }
            Err(err) => eprintln!("warning: asset {}: {err}", entry.id),
        }
    }
    let output = engine.render_offline(start_samples, end_samples)?;
    write_wav_f32(out, sample_rate, 2, &output)?;
    Ok(())
}

fn print_info(bundle: &Path) -> Result<(), AppError> {
    let loaded = open(bundle)?;
    let summary = scene_summary(&loaded.doc);
    println!("project: {}", loaded.doc.meta.name);
    println!("path: {}", bundle.display());
    println!("sample rate: {}", loaded.doc.meta.sample_rate.value());
    println!("tempo: {} bpm", loaded.doc.meta.tempo_bpm);
    println!(
        "time signature: {}/{}",
        loaded.doc.meta.time_sig_num, loaded.doc.meta.time_sig_den
    );
    println!("tracks: {}", summary.tracks);
    println!("clips: {}", summary.clips);
    println!("devices: {}", summary.devices);
    println!("assets: {}", summary.assets);
    for issue in &loaded.issues {
        println!("warning: {}: {}", issue.entity, issue.message);
    }
    Ok(())
}

#[allow(dead_code)]
fn _keep(_: AudioAssetBuffer) {}
