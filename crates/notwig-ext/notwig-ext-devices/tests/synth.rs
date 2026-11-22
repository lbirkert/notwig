use notwig_core::DeviceKind;
use notwig_core::dsp::DeviceUiSpec;
use notwig_ext_api::{
    DeviceExtension, DeviceMeterSnapshot, EguiFrame, EguiInput, EguiModifiers, NoteEvent, NoteKind,
    ThemeSnapshot, egui, egui_panel,
};
use notwig_ext_devices::{
    SYNTH_TYPE_ID, SynthDevice, synth_attack_id, synth_decay_id, synth_oscillator_id,
    synth_release_id, synth_sustain_id,
};

fn note(kind: NoteKind, pitch: u8, velocity: f32) -> NoteEvent {
    NoteEvent {
        sample_offset: 0,
        kind,
        pitch,
        velocity,
    }
}

fn draw() -> EguiFrame {
    let mut synth = SynthDevice::new();
    let context = egui::Context::default();
    let input = EguiInput {
        width: 260.0,
        height: 200.0,
        pixels_per_point: 1.0,
        time: Some(0.0),
        focused: true,
        max_texture_side: 4096,
        modifiers: EguiModifiers::default(),
        events: Vec::new(),
    };
    egui_panel::run_frame(&context, input, &ThemeSnapshot::default(), |ui| {
        synth.draw_egui(ui, DeviceMeterSnapshot::default());
    })
}

#[test]
fn spec_is_an_instrument_with_oscillator_and_adsr() {
    let spec = SynthDevice::spec();
    assert_eq!(spec.type_id, SYNTH_TYPE_ID);
    assert_eq!(spec.kind, DeviceKind::Instrument);
    assert_eq!(spec.name, "Synth");
    assert_eq!(spec.params.len(), 5);
    assert!(
        spec.params
            .iter()
            .any(|param| param.id == synth_oscillator_id())
    );
    assert!(
        spec.params
            .iter()
            .any(|param| param.id == synth_attack_id())
    );
    assert!(spec.params.iter().any(|param| param.id == synth_decay_id()));
    assert!(
        spec.params
            .iter()
            .any(|param| param.id == synth_sustain_id())
    );
    assert!(
        spec.params
            .iter()
            .any(|param| param.id == synth_release_id())
    );
    assert!(matches!(spec.ui, DeviceUiSpec::Custom { .. }));
}

#[test]
fn draw_produces_a_valid_frame() {
    let frame = draw();
    frame.validate().unwrap();
    assert_ne!(frame.primitives.len(), 0);
}

#[test]
fn note_on_produces_audio_and_note_off_releases_it() {
    let mut synth = SynthDevice::new();
    synth.prepare(48_000, 2);
    synth.set_param(synth_attack_id(), 0.001);
    synth.set_param(synth_decay_id(), 0.001);
    synth.set_param(synth_sustain_id(), 1.0);
    synth.set_param(synth_release_id(), 0.001);

    synth.note_event(note(NoteKind::On, 69, 1.0));
    let mut audio = vec![0.0_f32; 1024 * 2];
    synth.process(&mut audio);
    let peak = audio
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    assert!(peak > 0.1, "synth produced no audible note: {peak}");

    synth.note_event(note(NoteKind::Off, 69, 0.0));
    let mut silence = vec![0.0_f32; 1024 * 2];
    synth.process(&mut silence);
    let tail = &silence[(1024 - 128) * 2..];
    assert!(
        tail.iter().all(|sample| sample.abs() < 1e-3),
        "note did not release: {:?}",
        tail.iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()))
    );
}

#[test]
fn every_oscillator_mode_produces_audio() {
    for mode in 0..4 {
        let mut synth = SynthDevice::new();
        synth.prepare(48_000, 2);
        assert!(synth.set_param(synth_oscillator_id(), f64::from(mode)));
        synth.note_event(note(NoteKind::On, 60, 1.0));
        let mut audio = vec![0.0_f32; 512 * 2];
        synth.process(&mut audio);
        let peak = audio
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        assert!(peak > 0.05, "oscillator mode {mode} was silent");
    }
}

#[test]
fn unknown_parameters_are_rejected() {
    let mut synth = SynthDevice::new();
    assert!(!synth.set_param(notwig_core::ParamId::nil(), 1.0));
}
