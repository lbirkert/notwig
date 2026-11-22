use std::collections::BTreeMap;
use std::path::PathBuf;

use notwig_core::dsp::{
    CompiledPlan, NoteEvent, NoteKind, PlanDevice, ProcessContext, RenderConfig, TransportInfo,
};
use notwig_core::{DeviceId, DeviceKind, NoteId, ParamValue, TrackId};
use notwig_ext::ExtensionHost;

const LOOKAHEAD_SAMPLES: u32 = 64;

fn manifest_template() -> Option<String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(
        manifest_dir
            .parent()?
            .join("notwig-ext-devices/extension.json"),
    )
    .ok()
}

fn load_host() -> Option<(ExtensionHost, PathBuf)> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest_dir.parent()?.parent()?;
    let wasm = std::fs::read(
        workspace.join("target/wasm32-unknown-unknown/release/notwig_ext_devices.wasm"),
    )
    .ok()?;
    let manifest = manifest_template()?
        .replace("$$ENTRYPOINT", "notwig_ext_devices.wasm")
        .replace("$$TARGET", "wasm");
    let root = std::env::temp_dir().join(format!("notwig-ext-{}", uuid::Uuid::new_v4()));
    let package = root.join("notwig.ext.devices");
    std::fs::create_dir_all(&package).ok()?;
    std::fs::write(package.join("extension.json"), manifest).ok()?;
    std::fs::write(package.join("notwig_ext_devices.wasm"), wasm).ok()?;
    let host = ExtensionHost::load(&root, &BTreeMap::new()).ok()?;
    Some((host, root))
}

