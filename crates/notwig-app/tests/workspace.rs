use std::path::PathBuf;

use notwig_app::workspace::Workspace;
use notwig_core::ids::{Asset as AssetMarker, Clip as ClipMarker, Note as NoteMarker};
use notwig_core::protocol::CommandPayload;
use notwig_core::sequencer::NoteEntity;
use notwig_core::{ClipKind, ClipPayload, Db, DeviceKind, Id, Norm01, Rational, TrackKind};
use notwig_ui::{NotwigUi, RuntimeView, UiIntent};
use uuid::Uuid;

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("notwig-workspace-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn extension_instrument_spec() -> notwig_dsp::DeviceSpec {
    notwig_dsp::DeviceSpec {
        type_id: "test.phase".into(),
        version: notwig_core::SchemaVersion::V0,
        kind: DeviceKind::Instrument,
        ports: Vec::new(),
        params: Vec::new(),
        latency_samples: 0,
        name: "Phase".into(),
        vendor: "Test".into(),
        ui: notwig_dsp::DeviceUiSpec::default(),
    }
}

fn instrument_workspace_with_extension()
-> (Workspace, notwig_core::TrackId, notwig_core::DeviceChainId) {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.set_extension_devices(vec![extension_instrument_spec()], None);
    let track = workspace.create_track(TrackKind::Instrument).unwrap();
    let chain = workspace.doc.tracks[&track].main_chain;
    (workspace, track, chain)
}

#[test]
fn create_clip_save_and_render() {
    let dir = temp_dir();
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Instrument).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Instrument)
        .unwrap()
        .id;
    workspace
        .create_midi_clip(
            track,
            notwig_core::RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
        )
        .unwrap();
    assert_eq!(workspace.doc.arranger.clips.len(), 1);
    let clip = *workspace.doc.arranger.clips.keys().next().unwrap();
    assert_eq!(workspace.doc.arranger.clips[&clip].kind, ClipKind::Midi);
    let bundle = dir.join("Test.notwig");
    workspace.save(Some(&bundle)).unwrap();
    let out = dir.join("mix.wav");
    let frames = workspace.render_to(&out).unwrap();
    assert!(frames > 0);
    assert!(out.exists());
    let reopened = Workspace::open(&bundle).unwrap();
    assert_eq!(reopened.doc.tracks.len(), workspace.doc.tracks.len());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn creation_helpers_return_created_ids() {
    let mut workspace = Workspace::new_empty().unwrap();
    let instrument = workspace.create_track(TrackKind::Instrument).unwrap();
    assert!(workspace.doc.tracks.contains_key(&instrument));
    let midi_clip = workspace
        .create_midi_clip(
            instrument,
            notwig_core::RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
        )
        .unwrap();
    assert!(workspace.doc.arranger.clips.contains_key(&midi_clip));

    let audio = workspace.create_track(TrackKind::Audio).unwrap();
    assert!(workspace.doc.tracks.contains_key(&audio));
    let audio_clip = workspace
        .create_audio_clip(
            audio,
            notwig_core::RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
        )
        .unwrap();
    assert!(workspace.doc.arranger.clips.contains_key(&audio_clip));

    let group = workspace.group_tracks(&[instrument]).unwrap();
    assert_eq!(workspace.doc.tracks[&group].kind, TrackKind::Group);
}

#[test]
fn split_clip_generates_new_note_ids() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Instrument).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Instrument)
        .unwrap()
        .id;
    workspace
        .create_midi_clip(
            track,
            notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
        )
        .unwrap();
    let clip = *workspace.doc.arranger.clips.keys().next().unwrap();
    let note_id = Id::<NoteMarker>::from_uuid(Uuid::new_v4());
    workspace
        .apply(CommandPayload::SetNote {
            clip_id: clip,
            note: NoteEntity {
                id: note_id,
                pitch: 60,
                start: Rational::zero(),
                length: Rational::new(2, 1),
                velocity: Norm01::clamped(0.8),
                release_velocity: None,
                expression: std::collections::BTreeMap::default(),
            },
        })
        .unwrap();
    workspace.split_clip(clip, Rational::new(1, 1)).unwrap();
    assert_eq!(workspace.doc.arranger.clips.len(), 2);
    let notes = workspace
        .doc
        .arranger
        .clips
        .values()
        .filter_map(|clip| match &clip.payload {
            ClipPayload::Midi(midi) => Some(midi.notes.len()),
            ClipPayload::Audio(_) => None,
        })
        .sum::<usize>();
    assert_eq!(notes, 2);
}

#[test]
fn stop_returns_to_play_start() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.set_playhead(Rational::new(2, 1)).unwrap();
    workspace.play().unwrap();
    assert_eq!(workspace.play_start, Some(Rational::new(2, 1)));
    workspace.set_playhead(Rational::new(5, 1)).unwrap();
    workspace.stop().unwrap();
    assert!(!workspace.doc.transport.playing);
    assert_eq!(workspace.doc.transport.playhead, Rational::new(2, 1));
}

#[test]
fn import_audio_at_places_clip_on_track() {
    let dir = temp_dir();
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Audio).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Audio)
        .unwrap()
        .id;
    workspace.save(Some(&dir.join("Import.notwig"))).unwrap();
    let source = dir.join("tone.wav");
    let samples: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.02).sin()).collect();
    notwig_project::write_wav_f32(&source, 48_000, 1, &samples).unwrap();
    workspace
        .import_audio_at(&source, track, Rational::new(2, 1))
        .unwrap();
    assert_eq!(workspace.doc.assets.assets.len(), 1);
    let clip = workspace.doc.arranger.clips.values().next().unwrap();
    assert_eq!(clip.range.start, Rational::new(2, 1));
    assert_eq!(clip.name.as_deref(), Some("tone.wav"));
    assert_eq!(workspace.assets.len(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn device_parameters_affect_offline_render() {
    let dir = temp_dir();
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Audio).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Audio)
        .unwrap()
        .clone();
    workspace.save(Some(&dir.join("Devices.notwig"))).unwrap();
    let source = dir.join("tone.wav");
    let samples: Vec<f32> = (0..24_000).map(|_| 0.5).collect();
    notwig_project::write_wav_f32(&source, 48_000, 1, &samples).unwrap();
    workspace
        .import_audio_at(&source, track.id, Rational::zero())
        .unwrap();
    workspace
        .insert_stock(
            track.main_chain,
            notwig_dsp::stock::TOOL_TYPE_ID,
            DeviceKind::AudioFx,
            None,
        )
        .unwrap();
    let device = *workspace.doc.devices.keys().next().unwrap();
    workspace
        .apply(CommandPayload::SetParam {
            device_id: device,
            param_id: notwig_dsp::stock::p_output_gain(),
            value: notwig_core::ParamValue::F64(-60.0),
            ramp: None,
        })
        .unwrap();
    let out = dir.join("device.wav");
    workspace.render_to(&out).unwrap();
    let rendered = notwig_project::decode_audio(&out).unwrap();
    let peak = rendered
        .samples
        .iter()
        .fold(0.0_f32, |acc, value| acc.max(value.abs()));
    assert!(peak < 0.01, "device gain should apply, peak={peak}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn imports_and_renders_mp3() {
    let dir = temp_dir();
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Audio).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Audio)
        .unwrap()
        .id;
    workspace.save(Some(&dir.join("Mp3.notwig"))).unwrap();
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../notwig-project/tests/fixtures/tone.mp3");
    workspace
        .import_audio_at(&source, track, Rational::zero())
        .unwrap();
    assert_eq!(workspace.doc.assets.assets.len(), 1);
    let out = dir.join("mp3.wav");
    assert!(workspace.render_to(&out).unwrap() > 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn copy_paste_and_time_range_edits_work() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Instrument).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Instrument)
        .unwrap()
        .id;
    workspace
        .create_midi_clip(
            track,
            notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
        )
        .unwrap();
    let clip = *workspace.doc.arranger.clips.keys().next().unwrap();
    workspace.copy_clips(&[clip]);
    let result = workspace.paste_clips(Rational::new(4, 1)).unwrap();
    assert_eq!(workspace.doc.arranger.clips.len(), 2);
    assert_eq!(result.objects.len(), 1);
    assert!(result.time.is_none());
}

