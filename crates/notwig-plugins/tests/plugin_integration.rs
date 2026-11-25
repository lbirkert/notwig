//! Manual, end-to-end integration coverage for hosted plugin formats.
//!
//! These tests build/load a real truce plugin from
//! `tests/fixtures/truce_test_plugin`. They are deliberately ignored in the
//! normal workspace suite because they need `cargo-truce`, a native compiler
//! toolchain, and (for LV2) the system `lilv` library.
//!
//! Run them with `scripts/test-plugins.sh`, not by invoking `cargo test`
//! directly: the script builds the fixture, selects the format features, and
//! sets `LV2_PATH`.

#![cfg(any(
    feature = "vst3",
    feature = "vst2",
    feature = "clap",
    feature = "lv2",
    all(feature = "au", target_os = "macos"),
    all(feature = "au3", target_os = "macos")
))]

#[path = "plugin_integration/support.rs"]
mod support;

use notwig_core::ParamValue;
use notwig_plugins::{PluginFormat, PluginHost, PluginInstance, PluginState};
use support::{
    BLOCK_SIZE, SAMPLE_RATE, assert_close, assert_incompatible_state, check_fixture_descriptor,
    check_params, config, descriptor, expected_latency, fixture_bundles, gain_db, param, prepare,
    raw_gain, raw_mode, render_blocks, wrong_state,
};

struct FormatProfile {
    params: bool,
    state: bool,
}

const FULL_FORMAT: FormatProfile = FormatProfile {
    params: true,
    state: true,
};

// Documented LV2 gap: path-bounded discovery is registry-based and the
// adapter does not yet enumerate/set parameters or save/load state.
#[cfg(feature = "lv2")]
const LV2_FORMAT: FormatProfile = FormatProfile {
    params: false,
    state: false,
};

fn run_format(format: PluginFormat, profile: FormatProfile) {
    let _ = fixture_bundles();
    let mut host = PluginHost::new();
    let descriptor = descriptor(&mut host, format);
    check_fixture_descriptor(&descriptor, format);

    let mut instance = host
        .load(&descriptor)
        .unwrap_or_else(|err| panic!("{format:?} fixture load failed: {err}"));
    let capabilities = instance.capabilities();
    assert_eq!(capabilities.audio_inputs, 2);
    assert_eq!(capabilities.audio_outputs, 2);
    assert!(capabilities.supports_state);
    assert!(
        capabilities.has_editor,
        "{format:?} fixture should have an editor"
    );
    assert!(capabilities.reports_latency);

    let render_config = config(SAMPLE_RATE, BLOCK_SIZE);
    prepare(instance.as_mut(), render_config);

    // AU latency is only available after initialization; query it through
    // both capability and processor paths once the instance is prepared.
    let prepared = instance.capabilities();
    assert_eq!(
        prepared.latency_samples,
        expected_latency(format),
        "{format:?} latency reporting changed"
    );
    assert_eq!(
        instance.latency().samples,
        expected_latency(format),
        "{format:?} device latency changed"
    );

    // Default state: 0 dB, clean polarity.
    let output = render_blocks(instance.as_mut(), render_config, 0.25, 4);
    assert_close(output, 0.25);

    if profile.params {
        check_parameter_processing(format, instance.as_mut(), render_config);
    } else {
        assert!(
            instance.describe_parameters().is_empty(),
            "{format:?} parameter support changed; update the format profile"
        );
    }

    if profile.state {
        let mut restored =
            check_state_round_trip(format, &mut host, &descriptor, instance.as_mut());
        check_processor_tail(format, restored.as_mut(), render_config);
        drop(restored);
    } else {
        // The LV2 adapter documents save/load as unwired. Failures must be
        // explicit errors, not panics or silent success.
        let result = instance.save_state();
        assert!(
            result.is_err(),
            "{format:?} state support changed; update the format profile"
        );
        let dummy = PluginState {
            format,
            plugin_uid: descriptor.uid.clone(),
            plugin_version: descriptor.version.clone(),
            blob: Vec::new(),
        };
        assert!(
            instance.load_state(&dummy).is_err(),
            "{format:?} state load support changed; update the format profile"
        );
        check_processor_tail(format, instance.as_mut(), render_config);
    }

    // Repeated instantiation catches leaked module state / double-unload
    // problems that a single load cannot.
    for _ in 0..3 {
        let mut instance = host
            .load(&descriptor)
            .unwrap_or_else(|err| panic!("{format:?} repeated load failed: {err}"));
        prepare(instance.as_mut(), render_config);
        let output = render_blocks(instance.as_mut(), render_config, 0.2, 1);
        assert!(output.is_finite());
    }
}

fn check_parameter_processing(
    format: PluginFormat,
    instance: &mut dyn PluginInstance,
    render_config: notwig_core::dsp::RenderConfig,
) {
    check_params(format, instance);

    let params = instance.describe_parameters();
    let gain = param(&params, "Gain").id;
    let bypass = param(&params, "Bypass").id;
    let mode = param(&params, "Mode").id;

    // Continuous automatable parameter. Rack's VST3/VST2 adapters expose
    // normalized values while CLAP/AU use native dB; translate to the wire
    // form so the assertion covers DSP behavior rather than adapter scaling.
    let target_db = -6.0;
    let target_raw = raw_gain(format, target_db);
    assert!((gain_db(format, target_raw) - target_db).abs() < 1.0e-9);
    instance.set_param(gain, &ParamValue::F64(target_raw));
    let output = render_blocks(instance, render_config, 0.25, 4);
    let gain_factor = 10.0_f32.powf(target_db as f32 / 20.0);
    assert_close(output, 0.25 * gain_factor);

    // Enum parameter changes processing. Set the raw wire value: the
    // normalized adapters do not yet map host `EnumIndex` values through the
    // plugin's step count.
    instance.set_param(mode, &ParamValue::F64(raw_mode(format, 1)));
    let output = render_blocks(instance, render_config, 0.25, 4);
    assert_close(output, -0.25 * gain_factor);

    // Bool parameter bypasses everything else.
    instance.set_param(bypass, &ParamValue::Bool(true));
    let output = render_blocks(instance, render_config, 0.25, 4);
    assert_close(output, 0.25);
}

