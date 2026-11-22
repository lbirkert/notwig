use std::collections::BTreeMap;

use notwig_core::edit;
use notwig_core::ids::{Clip, DeviceChain, Note, Track};
use notwig_core::protocol::{Command, CommandPayload};
use notwig_core::sequencer::{ClipKind, ClipPayload, MidiClipData, NoteEntity};
use notwig_core::tracks::TrackKind;
use notwig_core::*;
use notwig_ui::test_api::midi_editor::{
    MidiModel, NoteSelection, Pitch, ROW_HEIGHT, RULER_H, absolute_offset, default_note_end,
    insert_note_clearing, lower_note_plan, snapped_shift,
};
use notwig_ui::{EditFocus, MidiEditorState, NotwigUi, OpenedEditor, RuntimeView, UiIntent};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn note(n: u128, pitch: u8, start: i64, len: i64) -> NoteEntity {
    NoteEntity {
        id: id::<Note>(n),
        pitch,
        start: Rational::new(start, 1),
        length: Rational::new(len, 1),
        velocity: Norm01::clamped(0.8),
        release_velocity: None,
        expression: BTreeMap::new(),
    }
}

fn note_frac(n: u128, pitch: u8, start: (i64, u32), len: (i64, u32)) -> NoteEntity {
    NoteEntity {
        id: id::<Note>(n),
        pitch,
        start: Rational::new(start.0, start.1),
        length: Rational::new(len.0, len.1),
        velocity: Norm01::clamped(0.8),
        release_velocity: None,
        expression: BTreeMap::new(),
    }
}

fn assert_no_overlap(notes: &[NoteEntity]) {
    for (index, a) in notes.iter().enumerate() {
        for b in notes.iter().skip(index + 1) {
            if a.pitch != b.pitch {
                continue;
            }
            let a_end = a.start + a.length;
            let b_end = b.start + b.length;
            assert!(
                a_end <= b.start || b_end <= a.start,
                "notes {} and {} overlap on pitch {}",
                a.id,
                b.id,
                a.pitch
            );
        }
    }
}

fn span(start: i64, end: i64) -> RangeMus {
    RangeMus::new(Rational::new(start, 1), Rational::new(end, 1)).unwrap()
}

fn midi_with_notes(notes: Vec<NoteEntity>) -> MidiClipData {
    MidiClipData {
        length: Rational::new(8, 1),
        content_start: Rational::zero(),
        notes,
    }
}

fn midi_doc(notes: Vec<NoteEntity>) -> (ProjectDoc, ClipId) {
    midi_doc_range(notes, 0, 8)
}

fn midi_doc_range(notes: Vec<NoteEntity>, start: i64, end: i64) -> (ProjectDoc, ClipId) {
    let meta = Meta {
        name: "Piano".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    let mut doc = ProjectDoc::new_template(meta, id::<Track>(1), id::<DeviceChain>(100)).unwrap();
    let track = id::<Track>(2);
    doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: track,
            chain_id: id::<DeviceChain>(102),
            kind: TrackKind::Instrument,
            name: "Keys".into(),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc;
    let clip = id::<Clip>(20);
    doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateClip {
            id: clip,
            track_id: track,
            kind: ClipKind::Midi,
            range: RangeMus::new(Rational::new(start, 1), Rational::new(end, 1)).unwrap(),
            payload: ClipPayload::Midi(MidiClipData {
                length: Rational::new(8, 1),
                content_start: Rational::zero(),
                notes,
            }),
        }),
    )
    .unwrap()
    .doc;
    (doc, clip)
}

fn run_ui(
    ctx: &egui::Context,
    ui: &mut NotwigUi,
    doc: &ProjectDoc,
    runtime: &RuntimeView,
    events: Vec<egui::Event>,
) -> Vec<UiIntent> {
    run_ui_at(ctx, ui, doc, runtime, events, None)
}

fn run_ui_at(
    ctx: &egui::Context,
    ui: &mut NotwigUi,
    doc: &ProjectDoc,
    runtime: &RuntimeView,
    events: Vec<egui::Event>,
    time: Option<f64>,
) -> Vec<UiIntent> {
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 900.0));
    let modifiers = events
        .iter()
        .find_map(|event| match event {
            egui::Event::Key { modifiers, .. } => Some(*modifiers),
            _ => None,
        })
        .unwrap_or_default();
    let mut intents = Vec::new();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            time,
            modifiers,
            events,
            ..Default::default()
        },
        |ctx| {
            intents = ui.show(ctx, doc, runtime);
        },
    );
    intents
}

fn run_ui_with_output(
    ctx: &egui::Context,
    ui: &mut NotwigUi,
    doc: &ProjectDoc,
    runtime: &RuntimeView,
    events: Vec<egui::Event>,
) -> (Vec<UiIntent>, egui::FullOutput) {
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 900.0));
    let mut intents = Vec::new();
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            events,
            ..Default::default()
        },
        |ctx| {
            intents = ui.show(ctx, doc, runtime);
        },
    );
    (intents, output)
}

fn assert_no_black_editor_frame(
    output: &egui::FullOutput,
    content: egui::Rect,
    screen: egui::Rect,
) {
    let region = egui::Rect::from_min_max(
        egui::pos2(screen.left(), content.top() + 1.0),
        egui::pos2(screen.right(), screen.bottom()),
    );
    for clipped in &output.shapes {
        if let egui::Shape::Rect(shape) = &clipped.shape
            && shape.fill == egui::Color32::BLACK
            && shape.rect.width() > 150.0
            && shape.rect.height() > 60.0
            && shape.rect.intersects(region)
        {
            panic!("editor is surrounded by a black frame: {:?}", shape.rect);
        }
    }
}

