use notwig_core::ids::{Clip, DeviceChain, Track};
use notwig_core::protocol::{Command, CommandPayload};
use notwig_core::sequencer::{ClipKind, ClipPayload, MidiClipData};
use notwig_core::tracks::TrackKind;
use notwig_core::*;
use notwig_ui::{NotwigUi, RuntimeView, snap_16};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn doc() -> ProjectDoc {
    let meta = Meta {
        name: "Shell".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    let doc = ProjectDoc::new_template(meta, id::<Track>(1), id::<DeviceChain>(100)).unwrap();
    let track = id::<Track>(2);
    let chain = id::<DeviceChain>(102);
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: track,
            chain_id: chain,
            kind: TrackKind::Instrument,
            name: "Keys".into(),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc;
    apply(
        &doc,
        &Command::new(CommandPayload::CreateClip {
            id: id::<Clip>(20),
            track_id: track,
            kind: ClipKind::Midi,
            range: RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
            payload: ClipPayload::Midi(MidiClipData {
                length: Rational::new(4, 1),
                content_start: Rational::zero(),
                notes: vec![],
            }),
        }),
    )
    .unwrap()
    .doc
}

#[test]
fn ui_renders_headless_without_panicking() {
    let doc = doc();
    let mut ui = NotwigUi::default();
    let ctx = egui::Context::default();
    let runtime = RuntimeView::default();
    let mut frames = 0;
    for _ in 0..2 {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            let intents = ui.show(ctx, &doc, &runtime);
            assert_eq!(intents.len(), 0);
            frames += 1;
        });
    }
    assert!(frames >= 2);
}

#[test]
fn snapping_uses_sixteenth_grid_and_alt_bypasses() {
    let time = Rational::new(1, 3);
    assert_eq!(snap_16(time, false), Rational::new(5, 16));
    assert_eq!(snap_16(time, true), time);
}
