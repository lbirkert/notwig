#![allow(dead_code)] // Shared by multiple test binaries; each uses a subset.

use std::path::{Path, PathBuf};

use notwig_core::dsp::{DeviceProcessor, ParamSpec, ProcessContext, RenderConfig, TransportInfo};
use notwig_plugins::{
    PluginDescriptor, PluginError, PluginFormat, PluginHost, PluginInstance, PluginState,
};

pub const FIXTURE_NAME: &str = "Notwig Test Plugin";
pub const FIXTURE_VENDOR: &str = "Notwig Tests";
pub const FIXTURE_LATENCY: u32 = 7;
pub const SAMPLE_RATE: u32 = 48_000;
pub const BLOCK_SIZE: usize = 512;

/// Where `scripts/test-plugins.sh` leaves `cargo truce build` output.
///
/// The test process cannot build the fixture itself because LV2 discovery
/// needs `LV2_PATH` set before the process starts. The script owns that and
/// points this variable at the resulting `bundles/` directory.
pub fn fixture_bundles() -> PathBuf {
    let path = std::env::var_os("NOTWIG_PLUGIN_FIXTURE_BUNDLES").map_or_else(
        || {
            let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
            let workspace = manifest
                .parent()
                .and_then(Path::parent)
                .expect("notwig-plugins lives under crates/");
            workspace.join("target/plugin-fixtures/bundles")
        },
        PathBuf::from,
    );
    assert!(
        path.is_dir(),
        "plugin fixture bundles not found at {}; run scripts/test-plugins.sh",
        path.display()
    );
    path
}

pub fn config(sample_rate: u32, max_block_size: usize) -> RenderConfig {
    RenderConfig {
        sample_rate,
        max_block_size,
        channels: 2,
        tempo_bpm: 120.0,
    }
}

pub fn param<'a>(params: &'a [ParamSpec], name: &str) -> &'a ParamSpec {
    params
        .iter()
        .find(|param| param.name == name)
        .unwrap_or_else(|| panic!("fixture did not expose parameter {name:?}"))
}

pub fn prepare(instance: &mut dyn DeviceProcessor, config: RenderConfig) {
    instance
        .prepare(config)
        .unwrap_or_else(|err| panic!("fixture prepare failed: {err}"));
}

/// Render `blocks` stereo blocks of `input` and return the last left sample.
pub fn render_blocks(
    instance: &mut dyn DeviceProcessor,
    config: RenderConfig,
    input: f32,
    blocks: usize,
) -> f32 {
    let transport = TransportInfo::default();
    let context = ProcessContext {
        config,
        sample_position: 0,
        playing: false,
        tempo_bpm: config.tempo_bpm,
        transport: &transport,
    };
    let mut audio = vec![0.0_f32; config.max_block_size * 2];
    let mut last = 0.0;
    for _ in 0..blocks {
        audio.fill(input);
        let status = instance.process(&context, &mut audio, &[]);
        assert!(
            !status.overloaded,
            "fixture reported an overloaded process block"
        );
        assert!(
            !status.produced_silence || input == 0.0,
            "fixture unexpectedly produced silence"
        );
        last = audio[0];
    }
    assert!(last.is_finite(), "fixture output was not finite");
    last
}

pub fn assert_close(actual: f32, expected: f32) {
    let tolerance = expected.abs().max(1.0) * 1.0e-3;
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected}, got {actual}"
    );
}

fn artifact_path(format: PluginFormat, bundles: &Path) -> Option<PathBuf> {
    let extension = match format {
        PluginFormat::Clap => "clap",
        PluginFormat::Vst3 => "vst3",
        PluginFormat::Vst2 => {
            if cfg!(target_os = "macos") {
                "vst"
            } else if cfg!(target_os = "windows") {
                "dll"
            } else {
                "so"
            }
        }
        // LV2 and Audio Unit discovery are registry-based; the staged bundle
        // is only used by the install path / LV2_PATH.
        PluginFormat::Lv2 | PluginFormat::Au | PluginFormat::Au3 => return None,
    };
    let mut matches: Vec<PathBuf> = std::fs::read_dir(bundles)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == extension))
        .collect();
    matches.sort();
    match matches.as_slice() {
        [path] => Some(path.clone()),
        [] => None,
        _ => panic!(
            "expected one {format:?} fixture artifact under {}, found {matches:?}",
            bundles.display()
        ),
    }
}

fn fixture_descriptor(
    found: Vec<PluginDescriptor>,
    format: PluginFormat,
    artifact: Option<&Path>,
) -> Option<PluginDescriptor> {
    let mut matches: Vec<PluginDescriptor> = found
        .into_iter()
        .filter(|descriptor| descriptor.format == format)
        .filter(|descriptor| descriptor.name.contains("Notwig Test Plugin"))
        .filter(|descriptor| {
            artifact.is_none_or(|artifact| {
                descriptor.path == artifact || descriptor.path.starts_with(artifact)
            })
        })
        .collect();
    matches.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.uid.cmp(&b.uid)));
    matches.dedup_by(|a, b| a.type_id == b.type_id);
    match matches.len() {
        0 => None,
        1 => matches.pop(),
        _ => panic!("fixture returned multiple {format:?} descriptors: {matches:#?}"),
    }
}