fn editor_scrollbar_strip(output: &egui::FullOutput, content: egui::Rect) -> egui::Rect {
    let panel_light = egui::Color32::from_rgb(31, 33, 39);
    for clipped in &output.shapes {
        if let egui::Shape::Rect(shape) = &clipped.shape {
            let rect = shape.rect;
            if shape.fill == panel_light
                && rect.width() > 300.0
                && (rect.top() - content.bottom()).abs() < 6.0
                && rect.height() > 0.0
                && rect.height() < 30.0
            {
                return rect;
            }
        }
    }
    panic!("editor scrollbar strip not found");
}

fn double_click(
    ctx: &egui::Context,
    ui: &mut NotwigUi,
    doc: &ProjectDoc,
    runtime: &RuntimeView,
    pos: egui::Pos2,
) -> Vec<UiIntent> {
    let _ = run_ui_at(
        ctx,
        ui,
        doc,
        runtime,
        vec![egui::Event::PointerMoved(pos)],
        Some(0.0),
    );
    let _ = run_ui_at(
        ctx,
        ui,
        doc,
        runtime,
        vec![pointer_event(pos, true)],
        Some(0.1),
    );
    let _ = run_ui_at(
        ctx,
        ui,
        doc,
        runtime,
        vec![pointer_event(pos, false)],
        Some(0.11),
    );
    let _ = run_ui_at(
        ctx,
        ui,
        doc,
        runtime,
        vec![pointer_event(pos, true)],
        Some(0.2),
    );
    run_ui_at(
        ctx,
        ui,
        doc,
        runtime,
        vec![pointer_event(pos, false)],
        Some(0.21),
    )
}

fn pointer_event(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::default(),
    }
}

fn open(ui: &mut NotwigUi, doc: &ProjectDoc, runtime: &RuntimeView, clip: ClipId) -> egui::Context {
    ui.open_editor_for_clip(doc, clip);
    let ctx = egui::Context::default();
    let _ = run_ui(&ctx, ui, doc, runtime, Vec::new());
    ctx
}

fn note_y(ui: &NotwigUi, content: egui::Rect, pitch: u8) -> f32 {
    let scroll = ui.midi_editor.as_ref().unwrap().pitch_scroll;
    content.top() + RULER_H + (127.0 - f32::from(pitch) - scroll) * ROW_HEIGHT + ROW_HEIGHT * 0.5
}

fn note_x(ui: &NotwigUi, content: egui::Rect, beat: f64) -> f32 {
    let state = ui.midi_editor.as_ref().unwrap();
    content.left()
        + ((beat - state.viewport.start.as_f64()) as f32) * state.viewport.pixels_per_beat
}

#[test]
fn opening_a_midi_clip_uses_the_midi_editor() {
    let (doc, clip) = midi_doc(vec![note(30, 60, 1, 1)]);
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = open(&mut ui, &doc, &runtime, clip);
    assert_eq!(ui.currently_opened, Some(OpenedEditor::MidiEditor));
    assert_eq!(ui.edit_focus, EditFocus::MidiEditor);
    assert!(ui.midi_editor.is_some());
    assert!(ui.midi_editor_content_rect.width() > 0.0);
    let _ = run_ui(&ctx, &mut ui, &doc, &runtime, Vec::new());
    assert!(ui.midi_editor.is_some());
}

#[test]
fn midi_editor_layout_has_no_black_frame_and_flush_scrollbar() {
    let (doc, clip) = midi_doc(vec![note(30, 60, 1, 1)]);
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = open(&mut ui, &doc, &runtime, clip);
    let (_, output) = run_ui_with_output(&ctx, &mut ui, &doc, &runtime, Vec::new());

    let content = ui.midi_editor_content_rect;
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 900.0));
    assert_no_black_editor_frame(&output, content, screen);

    let bar = editor_scrollbar_strip(&output, content);
    assert!(
        (bar.top() - content.bottom()).abs() < 0.5,
        "scrollbar should sit flush under the timeline: bar {bar:?}, content {content:?}"
    );
    assert!(bar.bottom() > bar.top());
    assert!(bar.bottom() <= screen.bottom() + 0.5);
}

#[test]
fn double_clicking_an_empty_row_creates_a_note() {
    let (doc, clip) = midi_doc(vec![note(30, 60, 1, 1)]);
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = open(&mut ui, &doc, &runtime, clip);
    let content = ui.midi_editor_content_rect;
    let pos = egui::pos2(note_x(&ui, content, 3.0), note_y(&ui, content, 64));
    let intents = double_click(&ctx, &mut ui, &doc, &runtime, pos);
    let created = intents
        .iter()
        .find_map(|intent| match intent {
            UiIntent::CreateMidiNote { note, .. } => Some(note.clone()),
            _ => None,
        })
        .expect("double click should create a note");
    assert_eq!(created.pitch, 64);
    assert_eq!(created.start, Rational::new(3, 1));
    assert!(created.length > Rational::zero());
    assert!(created.start + created.length <= Rational::new(8, 1));
    assert!(
        ui.midi_editor
            .as_ref()
            .is_some_and(|state| state.selected_contains(created.id))
    );
}

#[test]
fn creating_a_note_sets_the_local_time_selection_to_its_span() {
    let (doc, clip) = midi_doc(vec![note(30, 60, 1, 1)]);
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = open(&mut ui, &doc, &runtime, clip);
    let content = ui.midi_editor_content_rect;
    let pos = egui::pos2(note_x(&ui, content, 3.0), note_y(&ui, content, 64));
    let intents = double_click(&ctx, &mut ui, &doc, &runtime, pos);
    let created = intents
        .iter()
        .find_map(|intent| match intent {
            UiIntent::CreateMidiNote { note, .. } => Some(note.clone()),
            _ => None,
        })
        .expect("double click should create a note");

    let state = ui.midi_editor.as_ref().expect("MIDI editor open");
    let expected =
        RangeMus::new(created.start, created.start + created.length).expect("created note span");
    assert_eq!(
        state.time_selection(),
        Some(expected),
        "creating a note should update the local time selection to its span"
    );
}

