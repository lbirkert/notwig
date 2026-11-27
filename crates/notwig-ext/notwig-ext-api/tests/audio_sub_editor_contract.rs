use notwig_core::sequencer::AudioClipData;
use notwig_core::{AssetId, ClipId, Curve, CurvePoint, Db, FadeShape, RangeMus, TimeMus, TrackId};
use notwig_ext_api::{
    AudioSubEditorContext, AudioSubEditorExtension, AudioSubEditorResponse, CommandEdit,
    CommandEffect, Extension, PanelInput, ThemeSnapshot,
};
use uuid::Uuid;

struct TestEditor;

impl AudioSubEditorExtension for TestEditor {
    fn new() -> Self {
        Self
    }

    fn draw(
        &mut self,
        ui: &mut notwig_ext_api::egui::Ui,
        context: &AudioSubEditorContext,
    ) -> AudioSubEditorResponse {
        ui.label("Test editor");
        let curve = Curve::from_points([CurvePoint::new(TimeMus::zero(), 0.25)]).ok();
        AudioSubEditorResponse::from(CommandEffect::Apply {
            edits: vec![CommandEdit::AudioCurves {
                clip_id: context.clip_id,
                gain_curve: None,
                pan_curve: Some(curve),
            }],
            ui: Vec::new(),
        })
    }
}

fn context() -> AudioSubEditorContext {
    AudioSubEditorContext {
        editor_id: "com.test.editor".into(),
        revision: 7,
        clip_id: ClipId::from_uuid(Uuid::from_u128(1)),
        track_id: TrackId::from_uuid(Uuid::from_u128(2)),
        clip_range: RangeMus::new(TimeMus::zero(), TimeMus::new(4, 1)).expect("range"),
        clip: AudioClipData {
            asset_id: AssetId::from_uuid(Uuid::from_u128(3)),
            source_start_sample: 0,
            source_end_sample: 48_000,
            gain_db: Db(0.0),
            gain_curve: None,
            pan_curve: None,
            fade_in_samples: 0,
            fade_out_samples: 0,
            fade_shape: FadeShape::Linear,
            slices: Vec::new(),
            slices_enabled: false,
        }
        .into(),
        selected_slices: Vec::new(),
        time_selection: None,
        playhead: TimeMus::zero(),
        loop_range: None,
        tempo_bpm: 120.0,
        sample_rate: 48_000,
        snap_enabled: true,
        grid: TimeMus::new(1, 4),
        bar_length: TimeMus::new(4, 1),
        beat_length: TimeMus::new(1, 1),
        viewport_start: TimeMus::zero(),
        pixels_per_beat: 80.0,
        width: 320.0,
        height: 180.0,
        waveform: notwig_ext_api::AudioWaveformPreview::default(),
        actions: notwig_ext_api::AudioSubEditorSelectionActions::default(),
        clipboard: Vec::new(),
        theme: ThemeSnapshot::default(),
        input: PanelInput {
            width: 320.0,
            height: 180.0,
            pixels_per_point: 1.0,
            max_texture_side: 4096,
            ..Default::default()
        },
    }
}

#[test]
fn registered_audio_sub_editor_draws_a_frame_with_edits() {
    let mut extension = Extension::new();
    extension.register_audio_sub_editor::<TestEditor>("com.test.renderer");
    let handle = extension.create_audio_sub_editor("com.test.renderer");
    assert!(handle > 0);
    let ctx = context();
    let encoded_clip = postcard::to_allocvec(&ctx.clip).expect("clip");
    let _: notwig_ext_api::AudioSubEditorClip =
        postcard::from_bytes(&encoded_clip).expect("decode clip");
    let encoded_input = postcard::to_allocvec(&ctx.input).expect("input");
    let _: PanelInput = postcard::from_bytes(&encoded_input).expect("decode input");
    let encoded_range = postcard::to_allocvec(&ctx.clip_range).expect("range");
    let _: RangeMus = postcard::from_bytes(&encoded_range).expect("decode range");
    let context = postcard::to_allocvec(&ctx).expect("encode context");
    let frame = extension
        .draw_audio_sub_editor_frame(handle, &context)
        .expect("draw frame");
    frame.validate().expect("valid frame");
    let CommandEffect::Apply { edits, .. } = frame.effect else {
        panic!("expected apply effect");
    };
    assert!(matches!(
        edits.as_slice(),
        [CommandEdit::AudioCurves {
            pan_curve: Some(Some(_)),
            ..
        }]
    ));
    assert_eq!(extension.create_audio_sub_editor("missing"), -1);
    extension.destroy_audio_sub_editor(handle);
}