#[test]
fn audio_editor_copy_replaces_the_arranger_clipboard() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Instrument).unwrap();
    let instrument = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Instrument)
        .unwrap()
        .id;
    workspace
        .create_midi_clip(
            instrument,
            notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
        )
        .unwrap();
    let midi_clip = *workspace.doc.arranger.clips.keys().next().unwrap();
    assert_eq!(workspace.copy_clips(&[midi_clip]), 1);
    assert!(matches!(
        workspace.clipboard(),
        notwig_ui::Clipboard::Arranger(_)
    ));

    workspace.create_track(TrackKind::Audio).unwrap();
    let audio_track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Audio)
        .unwrap()
        .id;
    let asset = Id::<AssetMarker>::from_uuid(Uuid::from_u128(600));
    workspace
        .apply(CommandPayload::RegisterAsset {
            asset_id: asset,
            entry: notwig_core::AssetEntry {
                id: asset,
                kind: notwig_core::AssetKind::Audio,
                uri: "assets/test.wav".into(),
                original_name: None,
                hash: None,
                format: "wav".into(),
                sample_rate: Some(48_000),
                channels: Some(2),
                duration_samples: Some(96_000),
                original_sample_rate: Some(48_000),
                original_channels: Some(2),
            },
        })
        .unwrap();
    let audio_clip = Id::<ClipMarker>::from_uuid(Uuid::from_u128(700));
    let slice = notwig_core::SliceId::from_uuid(Uuid::from_u128(77));
    workspace
        .apply(CommandPayload::CreateClip {
            id: audio_clip,
            track_id: audio_track,
            kind: ClipKind::Audio,
            range: notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
            payload: ClipPayload::Audio(notwig_core::AudioClipData {
                asset_id: asset,
                source_start_sample: 0,
                source_end_sample: 96_000,
                gain_db: Db(0.0),
                fade_in_samples: 0,
                fade_out_samples: 0,
                fade_shape: notwig_core::sequencer::FadeShape::Linear,
                slices: vec![notwig_core::sequencer::AudioSlice {
                    id: slice,
                    start: Rational::zero(),
                    source_start_sample: 0,
                    source_end_sample: 48_000,
                    silent_prefix_samples: 0,
                    gain_db: Db(0.0),
                    asset_id: None,
                }],
                slices_enabled: true,
            }),
        })
        .unwrap();

    let mut ui = NotwigUi::default();
    ui.state
        .selection
        .select_clip(audio_clip, audio_track, false);
    ui.open_clip_editor(audio_clip);
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 900.0));
    let runtime = RuntimeView {
        clipboard: workspace.clipboard().clone(),
        ..RuntimeView::default()
    };
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, &workspace.doc, &runtime);
        },
    );
    ui.clip_editor
        .as_mut()
        .expect("clip editor open")
        .select_node(slice, false);
    let mut intents = Vec::new();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            events: vec![egui::Event::Copy],
            ..Default::default()
        },
        |ctx| {
            intents = ui.show(ctx, &workspace.doc, &runtime);
        },
    );
    let clipboard = intents
        .iter()
        .find_map(|intent| match intent {
            UiIntent::SetClipboard(clipboard) => Some(clipboard.clone()),
            _ => None,
        })
        .expect("audio copy should publish an extract to the shared clipboard");
    workspace.set_clipboard(clipboard);
    assert!(matches!(
        workspace.clipboard(),
        notwig_ui::Clipboard::AudioClip(_)
    ));

    let err = workspace
        .paste_clips(Rational::new(4, 1))
        .expect_err("an audio extract must not paste into the arranger");
    assert!(err.contains("another editor"));
    assert_eq!(
        workspace.doc.arranger.clips.len(),
        2,
        "the rejected paste must not change the arrangement"
    );
}

#[test]
fn node_duplicate_places_a_copy_after_the_source_clip() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Instrument).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Instrument)
        .unwrap()
        .id;
    workspace
        .create_midi_clip(
            track,
            notwig_core::RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
        )
        .unwrap();
    let clip = *workspace.doc.arranger.clips.keys().next().unwrap();
    workspace.duplicate_clip(clip).unwrap();
    let starts: Vec<_> = workspace
        .doc
        .arranger
        .clips
        .values()
        .map(|clip| clip.range.start)
        .collect();
    assert!(starts.contains(&Rational::zero()));
    assert!(starts.contains(&Rational::new(2, 1)));
}

#[test]
fn delete_time_range_splits_multiple_audio_slices() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Audio).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Audio)
        .unwrap()
        .id;
    let asset = Id::<AssetMarker>::from_uuid(Uuid::from_u128(600));
    workspace
        .apply(CommandPayload::RegisterAsset {
            asset_id: asset,
            entry: notwig_core::AssetEntry {
                id: asset,
                kind: notwig_core::AssetKind::Audio,
                uri: "assets/test.wav".into(),
                original_name: None,
                hash: None,
                format: "wav".into(),
                sample_rate: Some(48_000),
                channels: Some(2),
                duration_samples: Some(96_000),
                original_sample_rate: Some(48_000),
                original_channels: Some(2),
            },
        })
        .unwrap();
    let clip = Id::<ClipMarker>::from_uuid(Uuid::from_u128(700));
    workspace
        .apply(CommandPayload::CreateClip {
            id: clip,
            track_id: track,
            kind: ClipKind::Audio,
            range: notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
            payload: ClipPayload::Audio(notwig_core::AudioClipData {
                asset_id: asset,
                source_start_sample: 0,
                source_end_sample: 96_000,
                gain_db: Db(0.0),
                fade_in_samples: 0,
                fade_out_samples: 0,
                fade_shape: notwig_core::sequencer::FadeShape::Linear,
                slices: vec![
                    notwig_core::sequencer::AudioSlice {
                        id: notwig_core::SliceId::from_uuid(Uuid::from_u128(1)),
                        start: Rational::zero(),
                        source_start_sample: 0,
                        source_end_sample: 48_000,
                        silent_prefix_samples: 0,
                        gain_db: Db(0.0),
                        asset_id: None,
                    },
                    notwig_core::sequencer::AudioSlice {
                        id: notwig_core::SliceId::from_uuid(Uuid::from_u128(2)),
                        start: Rational::new(2, 1),
                        source_start_sample: 48_000,
                        source_end_sample: 96_000,
                        silent_prefix_samples: 0,
                        gain_db: Db(0.0),
                        asset_id: None,
                    },
                ],
                slices_enabled: true,
            }),
        })
        .unwrap();

    workspace
        .delete_time_range(
            notwig_core::RangeMus::new(Rational::new(1, 1), Rational::new(2, 1)).unwrap(),
            &[track],
        )
        .unwrap();

    let clips: Vec<_> = workspace
        .doc
        .arranger
        .clips
        .values()
        .filter(|clip| clip.track_id == track)
        .collect();
    assert_eq!(clips.len(), 2);
    let left = clips
        .iter()
        .find(|clip| clip.range.start == Rational::zero())
        .unwrap();
    let right = clips
        .iter()
        .find(|clip| clip.range.start == Rational::new(2, 1))
        .unwrap();
    let ClipPayload::Audio(left_audio) = &left.payload else {
        panic!("left audio");
    };
    let ClipPayload::Audio(right_audio) = &right.payload else {
        panic!("right audio");
    };
    assert_eq!(left_audio.slices.len(), 1);
    assert_eq!(left_audio.slices[0].source_end_sample, 24_000);
    assert_eq!(right_audio.slices.len(), 1);
    assert_eq!(right_audio.slices[0].start, Rational::zero());
    assert_eq!(right_audio.slices[0].source_start_sample, 48_000);
}