/// Scan the fixture bundle directory and return the descriptor for `format`.
pub fn descriptor(host: &mut PluginHost, format: PluginFormat) -> PluginDescriptor {
    let bundles = fixture_bundles();
    let artifact = artifact_path(format, &bundles);
    if let Some(artifact) = &artifact {
        assert!(
            artifact.exists(),
            "{format:?} fixture artifact is missing at {}",
            artifact.display()
        );
    }
    // CLAP/VST3 scan_path walks a directory, not a single bundle, so scan the
    // shared fixture root and filter by the format-specific artifact path.
    let found = host.scan(std::slice::from_ref(&bundles));
    fixture_descriptor(found, format, artifact.as_deref()).unwrap_or_else(|| {
        panic!(
            "{format:?} fixture was not discovered under {}. AU/AU3 need `cargo truce install`; \
             LV2 needs LV2_PATH to include the fixture bundles.",
            bundles.display()
        )
    })
}

pub fn expected_latency(format: PluginFormat) -> u32 {
    match format {
        PluginFormat::Vst3 | PluginFormat::Vst2 | PluginFormat::Au | PluginFormat::Au3 => {
            FIXTURE_LATENCY
        }
        // Documented adapter gaps: CLAP latency is not wired through
        // notwig-rack and LV2 path/state support is still partial.
        PluginFormat::Clap | PluginFormat::Lv2 => 0,
    }
}

pub fn check_fixture_descriptor(descriptor: &PluginDescriptor, format: PluginFormat) {
    assert_eq!(descriptor.format, format);
    assert_eq!(descriptor.name, FIXTURE_NAME);
    // The VST3 scanner only reads class metadata from the factory, so its
    // catalog entry has no vendor until an instance is loaded. Every other
    // format carries the vendor through scan.
    if format != PluginFormat::Vst3 {
        assert_eq!(descriptor.vendor, FIXTURE_VENDOR);
    }
    assert_ne!(descriptor.version, "");
    assert_ne!(descriptor.uid, "");
    assert!(!descriptor.is_instrument);
    assert_eq!(
        descriptor.type_id,
        PluginDescriptor::type_id_for(format, &descriptor.uid)
    );
    if !descriptor.path.as_os_str().is_empty() {
        assert!(
            descriptor.path.exists(),
            "descriptor path {} does not exist",
            descriptor.path.display()
        );
        // VST3/CLAP bundles are directories; Windows reports 0 for a
        // directory's metadata length, so only enforce this for files.
        if descriptor.path.is_file() {
            assert!(descriptor.binary_size > 0);
        }
    }
}

/// Rack's VST3 and VST2 adapters expose parameters in normalized `[0, 1]`
/// form; CLAP and AU expose the plugin's native units. The host currently
/// passes values through unchanged, so tests translate to the wire form.
pub fn params_are_normalized(format: PluginFormat) -> bool {
    matches!(format, PluginFormat::Vst3 | PluginFormat::Vst2)
}

pub fn raw_gain(format: PluginFormat, db: f64) -> f64 {
    if params_are_normalized(format) {
        (db + 60.0) / 66.0
    } else {
        db
    }
}

pub fn gain_db(format: PluginFormat, raw: f64) -> f64 {
    if params_are_normalized(format) {
        -60.0 + raw * 66.0
    } else {
        raw
    }
}

pub fn raw_mode(format: PluginFormat, index: u32) -> f64 {
    if params_are_normalized(format) {
        f64::from(index) / 2.0
    } else {
        f64::from(index)
    }
}

pub fn check_params(format: PluginFormat, instance: &dyn PluginInstance) {
    let params = instance.describe_parameters();
    let gain = param(&params, "Gain");
    let expected_default = raw_gain(format, 0.0);
    assert!(
        (gain.default.as_f64().unwrap_or(f64::NAN) - expected_default).abs() < 1.0e-4,
        "{format:?} gain default should be the 0 dB wire value, got {:?}",
        gain.default
    );
    assert_eq!(param(&params, "Bypass").default.as_f64(), Some(0.0));
    assert_eq!(param(&params, "Mode").default.as_f64(), Some(0.0));
}

pub fn wrong_state(state: &PluginState) -> PluginState {
    let mut wrong = state.clone();
    wrong.plugin_uid = "not-the-fixture".into();
    wrong
}

pub fn assert_incompatible_state(result: Result<(), PluginError>) {
    assert!(
        matches!(result, Err(PluginError::StateIncompatible(_))),
        "expected StateIncompatible, got {result:?}"
    );
}