#[test]
fn creating_a_note_updates_the_global_time_selection() {
    let (doc, clip) = midi_doc(vec![note(30, 60, 1, 1)]);
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = open(&mut ui, &doc, &runtime, clip);
    let content = ui.midi_editor_content_rect;
    let pos = egui::pos2(note_x(&ui, content, 3.0), note_y(&ui, content, 64));
    let intents = double_click(&ctx, &mut ui, &doc, &runtime, pos);
    let created = intents
        .iter()
        .find_map(|intent| match intent {
            UiIntent::CreateMidiNote { note, .. } => Some(note.clone()),
            _ => None,
        })
        .expect("double click should create a note");

    let doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateNote {
            clip_id: clip,
            note: created.clone(),
        }),
    )
    .expect("create note command")
    .doc;
    let _ = run_ui(&ctx, &mut ui, &doc, &runtime, Vec::new());

    let time = ui
        .state
        .selection
        .time
        .as_ref()
        .expect("creating a note should set the global time selection");
    let expected =
        RangeMus::new(created.start, created.start + created.length).expect("created note span");
    assert_eq!(time.range, expected);
    let track = doc.arranger.clips[&clip].track_id;
    assert_eq!(time.tracks.len(), 1);
    assert!(time.contains_track(track));
}

#[test]
fn single_click_on_empty_lane_sets_zero_width_time_selection() {
    let (doc, clip) = midi_doc(vec![note(30, 60, 1, 1)]);
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = open(&mut ui, &doc, &runtime, clip);
    let content = ui.midi_editor_content_rect;
    let pos = egui::pos2(note_x(&ui, content, 3.0), note_y(&ui, content, 64));
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![egui::Event::PointerMoved(pos)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(pos, true)],
    );
    let intents = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(pos, false)],
    );
    assert!(
        !intents
            .iter()
            .any(|intent| matches!(intent, UiIntent::CreateMidiNote { .. })),
        "single click must not create a note"
    );
    let state = ui.midi_editor.as_ref().expect("MIDI editor open");
    assert!(state.selected_is_empty());
    assert_eq!(
        state.time_selection(),
        Some(span(3, 3)),
        "single click should set a zero-width time selection"
    );
}

#[test]
fn double_clicking_an_existing_note_deletes_it() {
    let (doc, clip) = midi_doc(vec![note(30, 60, 1, 1)]);
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = open(&mut ui, &doc, &runtime, clip);
    let content = ui.midi_editor_content_rect;
    let pos = egui::pos2(note_x(&ui, content, 1.5), note_y(&ui, content, 60));
    let intents = double_click(&ctx, &mut ui, &doc, &runtime, pos);
    let removed = intents
        .iter()
        .find_map(|intent| match intent {
            UiIntent::EditMidiNotes { removed, .. } => Some(removed.clone()),
            _ => None,
        })
        .expect("double click should delete the note");
    assert_eq!(removed, vec![id::<Note>(30)]);
    assert!(
        !intents
            .iter()
            .any(|intent| matches!(intent, UiIntent::CreateMidiNote { .. })),
        "double clicking a note must not create another note"
    );
}

#[test]
fn double_clicking_empty_space_avoids_ending_inside_an_existing_note() {
    let existing = note_frac(30, 60, (31, 10), (1, 1));
    let (doc, clip) = midi_doc(vec![existing.clone()]);
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = open(&mut ui, &doc, &runtime, clip);
    let content = ui.midi_editor_content_rect;
    let pos = egui::pos2(note_x(&ui, content, 3.0), note_y(&ui, content, 60));
    let intents = double_click(&ctx, &mut ui, &doc, &runtime, pos);
    let created = intents
        .iter()
        .find_map(|intent| match intent {
            UiIntent::CreateMidiNote { note, .. } => Some(note.clone()),
            _ => None,
        })
        .expect("double click should create a note");
    assert_eq!(created.pitch, 60);
    assert_eq!(created.start, Rational::new(3, 1));
    assert!(
        created.start + created.length >= existing.start + existing.length,
        "created note should not end inside the existing note"
    );
}

#[test]
fn clicking_a_piano_key_previews_the_note() {
    let (doc, clip) = midi_doc(vec![note(30, 60, 1, 1)]);
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = open(&mut ui, &doc, &runtime, clip);
    let content = ui.midi_editor_content_rect;
    let pos = egui::pos2(content.left() - 20.0, note_y(&ui, content, 60));
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![egui::Event::PointerMoved(pos)],
    );
    let press_intents = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(pos, true)],
    );
    let release_intents = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(pos, false)],
    );
    let preview = press_intents
        .iter()
        .find_map(|intent| match intent {
            UiIntent::PreviewNote {
                track_id,
                pitch,
                velocity,
                length_mus,
                ..
            } => Some((*track_id, *pitch, *velocity, *length_mus)),
            _ => None,
        })
        .expect("pressing a key should preview its pitch");
    assert_eq!(preview.0, doc.arranger.clips[&clip].track_id);
    assert_eq!(preview.1, 60);
    assert!((preview.2 - 0.8).abs() < f32::EPSILON);
    assert_eq!(preview.3, None, "a held key should stay on until release");
    assert!(
        release_intents
            .iter()
            .any(|intent| matches!(intent, UiIntent::ReleasePreviewNote { .. })),
        "releasing the key should emit a preview note-off"
    );
}

#[test]
fn pitch_rows_round_trip_at_row_centers() {
    for scroll in [0.0_f32, 0.5, 36.0, 42.25] {
        for pitch in [0_u8, 48, 60, 64, 127] {
            let y = notwig_ui::test_api::midi_editor::y_for_pitch(pitch, 40.0, scroll)
                + ROW_HEIGHT * 0.5;
            assert_eq!(
                notwig_ui::test_api::midi_editor::pitch_for_y(y, 40.0, scroll),
                pitch,
                "pitch {pitch} at scroll {scroll} should round-trip at a row center"
            );
        }
    }
}