#[test]
fn deleting_an_empty_time_selection_splits_clips_at_the_point() {
    let mut workspace = Workspace::new_empty().unwrap();
    let track = workspace.create_track(TrackKind::Instrument).unwrap();
    workspace
        .create_midi_clip(
            track,
            notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
        )
        .unwrap();
    workspace
        .create_midi_clip(
            track,
            notwig_core::RangeMus::new(Rational::new(4, 1), Rational::new(8, 1)).unwrap(),
        )
        .unwrap();
    let point = notwig_core::RangeMus::new(Rational::new(2, 1), Rational::new(2, 1)).unwrap();

    let info = workspace
        .delete_selection(&[], Some((point, vec![track])))
        .unwrap();
    assert_eq!(workspace.doc.arranger.clips.len(), 3);
    assert!(info.time.is_some());
    assert!(
        workspace
            .doc
            .arranger
            .clips
            .values()
            .any(|clip| clip.range.start == Rational::zero()
                && clip.range.end == Rational::new(2, 1))
    );
    assert!(
        workspace
            .doc
            .arranger
            .clips
            .values()
            .any(|clip| clip.range.start == Rational::new(2, 1)
                && clip.range.end == Rational::new(4, 1))
    );
    assert!(
        workspace
            .doc
            .arranger
            .clips
            .values()
            .any(|clip| clip.range.start == Rational::new(4, 1)
                && clip.range.end == Rational::new(8, 1))
    );

    workspace
        .delete_selection(&[], Some((point, vec![track])))
        .unwrap();
    assert_eq!(workspace.doc.arranger.clips.len(), 3);
}

#[test]
fn adding_an_audio_event_keeps_the_clip_range() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Audio).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Audio)
        .unwrap()
        .id;
    let register = |workspace: &mut Workspace, asset: Id<AssetMarker>, duration: u64| {
        workspace
            .apply(CommandPayload::RegisterAsset {
                asset_id: asset,
                entry: notwig_core::AssetEntry {
                    id: asset,
                    kind: notwig_core::AssetKind::Audio,
                    uri: format!("assets/{asset}.wav"),
                    original_name: None,
                    hash: None,
                    format: "wav".into(),
                    sample_rate: Some(48_000),
                    channels: Some(2),
                    duration_samples: Some(duration),
                    original_sample_rate: Some(48_000),
                    original_channels: Some(2),
                },
            })
            .unwrap();
    };
    let base = Id::<AssetMarker>::from_uuid(Uuid::from_u128(700));
    let dropped = Id::<AssetMarker>::from_uuid(Uuid::from_u128(701));
    register(&mut workspace, base, 48_000);
    register(&mut workspace, dropped, 48_000);
    let clip = Id::<ClipMarker>::from_uuid(Uuid::from_u128(702));
    workspace
        .apply(CommandPayload::CreateClip {
            id: clip,
            track_id: track,
            kind: ClipKind::Audio,
            range: notwig_core::RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
            payload: ClipPayload::Audio(notwig_core::AudioClipData {
                asset_id: base,
                source_start_sample: 0,
                source_end_sample: 48_000,
                gain_db: Db(0.0),
                fade_in_samples: 0,
                fade_out_samples: 0,
                fade_shape: notwig_core::sequencer::FadeShape::Linear,
                slices: Vec::new(),
                slices_enabled: false,
            }),
        })
        .unwrap();

    workspace
        .add_audio_slice(clip, dropped, Rational::new(3, 2))
        .unwrap();
    let clip = &workspace.doc.arranger.clips[&clip];
    assert_eq!(clip.range.end, Rational::new(2, 1));
    let ClipPayload::Audio(audio) = &clip.payload else {
        panic!("audio clip");
    };
    assert_eq!(audio.slices.len(), 2);
    let added = audio
        .slices
        .iter()
        .find(|slice| slice.asset_id == Some(dropped))
        .expect("dropped event");
    assert_eq!(added.start, Rational::new(3, 2));
    assert_eq!(added.source_end_sample, 48_000);
}

#[test]
fn duplicate_clears_the_destination_range_before_inserting() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Instrument).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Instrument)
        .unwrap()
        .id;
    workspace
        .create_midi_clip(
            track,
            notwig_core::RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
        )
        .unwrap();
    workspace
        .create_midi_clip(
            track,
            notwig_core::RangeMus::new(Rational::new(2, 1), Rational::new(6, 1)).unwrap(),
        )
        .unwrap();
    let source = workspace
        .doc
        .arranger
        .clips
        .values()
        .find(|clip| clip.range.start == Rational::zero())
        .unwrap()
        .id;
    workspace.duplicate_clip(source).unwrap();

    let mut ranges: Vec<_> = workspace
        .doc
        .arranger
        .clips
        .values()
        .filter(|clip| clip.track_id == track)
        .map(|clip| (clip.range.start, clip.range.end))
        .collect();
    ranges.sort();
    assert_eq!(
        ranges,
        vec![
            (Rational::zero(), Rational::new(2, 1)),
            (Rational::new(2, 1), Rational::new(4, 1)),
            (Rational::new(4, 1), Rational::new(6, 1)),
        ]
    );
}

#[test]
fn copying_a_time_range_trims_audio_slice_windows() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Audio).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Audio)
        .unwrap()
        .id;
    let asset = Id::<AssetMarker>::from_uuid(Uuid::from_u128(800));
    workspace
        .apply(CommandPayload::RegisterAsset {
            asset_id: asset,
            entry: notwig_core::AssetEntry {
                id: asset,
                kind: notwig_core::AssetKind::Audio,
                uri: "assets/copy.wav".into(),
                original_name: None,
                hash: None,
                format: "wav".into(),
                sample_rate: Some(48_000),
                channels: Some(2),
                duration_samples: Some(96_000),
                original_sample_rate: Some(48_000),
                original_channels: Some(2),
            },
        })
        .unwrap();
    workspace
        .apply(CommandPayload::CreateClip {
            id: Id::<ClipMarker>::from_uuid(Uuid::from_u128(801)),
            track_id: track,
            kind: ClipKind::Audio,
            range: notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
            payload: ClipPayload::Audio(notwig_core::AudioClipData {
                asset_id: asset,
                source_start_sample: 0,
                source_end_sample: 96_000,
                gain_db: Db(0.0),
                fade_in_samples: 0,
                fade_out_samples: 0,
                fade_shape: notwig_core::sequencer::FadeShape::Linear,
                slices: vec![
                    notwig_core::sequencer::AudioSlice {
                        id: notwig_core::SliceId::from_uuid(Uuid::from_u128(1)),
                        start: Rational::zero(),
                        source_start_sample: 0,
                        source_end_sample: 48_000,
                        silent_prefix_samples: 0,
                        gain_db: Db(0.0),
                        asset_id: None,
                    },
                    notwig_core::sequencer::AudioSlice {
                        id: notwig_core::SliceId::from_uuid(Uuid::from_u128(2)),
                        start: Rational::new(2, 1),
                        source_start_sample: 48_000,
                        source_end_sample: 96_000,
                        silent_prefix_samples: 0,
                        gain_db: Db(0.0),
                        asset_id: None,
                    },
                ],
                slices_enabled: true,
            }),
        })
        .unwrap();
    workspace.copy_time_range(
        notwig_core::RangeMus::new(Rational::new(1, 1), Rational::new(3, 1)).unwrap(),
        &[track],
    );
    let result = workspace.paste_clips(Rational::new(4, 1)).unwrap();
    let time = result.time.expect("time selection");
    assert_eq!(
        time.span,
        notwig_core::RangeMus::new(Rational::new(4, 1), Rational::new(6, 1)).unwrap()
    );
    let pasted = workspace
        .doc
        .arranger
        .clips
        .values()
        .find(|clip| clip.range.start == Rational::new(4, 1))
        .expect("pasted clip");
    let ClipPayload::Audio(audio) = &pasted.payload else {
        panic!("pasted audio");
    };
    assert_eq!(audio.slices.len(), 2);
    assert_eq!(audio.slices[0].start, Rational::zero());
    assert_eq!(audio.slices[0].source_start_sample, 24_000);
    assert_eq!(audio.slices[0].source_end_sample, 48_000);
    assert_eq!(audio.slices[1].start, Rational::new(1, 1));
    assert_eq!(audio.slices[1].source_start_sample, 48_000);
    assert_eq!(audio.slices[1].source_end_sample, 72_000);
}

