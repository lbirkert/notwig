use notwig_core::ids::{Clip, DeviceChain, Track};
use notwig_core::protocol::{Command, CommandPayload};
use notwig_core::sequencer::{AudioClipData, ClipKind, ClipPayload, FadeShape};
use notwig_core::time::Color;
use notwig_core::tracks::TrackKind;
use notwig_core::{ClipEntity, Db, Id, Meta, ProjectDoc, RangeMus, SampleRate, TimeMus, apply};
use notwig_ui::RuntimeView;
use notwig_ui::test_api::timeline::clip::clip_color;
use notwig_ui::test_api::timeline::playhead_for_strip;
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn meta() -> Meta {
    Meta {
        name: "Clip Chrome".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    }
}

fn doc_with_track_color(color: Option<Color>) -> ProjectDoc {
    let doc = ProjectDoc::new_template(meta(), id::<Track>(1), id::<DeviceChain>(100)).unwrap();
    apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: id::<Track>(2),
            chain_id: id::<DeviceChain>(102),
            kind: TrackKind::Audio,
            name: "Audio".into(),
            parent: None,
            index: None,
            color,
        }),
    )
    .unwrap()
    .doc
}

fn clip(color: Option<Color>) -> ClipEntity {
    ClipEntity {
        id: id::<Clip>(60),
        track_id: id::<Track>(2),
        kind: ClipKind::Audio,
        range: RangeMus::new(TimeMus::zero(), TimeMus::new(2, 1)).unwrap(),
        name: None,
        color,
        muted: false,
        payload: ClipPayload::Audio(AudioClipData {
            asset_id: id(600),
            source_start_sample: 0,
            source_end_sample: 48_000,
            gain_db: Db(0.0),
            gain_curve: None,
            fade_in_samples: 0,
            fade_out_samples: 0,
            fade_shape: FadeShape::Linear,
            slices: Vec::new(),
            slices_enabled: false,
        }),
    }
}

fn color(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b, a: 255 }
}

#[test]
fn clip_color_prefers_clip_then_track_then_kind() {
    let clip_color_value = color(200, 10, 20);
    let track_color = color(10, 200, 30);
    let doc = doc_with_track_color(Some(track_color));
    assert_eq!(
        clip_color(&doc, &clip(Some(clip_color_value))),
        egui::Color32::from_rgb(200, 10, 20)
    );
    assert_eq!(
        clip_color(&doc, &clip(None)),
        egui::Color32::from_rgb(10, 200, 30)
    );

    let uncolored = doc_with_track_color(None);
    assert_eq!(
        clip_color(&uncolored, &clip(None)),
        notwig_ui::palette::CLIP_AUDIO
    );
}

#[test]
fn clip_color_inherits_from_group_ancestry() {
    let group_color = color(40, 90, 160);
    let doc = ProjectDoc::new_template(meta(), id::<Track>(1), id::<DeviceChain>(100)).unwrap();
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: id::<Track>(3),
            chain_id: id::<DeviceChain>(103),
            kind: TrackKind::Group,
            name: "Group".into(),
            parent: None,
            index: None,
            color: Some(group_color),
        }),
    )
    .unwrap()
    .doc;
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: id::<Track>(2),
            chain_id: id::<DeviceChain>(102),
            kind: TrackKind::Audio,
            name: "Audio".into(),
            parent: Some(id::<Track>(3)),
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc;

    assert_eq!(
        clip_color(&doc, &clip(None)),
        egui::Color32::from_rgb(40, 90, 160)
    );
}

#[test]
fn shared_playhead_is_hidden_while_stopped() {
    let mut runtime = RuntimeView {
        playhead: TimeMus::new(3, 1),
        ..RuntimeView::default()
    };
    assert!(!runtime.playing);
    assert_eq!(
        playhead_for_strip(&runtime),
        TimeMus::new(-1_000_000_000, 1)
    );

    runtime.playing = true;
    assert_eq!(playhead_for_strip(&runtime), TimeMus::new(3, 1));
}
