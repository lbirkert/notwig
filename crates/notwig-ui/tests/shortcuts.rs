use egui::{Context, Event, Key, Modifiers, RawInput};
use notwig_core::{DeviceChainId, Meta, ProjectDoc, SampleRate, TrackId};
use notwig_ui::{KeyChord, NotwigUi, RuntimeView, SettingsCategory, ShortcutCommand, Shortcuts};

fn doc() -> ProjectDoc {
    ProjectDoc::new_template(
        Meta {
            name: "Shortcuts".into(),
            created_utc: 0,
            modified_utc: 0,
            sample_rate: SampleRate::DEFAULT,
            tempo_bpm: 120.0,
            time_sig_num: 4,
            time_sig_den: 4,
        },
        TrackId::nil(),
        DeviceChainId::nil(),
    )
    .unwrap()
}

#[test]
fn a_command_can_have_multiple_shortcuts() {
    let mut shortcuts = Shortcuts::default();
    let alt_f4 = KeyChord {
        key: Key::F4,
        command: false,
        ctrl: false,
        alt: true,
        shift: false,
    };
    shortcuts.assign(ShortcutCommand::Quit, alt_f4);
    assert_eq!(shortcuts.bindings(ShortcutCommand::Quit).len(), 2);
    assert!(shortcuts.has_binding(ShortcutCommand::Quit, alt_f4));
    assert!(shortcuts.has_binding(ShortcutCommand::Quit, KeyChord::command(Key::Q)));
}

#[test]
fn assigning_a_chord_removes_the_previous_owner() {
    let mut shortcuts = Shortcuts::default();
    let command_q = KeyChord::command(Key::Q);
    shortcuts.assign(ShortcutCommand::Copy, command_q);
    assert_eq!(shortcuts.bindings(ShortcutCommand::Quit), []);
    assert!(shortcuts.has_binding(ShortcutCommand::Copy, command_q));
}

#[test]
fn config_round_trip_preserves_custom_bindings_and_empty_commands() {
    let mut shortcuts = Shortcuts::default();
    shortcuts.assign(
        ShortcutCommand::Quit,
        KeyChord {
            key: Key::F4,
            command: false,
            ctrl: false,
            alt: true,
            shift: false,
        },
    );
    shortcuts.remove(ShortcutCommand::Save, KeyChord::command(Key::S));

    let loaded = Shortcuts::from_config(&shortcuts.to_config());
    assert_eq!(
        loaded.bindings(ShortcutCommand::Quit),
        shortcuts.bindings(ShortcutCommand::Quit)
    );
    assert_eq!(loaded.bindings(ShortcutCommand::Save), []);
}

#[test]
fn was_pressed_matches_exact_modifiers() {
    let shortcuts = Shortcuts::default();
    let ctx = Context::default();
    let modifiers = Modifiers {
        command: true,
        ..Modifiers::NONE
    };
    let mut copy = false;
    let mut delete = false;
    let _ = ctx.run(
        RawInput {
            modifiers,
            events: vec![Event::Key {
                key: Key::C,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        },
        |ctx| {
            copy = shortcuts.was_pressed(ctx, ShortcutCommand::Copy);
            delete = shortcuts.was_pressed(ctx, ShortcutCommand::Delete);
        },
    );
    assert!(copy);
    assert!(!delete);
}

#[test]
fn shortcut_settings_page_renders() {
    let mut ui = NotwigUi::default();
    ui.settings_open = true;
    ui.settings_category = SettingsCategory::Shortcuts;
    let ctx = Context::default();
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 900.0));
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, &doc(), &RuntimeView::default());
        },
    );
    assert!(ui.settings_open);
}

#[test]
fn capturing_a_chord_assigns_it_to_the_command() {
    use notwig_ui::test_api::shell::{begin_shortcut_capture, capture_shortcut};
    let mut ui = NotwigUi::default();
    begin_shortcut_capture(&mut ui, ShortcutCommand::Play);
    let ctx = Context::default();
    let modifiers = Modifiers {
        alt: true,
        ..Modifiers::NONE
    };
    let chord = KeyChord {
        key: Key::F4,
        command: false,
        ctrl: false,
        alt: true,
        shift: false,
    };
    let _ = ctx.run(
        RawInput {
            modifiers,
            events: vec![Event::Key {
                key: Key::F4,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        },
        |ctx| {
            capture_shortcut(&mut ui, ctx);
        },
    );
    assert!(ui.shortcuts().has_binding(ShortcutCommand::Play, chord));
}
