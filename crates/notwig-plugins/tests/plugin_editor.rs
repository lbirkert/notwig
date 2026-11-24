//! Manual native-editor coverage for the local truce fixture.
//!
//! Notwig hosts plugin editor windows on macOS only; all other platforms use
//! the backend-unavailable stub. These tests are ignored because they open
//! real `AppKit` windows and need `scripts/test-plugins.sh` to build/install the
//! fixture first.

#[cfg(all(
    target_os = "macos",
    any(
        feature = "vst3",
        feature = "vst2",
        feature = "clap",
        feature = "lv2",
        feature = "au",
        feature = "au3"
    )
))]
#[path = "plugin_integration/support.rs"]
mod support;

#[cfg(all(
    target_os = "macos",
    any(
        feature = "vst3",
        feature = "vst2",
        feature = "clap",
        feature = "lv2",
        feature = "au",
        feature = "au3"
    )
))]
use notwig_core::dsp::DeviceProcessor;
#[cfg(all(
    target_os = "macos",
    any(
        feature = "vst3",
        feature = "vst2",
        feature = "clap",
        feature = "lv2",
        feature = "au",
        feature = "au3"
    )
))]
use notwig_plugins::{
    NativePluginWindow, PluginFormat, PluginHost, PluginInstance, prepare_window_host,
};
#[cfg(all(
    target_os = "macos",
    any(
        feature = "vst3",
        feature = "vst2",
        feature = "clap",
        feature = "lv2",
        feature = "au",
        feature = "au3"
    )
))]
use support::{BLOCK_SIZE, SAMPLE_RATE, config, descriptor, fixture_bundles, param, prepare};

#[cfg(not(target_os = "macos"))]
#[test]
fn native_editor_reports_backend_unavailable() {
    use std::path::PathBuf;

    use notwig_plugins::{NativePluginWindow, PluginDescriptor, PluginError, PluginFormat};

    let descriptor = PluginDescriptor {
        type_id: "vst3.test".into(),
        format: PluginFormat::Vst3,
        path: PathBuf::from("/nonexistent/test.vst3"),
        name: "Test".into(),
        vendor: "Test".into(),
        version: "1.0".into(),
        uid: "test".into(),
        is_instrument: false,
        binary_size: 0,
        modified_utc: 0,
    };
    match NativePluginWindow::open(&descriptor, 48_000, None) {
        Err(PluginError::BackendUnavailable(PluginFormat::Vst3)) => {}
        Err(other) => panic!("unexpected editor error: {other}"),
        Ok(_) => panic!("native plugin editors must be unavailable off macOS"),
    }
}

#[cfg(target_os = "macos")]
fn run_editor(format: PluginFormat) {
    let _ = fixture_bundles();
    let mut host = PluginHost::new();
    let descriptor = descriptor(&mut host, format);

    // Resolve the fixture's gain parameter through the DSP path so the editor
    // test exercises the same parameter IDs the engine uses.
    let instance = host
        .load(&descriptor)
        .unwrap_or_else(|err| panic!("{format:?} fixture load failed: {err}"));
    let gain = param(&instance.describe_parameters(), "Gain").id;

    prepare_window_host();
    let mut window = NativePluginWindow::open(&descriptor, SAMPLE_RATE, None)
        .unwrap_or_else(|err| panic!("{format:?} editor open failed: {err}"));
    assert!(window.is_open());
    let size = window.size();
    assert!(
        size.0 >= 128 && size.1 >= 128,
        "{format:?} editor reported implausible size {size:?}"
    );

    // Pumping must keep the size stable and must not close the window.
    window.pump();
    assert_eq!(window.size(), size);
    assert!(window.is_open());
    window.focus();

    // Host -> editor parameter interaction must reach the editor instance and
    // round-trip through editor state.
    let initial = window
        .save_state()
        .unwrap_or_else(|err| panic!("{format:?} editor save_state failed: {err}"));
    window.set_parameter(gain, -18.0);
    window.pump();
    let changed = window
        .save_state()
        .unwrap_or_else(|err| panic!("{format:?} editor save_state failed: {err}"));
    assert_ne!(
        changed.blob, initial.blob,
        "{format:?} editor ignored a host parameter change"
    );

    window
        .load_state(&initial)
        .unwrap_or_else(|err| panic!("{format:?} editor load_state failed: {err}"));
    let restored = window
        .save_state()
        .unwrap_or_else(|err| panic!("{format:?} editor save_state failed: {err}"));
    assert_eq!(restored.format, format);
    assert_eq!(restored.plugin_uid, descriptor.uid);
    assert_ne!(
        restored.blob, changed.blob,
        "{format:?} editor did not restore the previous state"
    );

    // Closing and reopening with saved state is the production lifecycle.
    drop(window);
    let mut reopened = NativePluginWindow::open(&descriptor, SAMPLE_RATE, None)
        .unwrap_or_else(|err| panic!("{format:?} editor reopen failed: {err}"));
    reopened
        .load_state(&changed)
        .unwrap_or_else(|err| panic!("{format:?} reopened editor load_state failed: {err}"));
    reopened.pump();
    assert!(reopened.is_open());

    // Keep the DSP instance alive through the editor lifecycle to make sure
    // the plugin module tolerates an editor instance next to a processing one.
    let mut instance = instance;
    prepare(instance.as_mut(), config(SAMPLE_RATE, BLOCK_SIZE));
    let _ = notwig_core::dsp::DeviceProcessor::process(
        instance.as_mut(),
        &notwig_core::dsp::ProcessContext {
            config: config(SAMPLE_RATE, BLOCK_SIZE),
            sample_position: 0,
            playing: false,
            tempo_bpm: 120.0,
            transport: &notwig_core::dsp::TransportInfo::default(),
        },
        &mut [0.0; BLOCK_SIZE * 2],
        &[],
    );
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "manual: opens an AppKit editor window; run scripts/test-plugins.sh"]
#[cfg(feature = "vst3")]
fn vst3_native_editor_opens_sizes_and_round_trips_state() {
    run_editor(PluginFormat::Vst3);
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "manual: opens an AppKit editor window; run scripts/test-plugins.sh"]
#[cfg(feature = "vst2")]
fn vst2_native_editor_opens_sizes_and_round_trips_state() {
    run_editor(PluginFormat::Vst2);
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "manual: opens an AppKit editor window; run scripts/test-plugins.sh"]
#[cfg(feature = "clap")]
fn clap_native_editor_opens_sizes_and_round_trips_state() {
    run_editor(PluginFormat::Clap);
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "manual: opens an AppKit editor window; run scripts/test-plugins.sh"]
#[cfg(feature = "lv2")]
fn lv2_native_editor_opens_sizes_and_round_trips_state() {
    run_editor(PluginFormat::Lv2);
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "manual: opens an AppKit editor window; run scripts/test-plugins.sh"]
#[cfg(feature = "au")]
fn au_native_editor_opens_sizes_and_round_trips_state() {
    run_editor(PluginFormat::Au);
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "manual: opens an AppKit editor window; run scripts/test-plugins.sh"]
#[cfg(feature = "au3")]
fn au3_native_editor_opens_sizes_and_round_trips_state() {
    run_editor(PluginFormat::Au3);
}