#[test]
fn paste_clears_the_destination_range_before_inserting() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Instrument).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Instrument)
        .unwrap()
        .id;
    workspace
        .create_midi_clip(
            track,
            notwig_core::RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
        )
        .unwrap();
    let source = *workspace.doc.arranger.clips.keys().next().unwrap();
    workspace.copy_clips(&[source]);
    workspace.paste_clips(Rational::new(1, 1)).unwrap();
    let mut ranges: Vec<_> = workspace
        .doc
        .arranger
        .clips
        .values()
        .filter(|clip| clip.track_id == track)
        .map(|clip| (clip.range.start, clip.range.end))
        .collect();
    ranges.sort();
    assert_eq!(
        ranges,
        vec![
            (Rational::zero(), Rational::new(1, 1)),
            (Rational::new(1, 1), Rational::new(3, 1)),
        ]
    );
}

#[test]
fn duplicate_places_a_fresh_clip_after_the_source() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Instrument).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Instrument)
        .unwrap()
        .id;
    workspace
        .create_midi_clip(
            track,
            notwig_core::RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
        )
        .unwrap();
    let clip = *workspace.doc.arranger.clips.keys().next().unwrap();
    let new_id = workspace.duplicate_clip(clip).unwrap();
    assert_ne!(new_id, clip);
    assert_eq!(
        workspace.doc.arranger.clips[&new_id].range,
        notwig_core::RangeMus::new(Rational::new(2, 1), Rational::new(4, 1)).unwrap()
    );
}

#[test]
fn duplicate_and_delete_time_range_work() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Instrument).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Instrument)
        .unwrap()
        .id;
    workspace
        .create_midi_clip(
            track,
            notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
        )
        .unwrap();
    workspace
        .duplicate_time_range(
            notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
            &[track],
        )
        .unwrap();
    assert_eq!(workspace.doc.arranger.clips.len(), 2);

    workspace
        .delete_time_range(
            notwig_core::RangeMus::new(Rational::new(1, 1), Rational::new(3, 1)).unwrap(),
            &[track],
        )
        .unwrap();
    let clips: Vec<_> = workspace.doc.arranger.clips.values().collect();
    assert!(clips.iter().any(|clip| {
        clip.range.start == Rational::zero() && clip.range.end == Rational::new(1, 1)
    }));
    assert!(clips.iter().any(|clip| {
        clip.range.start == Rational::new(3, 1) && clip.range.end == Rational::new(4, 1)
    }));
}

#[test]
fn copy_bar_selection_then_paste_at_new_time() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Instrument).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Instrument)
        .unwrap()
        .id;
    workspace
        .create_midi_clip(
            track,
            notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
        )
        .unwrap();
    workspace.copy_selection(
        &[],
        Some((
            notwig_core::RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
            vec![track],
        )),
    );
    let result = workspace.paste_clips(Rational::new(8, 1)).unwrap();
    assert!(
        workspace
            .doc
            .arranger
            .clips
            .values()
            .any(|clip| clip.range.start == Rational::new(8, 1))
    );
    let time = result.time.expect("bar selection should be selected");
    assert_eq!(time.span.start, Rational::new(8, 1));
    assert_eq!(time.span.end, Rational::new(10, 1));
    let tracks: Vec<_> = time.lanes.iter().copied().collect();
    assert_eq!(tracks, vec![track]);
}

#[test]
fn grouping_tracks_moves_them_under_a_group_bus() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Audio).unwrap();
    workspace.create_track(TrackKind::Audio).unwrap();
    let tracks: Vec<_> = workspace
        .doc
        .tracks
        .values()
        .filter(|track| track.kind == TrackKind::Audio)
        .map(|track| track.id)
        .collect();
    let group = workspace.group_tracks(&tracks).unwrap();
    let group_track = workspace.doc.tracks.get(&group).unwrap();
    assert_eq!(group_track.kind, TrackKind::Group);
    for track in &tracks {
        assert_eq!(workspace.doc.tracks[track].parent, Some(group));
    }
    let children = workspace
        .doc
        .track_order
        .children
        .get(&group)
        .expect("group children");
    assert_eq!(children.len(), tracks.len());
    for track in &tracks {
        assert!(children.contains(track));
    }
}

#[test]
fn track_color_and_comment_round_trip() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Audio).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Audio)
        .unwrap()
        .id;
    let color = notwig_core::Color {
        r: 64,
        g: 128,
        b: 192,
        a: 255,
    };
    workspace
        .apply(CommandPayload::SetTrackColor {
            id: track,
            color: Some(color),
        })
        .unwrap();
    workspace
        .apply(CommandPayload::SetTrackComment {
            id: track,
            comment: "Lead bus".into(),
        })
        .unwrap();
    assert_eq!(workspace.doc.tracks[&track].color, Some(color));
    assert_eq!(workspace.doc.tracks[&track].comment, "Lead bus");
}

#[test]
fn duplicating_a_track_copies_clips_and_devices() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Instrument).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Instrument)
        .unwrap()
        .id;
    workspace
        .create_midi_clip(
            track,
            notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
        )
        .unwrap();
    workspace
        .insert_stock(
            workspace.doc.tracks[&track].main_chain,
            notwig_dsp::stock::TOOL_TYPE_ID,
            DeviceKind::AudioFx,
            None,
        )
        .unwrap();
    let copy = workspace.duplicate_track(track).unwrap();
    assert_eq!(workspace.doc.tracks[&copy].parent, None);
    assert_eq!(
        workspace.doc.arranger.lanes[&copy].clips.len(),
        workspace.doc.arranger.lanes[&track].clips.len()
    );
    assert_eq!(
        workspace.doc.chains[&workspace.doc.tracks[&copy].main_chain]
            .devices
            .len(),
        1
    );
}

#[test]
fn grouping_a_group_keeps_the_track_forest_valid() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Audio).unwrap();
    workspace.create_track(TrackKind::Audio).unwrap();
    let tracks: Vec<_> = workspace
        .doc
        .tracks
        .values()
        .filter(|track| track.kind == TrackKind::Audio)
        .map(|track| track.id)
        .collect();
    let inner = workspace.group_tracks(&tracks).unwrap();
    let outer = workspace.group_tracks(&[inner]).unwrap();
    assert_eq!(workspace.doc.tracks[&inner].parent, Some(outer));
    workspace.doc.validate().unwrap();
}

#[test]
fn undo_redo_restores_document_snapshots() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Instrument).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Instrument)
        .unwrap()
        .id;
    workspace
        .create_midi_clip(
            track,
            notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
        )
        .unwrap();
    assert_eq!(workspace.doc.arranger.clips.len(), 1);
    workspace.undo().unwrap();
    assert_eq!(workspace.doc.arranger.clips.len(), 0);
    workspace.redo().unwrap();
    assert_eq!(workspace.doc.arranger.clips.len(), 1);
}

