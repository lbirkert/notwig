#![forbid(unsafe_code)]

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{Parser, Subcommand};
use notwig_app::{gui, logging};
use notwig_core::dsp::EngineCaps;
use notwig_core::ids::{Asset, DeviceChain, Track};
use notwig_core::protocol::{Command, CommandPayload, EngineMsg, PlanHandle, PlaybackSchedule};
use notwig_core::sequencer::{AudioClipData, ClipPayload, FadeShape};
use notwig_core::tracks::TrackKind;
use notwig_core::{
    AssetKind, AudioAssetBuffer, ClipKind, Db, Id, Meta, ProjectDoc, RangeMus, Rational,
    SampleRate, TrackId, apply,
};
use notwig_engine::{Engine, EngineConfig, PluginDeviceFactory};
use notwig_project::{ProjectError, import_asset, open, save, write_wav_f32};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("project error: {0}")]
    Project(#[from] ProjectError),
    #[error("core error: {0}")]
    Core(#[from] notwig_core::CoreError),
    #[error("engine error: {0}")]
    Engine(#[from] notwig_engine::EngineError),
    #[error("graph error: {0}")]
    Graph(#[from] notwig_core::dsp::GraphError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid argument: {0}")]
    Argument(String),
    #[error("gui error: {0}")]
    Gui(String),
}

/// Notwig digital audio workstation.
#[derive(Debug, Parser)]
#[command(name = "notwig", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<CliCommand>,
}

#[derive(Debug, Subcommand)]
enum CliCommand {
    /// Open the graphical editor.
    Ui {
        /// Project bundle to open.
        #[arg(value_name = "BUNDLE")]
        bundle: Option<PathBuf>,
    },
    /// Create a new project bundle.
    New {
        /// Path of the bundle to create.
        #[arg(value_name = "BUNDLE")]
        bundle: PathBuf,
    },
    /// Generate a demo project and render it to WAV.
    Demo {
        /// Directory for the bundle, source WAV, and rendered WAV.
        #[arg(value_name = "DIR", default_value = "notwig-demo")]
        dir: PathBuf,
    },
    /// Render a project bundle to WAV.
    Render {
        /// Project bundle to render.
        #[arg(value_name = "BUNDLE")]
        bundle: PathBuf,
        /// Destination WAV file.
        #[arg(value_name = "OUT")]
        out: PathBuf,
        /// Start beat.
        #[arg(long, value_name = "BEATS", default_value_t = 0.0)]
        start: f64,
        /// End beat (defaults to the arrangement end).
        #[arg(long, value_name = "BEATS")]
        end: Option<f64>,
    },
    /// Print project information.
    Info {
        /// Project bundle to inspect.
        #[arg(value_name = "BUNDLE")]
        bundle: PathBuf,
    },
    /// Scan installed plugins into a cache file.
    #[command(hide = true)]
    ScanPlugins {
        /// Destination cache file.
        #[arg(value_name = "CACHE")]
        cache: PathBuf,
    },
    /// Measure output-to-input loopback latency.
    Loopback {
        /// Audio driver name.
        #[arg(long, value_name = "NAME")]
        driver: Option<String>,
        /// Output device name.
        #[arg(long, value_name = "NAME")]
        device: Option<String>,
        /// Input device name.
        #[arg(long, value_name = "NAME")]
        input_device: Option<String>,
        /// Sample rate in Hz.
        #[arg(long, value_name = "HZ")]
        rate: Option<u32>,
        /// Measurement duration in seconds.
        #[arg(long, value_name = "SECONDS", default_value_t = 6.0)]
        duration: f64,
        /// Click interval in milliseconds.
        #[arg(long, value_name = "MS", default_value_t = 500)]
        interval_ms: u64,
        /// Save the captured input to a WAV file.
        #[arg(long, value_name = "PATH")]
        out: Option<PathBuf>,
    },
    /// Internal engine process entrypoint.
    #[command(hide = true)]
    EngineHost,
    /// Internal plugin load probe entrypoint.
    #[command(hide = true)]
    LoadPlugin,
    /// Internal plugin metadata probe entrypoint.
    #[command(hide = true)]
    ProbePlugin,
}

fn main() {
    logging::init();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "Notwig starting");
    let cli = Cli::parse();
    if let Err(err) = run(cli) {
        tracing::error!(error = %err, "notwig failed");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), AppError> {
    match cli.command {
        None => gui::run(None).map_err(AppError::Gui),
        Some(CliCommand::Ui { bundle }) => gui::run(bundle).map_err(AppError::Gui),
        Some(CliCommand::New { bundle }) => {
            let name = bundle
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("Untitled");
            let doc = create_project(name)?;
            let saved = save(&doc, &bundle)?;
            println!("created {}", saved.path.display());
            Ok(())
        }
        Some(CliCommand::Demo { dir }) => run_demo(&dir),
        Some(CliCommand::Render {
            bundle,
            out,
            start,
            end,
        }) => render_bundle(&bundle, &out, start, end),
        Some(CliCommand::Info { bundle }) => print_info(&bundle),
        Some(CliCommand::ScanPlugins { cache }) => scan_plugins(&cache),
        Some(CliCommand::Loopback {
            driver,
            device,
            input_device,
            rate,
            duration,
            interval_ms,
            out,
        }) => run_loopback(
            driver,
            device,
            input_device,
            rate,
            duration,
            interval_ms,
            out,
        ),
        Some(CliCommand::EngineHost) => {
            notwig_app::engine_process::run_engine_host().map_err(AppError::Gui)
        }
        Some(CliCommand::LoadPlugin) => load_plugin_probe(false),
        Some(CliCommand::ProbePlugin) => load_plugin_probe(true),
    }
}

fn run_loopback(
    driver: Option<String>,
    device: Option<String>,
    input_device: Option<String>,
    rate: Option<u32>,
    duration: f64,
    interval_ms: u64,
    out: Option<PathBuf>,
) -> Result<(), AppError> {
    let saved = notwig_app::audio_config::load();
    let mut options = notwig_app::loopback::LoopbackOptions {
        driver: saved.driver,
        device: saved.device,
        input_device,
        sample_rate: rate.unwrap_or_else(|| saved.sample_rate.unwrap_or(48_000)),
        duration_ms: (duration.max(0.5) * 1000.0).round() as u64,
        interval_ms: interval_ms.clamp(100, 10_000),
        output: out,
    };
    if let Some(driver) = driver {
        options.driver = driver;
    }
    if let Some(device) = device {
        options.device = device;
    }
    let report = notwig_app::loopback::run(&options).map_err(AppError::Argument)?;
    println!("{}", report.summary());
    for (index, latency) in report.latencies_ms.iter().enumerate() {
        println!("click {}: {:.3} ms", index + 1, latency);
    }
    if let Some(path) = &options.output {
        println!("recorded input: {}", path.display());
    }
    Ok(())
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
    let eq_id = Id::<notwig_core::ids::Device>::from_uuid(Uuid::new_v4());
    doc = apply(
        &doc,
        &Command::new(CommandPayload::InsertDevice {
            id: eq_id,
            type_id: notwig_core::dsp::stock::EQ_TYPE_ID.into(),
            version: Some(notwig_core::SchemaVersion::V0),
            kind: notwig_core::DeviceKind::AudioFx,
            target_chain: chain_id,
            index: None,
            child_chain_ids: Vec::new(),
            name: None,
        }),
    )?
    .doc;
    for (param, value) in [
        (
            notwig_core::dsp::stock::band_param(1, 0),
            notwig_core::ParamValue::Bool(true),
        ),
        (
            notwig_core::dsp::stock::band_param(1, 2),
            notwig_core::ParamValue::F64(180.0),
        ),
        (
            notwig_core::dsp::stock::band_param(1, 3),
            notwig_core::ParamValue::F64(-5.0),
        ),
        (
            notwig_core::dsp::stock::band_param(2, 0),
            notwig_core::ParamValue::Bool(true),
        ),
        (
            notwig_core::dsp::stock::band_param(2, 2),
            notwig_core::ParamValue::F64(3_500.0),
        ),
        (
            notwig_core::dsp::stock::band_param(2, 3),
            notwig_core::ParamValue::F64(4.0),
        ),
    ] {
        doc = apply(
            &doc,
            &Command::new(CommandPayload::SetParam {
                device_id: eq_id,
                param_id: param,
                value,
                ramp: None,
            }),
        )?
        .doc;
    }
    let tool_id = Id::<notwig_core::ids::Device>::from_uuid(Uuid::new_v4());
    doc = apply(
        &doc,
        &Command::new(CommandPayload::InsertDevice {
            id: tool_id,
            type_id: notwig_core::dsp::stock::TOOL_TYPE_ID.into(),
            version: Some(notwig_core::SchemaVersion::V0),
            kind: notwig_core::DeviceKind::AudioFx,
            target_chain: chain_id,
            index: None,
            child_chain_ids: Vec::new(),
            name: None,
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
                slices: Vec::new(),
                slices_enabled: false,
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
        tracing::warn!(entity = %issue.entity, message = %issue.message, "project issue");
    }
    render_doc(&loaded.doc, bundle, out, start_beat, end_beat)
}

fn scan_plugins(cache_path: &Path) -> Result<(), AppError> {
    let started = std::time::Instant::now();
    let mut host = notwig_plugins::PluginHost::new();
    let descriptors = host.scan(&plugin_scan_paths());
    host.save_cache(cache_path)
        .map_err(|err| AppError::Gui(err.to_string()))?;
    tracing::info!(
        count = descriptors.len(),
        elapsed_ms = started.elapsed().as_millis(),
        "scanned audio plugins"
    );
    Ok(())
}

fn load_plugin_probe(use_probe: bool) -> Result<(), AppError> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(AppError::Io)?;
    let descriptor: notwig_plugins::PluginDescriptor =
        serde_json::from_str(&text).map_err(|err| AppError::Argument(err.to_string()))?;
    tracing::debug!(
        plugin = %descriptor.name,
        format = ?descriptor.format,
        probe = use_probe,
        "running plugin helper"
    );
    let metadata = if use_probe {
        notwig_app::probe_plugin_metadata(&descriptor)
            .map_err(|err| AppError::Gui(err.to_string()))?
    } else {
        let mut host = notwig_plugins::PluginHost::new();
        let instance = host
            .load(&descriptor)
            .map_err(|err| AppError::Gui(err.to_string()))?;
        notwig_plugins::PluginMetadata {
            capabilities: instance.capabilities(),
            params: instance.describe_parameters(),
        }
    };
    println!(
        "{}{}",
        notwig_plugins::PLUGIN_METADATA_PREFIX,
        serde_json::to_string(&metadata).map_err(|err| AppError::Argument(err.to_string()))?
    );
    tracing::debug!(plugin = %descriptor.name, "plugin helper finished");
    Ok(())
}

fn plugin_scan_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        paths.push(home.join("Library/Audio/Plug-Ins/VST3"));
        paths.push(home.join("Library/Audio/Plug-Ins/VST"));
        paths.push(home.join("Library/Audio/Plug-Ins/Components"));
        paths.push(home.join("Library/Audio/Plug-Ins/CLAP"));
    }
    paths.push(PathBuf::from("/Library/Audio/Plug-Ins/VST3"));
    paths.push(PathBuf::from("/Library/Audio/Plug-Ins/VST"));
    paths.push(PathBuf::from("/Library/Audio/Plug-Ins/Components"));
    paths.push(PathBuf::from("/Library/Audio/Plug-Ins/CLAP"));
    paths
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
    let plan = notwig_core::dsp::compile(doc, &notwig_core::dsp::stock_registry(), caps)?;
    let config = EngineConfig::new(sample_rate, 512);
    let mut plugins = notwig_plugins::PluginHost::new();
    let descriptors = if doc.devices.values().any(|device| {
        device.type_id.starts_with("vst3.")
            || device.type_id.starts_with("vst2.")
            || device.type_id.starts_with("au.")
            || device.type_id.starts_with("au3.")
            || device.type_id.starts_with("clap.")
            || device.type_id.starts_with("lv2.")
    }) {
        let paths = plugin_scan_paths();
        let _ = plugins.scan(&paths);
        plugins.cache().descriptors()
    } else {
        Vec::new()
    };
    let mut engine = Engine::with_factory(
        config,
        Arc::new(PluginDeviceFactory::new(
            plugins,
            descriptors,
            std::collections::BTreeMap::new(),
        )),
    );
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
            Err(err) => {
                tracing::warn!(asset = %entry.id, error = %err, "could not load audio asset");
            }
        }
    }
    let output = engine.render_offline(start_samples, end_samples)?;
    write_wav_f32(out, sample_rate, 2, &output)?;
    tracing::info!(
        path = %out.display(),
        frames = output.len() / 2,
        "rendered project to WAV"
    );
    Ok(())
}

fn print_info(bundle: &Path) -> Result<(), AppError> {
    let loaded = open(bundle)?;
    println!("project: {}", loaded.doc.meta.name);
    println!("path: {}", bundle.display());
    println!("sample rate: {}", loaded.doc.meta.sample_rate.value());
    println!("tempo: {} bpm", loaded.doc.meta.tempo_bpm);
    println!(
        "time signature: {}/{}",
        loaded.doc.meta.time_sig_num, loaded.doc.meta.time_sig_den
    );
    println!("tracks: {}", loaded.doc.tracks.len());
    println!("clips: {}", loaded.doc.arranger.clips.len());
    println!("devices: {}", loaded.doc.devices.len());
    println!("assets: {}", loaded.doc.assets.assets.len());
    for issue in &loaded.issues {
        println!("warning: {}: {}", issue.entity, issue.message);
    }
    Ok(())
}

#[allow(dead_code)]
fn _keep(_: AudioAssetBuffer) {}
