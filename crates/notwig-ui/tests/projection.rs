use notwig_core::ids::{Clip, DeviceChain, Track};
use notwig_core::protocol::{Command, CommandPayload};
use notwig_core::tracks::TrackKind;
use notwig_core::*;
use notwig_ui::{Viewport, project_arranger, project_devices, project_mixer, scene_summary};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn doc() -> ProjectDoc {
    let meta = Meta {
        name: "UI".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    ProjectDoc::new_template(meta, id::<Track>(1), id::<DeviceChain>(100)).unwrap()
}

#[test]
fn arranger_projects_tracks_and_clips() {
    let doc = doc();
    let track = id::<Track>(2);
    let chain = id::<DeviceChain>(102);
    let result = apply(
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
    .unwrap();
    let doc = apply(
        &result.doc,
        &Command::new(CommandPayload::CreateClip {
            id: id::<Clip>(20),
            track_id: track,
            kind: ClipKind::Midi,
            range: RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
            payload: ClipPayload::Midi(MidiClipData {
                length: Rational::new(2, 1),
                content_start: Rational::zero(),
                notes: vec![],
            }),
        }),
    )
    .unwrap()
    .doc;
    let view = project_arranger(&doc, None);
    assert!(view.rows.iter().any(|row| row.track == track));
    let row = view.rows.iter().find(|row| row.track == track).unwrap();
    assert_eq!(row.clips.len(), 1);
    assert_eq!(row.clips[0].range.end, Rational::new(2, 1));
    assert_eq!(scene_summary(&doc).clips, 1);
    assert!(project_devices(&doc, track).unwrap().is_empty());
    assert!(!project_mixer(&doc, &[]).strips.is_empty());
}

#[test]
fn viewport_roundtrips_beats() {
    let viewport = Viewport::default();
    let beat = Rational::new(4, 1);
    let x = viewport.beat_to_x(beat);
    assert!((viewport.x_to_beat(x) - beat).as_f64().abs() < 1e-6);
}