#[test]
fn inserting_stock_device_rebuilds_engine() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Audio).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Audio)
        .unwrap()
        .clone();
    workspace
        .insert_stock(
            track.main_chain,
            notwig_dsp::stock::TOOL_TYPE_ID,
            DeviceKind::AudioFx,
            None,
        )
        .unwrap();
    assert_eq!(workspace.doc.devices.len(), 1);
    while workspace.engine_building() {
        workspace.poll_engine_build();
        std::thread::yield_now();
    }
    assert!(workspace.engine_ready());
}

#[test]
fn time_selection_move_clips_notes_and_undoes_once() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Instrument).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Instrument)
        .unwrap()
        .id;
    workspace
        .create_midi_clip(
            track,
            notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
        )
        .unwrap();
    let clip = *workspace.doc.arranger.clips.keys().next().unwrap();
    workspace
        .apply(CommandPayload::SetNote {
            clip_id: clip,
            note: NoteEntity {
                id: Id::<NoteMarker>::from_uuid(Uuid::new_v4()),
                pitch: 60,
                start: Rational::new(3, 2),
                length: Rational::new(1, 2),
                velocity: Norm01::clamped(0.8),
                release_velocity: None,
                expression: std::collections::BTreeMap::default(),
            },
        })
        .unwrap();
    let before = workspace.doc.clone();
    let info = workspace
        .move_time_selection(
            notwig_core::RangeMus::new(Rational::new(1, 1), Rational::new(2, 1)).unwrap(),
            &[track],
            Rational::new(2, 1),
        )
        .unwrap();
    assert_eq!(
        info.time.expect("time selection").span,
        notwig_core::RangeMus::new(Rational::new(3, 1), Rational::new(4, 1)).unwrap()
    );
    let mut ranges: Vec<_> = workspace
        .doc
        .arranger
        .clips
        .values()
        .filter(|clip| clip.track_id == track)
        .map(|clip| (clip.range.start, clip.range.end))
        .collect();
    ranges.sort();
    assert_eq!(
        ranges,
        vec![
            (Rational::zero(), Rational::new(1, 1)),
            (Rational::new(2, 1), Rational::new(3, 1)),
            (Rational::new(3, 1), Rational::new(4, 1)),
        ]
    );
    let moved = workspace
        .doc
        .arranger
        .clips
        .values()
        .find(|clip| clip.range.start == Rational::new(3, 1))
        .expect("moved clip");
    let ClipPayload::Midi(midi) = &moved.payload else {
        panic!("moved clip is MIDI");
    };
    assert_eq!(midi.notes.len(), 1);
    assert_eq!(midi.notes[0].start, Rational::new(1, 2));
    assert_eq!(midi.notes[0].length, Rational::new(1, 2));

    workspace.undo().unwrap();
    assert_eq!(workspace.doc.arranger.clips, before.arranger.clips);
}

#[test]
fn trimming_a_midi_clip_start_advances_the_content_window() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Instrument).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Instrument)
        .unwrap()
        .id;
    workspace
        .create_midi_clip(
            track,
            notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
        )
        .unwrap();
    let clip = *workspace.doc.arranger.clips.keys().next().unwrap();
    workspace
        .apply(CommandPayload::SetNote {
            clip_id: clip,
            note: NoteEntity {
                id: Id::<NoteMarker>::from_uuid(Uuid::new_v4()),
                pitch: 60,
                start: Rational::new(1, 1),
                length: Rational::new(1, 1),
                velocity: Norm01::clamped(0.8),
                release_velocity: None,
                expression: std::collections::BTreeMap::default(),
            },
        })
        .unwrap();
    workspace
        .trim_clip(
            clip,
            notwig_core::RangeMus::new(Rational::new(1, 1), Rational::new(4, 1)).unwrap(),
        )
        .unwrap();
    let clip = &workspace.doc.arranger.clips[&clip];
    assert_eq!(
        clip.range,
        notwig_core::RangeMus::new(Rational::new(1, 1), Rational::new(4, 1)).unwrap()
    );
    let ClipPayload::Midi(midi) = &clip.payload else {
        panic!("clip is MIDI");
    };
    assert_eq!(midi.content_start, Rational::new(1, 1));
    assert_eq!(midi.notes[0].start, Rational::new(1, 1));
}

#[test]
fn duplicating_a_clip_remaps_note_ids() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Instrument).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Instrument)
        .unwrap()
        .id;
    workspace
        .create_midi_clip(
            track,
            notwig_core::RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
        )
        .unwrap();
    let clip = *workspace.doc.arranger.clips.keys().next().unwrap();
    let note_id = Id::<NoteMarker>::from_uuid(Uuid::new_v4());
    workspace
        .apply(CommandPayload::SetNote {
            clip_id: clip,
            note: NoteEntity {
                id: note_id,
                pitch: 60,
                start: Rational::zero(),
                length: Rational::new(1, 1),
                velocity: Norm01::clamped(0.8),
                release_velocity: None,
                expression: std::collections::BTreeMap::default(),
            },
        })
        .unwrap();
    let copy = workspace.duplicate_clip(clip).unwrap();
    let ClipPayload::Midi(midi) = &workspace.doc.arranger.clips[&copy].payload else {
        panic!("copy is MIDI");
    };
    assert_eq!(midi.notes.len(), 1);
    assert_ne!(midi.notes[0].id, note_id);
}

#[test]
fn new_workspace_requests_engine_build_without_waiting() {
    let workspace = Workspace::new_empty().unwrap();
    assert!(workspace.engine_building());
    assert!(workspace.engine_build_pending());
    assert_eq!(workspace.engine_build_requests(), 1);
}

#[test]
fn poll_engine_build_installs_engine_and_clears_state() {
    let mut workspace = Workspace::new_empty().unwrap();
    let requests = workspace.engine_build_requests();
    while workspace.engine_building() {
        workspace.poll_engine_build();
    }
    assert!(!workspace.engine_building());
    assert_eq!(workspace.engine_build_requests(), requests);
    assert!(
        workspace.error.is_none(),
        "unexpected engine build error: {:?}",
        workspace.error
    );
    assert!(workspace.engine_ready());
}

#[test]
fn non_structural_undo_does_not_request_engine_rebuild() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Audio).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Audio)
        .unwrap()
        .id;
    let requests = workspace.engine_build_requests();

    workspace
        .apply(CommandPayload::RenameTrack {
            id: track,
            name: "Renamed".into(),
        })
        .unwrap();
    workspace
        .apply(CommandPayload::SetTrackComment {
            id: track,
            comment: "A comment".into(),
        })
        .unwrap();
    workspace
        .set_mixer(
            track,
            Some(Db(-6.0)),
            Some(notwig_core::Pan::clamped(-0.5)),
            Some(true),
            None,
        )
        .unwrap();
    assert_eq!(workspace.engine_build_requests(), requests);

    workspace.undo().unwrap();
    assert_eq!(workspace.engine_build_requests(), requests);
    workspace.undo().unwrap();
    assert_eq!(workspace.engine_build_requests(), requests);
    workspace.undo().unwrap();
    assert_eq!(workspace.engine_build_requests(), requests);
    workspace.redo().unwrap();
    assert_eq!(workspace.engine_build_requests(), requests);
    workspace.redo().unwrap();
    assert_eq!(workspace.engine_build_requests(), requests);
    workspace.redo().unwrap();
    assert_eq!(workspace.engine_build_requests(), requests);
}

#[test]
fn structural_undo_and_redo_request_engine_rebuild() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Audio).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Audio)
        .unwrap()
        .clone();
    let before_insert = workspace.engine_build_requests();
    workspace
        .insert_stock(
            track.main_chain,
            notwig_dsp::stock::TOOL_TYPE_ID,
            DeviceKind::AudioFx,
            None,
        )
        .unwrap();
    assert_eq!(workspace.engine_build_requests(), before_insert + 1);

    let before_undo = workspace.engine_build_requests();
    workspace.undo().unwrap();
    assert_eq!(workspace.engine_build_requests(), before_undo + 1);
    assert!(workspace.doc.devices.is_empty());

    let before_redo = workspace.engine_build_requests();
    workspace.redo().unwrap();
    assert_eq!(workspace.engine_build_requests(), before_redo + 1);
    assert_eq!(workspace.doc.devices.len(), 1);
}

