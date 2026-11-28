use notwig_ext_api::{
    AUDIO_SUB_EDITOR_CONTRACT, AUDIO_SUB_EDITOR_POINT, COMMAND_CONTRACT, COMMAND_POINT,
    CommandContext, CommandDescriptor, CommandEdit, CommandEffect, CommandExtension, CommandFocus,
    CommandSelection, EP_COMMANDS, Extension, ExtensionManifest, evaluate_when, validate_when,
};

#[derive(Default, notwig_ext_api::ExtensionCall)]
#[extension_call(CommandExtension)]
struct EchoCommand;

impl CommandExtension for EchoCommand {
    fn new() -> Self {
        Self
    }

    fn descriptor() -> CommandDescriptor {
        CommandDescriptor {
            id: "com.example.humanize".into(),
            title: "Echo".into(),
            category: "Test".into(),
            description: String::new(),
            when: None,
            default_bindings: Vec::new(),
            extension_id: None,
        }
    }

    fn execute(&mut self, context: &CommandContext) -> CommandEffect {
        CommandEffect::Apply {
            edits: vec![CommandEdit::MidiNotes {
                clip_id: context.active_clip.expect("clip"),
                notes: context.selection.notes.clone(),
                removed: Vec::new(),
            }],
            ui: Vec::new(),
        }
    }
}

fn manifest(version: u32, with_contribution: bool) -> String {
    let contributions = if with_contribution {
        format!(
            r#",
            "contributes": [{{
                "id": "com.example.humanize",
                "point": "{COMMAND_POINT}",
                "contract": "{COMMAND_CONTRACT}",
                "title": "Humanize",
                "category": "MIDI",
                "when": "focus == midi_editor && selection.notes > 0",
                "default_bindings": ["Ctrl+Alt+H"]
            }}]"#
        )
    } else {
        String::new()
    };
    format!(
        r#"{{
            "manifest_version": {version},
            "id": "com.example.test",
            "name": "Test",
            "version": "0.1.0",
            "api_version": 9,
            "capabilities": ["command"],
            "devices": []{contributions}
        }}"#
    )
}

#[test]
fn when_expression_reads_focus_and_selection() {
    let mut context = CommandContext {
        focus: CommandFocus::MidiEditor,
        ..Default::default()
    };
    context.selection.notes = vec![notwig_core::sequencer::NoteEntity {
        id: notwig_core::NoteId::from_uuid(uuid::Uuid::from_u128(1)),
        pitch: 60,
        start: notwig_core::TimeMus::zero(),
        length: notwig_core::TimeMus::new(1, 4),
        velocity: notwig_core::Norm01(0.5),
        release_velocity: None,
        expression: std::collections::BTreeMap::default(),
    }];
    assert!(evaluate_when("focus == midi_editor && selection.notes > 0", &context).unwrap());
    assert!(!evaluate_when("focus == audio_editor", &context).unwrap());
    assert!(evaluate_when("focus != audio_editor && selection.notes >= 1", &context).unwrap());
    assert!(evaluate_when("!(focus == timeline) && (selection.notes > 0)", &context).unwrap());
    assert!(!evaluate_when("selection.time", &context).unwrap());
}

#[test]
fn when_expression_rejects_unknown_fields() {
    assert!(validate_when("mystery > 0").is_err());
    assert!(validate_when("focus == midi_editor &&").is_err());
    assert!(validate_when("focus ~ midi_editor").is_err());
}

#[test]
fn manifest_v2_accepts_command_contribution() {
    let manifest: ExtensionManifest =
        serde_json::from_str(&manifest(2, true)).expect("parse manifest");
    manifest.validate_source().expect("valid manifest");
    let contribution = manifest.command_contributions().next().expect("command");
    assert_eq!(contribution.id, "com.example.humanize");
    assert_eq!(contribution.title.as_deref(), Some("Humanize"));
}

#[test]
fn contributions_require_manifest_v2() {
    let manifest: ExtensionManifest =
        serde_json::from_str(&manifest(1, true)).expect("parse manifest");
    assert!(manifest.validate_source().is_err());
}

#[test]
fn command_runtime_executes_a_registered_handler() {
    let mut extension = Extension::new();
    extension.register_extension::<EchoCommand>(EP_COMMANDS);
    let mut context = CommandContext {
        active_clip: Some(notwig_core::ClipId::from_uuid(uuid::Uuid::from_u128(2))),
        ..Default::default()
    };
    context.selection = CommandSelection::default();
    let bytes = postcard::to_allocvec(&context).expect("encode context");
    let result = extension
        .execute_command_bytes("com.example.humanize", &bytes)
        .expect("command result");
    let effect: CommandEffect = postcard::from_bytes(&result).expect("decode effect");
    assert!(matches!(effect, CommandEffect::Apply { .. }));
}

#[test]
fn command_runtime_rejects_unknown_ids() {
    let extension = Extension::new();
    let bytes = postcard::to_allocvec(&CommandContext::default()).expect("encode context");
    assert!(
        extension
            .execute_command_bytes("com.example.missing", &bytes)
            .is_none()
    );
}

#[test]
fn manifest_accepts_audio_sub_editor_contribution() {
    let manifest: ExtensionManifest = serde_json::from_str(&format!(
        r#"{{
            "manifest_version": 2,
            "id": "com.example.pan",
            "name": "Pan",
            "version": "0.1.0",
            "api_version": 9,
            "capabilities": ["audio_editor"],
            "contributes": [{{
                "id": "com.example.pan.editor",
                "point": "{AUDIO_SUB_EDITOR_POINT}",
                "contract": "{AUDIO_SUB_EDITOR_CONTRACT}",
                "title": "Clip Pan",
                "category": "Audio",
                "order": 3
            }}]
        }}"#
    ))
    .expect("parse manifest");
    manifest.validate_source().expect("valid manifest");
    let contribution = manifest
        .audio_sub_editor_contributions()
        .next()
        .expect("pan sub-editor");
    assert_eq!(contribution.order, 3);
}
