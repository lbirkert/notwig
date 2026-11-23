use std::collections::BTreeMap;

use egui::Context;

use super::{COMMANDS, KeyChord, ShortcutCommand};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcuts {
    bindings: BTreeMap<ShortcutCommand, Vec<KeyChord>>,
    dirty: bool,
}

impl Default for Shortcuts {
    fn default() -> Self {
        Self {
            bindings: COMMANDS
                .iter()
                .map(|spec| (spec.command, spec.defaults.to_vec()))
                .collect(),
            dirty: false,
        }
    }
}

impl Shortcuts {
    #[must_use]
    pub fn bindings(&self, command: ShortcutCommand) -> &[KeyChord] {
        self.bindings.get(&command).map_or(&[], Vec::as_slice)
    }

    #[must_use]
    pub fn has_binding(&self, command: ShortcutCommand, chord: KeyChord) -> bool {
        self.bindings
            .get(&command)
            .is_some_and(|bindings| bindings.contains(&chord))
    }

    pub fn assign(&mut self, command: ShortcutCommand, chord: KeyChord) {
        for bindings in self.bindings.values_mut() {
            bindings.retain(|existing| *existing != chord);
        }
        self.bindings.entry(command).or_default().push(chord);
        self.dirty = true;
    }

    pub fn remove(&mut self, command: ShortcutCommand, chord: KeyChord) {
        if let Some(bindings) = self.bindings.get_mut(&command) {
            let before = bindings.len();
            bindings.retain(|existing| *existing != chord);
            if bindings.len() != before {
                self.dirty = true;
            }
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
        self.dirty = true;
    }

    #[must_use]
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    #[must_use]
    pub fn was_pressed(&self, ctx: &Context, command: ShortcutCommand) -> bool {
        self.bindings.get(&command).is_some_and(|bindings| {
            ctx.input(|input| bindings.iter().any(|chord| chord.is_pressed(input)))
        })
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
        COMMANDS
            .iter()
            .map(|spec| {
                let bindings = self
                    .bindings
                    .get(&spec.command)
                    .map(|bindings| bindings.iter().map(|chord| chord.config_string()).collect())
                    .unwrap_or_default();
                (spec.id.to_owned(), bindings)
            })
            .collect()
    }

    #[must_use]
    pub fn from_config(config: &BTreeMap<String, Vec<String>>) -> Self {
        let mut shortcuts = Self::default();
        for spec in COMMANDS {
            let Some(values) = config.get(spec.id) else {
                continue;
            };
            shortcuts.bindings.insert(spec.command, Vec::new());
            for value in values {
                if let Some(chord) = KeyChord::parse(value) {
                    shortcuts.assign(spec.command, chord);
                }
            }
        }
        shortcuts.dirty = false;
        shortcuts
    }
}