#[test]
fn apply_batch_records_one_undo_entry() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Audio).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Audio)
        .unwrap()
        .id;
    let original_name = workspace.doc.tracks[&track].name.clone();
    let original_comment = workspace.doc.tracks[&track].comment.clone();
    workspace
        .apply_batch(vec![
            CommandPayload::RenameTrack {
                id: track,
                name: "Batch".into(),
            },
            CommandPayload::SetTrackComment {
                id: track,
                comment: "Batch comment".into(),
            },
        ])
        .unwrap();
    assert_eq!(workspace.doc.tracks[&track].name, "Batch");
    assert_eq!(workspace.doc.tracks[&track].comment, "Batch comment");

    workspace.undo().unwrap();
    assert_eq!(workspace.doc.tracks[&track].name, original_name);
    assert_eq!(workspace.doc.tracks[&track].comment, original_comment);

    workspace.redo().unwrap();
    assert_eq!(workspace.doc.tracks[&track].name, "Batch");
    assert_eq!(workspace.doc.tracks[&track].comment, "Batch comment");
}

#[test]
fn creating_an_empty_audio_clip_keeps_it_silent() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Audio).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Audio)
        .unwrap()
        .id;
    let asset = Id::<AssetMarker>::from_uuid(Uuid::from_u128(900));
    workspace
        .apply(CommandPayload::RegisterAsset {
            asset_id: asset,
            entry: notwig_core::AssetEntry {
                id: asset,
                kind: notwig_core::AssetKind::Audio,
                uri: "assets/silent.wav".into(),
                original_name: None,
                hash: None,
                format: "wav".into(),
                sample_rate: Some(48_000),
                channels: Some(2),
                duration_samples: Some(48_000),
                original_sample_rate: Some(48_000),
                original_channels: Some(2),
            },
        })
        .unwrap();
    let range = notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap();
    workspace.create_audio_clip(track, range).unwrap();
    let clip = workspace
        .doc
        .arranger
        .clips
        .values()
        .find(|clip| clip.track_id == track)
        .expect("audio clip");
    assert_eq!(clip.kind, ClipKind::Audio);
    assert_eq!(clip.range, range);
    let ClipPayload::Audio(audio) = &clip.payload else {
        panic!("audio clip");
    };
    assert!(audio.slices_enabled);
    assert_eq!(audio.slices.len(), 0);
}

#[test]
fn empty_audio_clip_without_assets_round_trips_and_accepts_slices() {
    let dir = temp_dir();
    let mut workspace = Workspace::new_empty().unwrap();
    let track = workspace.create_track(TrackKind::Audio).unwrap();
    let range = notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap();
    let clip = workspace.create_audio_clip(track, range).unwrap();
    workspace.doc.validate().unwrap();

    let ClipPayload::Audio(audio) = &workspace.doc.arranger.clips[&clip].payload else {
        panic!("audio clip");
    };
    assert_eq!(audio.asset_id, notwig_core::AssetId::nil());
    assert!(audio.slices_enabled);
    assert_eq!(audio.slices.len(), 0);

    let bundle = dir.join("Empty.notwig");
    workspace.save(Some(&bundle)).unwrap();
    let mut reopened = Workspace::open(&bundle).unwrap();
    assert!(reopened.doc.arranger.clips.contains_key(&clip));
    reopened.doc.validate().unwrap();

    let asset = Id::<AssetMarker>::from_uuid(Uuid::from_u128(902));
    reopened
        .apply(CommandPayload::RegisterAsset {
            asset_id: asset,
            entry: notwig_core::AssetEntry {
                id: asset,
                kind: notwig_core::AssetKind::Audio,
                uri: "assets/later.wav".into(),
                original_name: None,
                hash: None,
                format: "wav".into(),
                sample_rate: Some(48_000),
                channels: Some(2),
                duration_samples: Some(48_000),
                original_sample_rate: Some(48_000),
                original_channels: Some(2),
            },
        })
        .unwrap();
    reopened
        .add_audio_slice(clip, asset, Rational::new(1, 1))
        .unwrap();
    let ClipPayload::Audio(audio) = &reopened.doc.arranger.clips[&clip].payload else {
        panic!("audio clip");
    };
    assert_eq!(audio.slices.len(), 1);
    assert_eq!(audio.slices[0].asset_id, Some(asset));
    reopened.doc.validate().unwrap();

    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn clip_range_changes_keep_audio_events_intact() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Audio).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Audio)
        .unwrap()
        .id;
    let asset = Id::<AssetMarker>::from_uuid(Uuid::from_u128(901));
    workspace
        .apply(CommandPayload::RegisterAsset {
            asset_id: asset,
            entry: notwig_core::AssetEntry {
                id: asset,
                kind: notwig_core::AssetKind::Audio,
                uri: "assets/events.wav".into(),
                original_name: None,
                hash: None,
                format: "wav".into(),
                sample_rate: Some(48_000),
                channels: Some(2),
                duration_samples: Some(48_000),
                original_sample_rate: Some(48_000),
                original_channels: Some(2),
            },
        })
        .unwrap();
    let clip_id = Id::<ClipMarker>::from_uuid(Uuid::from_u128(902));
    let slice = notwig_core::sequencer::AudioSlice {
        id: notwig_core::SliceId::from_uuid(Uuid::from_u128(903)),
        start: Rational::zero(),
        source_start_sample: 0,
        source_end_sample: 48_000,
        silent_prefix_samples: 0,
        gain_db: Db(0.0),
        asset_id: None,
    };
    workspace
        .apply(CommandPayload::CreateClip {
            id: clip_id,
            track_id: track,
            kind: ClipKind::Audio,
            range: notwig_core::RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
            payload: ClipPayload::Audio(notwig_core::AudioClipData {
                asset_id: asset,
                source_start_sample: 0,
                source_end_sample: 48_000,
                gain_db: Db(0.0),
                fade_in_samples: 0,
                fade_out_samples: 0,
                fade_shape: notwig_core::sequencer::FadeShape::Linear,
                slices: vec![slice.clone()],
                slices_enabled: true,
            }),
        })
        .unwrap();
    let range = notwig_core::RangeMus::new(Rational::new(1, 1), Rational::new(8, 1)).unwrap();
    workspace.set_clip_range(clip_id, range).unwrap();
    let clip = &workspace.doc.arranger.clips[&clip_id];
    assert_eq!(clip.range, range);
    let ClipPayload::Audio(audio) = &clip.payload else {
        panic!("audio clip");
    };
    assert_eq!(audio.slices.len(), 1);
    let shifted = &audio.slices[0];
    assert_eq!(shifted.id, slice.id);
    assert_eq!(shifted.source_start_sample, slice.source_start_sample);
    assert_eq!(shifted.source_end_sample, slice.source_end_sample);
    assert_eq!(
        range.start + shifted.start,
        slice.start,
        "events stay anchored to absolute time"
    );

    let outside = notwig_core::sequencer::AudioSlice {
        id: notwig_core::SliceId::from_uuid(Uuid::from_u128(904)),
        start: Rational::new(-1, 1),
        source_start_sample: 0,
        source_end_sample: 48_000,
        silent_prefix_samples: 0,
        gain_db: Db(0.0),
        asset_id: None,
    };
    workspace
        .apply(CommandPayload::SetClipSlices {
            clip_id,
            slices: vec![outside],
        })
        .expect("audio events may extend beyond the clip container");
}

