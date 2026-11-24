use notwig_core::DeviceKind;
use notwig_core::dsp::ContainerRouting;
use notwig_ext_api::{
    DeviceExtension, DeviceMeterSnapshot, PanelFrame, PanelInput, PanelModifiers, ThemeSnapshot,
    egui, panel_frame,
};
use notwig_ext_devices::{
    REVERB_TYPE_ID, ReverbDevice, reverb_damping_id, reverb_mix_id, reverb_predelay_id,
    reverb_size_id, reverb_type_id, reverb_width_id,
};

fn draw_with_size(meter: DeviceMeterSnapshot, width: f32, height: f32) -> PanelFrame {
    let mut reverb = ReverbDevice::new();
    let context = egui::Context::default();
    let input = PanelInput {
        width,
        height,
        pixels_per_point: 1.0,
        time: Some(0.0),
        focused: true,
        max_texture_side: 4096,
        modifiers: PanelModifiers::default(),
        events: Vec::new(),
    };
    panel_frame::run_frame(&context, input, &ThemeSnapshot::default(), |ui| {
        reverb.draw(ui, meter);
    })
}

fn draw(meter: DeviceMeterSnapshot) -> PanelFrame {
    draw_with_size(meter, 360.0, 240.0)
}

#[test]
fn spec_is_a_wet_container() {
    let spec = ReverbDevice::spec();
    assert_eq!(spec.type_id, REVERB_TYPE_ID);
    assert_eq!(spec.name, "Reverb");
    assert_eq!(spec.kind, DeviceKind::Container);
    let container = spec.container.as_ref().expect("container config");
    assert_eq!(container.routing, ContainerRouting::StageThenChildren);
    assert_eq!(container.wet_param, Some(reverb_mix_id()));
    assert_eq!(spec.latency_samples, 0);
    assert_eq!(spec.params.len(), 6);
    for id in [
        reverb_type_id(),
        reverb_size_id(),
        reverb_damping_id(),
        reverb_predelay_id(),
        reverb_width_id(),
        reverb_mix_id(),
    ] {
        assert!(spec.params.iter().any(|param| param.id == id));
    }
}

#[test]
fn draw_produces_a_valid_frame() {
    let frame = draw(DeviceMeterSnapshot::default());
    frame.validate().unwrap();
    assert_ne!(frame.primitives.len(), 0);
}

#[test]
fn draw_fits_a_compact_container_panel() {
    let width = 318.0;
    let height = 150.0;
    let frame = draw_with_size(DeviceMeterSnapshot::default(), width, height);
    frame.validate().unwrap();
    assert!(
        frame.primitives.iter().all(|primitive| {
            let [min_x, min_y, max_x, max_y] = primitive.clip_rect;
            min_x >= -0.01 && min_y >= -0.01 && max_x <= width + 0.01 && max_y <= height + 0.01
        }),
        "reverb content must stay inside the panel"
    );
}

#[test]
fn impulse_produces_a_finite_decaying_tail() {
    let mut reverb = ReverbDevice::new();
    reverb.prepare(48_000, 2);
    let mut audio = vec![0.0_f32; 48_000 * 2];
    audio[0] = 1.0;
    audio[1] = 1.0;
    reverb.process(&mut audio);

    let predelay = 240_usize;
    assert!(
        audio[..predelay * 2].iter().all(|sample| *sample == 0.0),
        "reverb should be silent before the pre-delay"
    );
    let tail: f32 = audio[24_000 * 2..]
        .iter()
        .map(|sample| sample * sample)
        .sum();
    assert!(tail > 1e-6, "reverb tail was silent");
    let peak = audio
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    assert!(peak.is_finite() && peak < 8.0, "reverb peak was {peak}");
}

#[test]
fn room_hall_and_plate_produce_finite_tails() {
    for (index, name) in [(0.0, "Room"), (1.0, "Hall"), (2.0, "Plate")] {
        let mut reverb = ReverbDevice::new();
        reverb.prepare(48_000, 2);
        assert!(reverb.set_param(reverb_type_id(), index));
        reverb.set_param(reverb_predelay_id(), 0.0);
        let mut audio = vec![0.0_f32; 24_000 * 2];
        audio[0] = 1.0;
        audio[1] = 1.0;
        reverb.process(&mut audio);
        let tail: f32 = audio[8_000 * 2..]
            .iter()
            .map(|sample| sample * sample)
            .sum();
        assert!(tail > 1e-6, "{name} reverb tail was silent");
        assert!(audio.iter().all(|sample| sample.is_finite()));
    }
}

#[test]
fn reset_clears_the_tail() {
    let mut reverb = ReverbDevice::new();
    reverb.prepare(48_000, 2);
    let mut audio = vec![0.0_f32; 8_192 * 2];
    audio[0] = 1.0;
    audio[1] = 1.0;
    reverb.process(&mut audio);
    reverb.prepare(48_000, 2);
    let mut silence = vec![0.0_f32; 8_192 * 2];
    reverb.process(&mut silence);
    assert!(
        silence.iter().all(|sample| sample.abs() < 1e-9),
        "reset should clear the reverb tail"
    );
}

#[test]
fn parameters_are_clamped_and_unknown_ids_rejected() {
    let mut reverb = ReverbDevice::new();
    assert!(reverb.set_param(reverb_size_id(), 4.0));
    assert!(reverb.set_param(reverb_damping_id(), -2.0));
    assert!(reverb.set_param(reverb_predelay_id(), 9.0));
    assert!(reverb.set_param(reverb_width_id(), 3.0));
    assert!(reverb.set_param(reverb_mix_id(), -1.0));
    assert!(!reverb.set_param(notwig_core::ParamId::nil(), 1.0));
}

#[test]
fn predelay_delays_the_first_wet_output() {
    let first_nonzero = |predelay: f64| {
        let mut reverb = ReverbDevice::new();
        reverb.prepare(48_000, 2);
        reverb.set_param(reverb_predelay_id(), predelay);
        reverb.set_param(reverb_size_id(), 0.8);
        reverb.set_param(reverb_damping_id(), 0.2);
        let mut audio = vec![0.0_f32; 8_192 * 2];
        audio[0] = 1.0;
        audio[1] = 1.0;
        reverb.process(&mut audio);
        audio
            .chunks_exact(2)
            .position(|frame| frame.iter().any(|sample| sample.abs() > 1e-7))
            .unwrap()
    };
    let early = first_nonzero(0.0);
    let late = first_nonzero(0.05);
    assert!(
        late > early + 2_000,
        "pre-delay should move the first output: early={early}, late={late}"
    );
}
