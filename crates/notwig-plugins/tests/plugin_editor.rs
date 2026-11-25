//! Manual native-editor coverage for the local truce fixture.
//!
//! Notwig hosts plugin editor windows on macOS only. This target uses a custom
//! harness (`harness = false`) so every editor is created on the process main
//! thread: libtest runs `#[test]` bodies on worker threads, and AppKit can
//! throw a foreign exception across Rust when windows/view factories are used
//! off the main thread.

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
use notwig_plugins::{NativePluginWindow, PluginFormat, PluginHost, prepare_window_host};
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
use support::{
    BLOCK_SIZE, SAMPLE_RATE, config, descriptor, fixture_bundles, param, prepare, render_blocks,
};

fn main() {
    #[cfg(not(target_os = "macos"))]
    {
        native_editor_reports_backend_unavailable();
    }

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
    {
        if std::env::var("NOTWIG_PLUGIN_TEST_EDITOR").as_deref() != Ok("1") {
            eprintln!(
                "skipping native editor tests; set NOTWIG_PLUGIN_TEST_EDITOR=1 to open AppKit windows"
            );
            return;
        }

        prepare_window_host();
        #[cfg(feature = "vst3")]
        run_editor(PluginFormat::Vst3);
        #[cfg(feature = "vst2")]
        run_editor(PluginFormat::Vst2);
        #[cfg(feature = "clap")]
        run_editor(PluginFormat::Clap);
        #[cfg(feature = "lv2")]
        run_editor(PluginFormat::Lv2);
        #[cfg(feature = "au")]
        run_editor(PluginFormat::Au);
        #[cfg(feature = "au3")]
        run_editor(PluginFormat::Au3);
    }

    #[cfg(all(
        target_os = "macos",
        not(any(
            feature = "vst3",
            feature = "vst2",
            feature = "clap",
            feature = "lv2",
            feature = "au",
            feature = "au3"
        ))
    ))]
    {
        eprintln!("no plugin format features enabled; skipping native editor tests");
    }
}

#[cfg(not(target_os = "macos"))]
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
    let mut instance = host
        .load(&descriptor)
        .unwrap_or_else(|err| panic!("{format:?} fixture load failed: {err}"));
    // VST3 parameter metadata is only complete after activation and the first
    // process block (the host integration tests follow the same order).
    let render_config = config(SAMPLE_RATE, BLOCK_SIZE);
    prepare(instance.as_mut(), render_config);
    let _ = render_blocks(instance.as_mut(), render_config, 0.0, 1);
    let gain = param(&instance.describe_parameters(), "Gain")
        .id
        .into_uuid()
        .as_u128() as u32;

    let mut window = NativePluginWindow::open(&descriptor, SAMPLE_RATE, None)
        .unwrap_or_else(|err| panic!("{format:?} editor open failed: {err}"));
    assert!(window.is_open());
    let size = window.size();
    assert!(
        size.0 >= 64 && size.1 >= 64,
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