#[test]
fn extending_an_audio_clip_left_adds_leading_silence() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Audio).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Audio)
        .unwrap()
        .id;
    let asset = Id::<AssetMarker>::from_uuid(Uuid::from_u128(905));
    workspace
        .apply(CommandPayload::RegisterAsset {
            asset_id: asset,
            entry: notwig_core::AssetEntry {
                id: asset,
                kind: notwig_core::AssetKind::Audio,
                uri: "assets/leading_silence.wav".into(),
                original_name: None,
                hash: None,
                format: "wav".into(),
                sample_rate: Some(48_000),
                channels: Some(2),
                duration_samples: Some(24_000),
                original_sample_rate: Some(48_000),
                original_channels: Some(2),
            },
        })
        .unwrap();
    let clip_id = Id::<ClipMarker>::from_uuid(Uuid::from_u128(906));
    workspace
        .apply(CommandPayload::CreateClip {
            id: clip_id,
            track_id: track,
            kind: ClipKind::Audio,
            range: notwig_core::RangeMus::new(Rational::new(1, 1), Rational::new(2, 1)).unwrap(),
            payload: ClipPayload::Audio(notwig_core::AudioClipData {
                asset_id: asset,
                source_start_sample: 0,
                source_end_sample: 24_000,
                gain_db: Db(0.0),
                fade_in_samples: 0,
                fade_out_samples: 0,
                fade_shape: notwig_core::sequencer::FadeShape::Linear,
                slices: Vec::new(),
                slices_enabled: false,
            }),
        })
        .unwrap();
    workspace
        .set_clip_range(
            clip_id,
            notwig_core::RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
        )
        .expect("left extension should add silence, not reject");
    let clip = &workspace.doc.arranger.clips[&clip_id];
    assert_eq!(
        clip.range,
        notwig_core::RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap()
    );
    let ClipPayload::Audio(audio) = &clip.payload else {
        panic!("audio clip");
    };
    assert!(audio.uses_slices());
    assert_eq!(audio.slices.len(), 1);
    assert_eq!(audio.slices[0].start, Rational::new(1, 1));
    assert_eq!(audio.slices[0].source_start_sample, 0);
    assert_eq!(audio.slices[0].source_end_sample, 24_000);
}

#[test]
fn setting_the_sample_rate_rescales_without_touching_undo_history() {
    let mut workspace = Workspace::new_empty().unwrap();
    let before = workspace.doc.meta.sample_rate;
    workspace.set_sample_rate(44_100).unwrap();
    assert_eq!(workspace.doc.meta.sample_rate.value(), 44_100);
    let _ = workspace.undo();
    assert_eq!(workspace.doc.meta.sample_rate.value(), 44_100);
    assert_ne!(workspace.doc.meta.sample_rate, before);
}

#[test]
fn importing_audio_does_not_request_an_engine_rebuild() {
    let dir = temp_dir();
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Audio).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Audio)
        .unwrap()
        .id;
    workspace.save(Some(&dir.join("Import.notwig"))).unwrap();
    let requests = workspace.engine_build_requests();
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../notwig-project/tests/fixtures/tone.mp3");
    workspace
        .import_audio_at(&source, track, Rational::zero())
        .unwrap();
    assert_eq!(workspace.engine_build_requests(), requests);
    assert_eq!(workspace.doc.assets.assets.len(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn importing_audio_before_saving_stages_the_asset() {
    let dir = temp_dir();
    let mut workspace = Workspace::new_empty().unwrap();
    assert!(workspace.bundle.is_none());
    workspace.create_track(TrackKind::Audio).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Audio)
        .unwrap()
        .id;
    let source = dir.join("staged.wav");
    let samples: Vec<f32> = (0..2400).map(|i| (i as f32 * 0.02).sin()).collect();
    notwig_project::write_wav_f32(&source, 48_000, 1, &samples).unwrap();
    workspace
        .import_audio_at(&source, track, Rational::zero())
        .unwrap();
    assert_eq!(workspace.doc.assets.assets.len(), 1);
    let bundle = dir.join("Staged.notwig");
    workspace.save(Some(&bundle)).unwrap();
    let entry = workspace.doc.assets.assets.values().next().unwrap();
    assert!(bundle.join(&entry.uri).exists());
    let reopened = Workspace::open(&bundle).unwrap();
    assert_eq!(reopened.assets.len(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn changing_sample_rate_rescales_assets_and_clip_windows() {
    let dir = temp_dir();
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Audio).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Audio)
        .unwrap()
        .id;
    let source = dir.join("rescale.wav");
    let samples: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.02).sin()).collect();
    notwig_project::write_wav_f32(&source, 48_000, 1, &samples).unwrap();
    workspace
        .import_audio_at(&source, track, Rational::zero())
        .unwrap();
    let original_end = workspace
        .doc
        .arranger
        .clips
        .values()
        .find_map(|clip| match &clip.payload {
            ClipPayload::Audio(audio) => Some(audio.source_end_sample),
            ClipPayload::Midi(_) => None,
        })
        .unwrap();
    workspace.set_sample_rate(24_000).unwrap();
    let rescaled_end = workspace
        .doc
        .arranger
        .clips
        .values()
        .find_map(|clip| match &clip.payload {
            ClipPayload::Audio(audio) => Some(audio.source_end_sample),
            ClipPayload::Midi(_) => None,
        })
        .unwrap();
    assert_eq!(rescaled_end, original_end / 2);
    assert_eq!(
        workspace
            .doc
            .assets
            .assets
            .values()
            .next()
            .unwrap()
            .sample_rate,
        Some(24_000)
    );
    assert!(
        workspace
            .assets
            .values()
            .all(|buffer| buffer.sample_rate == 24_000)
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn render_range_supports_aiff_and_normalization() {
    let dir = temp_dir();
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Audio).unwrap();
    let path = dir.join("mix.aiff");
    let frames = workspace
        .render_range_to(
            &path,
            Rational::zero(),
            Rational::new(1, 1),
            notwig_project::ExportOptions {
                format: notwig_project::ExportFormat::Aiff,
                bit_depth: notwig_project::ExportBitDepth::Int24,
                dither: false,
                normalize: true,
            },
        )
        .unwrap();
    assert!(frames > 0);
    let decoded = notwig_project::decode_audio(&path).unwrap();
    assert_eq!(decoded.sample_rate, workspace.doc.meta.sample_rate.value());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn render_stems_writes_one_file_per_track() {
    let dir = temp_dir();
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Audio).unwrap();
    workspace.create_track(TrackKind::Instrument).unwrap();
    let stems = workspace
        .render_stems_to(
            &dir,
            Rational::zero(),
            Rational::new(1, 1),
            notwig_project::ExportOptions::default(),
        )
        .unwrap();
    let non_master = workspace
        .doc
        .tracks
        .values()
        .filter(|track| track.kind != TrackKind::Master)
        .count();
    assert_eq!(stems.len(), non_master);
    assert!(
        stems
            .iter()
            .all(|(path, frames)| path.exists() && *frames > 0)
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn plugin_state_round_trips_through_the_bundle() {
    let dir = temp_dir();
    let mut workspace = Workspace::new_empty().unwrap();
    let master = workspace.doc.master_track().unwrap();
    let chain = workspace.doc.tracks.get(&master).unwrap().main_chain;
    workspace
        .insert_stock(
            chain,
            notwig_dsp::stock::TOOL_TYPE_ID,
            DeviceKind::AudioFx,
            None,
        )
        .unwrap();
    let device_id = *workspace
        .doc
        .devices
        .iter()
        .find(|(_, device)| device.type_id == notwig_dsp::stock::TOOL_TYPE_ID)
        .map(|(id, _)| id)
        .unwrap();
    let state = notwig_plugins::PluginState {
        format: notwig_plugins::PluginFormat::Vst3,
        plugin_uid: "test-plugin".into(),
        plugin_version: "1.0".into(),
        blob: vec![1, 2, 3, 4],
    };
    workspace.set_plugin_state(device_id, state.clone());
    let bundle = dir.join("State.notwig");
    workspace.save(Some(&bundle)).unwrap();
    assert!(
        bundle
            .join("state")
            .join(format!("{device_id}.json"))
            .exists()
    );
    let reopened = Workspace::open(&bundle).unwrap();
    assert_eq!(reopened.plugin_state(device_id), Some(&state));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn staged_audio_import_creates_the_clip_before_analysis_finishes() {
    let dir = temp_dir();
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Audio).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Audio)
        .unwrap()
        .id;
    let source = dir.join("staged-async.wav");
    let samples: Vec<f32> = (0..24_000).map(|i| (i as f32 * 0.02).sin()).collect();
    notwig_project::write_wav_f32(&source, 48_000, 1, &samples).unwrap();
    let (asset_id, target) = workspace
        .stage_audio_clip_import(&source, track, Rational::zero())
        .unwrap();
    assert_eq!(workspace.doc.arranger.clips.len(), 1);
    assert!(workspace.waveforms.is_empty());
    assert!(workspace.save(Some(&dir.join("Pending.notwig"))).is_err());
    let imported = notwig_project::import_asset(&source, &target, asset_id, 48_000).unwrap();
    workspace.complete_audio_import(asset_id, imported).unwrap();
    assert!(!workspace.waveforms.is_empty());
    assert!(workspace.save(Some(&dir.join("Done.notwig"))).is_ok());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn join_clips_merges_adjacent_midi_clips_and_preserves_notes() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Instrument).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Instrument)
        .unwrap()
        .id;
    workspace
        .create_midi_clip(
            track,
            notwig_core::RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
        )
        .unwrap();
    let first = *workspace.doc.arranger.clips.keys().next().unwrap();
    workspace
        .create_midi_clip(
            track,
            notwig_core::RangeMus::new(Rational::new(2, 1), Rational::new(4, 1)).unwrap(),
        )
        .unwrap();
    let second = *workspace
        .doc
        .arranger
        .clips
        .keys()
        .find(|id| **id != first)
        .unwrap();
    for (clip, start, pitch) in [
        (first, Rational::new(1, 2), 60),
        (second, Rational::new(1, 2), 64),
    ] {
        workspace
            .create_note(
                clip,
                NoteEntity {
                    id: Id::<NoteMarker>::from_uuid(Uuid::new_v4()),
                    pitch,
                    start,
                    length: Rational::new(1, 2),
                    velocity: Norm01::clamped(0.8),
                    release_velocity: None,
                    expression: std::collections::BTreeMap::default(),
                },
            )
            .unwrap();
    }
    workspace.join_clips(&[first, second]).unwrap();
    assert_eq!(workspace.doc.arranger.clips.len(), 1);
    let clip = workspace.doc.arranger.clips.values().next().unwrap();
    assert_eq!(clip.range.start, Rational::zero());
    assert_eq!(clip.range.end, Rational::new(4, 1));
    let ClipPayload::Midi(midi) = &clip.payload else {
        panic!("joined clip should be MIDI");
    };
    let mut starts: Vec<_> = midi.notes.iter().map(|note| note.start).collect();
    starts.sort();
    assert_eq!(starts, vec![Rational::new(1, 2), Rational::new(5, 2)]);
}