#[test]
fn dragging_in_empty_space_selects_notes_with_a_marquee() {
    let (doc, clip) = midi_doc(vec![note(30, 60, 1, 1)]);
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = open(&mut ui, &doc, &runtime, clip);
    let content = ui.midi_editor_content_rect;
    let origin = egui::pos2(note_x(&ui, content, 0.5), note_y(&ui, content, 67));
    let end = egui::pos2(note_x(&ui, content, 3.0), note_y(&ui, content, 55));
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![egui::Event::PointerMoved(origin)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(origin, true)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![egui::Event::PointerMoved(end)],
    );
    let intents = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(end, false)],
    );
    assert!(
        !intents
            .iter()
            .any(|intent| matches!(intent, UiIntent::CreateMidiNote { .. })),
        "marquee drag must not create a note"
    );
    let state = ui.midi_editor.as_ref().expect("MIDI editor open");
    assert_eq!(state.selected_ids(), vec![id::<Note>(30)]);
    assert_eq!(
        state.time_selection(),
        Some(RangeMus::new(Rational::new(1, 2), Rational::new(3, 1)).expect("marquee range"))
    );
}

#[test]
fn marquee_time_selection_expands_to_cover_selected_notes() {
    let (doc, clip) = midi_doc(vec![note(30, 60, 1, 1)]);
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = open(&mut ui, &doc, &runtime, clip);
    let content = ui.midi_editor_content_rect;
    let origin = egui::pos2(note_x(&ui, content, 1.5), note_y(&ui, content, 60));
    let end = egui::pos2(note_x(&ui, content, 1.6), note_y(&ui, content, 60));
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![egui::Event::PointerMoved(origin)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(origin, true)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![egui::Event::PointerMoved(end)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(end, false)],
    );
    let state = ui.midi_editor.as_ref().expect("MIDI editor open");
    assert_eq!(state.selected_ids(), vec![id::<Note>(30)]);
    assert_eq!(state.time_selection(), Some(span(1, 2)));
}

#[test]
fn deleting_a_selected_note_emits_a_removal() {
    let note_id = id::<Note>(30);
    let (doc, clip) = midi_doc(vec![note(30, 60, 1, 1)]);
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = open(&mut ui, &doc, &runtime, clip);
    let content = ui.midi_editor_content_rect;
    let pos = egui::pos2(note_x(&ui, content, 1.5), note_y(&ui, content, 60));
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![egui::Event::PointerMoved(pos)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(pos, true)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(pos, false)],
    );
    assert!(
        ui.midi_editor
            .as_ref()
            .is_some_and(|state| state.selected_contains(note_id)),
        "click should select the note"
    );
    let intents = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![egui::Event::Key {
            key: egui::Key::Delete,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        }],
    );
    let removed = intents
        .iter()
        .find_map(|intent| match intent {
            UiIntent::EditMidiNotes { removed, .. } => Some(removed.clone()),
            _ => None,
        })
        .expect("delete should emit a note edit");
    assert_eq!(removed, vec![note_id]);
}

#[test]
fn note_move_snaps_to_the_absolute_grid_with_nonzero_content_start() {
    let clip_start = TimeMus::new(3, 10);
    let content_start = TimeMus::new(1, 10);
    let offset = absolute_offset(clip_start, content_start);
    let anchor = TimeMus::new(1, 1);
    let shift = snapped_shift(anchor, offset, 0.68, TimeMus::new(1, 4), false);
    assert_eq!(anchor + shift, TimeMus::new(9, 5));
    assert_eq!(offset + anchor + shift, TimeMus::new(2, 1));
}

#[test]
fn note_resize_start_snaps_to_the_absolute_grid_with_nonzero_content_start() {
    let offset = absolute_offset(TimeMus::new(3, 10), TimeMus::new(1, 10));
    let anchor = TimeMus::new(1, 1);
    let shift = snapped_shift(anchor, offset, 0.68, TimeMus::new(1, 4), false);
    assert_eq!(offset + anchor + shift, TimeMus::new(2, 1));
}

#[test]
fn note_resize_end_snaps_to_the_absolute_grid_with_nonzero_content_start() {
    let offset = absolute_offset(TimeMus::new(3, 10), TimeMus::new(1, 10));
    let anchor = TimeMus::new(3, 1);
    let shift = snapped_shift(anchor, offset, 0.68, TimeMus::new(1, 4), false);
    assert_eq!(offset + anchor + shift, TimeMus::new(4, 1));
}

#[test]
fn dragging_a_note_moves_time_and_pitch() {
    let (doc, clip) = midi_doc(vec![note(30, 60, 1, 1)]);
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = open(&mut ui, &doc, &runtime, clip);
    let content = ui.midi_editor_content_rect;
    let ppb = ui.midi_editor.as_ref().unwrap().viewport.pixels_per_beat;
    let origin = egui::pos2(note_x(&ui, content, 1.25), note_y(&ui, content, 60));
    let end = egui::pos2(origin.x + ppb, origin.y - ROW_HEIGHT * 2.0);
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![egui::Event::PointerMoved(origin)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(origin, true)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![egui::Event::PointerMoved(end)],
    );
    let intents = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(end, false)],
    );
    let changed = intents
        .iter()
        .find_map(|intent| match intent {
            UiIntent::EditMidiNotes { notes, .. } => Some(notes.clone()),
            _ => None,
        })
        .expect("drag should emit a note edit");
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].start, Rational::new(2, 1));
    assert_eq!(changed[0].pitch, 62);
    assert_eq!(changed[0].length, Rational::new(1, 1));
}