fn load_native_host() -> Option<(ExtensionHost, PathBuf)> {
    if notwig_ext_api::target::current() != Some(notwig_ext_api::target::MACOS_AARCH64) {
        return None;
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest_dir.parent()?.parent()?;
    let candidates = [
        workspace.join("target/aarch64-apple-darwin/release/libnotwig_ext_devices.dylib"),
        workspace.join("target/release/libnotwig_ext_devices.dylib"),
    ];
    let library = candidates
        .iter()
        .find_map(|path| std::fs::read(path).ok())?;
    let manifest = manifest_template()?
        .replace("$$ENTRYPOINT", "notwig_ext_devices.dylib")
        .replace("$$TARGET", notwig_ext_api::target::MACOS_AARCH64);
    let root = std::env::temp_dir().join(format!("notwig-ext-native-{}", uuid::Uuid::new_v4()));
    let package = root.join("notwig.ext.devices");
    std::fs::create_dir_all(&package).ok()?;
    std::fs::write(package.join("extension.json"), manifest).ok()?;
    std::fs::write(package.join("notwig_ext_devices.dylib"), library).ok()?;
    let host = ExtensionHost::load(&root, &BTreeMap::new()).ok()?;
    Some((host, root))
}

#[test]
fn built_limiter_extension_loads_and_processes_audio() {
    let Some((host, root)) = load_host() else {
        eprintln!("skipping: build notwig-ext-devices for wasm32-unknown-unknown first");
        return;
    };
    let specs = host.specs();
    assert_eq!(specs.len(), 4);
    let limiter_spec = specs
        .iter()
        .find(|spec| spec.type_id == "notwig.ext.devices.limiter")
        .expect("limiter spec");
    assert_eq!(limiter_spec.name, "Limiter");
    assert!(
        specs
            .iter()
            .any(|spec| spec.type_id == "notwig.ext.devices.gate")
    );
    let synth = specs
        .iter()
        .find(|spec| spec.type_id == "notwig.ext.devices.synth")
        .expect("synth spec");
    assert_eq!(synth.kind, DeviceKind::Instrument);

    let config = RenderConfig {
        sample_rate: 48_000,
        max_block_size: 64,
        channels: 2,
        tempo_bpm: 120.0,
    };
    let plan = CompiledPlan {
        config,
        master: TrackId::nil(),
        order: Vec::new(),
        tracks: BTreeMap::new(),
        devices: BTreeMap::new(),
        chains: BTreeMap::new(),
        total_latency: LOOKAHEAD_SAMPLES,
    };
    let device = PlanDevice {
        id: DeviceId::nil(),
        type_id: "notwig.ext.devices.limiter".into(),
        kind: DeviceKind::AudioFx,
        spec: limiter_spec.clone(),
        param_values: BTreeMap::new(),
        bypass: false,
        child_chains: Vec::new(),
        latency_samples: LOOKAHEAD_SAMPLES,
        resolved: true,
    };
    let mut processor = host
        .factory()
        .create(&plan, &device)
        .expect("limiter processor");
    processor.prepare(config).unwrap();
    let transport = TransportInfo::default();
    let context = ProcessContext {
        config,
        sample_position: 0,
        playing: true,
        tempo_bpm: 120.0,
        transport: &transport,
    };
    let mut audio = vec![2.0_f32; 256];
    let (first, second) = audio.split_at_mut(128);
    assert!(!processor.process(&context, first, &[]).overloaded);
    let status = processor.process(&context, second, &[]);
    assert!(!status.overloaded);
    // The limiter delays by LOOKAHEAD_SAMPLES frames, so inspect the second
    // block after the initial lookahead has filled.
    let peak = second
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    let threshold = 10.0_f32.powf(-6.0 / 20.0);
    assert!(peak <= threshold + 1e-3, "peak {peak} exceeded {threshold}");
    assert!(
        peak >= threshold - 1e-3,
        "peak {peak} was below {threshold}"
    );
    assert_eq!(processor.latency().samples, LOOKAHEAD_SAMPLES);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn built_synth_extension_plays_notes() {
    let Some((host, root)) = load_host() else {
        eprintln!("skipping: build notwig-ext-devices for wasm32-unknown-unknown first");
        return;
    };
    let spec = host
        .specs()
        .into_iter()
        .find(|spec| spec.type_id == "notwig.ext.devices.synth")
        .expect("synth spec");
    let config = RenderConfig {
        sample_rate: 48_000,
        max_block_size: 512,
        channels: 2,
        tempo_bpm: 120.0,
    };
    let plan = CompiledPlan {
        config,
        master: TrackId::nil(),
        order: Vec::new(),
        tracks: BTreeMap::new(),
        devices: BTreeMap::new(),
        chains: BTreeMap::new(),
        total_latency: 0,
    };
    let device = PlanDevice {
        id: DeviceId::nil(),
        type_id: "notwig.ext.devices.synth".into(),
        kind: DeviceKind::Instrument,
        spec: spec.clone(),
        param_values: BTreeMap::new(),
        bypass: false,
        child_chains: Vec::new(),
        latency_samples: 0,
        resolved: true,
    };
    let mut processor = host
        .factory()
        .create(&plan, &device)
        .expect("synth processor");
    processor.prepare(config).unwrap();
    let param = |name: &str| {
        spec.params
            .iter()
            .find(|param| param.name == name)
            .expect("synth parameter")
            .id
    };
    processor.set_param(param("Attack"), &ParamValue::F64(0.001));
    processor.set_param(param("Decay"), &ParamValue::F64(0.001));
    processor.set_param(param("Sustain"), &ParamValue::F64(1.0));
    processor.set_param(param("Release"), &ParamValue::F64(0.001));

    let transport = TransportInfo::default();
    let context = ProcessContext {
        config,
        sample_position: 0,
        playing: true,
        tempo_bpm: 120.0,
        transport: &transport,
    };
    let note_on = NoteEvent {
        sample_offset: 0,
        kind: NoteKind::On,
        note_id: NoteId::nil(),
        pitch: 69,
        velocity: 1.0,
    };
    let mut audio = vec![0.0_f32; 512 * 2];
    let status = processor.process(&context, &mut audio, &[note_on]);
    assert!(!status.overloaded);
    let peak = audio
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    assert!(peak > 0.05, "synth was silent: {peak}");

    let note_off = NoteEvent {
        kind: NoteKind::Off,
        ..note_on
    };
    let mut tail = vec![0.0_f32; 512 * 2];
    processor.process(&context, &mut tail, &[note_off]);
    let tail_peak = tail[(512 - 128) * 2..]
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    assert!(tail_peak < 1e-3, "synth did not release: {tail_peak}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn built_native_extension_loads_and_processes_audio() {
    let Some((host, root)) = load_native_host() else {
        eprintln!("skipping: build notwig-ext-devices for aarch64-apple-darwin first");
        return;
    };
    let specs = host.specs();
    assert_eq!(specs.len(), 4);
    let limiter_spec = specs
        .iter()
        .find(|spec| spec.type_id == "notwig.ext.devices.limiter")
        .expect("limiter spec")
        .clone();
    let config = RenderConfig {
        sample_rate: 48_000,
        max_block_size: 64,
        channels: 2,
        tempo_bpm: 120.0,
    };
    let plan = CompiledPlan {
        config,
        master: TrackId::nil(),
        order: Vec::new(),
        tracks: BTreeMap::new(),
        devices: BTreeMap::new(),
        chains: BTreeMap::new(),
        total_latency: LOOKAHEAD_SAMPLES,
    };
    let device = PlanDevice {
        id: DeviceId::nil(),
        type_id: "notwig.ext.devices.limiter".into(),
        kind: DeviceKind::AudioFx,
        spec: limiter_spec,
        param_values: BTreeMap::new(),
        bypass: false,
        child_chains: Vec::new(),
        latency_samples: LOOKAHEAD_SAMPLES,
        resolved: true,
    };
    let mut processor = host
        .factory()
        .create(&plan, &device)
        .expect("native limiter processor");
    processor.prepare(config).unwrap();
    let transport = TransportInfo::default();
    let context = ProcessContext {
        config,
        sample_position: 0,
        playing: true,
        tempo_bpm: 120.0,
        transport: &transport,
    };
    let mut audio = vec![2.0_f32; 256];
    let (first, second) = audio.split_at_mut(128);
    assert!(!processor.process(&context, first, &[]).overloaded);
    assert!(!processor.process(&context, second, &[]).overloaded);
    let peak = second
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    let threshold = 10.0_f32.powf(-6.0 / 20.0);
    assert!(peak <= threshold + 1e-3, "peak {peak} exceeded {threshold}");
    let _ = std::fs::remove_dir_all(root);
}