#[test]
fn extending_a_midi_clip_left_keeps_notes_at_their_absolute_time() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Instrument).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Instrument)
        .unwrap()
        .id;
    workspace
        .create_midi_clip(
            track,
            notwig_core::RangeMus::new(Rational::new(2, 1), Rational::new(6, 1)).unwrap(),
        )
        .unwrap();
    let clip_id = *workspace.doc.arranger.clips.keys().next().unwrap();
    workspace
        .create_note(
            clip_id,
            NoteEntity {
                id: Id::<NoteMarker>::from_uuid(Uuid::new_v4()),
                pitch: 60,
                start: Rational::new(1, 1),
                length: Rational::new(1, 1),
                velocity: Norm01::clamped(0.8),
                release_velocity: None,
                expression: std::collections::BTreeMap::default(),
            },
        )
        .unwrap();
    workspace
        .trim_clip(
            clip_id,
            notwig_core::RangeMus::new(Rational::new(1, 1), Rational::new(6, 1)).unwrap(),
        )
        .unwrap();
    let clip = &workspace.doc.arranger.clips[&clip_id];
    assert_eq!(clip.range.start, Rational::new(1, 1));
    let ClipPayload::Midi(midi) = &clip.payload else {
        panic!("clip should be MIDI");
    };
    assert_eq!(midi.content_start, Rational::zero());
    assert_eq!(midi.notes[0].start, Rational::new(2, 1));
    let absolute = clip.range.start + midi.notes[0].start - midi.content_start;
    assert_eq!(absolute, Rational::new(3, 1));
}

#[test]
fn inserting_named_extension_instrument_renames_fresh_track() {
    let (mut workspace, track, chain) = instrument_workspace_with_extension();
    let original = workspace.doc.tracks[&track].name.clone();
    assert_ne!(original, "Phase");
    assert!(workspace.doc.tracks[&track].name_is_auto);

    workspace
        .insert_device(
            chain,
            "test.phase",
            notwig_core::SchemaVersion::V0,
            DeviceKind::Instrument,
            None,
        )
        .unwrap();

    assert_eq!(workspace.doc.tracks[&track].name, "Phase");
    assert!(workspace.doc.tracks[&track].name_is_auto);
}

#[test]
fn inserting_named_extension_instrument_preserves_manual_track_name() {
    let (mut workspace, track, chain) = instrument_workspace_with_extension();
    workspace
        .apply(CommandPayload::RenameTrack {
            id: track,
            name: "Keys".into(),
        })
        .unwrap();

    workspace
        .insert_device(
            chain,
            "test.phase",
            notwig_core::SchemaVersion::V0,
            DeviceKind::Instrument,
            None,
        )
        .unwrap();

    assert_eq!(workspace.doc.tracks[&track].name, "Keys");
    assert!(!workspace.doc.tracks[&track].name_is_auto);
}

#[test]
fn undo_and_redo_restore_instrument_name_and_device_together() {
    let (mut workspace, track, chain) = instrument_workspace_with_extension();
    let original = workspace.doc.tracks[&track].name.clone();

    workspace
        .insert_device(
            chain,
            "test.phase",
            notwig_core::SchemaVersion::V0,
            DeviceKind::Instrument,
            None,
        )
        .unwrap();
    assert_eq!(workspace.doc.tracks[&track].name, "Phase");
    assert_eq!(workspace.doc.devices.len(), 1);

    workspace.undo().unwrap();
    assert_eq!(workspace.doc.tracks[&track].name, original);
    assert!(workspace.doc.tracks[&track].name_is_auto);
    assert!(workspace.doc.devices.is_empty());

    workspace.redo().unwrap();
    assert_eq!(workspace.doc.tracks[&track].name, "Phase");
    assert!(workspace.doc.tracks[&track].name_is_auto);
    assert_eq!(workspace.doc.devices.len(), 1);
}

#[test]
fn duplicated_track_is_not_auto_named() {
    let (mut workspace, track, chain) = instrument_workspace_with_extension();
    workspace
        .insert_device(
            chain,
            "test.phase",
            notwig_core::SchemaVersion::V0,
            DeviceKind::Instrument,
            None,
        )
        .unwrap();
    assert_eq!(workspace.doc.tracks[&track].name, "Phase");

    let copy = workspace.duplicate_track(track).unwrap();
    assert_eq!(workspace.doc.tracks[&copy].name, "Phase copy");
    assert!(!workspace.doc.tracks[&copy].name_is_auto);
}