#[test]
fn dragging_the_velocity_lane_updates_velocity() {
    let note_id = id::<Note>(30);
    let (doc, clip) = midi_doc(vec![note(30, 60, 1, 1)]);
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = open(&mut ui, &doc, &runtime, clip);
    let content = ui.midi_editor_content_rect;
    let x = note_x(&ui, content, 1.0) + 2.0;
    let top = content.bottom() - 62.0 + 6.0;
    let bottom = content.bottom() - 8.0;
    let origin = egui::pos2(x, top);
    let end = egui::pos2(x, bottom);
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![egui::Event::PointerMoved(origin)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(origin, true)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![egui::Event::PointerMoved(end)],
    );
    let intents = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(end, false)],
    );
    let changed = intents
        .iter()
        .find_map(|intent| match intent {
            UiIntent::EditMidiNotes { notes, .. } => Some(notes.clone()),
            _ => None,
        })
        .expect("velocity drag should emit a note edit");
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].id, note_id);
    assert!(changed[0].velocity.0 < 0.5);
}

#[test]
fn copy_and_paste_places_a_note_at_the_playhead() {
    let (doc, clip) = midi_doc(vec![note(30, 60, 1, 1)]);
    let mut runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = open(&mut ui, &doc, &runtime, clip);
    let content = ui.midi_editor_content_rect;
    let pos = egui::pos2(note_x(&ui, content, 1.5), note_y(&ui, content, 60));
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![egui::Event::PointerMoved(pos)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(pos, true)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(pos, false)],
    );
    let _ = run_ui(&ctx, &mut ui, &doc, &runtime, vec![egui::Event::Copy]);
    ui.midi_editor.as_mut().unwrap().clear_selection();
    runtime.playhead = Rational::new(4, 1);
    let intents = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![egui::Event::Paste(String::new())],
    );
    let changed = intents
        .iter()
        .find_map(|intent| match intent {
            UiIntent::EditMidiNotes { notes, .. } => Some(notes.clone()),
            _ => None,
        })
        .expect("paste should emit a note edit");
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].start, Rational::new(4, 1));
    assert_eq!(changed[0].pitch, 60);
    assert_ne!(changed[0].id, id::<Note>(30));
}

#[test]
fn lowering_paste_keeps_offsets_and_assigns_new_ids() {
    let (doc, clip) = midi_doc(vec![note(30, 60, 1, 1)]);
    let midi = match &doc.arranger.clips[&clip].payload {
        ClipPayload::Midi(midi) => midi,
        ClipPayload::Audio(_) => panic!("expected MIDI"),
    };
    let model = MidiModel::new(midi);
    let selection = notwig_core::edit::Selection::objects([id::<Note>(30)]);
    let extract = edit::copy(&model, &selection).unwrap();
    let plan = edit::paste(&model, &extract, Rational::new(4, 1)).unwrap();
    let lowered = lower_note_plan(midi.length, &midi.notes, plan);
    assert_eq!(lowered.notes.len(), 2);
    let pasted = lowered
        .notes
        .iter()
        .find(|note| note.start == Rational::new(4, 1))
        .expect("pasted note");
    assert_ne!(pasted.id, id::<Note>(30));
    assert_eq!(pasted.pitch, 60);
    assert_eq!(pasted.length, Rational::new(1, 1));
}

#[test]
fn selecting_notes_derives_the_time_selection() {
    let notes = vec![note(1, 60, 1, 2), note(2, 62, 4, 1)];
    let mut state = MidiEditorState::new(id::<Clip>(20));
    state.sync_notes(&notes);

    state.select_note(id::<Note>(1), false);
    assert_eq!(state.time_selection(), Some(span(1, 3)));

    state.select_note(id::<Note>(2), true);
    assert_eq!(state.time_selection(), Some(span(1, 5)));

    state.select_note(id::<Note>(1), true);
    assert_eq!(state.time_selection(), Some(span(4, 5)));

    state.select_note(id::<Note>(2), true);
    assert_eq!(state.time_selection(), None);
    assert!(state.selected_is_empty());
}

#[test]
fn time_only_selection_copies_and_deletes_without_objects() {
    let notes = vec![note(1, 60, 1, 2), note(2, 62, 3, 1)];
    let midi = midi_with_notes(notes.clone());
    let model = MidiModel::new(&midi);
    let mut state = MidiEditorState::new(id::<Clip>(20));
    state.sync_notes(&notes);
    state.select_time(span(1, 3));

    let selection = state.selection();
    assert!(selection.objects.is_empty());
    assert_eq!(
        selection.mode(),
        Some(notwig_core::edit::SelectionMode::Time)
    );

    let extract = edit::copy(&model, &selection).unwrap();
    assert_eq!(extract.mode, notwig_core::edit::SelectionMode::Time);
    assert_eq!(extract.fragments.len(), 1);

    let plan = edit::delete(&model, &selection).unwrap();
    let lowered = lower_note_plan(midi.length, &midi.notes, plan);
    assert_eq!(lowered.notes.len(), 1);
    assert_eq!(lowered.notes[0].id, id::<Note>(2));
    match lowered.selection {
        NoteSelection::Time(range) => assert_eq!(range, span(1, 3)),
        _ => panic!("delete should keep the time selection"),
    }
}

#[test]
fn marquee_selection_sets_objects_and_the_marquee_range() {
    let notes = vec![note(1, 60, 1, 2), note(2, 62, 4, 1)];
    let mut state = MidiEditorState::new(id::<Clip>(20));
    state.sync_notes(&notes);
    let marquee = span(2, 5);
    state.select_marquee([id::<Note>(1)], marquee);

    assert_eq!(state.selected_ids(), vec![id::<Note>(1)]);
    assert_eq!(state.time_selection(), Some(marquee));
    assert_eq!(
        state.selection().mode(),
        Some(notwig_core::edit::SelectionMode::Objects)
    );
}

