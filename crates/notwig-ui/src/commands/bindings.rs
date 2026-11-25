use std::collections::BTreeMap;

use egui::Context;
use notwig_ext_api::CommandDescriptor;

use super::{COMMANDS, KeyChord, ShortcutCommand};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcuts {
    bindings: BTreeMap<String, Vec<KeyChord>>,
    dirty: bool,
}

impl Default for Shortcuts {
    fn default() -> Self {
        Self {
            bindings: builtin_bindings(),
            dirty: false,
        }
    }
}

fn builtin_bindings() -> BTreeMap<String, Vec<KeyChord>> {
    COMMANDS
        .iter()
        .map(|spec| (spec.id.to_owned(), spec.defaults.to_vec()))
        .collect()
}

impl Shortcuts {
    #[must_use]
    pub fn bindings(&self, command: ShortcutCommand) -> &[KeyChord] {
        self.bindings_for(command.id())
    }

    #[must_use]
    pub fn bindings_for(&self, command_id: &str) -> &[KeyChord] {
        self.bindings.get(command_id).map_or(&[], Vec::as_slice)
    }

    #[must_use]
    pub fn has_binding(&self, command: ShortcutCommand, chord: KeyChord) -> bool {
        self.bindings(command).contains(&chord)
    }

    pub fn assign(&mut self, command: ShortcutCommand, chord: KeyChord) {
        self.assign_id(command.id(), chord);
    }

    pub fn assign_id(&mut self, command_id: &str, chord: KeyChord) {
        for bindings in self.bindings.values_mut() {
            bindings.retain(|existing| *existing != chord);
        }
        self.bindings
            .entry(command_id.to_owned())
            .or_default()
            .push(chord);
        self.dirty = true;
    }

    pub fn remove(&mut self, command: ShortcutCommand, chord: KeyChord) {
        self.remove_id(command.id(), chord);
    }

    pub fn remove_id(&mut self, command_id: &str, chord: KeyChord) {
        if let Some(bindings) = self.bindings.get_mut(command_id) {
            let before = bindings.len();
            bindings.retain(|existing| *existing != chord);
            if bindings.len() != before {
                self.dirty = true;
            }
        }
    }

    /// Adds default bindings for commands that do not have an entry yet.
    ///
    /// A command with an explicit empty entry keeps it, so clearing a binding is
    /// not undone when extensions reload. Suggested chords never steal a chord
    /// already assigned to another command; conflicts are resolved by the
    /// shortcut editor instead of by load order.
    pub fn add_defaults(&mut self, descriptors: &[CommandDescriptor]) {
        let mut ordered = descriptors.to_vec();
        ordered.sort_by(|a, b| a.id.cmp(&b.id));
        let mut additions: Vec<(String, Vec<KeyChord>)> = Vec::new();
        for descriptor in ordered {
            if self.bindings.contains_key(&descriptor.id)
                || additions.iter().any(|(id, _)| id == &descriptor.id)
            {
                continue;
            }
            let mut chords = Vec::new();
            for value in &descriptor.default_bindings {
                let Some(chord) = KeyChord::parse(value) else {
                    continue;
                };
                let used = self
                    .bindings
                    .values()
                    .any(|bindings| bindings.contains(&chord))
                    || additions
                        .iter()
                        .any(|(_, bindings)| bindings.contains(&chord));
                if !used && !chords.contains(&chord) {
                    chords.push(chord);
                }
            }
            additions.push((descriptor.id, chords));
        }
        for (id, chords) in additions {
            self.bindings.insert(id, chords);
        }
    }

    pub fn reset(&mut self) {
        *self = Self {
            bindings: builtin_bindings(),
            dirty: true,
        };
    }

    #[must_use]
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    #[must_use]
    pub fn was_pressed(&self, ctx: &Context, command: ShortcutCommand) -> bool {
        self.was_pressed_id(ctx, command.id())
    }

    #[must_use]
    pub fn was_pressed_id(&self, ctx: &Context, command_id: &str) -> bool {
        self.bindings_for(command_id)
            .iter()
            .any(|chord| ctx.input(|input| chord.is_pressed(input)))
    }

    /// Resolves the effective copy, cut, and paste gestures for this frame.
    ///
    /// egui emits native clipboard events for the platform chords even after a
    /// command is rebound, so a native event only counts while the command is
    /// still bound to that platform chord. This keeps shell shortcuts and the
    /// clip/MIDI editors using the same rule.
    #[must_use]
    pub(crate) fn copy_cut_paste_pressed(&self, ctx: &Context) -> (bool, bool, bool) {
        let (copy_event, cut_event, paste_event) = ctx.input(|input| {
            (
                input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Copy)),
                input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Cut)),
                input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Paste(_))),
            )
        });
        (
            self.was_pressed(ctx, ShortcutCommand::Copy)
                || (copy_event
                    && self.has_binding(ShortcutCommand::Copy, KeyChord::command(egui::Key::C))),
            self.was_pressed(ctx, ShortcutCommand::Cut)
                || (cut_event
                    && self.has_binding(ShortcutCommand::Cut, KeyChord::command(egui::Key::X))),
            self.was_pressed(ctx, ShortcutCommand::Paste)
                || (paste_event
                    && self.has_binding(ShortcutCommand::Paste, KeyChord::command(egui::Key::V))),
        )
    }

    #[must_use]
    pub fn to_config(&self) -> BTreeMap<String, Vec<String>> {
        self.bindings
            .iter()
            .map(|(id, bindings)| {
                (
                    id.clone(),
                    bindings.iter().map(|chord| chord.config_string()).collect(),
                )
            })
            .collect()
    }

    #[must_use]
    pub fn from_config(config: &BTreeMap<String, Vec<String>>) -> Self {
        let mut shortcuts = Self::default();
        for (id, values) in config {
            let mut chords = Vec::new();
            for value in values {
                if let Some(chord) = KeyChord::parse(value) {
                    if !chords.contains(&chord) {
                        chords.push(chord);
                    }
                }
            }
            shortcuts.bindings.insert(id.clone(), chords);
        }
        shortcuts.dirty = false;
        shortcuts
    }
}