fn check_state_round_trip(
    format: PluginFormat,
    host: &mut PluginHost,
    descriptor: &notwig_plugins::PluginDescriptor,
    instance: &mut dyn PluginInstance,
) -> Box<dyn PluginInstance> {
    let params = instance.describe_parameters();
    let gain = param(&params, "Gain").id;
    let bypass = param(&params, "Bypass").id;
    let mode = param(&params, "Mode").id;

    // State round-trip: -12 dB, inverted.
    instance.set_param(bypass, &ParamValue::Bool(false));
    instance.set_param(gain, &ParamValue::F64(raw_gain(format, -12.0)));
    instance.set_param(mode, &ParamValue::F64(raw_mode(format, 1)));
    let render_config = config(SAMPLE_RATE, BLOCK_SIZE);
    // CLAP queues host parameter changes until the next process/flush. Save
    // only after a block so the snapshot reflects the new values (the engine
    // always processes between a parameter edit and a state save).
    let _ = render_blocks(instance, render_config, 0.0, 1);
    let state = instance
        .save_state()
        .unwrap_or_else(|err| panic!("{format:?} save_state failed: {err}"));
    assert_eq!(state.format, format);
    assert_eq!(state.plugin_uid, descriptor.uid);
    assert_ne!(state.blob, Vec::<u8>::new());

    let mut restored = host
        .load(descriptor)
        .unwrap_or_else(|err| panic!("{format:?} restore load failed: {err}"));
    restored
        .load_state(&state)
        .unwrap_or_else(|err| panic!("{format:?} load_state failed: {err}"));
    prepare(restored.as_mut(), render_config);
    let output = render_blocks(restored.as_mut(), render_config, 0.25, 8);
    let restored_factor = 10.0_f32.powf(-12.0 / 20.0);
    assert_close(output, -0.25 * restored_factor);

    // State saved by another plugin must be rejected, not applied.
    assert_incompatible_state(restored.load_state(&wrong_state(&state)));

    // Corrupt state must not panic or wedge the processor. Formats may either
    // reject it or ignore it, but the next block must still be safe.
    let corrupt = PluginState {
        blob: vec![0xff; 32],
        ..state.clone()
    };
    let _ = restored.load_state(&corrupt);
    let output = render_blocks(restored.as_mut(), render_config, 0.25, 2);
    assert!(output.is_finite());
    restored
}

fn check_processor_tail(
    format: PluginFormat,
    instance: &mut dyn notwig_core::dsp::DeviceProcessor,
    render_config: notwig_core::dsp::RenderConfig,
) {
    // Re-preparing at another sample rate / block size is a normal engine
    // reconfiguration path.
    let alt_config = config(44_100, 64);
    prepare(instance, alt_config);
    let output = render_blocks(instance, alt_config, 0.1, 4);
    assert!(output.is_finite(), "{format:?} output was not finite");

    // Zero-frame blocks are legal on the device path and must not fault.
    let transport = notwig_core::dsp::TransportInfo::default();
    let context = notwig_core::dsp::ProcessContext {
        config: alt_config,
        sample_position: 0,
        playing: false,
        tempo_bpm: alt_config.tempo_bpm,
        transport: &transport,
    };
    let status = notwig_core::dsp::DeviceProcessor::process(instance, &context, &mut [], &[]);
    assert!(!status.overloaded);

    // Silence is a legal block and must stay finite. Restore the original
    // block size first: prepare sizes the scratch buffers.
    prepare(instance, render_config);
    let output = render_blocks(instance, render_config, 0.0, 2);
    assert_close(output, 0.0);
}
#[test]
#[ignore = "manual: requires scripts/test-plugins.sh and a native plugin toolchain"]
#[cfg(feature = "vst3")]
fn vst3_fixture_is_discovered_and_hosted() {
    run_format(PluginFormat::Vst3, FULL_FORMAT);
}

#[test]
#[ignore = "manual: requires scripts/test-plugins.sh and a native plugin toolchain"]
#[cfg(feature = "vst2")]
fn vst2_fixture_is_discovered_and_hosted() {
    run_format(PluginFormat::Vst2, FULL_FORMAT);
}

#[test]
#[ignore = "manual: requires scripts/test-plugins.sh and a native plugin toolchain"]
#[cfg(feature = "clap")]
fn clap_fixture_is_discovered_and_hosted() {
    run_format(PluginFormat::Clap, FULL_FORMAT);
}

#[test]
#[ignore = "manual: requires scripts/test-plugins.sh, lilv, and LV2_PATH"]
#[cfg(feature = "lv2")]
fn lv2_fixture_is_discovered_and_hosted() {
    run_format(PluginFormat::Lv2, LV2_FORMAT);
}

#[test]
#[ignore = "manual: requires scripts/test-plugins.sh and installed AU v2 component"]
#[cfg(all(feature = "au", target_os = "macos"))]
fn au_fixture_is_discovered_and_hosted() {
    run_format(PluginFormat::Au, FULL_FORMAT);
}

#[test]
#[ignore = "manual: requires scripts/test-plugins.sh and installed AU v3 extension"]
#[cfg(all(feature = "au3", target_os = "macos"))]
fn au3_fixture_is_discovered_and_hosted() {
    run_format(PluginFormat::Au3, FULL_FORMAT);
}