#[test]
fn moving_a_note_onto_a_stationary_note_splits_it() {
    let notes = vec![note(1, 60, 5, 1), note(2, 60, 1, 3)];
    let midi = midi_with_notes(notes.clone());
    let model = MidiModel::new(&midi);
    let moves = vec![edit::ObjectMove {
        object: id::<Note>(1),
        lane: Pitch(60),
        span: span(2, 3),
    }];
    let plan = edit::relocate_objects(&model, &moves).unwrap();
    let lowered = lower_note_plan(midi.length, &midi.notes, plan);

    assert_eq!(lowered.notes.len(), 3);
    let relocated = lowered
        .notes
        .iter()
        .find(|note| note.id == id::<Note>(1))
        .expect("moved note survives");
    assert_eq!(relocated.start, Rational::new(2, 1));
    assert_eq!(relocated.length, Rational::new(1, 1));

    let left = lowered
        .notes
        .iter()
        .find(|note| note.id == id::<Note>(2))
        .expect("stationary note keeps its left part");
    assert_eq!(left.start, Rational::new(1, 1));
    assert_eq!(left.length, Rational::new(1, 1));

    let right = lowered
        .notes
        .iter()
        .find(|note| note.start == Rational::new(3, 1))
        .expect("stationary note keeps its right part");
    assert_eq!(right.length, Rational::new(1, 1));
    assert_ne!(right.id, id::<Note>(2));
    assert_no_overlap(&lowered.notes);
}

#[test]
fn resizing_a_note_over_another_trims_the_stationary_note() {
    let notes = vec![note(1, 60, 1, 1), note(2, 60, 3, 2)];
    let midi = midi_with_notes(notes.clone());
    let model = MidiModel::new(&midi);
    let plan =
        edit::trim_object(&model, id::<Note>(1), edit::Edge::End, Rational::new(4, 1)).unwrap();
    let lowered = lower_note_plan(midi.length, &midi.notes, plan);

    assert_eq!(lowered.notes.len(), 2);
    let resized = lowered
        .notes
        .iter()
        .find(|note| note.id == id::<Note>(1))
        .expect("resized note");
    assert_eq!(resized.start, Rational::new(1, 1));
    assert_eq!(resized.length, Rational::new(3, 1));

    let trimmed = lowered
        .notes
        .iter()
        .find(|note| note.id == id::<Note>(2))
        .expect("stationary note");
    assert_eq!(trimmed.start, Rational::new(4, 1));
    assert_eq!(trimmed.length, Rational::new(1, 1));
    assert_no_overlap(&lowered.notes);
}

#[test]
fn creating_a_note_clears_its_destination_span() {
    let notes = vec![note(1, 60, 1, 3)];
    let created = note(2, 60, 2, 1);
    let updated = insert_note_clearing(&notes, created.clone());

    assert_eq!(updated.len(), 3);
    let inserted = updated
        .iter()
        .find(|note| note.id == created.id)
        .expect("inserted note");
    assert_eq!(inserted.start, Rational::new(2, 1));
    assert_eq!(inserted.length, Rational::new(1, 1));

    let left = updated
        .iter()
        .find(|note| note.id == id::<Note>(1))
        .expect("left fragment");
    assert_eq!(left.start, Rational::new(1, 1));
    assert_eq!(left.length, Rational::new(1, 1));

    let right = updated
        .iter()
        .find(|note| note.start == Rational::new(3, 1))
        .expect("right fragment");
    assert_eq!(right.length, Rational::new(1, 1));
    assert_no_overlap(&updated);
}

#[test]
fn default_note_end_extends_past_notes_it_would_land_inside() {
    let notes = vec![
        note(1, 60, 1, 1),
        note_frac(2, 60, (3, 2), (3, 2)),
        note_frac(3, 60, (5, 2), (3, 2)),
    ];
    let end = default_note_end(
        Rational::new(1, 1),
        Rational::new(1, 1),
        60,
        &notes,
        Rational::new(8, 1),
    );
    assert_eq!(end, Rational::new(4, 1));

    let abutting = vec![note(1, 60, 2, 2)];
    let end = default_note_end(
        Rational::new(1, 1),
        Rational::new(1, 1),
        60,
        &abutting,
        Rational::new(8, 1),
    );
    assert_eq!(end, Rational::new(2, 1));
}

#[test]
fn copy_and_paste_uses_the_time_selection_start() {
    let (doc, clip) = midi_doc(vec![note(30, 60, 1, 1)]);
    let mut runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = open(&mut ui, &doc, &runtime, clip);
    let content = ui.midi_editor_content_rect;
    let pos = egui::pos2(note_x(&ui, content, 1.5), note_y(&ui, content, 60));
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![egui::Event::PointerMoved(pos)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(pos, true)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(pos, false)],
    );
    let _ = run_ui(&ctx, &mut ui, &doc, &runtime, vec![egui::Event::Copy]);
    runtime.playhead = Rational::new(4, 1);
    let intents = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![egui::Event::Paste(String::new())],
    );
    let changed = intents
        .iter()
        .find_map(|intent| match intent {
            UiIntent::EditMidiNotes { notes, .. } => Some(notes.clone()),
            _ => None,
        })
        .expect("paste should emit a note edit");
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].start, Rational::new(1, 1));
    assert_eq!(changed[0].pitch, 60);
    assert_ne!(changed[0].id, id::<Note>(30));
}

