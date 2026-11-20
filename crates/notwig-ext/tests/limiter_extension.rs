use std::collections::BTreeMap;
use std::path::PathBuf;

use notwig_core::{DeviceId, DeviceKind, TrackId};
use notwig_dsp::{CompiledPlan, PlanDevice, ProcessContext, RenderConfig, TransportInfo};
use notwig_ext::ExtensionHost;

fn built_extension() -> Option<Vec<u8>> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest_dir.parent()?.parent()?;
    let path = workspace.join("target/wasm32-unknown-unknown/release/notwig_ext_devices.wasm");
    std::fs::read(path).ok()
}

#[test]
fn built_limiter_extension_loads_and_processes_audio() {
    let Some(wasm) = built_extension() else {
        eprintln!("skipping: build notwig-ext-devices for wasm32-unknown-unknown first");
        return;
    };
    let manifest = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("notwig-ext-devices/extension.json"),
    )
    .unwrap();
    let root = std::env::temp_dir().join(format!("notwig-limiter-ext-{}", uuid::Uuid::new_v4()));
    let package = root.join("notwig.ext.devices");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::write(package.join("extension.json"), manifest).unwrap();
    std::fs::write(package.join("notwig_ext_devices.wasm"), wasm).unwrap();

    let host = ExtensionHost::load(&root, &BTreeMap::new()).unwrap();
    let specs = host.specs();
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].name, "Limiter");

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
        total_latency: 0,
    };
    let device = PlanDevice {
        id: DeviceId::nil(),
        type_id: "notwig.ext.devices.limiter".into(),
        kind: DeviceKind::AudioFx,
        spec: specs[0].clone(),
        param_values: BTreeMap::new(),
        bypass: false,
        child_chains: Vec::new(),
        latency_samples: 0,
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
    assert_eq!(processor.latency().samples, 0);
    let _ = std::fs::remove_dir_all(root);
}
