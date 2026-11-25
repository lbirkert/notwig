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

#[test]
fn extension_defaults_do_not_steal_builtin_bindings() {
    use notwig_ext_api::CommandDescriptor;
    let mut shortcuts = Shortcuts::default();
    shortcuts.add_defaults(&[CommandDescriptor {
        id: "com.example.steal".into(),
        title: "Steal".into(),
        category: "Test".into(),
        description: String::new(),
        when: None,
        default_bindings: vec!["Cmd+S".into(), "Ctrl+Alt+H".into()],
        extension_id: Some("com.example".into()),
    }]);
    assert!(
        shortcuts
            .bindings_for("com.example.steal")
            .contains(&KeyChord {
                key: Key::H,
                command: false,
                ctrl: true,
                alt: true,
                shift: false,
            })
    );
    assert!(
        !shortcuts
            .bindings_for("com.example.steal")
            .contains(&KeyChord::command(Key::S))
    );
    assert_eq!(
        shortcuts.bindings(ShortcutCommand::Save),
        [KeyChord::command(Key::S)]
    );
}

#[test]
fn extension_bindings_round_trip_by_string_id() {
    let mut shortcuts = Shortcuts::default();
    let chord = KeyChord {
        key: Key::F6,
        command: false,
        ctrl: true,
        alt: false,
        shift: false,
    };
    shortcuts.assign_id("com.example.command", chord);
    let loaded = Shortcuts::from_config(&shortcuts.to_config());
    assert_eq!(loaded.bindings_for("com.example.command"), [chord]);
}

#[test]
fn command_registry_keeps_builtins_authoritative_and_sorted() {
    use notwig_ext_api::CommandDescriptor;
    use notwig_ui::CommandRegistry;
    let mut registry = CommandRegistry::new();
    registry.replace_extension_commands(vec![
        CommandDescriptor {
            id: "com.example.z".into(),
            title: "Zeta".into(),
            category: "Test".into(),
            description: String::new(),
            when: Some("focus == midi_editor".into()),
            default_bindings: Vec::new(),
            extension_id: Some("com.example".into()),
        },
        CommandDescriptor {
            id: "play".into(),
            title: "Fake Play".into(),
            category: "Fake".into(),
            description: String::new(),
            when: None,
            default_bindings: Vec::new(),
            extension_id: Some("com.example".into()),
        },
    ]);
    assert_eq!(registry.get("play").unwrap().title, "Play / stop");
    let ids: Vec<_> = registry
        .sorted()
        .iter()
        .map(|command| command.id.as_str())
        .collect();
    assert!(ids.windows(2).all(|pair| {
        let left = registry.get(pair[0]).unwrap();
        let right = registry.get(pair[1]).unwrap();
        (
            left.category.as_str(),
            left.title.as_str(),
            left.id.as_str(),
        ) <= (
            right.category.as_str(),
            right.title.as_str(),
            right.id.as_str(),
        )
    }));
}

#[test]
fn command_palette_opens_from_f1() {
    use notwig_ui::test_api::shell::{command_palette_open, handle_shortcuts};
    let doc = doc();
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = Context::default();
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 900.0));
    let _ = ctx.run(
        RawInput {
            screen_rect: Some(screen),
            events: vec![Event::Key {
                key: Key::F1,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            ..Default::default()
        },
        |ctx| {
            handle_shortcuts(&mut ui, ctx, &doc, &runtime);
            let _ = ui.show(ctx, &doc, &runtime);
        },
    );
    assert!(command_palette_open(&ui));
}

#[test]
fn command_palette_renders_extension_commands() {
    use notwig_ext_api::CommandDescriptor;
    use notwig_ui::test_api::shell::{open_command_palette, set_extension_commands};
    let doc = doc();
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    set_extension_commands(
        &mut ui,
        vec![CommandDescriptor {
            id: "com.example.humanize".into(),
            title: "Humanize Notes".into(),
            category: "MIDI".into(),
            description: "Nudge notes.".into(),
            when: Some("focus == midi_editor".into()),
            default_bindings: Vec::new(),
            extension_id: Some("com.example".into()),
        }],
    );
    open_command_palette(&mut ui);
    let ctx = Context::default();
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 900.0));
    let _ = ctx.run(
        RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ctx| {
            let _ = ui.show(ctx, &doc, &runtime);
        },
    );
}