#[test]
fn arrow_keys_move_selected_notes_on_the_grid_and_in_pitch() {
    let notes = vec![note(1, 60, 2, 1)];
    let midi = midi_with_notes(notes);
    let clip = ClipEntity {
        id: id::<Clip>(20),
        track_id: id::<Track>(2),
        kind: ClipKind::Midi,
        range: RangeMus::new(Rational::zero(), Rational::new(8, 1)).unwrap(),
        name: None,
        color: None,
        muted: false,
        payload: ClipPayload::Midi(midi.clone()),
    };
    let mut state = MidiEditorState::new(clip.id);
    state.sync_notes(&midi.notes);
    state.select_note(id::<Note>(1), false);
    let mut ui = NotwigUi::default();
    ui.edit_focus = EditFocus::MidiEditor;
    let mut midi = midi;
    let ctx = egui::Context::default();

    let run = |ctx: &egui::Context,
               ui: &mut NotwigUi,
               midi: &mut MidiClipData,
               state: &mut MidiEditorState,
               key: egui::Key,
               shift: bool| {
        ui.intents.clear();
        let modifiers = egui::Modifiers {
            shift,
            ..Default::default()
        };
        let _ = ctx.run(
            egui::RawInput {
                modifiers,
                events: vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                ..Default::default()
            },
            |ctx| {
                notwig_ui::test_api::midi_editor::handle_midi_editor_actions(
                    ui,
                    ctx,
                    state,
                    midi,
                    &clip,
                    Rational::new(1, 8),
                    Rational::zero(),
                    120.0,
                );
            },
        );
    };

    run(
        &ctx,
        &mut ui,
        &mut midi,
        &mut state,
        egui::Key::ArrowRight,
        false,
    );
    assert_eq!(
        midi.notes[0].start,
        Rational::new(2, 1) + Rational::new(1, 8)
    );
    assert!(
        ui.intents
            .iter()
            .any(|intent| matches!(intent, UiIntent::PreviewNote { pitch: 60, .. }))
    );
    run(
        &ctx,
        &mut ui,
        &mut midi,
        &mut state,
        egui::Key::ArrowUp,
        false,
    );
    assert_eq!(midi.notes[0].pitch, 61);
    assert!(
        ui.intents
            .iter()
            .any(|intent| matches!(intent, UiIntent::PreviewNote { pitch: 61, .. }))
    );
    run(
        &ctx,
        &mut ui,
        &mut midi,
        &mut state,
        egui::Key::ArrowDown,
        true,
    );
    assert_eq!(midi.notes[0].pitch, 49);
    assert!(
        ui.intents
            .iter()
            .any(|intent| matches!(intent, UiIntent::PreviewNote { pitch: 49, .. }))
    );
}

#[test]
fn duplicate_uses_the_time_selection_as_the_rightward_offset() {
    let notes = vec![note(1, 60, 1, 1)];
    let midi = midi_with_notes(notes);
    let clip = ClipEntity {
        id: id::<Clip>(20),
        track_id: id::<Track>(2),
        kind: ClipKind::Midi,
        range: RangeMus::new(Rational::new(4, 1), Rational::new(12, 1)).unwrap(),
        name: None,
        color: None,
        muted: false,
        payload: ClipPayload::Midi(midi.clone()),
    };
    let mut state = MidiEditorState::new(clip.id);
    state.sync_notes(&midi.notes);
    state.select_marquee([id::<Note>(1)], span(0, 4));
    let mut ui = NotwigUi::default();
    ui.edit_focus = EditFocus::MidiEditor;
    let mut midi = midi;
    let ctx = egui::Context::default();
    let _ = ctx.run(
        egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::D,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    command: true,
                    ..Default::default()
                },
            }],
            modifiers: egui::Modifiers {
                command: true,
                ..Default::default()
            },
            ..Default::default()
        },
        |ctx| {
            notwig_ui::test_api::midi_editor::handle_midi_editor_actions(
                &mut ui,
                ctx,
                &mut state,
                &mut midi,
                &clip,
                Rational::new(1, 8),
                Rational::zero(),
                120.0,
            );
        },
    );
    let duplicated = midi
        .notes
        .iter()
        .find(|note| note.id != id::<Note>(1))
        .expect("duplicate should create a note");
    assert_eq!(duplicated.start, Rational::new(5, 1));
    let time = state.time_selection().expect("time selection is preserved");
    assert_eq!(time.start, Rational::new(4, 1));
    assert_eq!(time.length(), Rational::new(4, 1));
}

#[test]
fn notes_can_be_created_beyond_the_clip_range() {
    let (doc, clip) = midi_doc_range(Vec::new(), 4, 8);
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = open(&mut ui, &doc, &runtime, clip);
    let content = ui.midi_editor_content_rect;
    let before = egui::pos2(note_x(&ui, content, 2.0), note_y(&ui, content, 60));
    let after = egui::pos2(note_x(&ui, content, 10.0), note_y(&ui, content, 64));

    let before_intents = double_click(&ctx, &mut ui, &doc, &runtime, before);
    let ctx_after = egui::Context::default();
    let after_intents = double_click(&ctx_after, &mut ui, &doc, &runtime, after);
    let created: Vec<NoteEntity> = before_intents
        .iter()
        .chain(after_intents.iter())
        .filter_map(|intent| match intent {
            UiIntent::CreateMidiNote { note, .. } => Some(note.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(created.len(), 2, "both clicks should create notes");
    assert!(
        created
            .iter()
            .any(|note| note.pitch == 60 && note.start < Rational::zero())
    );
    assert!(
        created
            .iter()
            .any(|note| note.pitch == 64 && note.start > Rational::new(4, 1))
    );
}

#[test]
fn clicking_a_note_previews_a_capped_duration() {
    let (doc, clip) = midi_doc(vec![note(30, 60, 1, 2)]);
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = open(&mut ui, &doc, &runtime, clip);
    let content = ui.midi_editor_content_rect;
    let pos = egui::pos2(note_x(&ui, content, 1.5), note_y(&ui, content, 60));
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![egui::Event::PointerMoved(pos)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(pos, true)],
    );
    let intents = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(pos, false)],
    );
    let preview = intents.iter().find_map(|intent| match intent {
        UiIntent::PreviewNote {
            pitch, length_mus, ..
        } => Some((*pitch, *length_mus)),
        _ => None,
    });
    assert_eq!(
        preview,
        Some((60, Some(Rational::new(1, 2)))),
        "a click should audition a short capped preview, not the full note"
    );
}

#[test]
fn clicking_a_long_note_does_not_audition_its_full_length() {
    let long = note(30, 60, 1, 4);
    let (doc, clip) = midi_doc(vec![long.clone()]);
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = open(&mut ui, &doc, &runtime, clip);
    let content = ui.midi_editor_content_rect;
    let pos = egui::pos2(note_x(&ui, content, 1.5), note_y(&ui, content, 60));
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![egui::Event::PointerMoved(pos)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(pos, true)],
    );
    let intents = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(pos, false)],
    );
    let length = intents
        .iter()
        .find_map(|intent| match intent {
            UiIntent::PreviewNote {
                pitch, length_mus, ..
            } if *pitch == 60 => *length_mus,
            _ => None,
        })
        .expect("clicking a note should preview it");
    assert!(
        length < long.length,
        "the audition length {length:?} should be capped below the stored length {:?}",
        long.length
    );
    assert_eq!(length, Rational::new(1, 2));
}

#[test]
fn arrow_keys_preview_every_selected_note_in_a_stack() {
    let notes = vec![note(1, 60, 1, 1), note(2, 64, 1, 1), note(3, 67, 1, 1)];
    let midi = midi_with_notes(notes);
    let clip = ClipEntity {
        id: id::<Clip>(21),
        track_id: id::<Track>(2),
        kind: ClipKind::Midi,
        range: RangeMus::new(Rational::zero(), Rational::new(8, 1)).unwrap(),
        name: None,
        color: None,
        muted: false,
        payload: ClipPayload::Midi(midi.clone()),
    };
    let mut state = MidiEditorState::new(clip.id);
    state.sync_notes(&midi.notes);
    state.select_notes([id::<Note>(1), id::<Note>(2), id::<Note>(3)]);
    let mut ui = NotwigUi::default();
    ui.edit_focus = EditFocus::MidiEditor;
    let mut midi = midi;
    let ctx = egui::Context::default();
    let modifiers = egui::Modifiers::default();
    let _ = ctx.run(
        egui::RawInput {
            modifiers,
            events: vec![egui::Event::Key {
                key: egui::Key::ArrowUp,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        },
        |ctx| {
            notwig_ui::test_api::midi_editor::handle_midi_editor_actions(
                &mut ui,
                ctx,
                &mut state,
                &mut midi,
                &clip,
                Rational::new(1, 8),
                Rational::zero(),
                120.0,
            );
        },
    );
    let mut pitches: Vec<u8> = ui
        .intents
        .iter()
        .filter_map(|intent| match intent {
            UiIntent::PreviewNote { pitch, .. } => Some(*pitch),
            _ => None,
        })
        .collect();
    pitches.sort_unstable();
    assert_eq!(pitches, vec![61, 65, 68]);
}

#[test]
fn copying_notes_also_sets_the_platform_clipboard_text() {
    let (doc, clip) = midi_doc(vec![note(30, 60, 1, 1)]);
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = open(&mut ui, &doc, &runtime, clip);
    let content = ui.midi_editor_content_rect;
    let pos = egui::pos2(note_x(&ui, content, 1.5), note_y(&ui, content, 60));
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![egui::Event::PointerMoved(pos)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(pos, true)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(pos, false)],
    );
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 900.0));
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            events: vec![egui::Event::Copy],
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, &doc, &runtime);
        },
    );
    assert!(
        output
            .platform_output
            .commands
            .iter()
            .any(|command| matches!(command, egui::OutputCommand::CopyText(_))),
        "copy should also set the platform clipboard text"
    );
}

#[test]
fn command_a_selects_all_notes() {
    let (doc, clip) = midi_doc(vec![note(30, 60, 1, 1), note(31, 64, 3, 1)]);
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = open(&mut ui, &doc, &runtime, clip);
    let modifiers = egui::Modifiers {
        command: true,
        ..Default::default()
    };
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }],
    );
    let state = ui.midi_editor.as_ref().expect("MIDI editor");
    assert_eq!(state.selected_ids().len(), 2);
    assert_eq!(state.time_selection(), Some(span(1, 4)));
}

#[test]
fn deleting_a_zero_width_time_selection_splits_every_crossing_note() {
    let (doc, clip) = midi_doc(vec![note(30, 60, 1, 3), note(31, 64, 1, 3)]);
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = open(&mut ui, &doc, &runtime, clip);
    ui.midi_editor
        .as_mut()
        .expect("MIDI editor")
        .select_time(span(2, 2));
    let intents = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![egui::Event::Key {
            key: egui::Key::Delete,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        }],
    );
    let (notes, removed) = intents
        .iter()
        .find_map(|intent| match intent {
            UiIntent::EditMidiNotes { notes, removed, .. } => {
                Some((notes.clone(), removed.clone()))
            }
            _ => None,
        })
        .expect("delete should emit a note edit");
    assert_eq!(removed.len(), 0);
    assert_eq!(notes.len(), 4);
    for (pitch, left_id) in [(60, id::<Note>(30)), (64, id::<Note>(31))] {
        let left = notes
            .iter()
            .find(|note| note.id == left_id)
            .expect("left half keeps its id");
        assert_eq!(left.pitch, pitch);
        assert_eq!(left.start, Rational::new(1, 1));
        assert_eq!(left.length, Rational::new(1, 1));
        let right = notes
            .iter()
            .find(|note| note.pitch == pitch && note.id != left_id)
            .expect("right half is created");
        assert_eq!(right.start, Rational::new(2, 1));
        assert_eq!(right.length, Rational::new(2, 1));
    }
}
