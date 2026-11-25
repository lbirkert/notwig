use egui::{Align2, Key, RichText, TextEdit};

use notwig_core::{ClipPayload, ProjectDoc};
use notwig_ext_api::{
    CommandContext, CommandFocus, CommandSelection, CommandTimeSelection,
    MAX_COMMAND_CONTEXT_NOTES, MAX_COMMAND_CONTEXT_SLICES,
};

use crate::commands::ShortcutCommand;
use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::state::OpenedEditor;
use crate::theme;

use super::NotwigUi;

#[derive(Debug, Clone, Default)]
pub(crate) struct CommandPaletteState {
    query: String,
    selected: usize,
    focus_requested: bool,
}

impl NotwigUi {
    pub(crate) fn open_command_palette(&mut self) {
        self.command_palette = Some(CommandPaletteState::default());
    }

    pub(crate) fn command_context(
        &self,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) -> CommandContext {
        let focus = match self.currently_opened {
            Some(OpenedEditor::MidiEditor) => CommandFocus::MidiEditor,
            Some(OpenedEditor::ClipEditor) => CommandFocus::AudioEditor,
            Some(OpenedEditor::DeviceEditor) => CommandFocus::DeviceEditor,
            None => CommandFocus::Timeline,
        };
        let selection = &self.state.selection;
        let mut active_clip = None;
        let mut notes = Vec::new();
        let mut slices = Vec::new();
        match self.currently_opened {
            Some(OpenedEditor::MidiEditor) => {
                if let Some(editor) = &self.midi_editor {
                    active_clip = Some(editor.clip_id);
                    if let Some(clip) = doc.arranger.clips.get(&editor.clip_id)
                        && let ClipPayload::Midi(midi) = &clip.payload
                    {
                        let selected = editor.selected_note_ids();
                        notes = selected
                            .iter()
                            .take(MAX_COMMAND_CONTEXT_NOTES)
                            .filter_map(|id| midi.note(*id).cloned())
                            .collect();
                    }
                }
            }
            Some(OpenedEditor::ClipEditor) => {
                if let Some(editor) = &self.clip_editor {
                    active_clip = Some(editor.clip_id);
                    if let Some(clip) = doc.arranger.clips.get(&editor.clip_id)
                        && let ClipPayload::Audio(audio) = &clip.payload
                    {
                        let selected = editor.selected_slice_ids();
                        slices = selected
                            .iter()
                            .take(MAX_COMMAND_CONTEXT_SLICES)
                            .filter_map(|id| {
                                audio.slices.iter().find(|slice| slice.id == *id).cloned()
                            })
                            .collect();
                    }
                }
            }
            Some(OpenedEditor::DeviceEditor) | None => {}
        }
        let active_track = active_clip
            .and_then(|clip| doc.arranger.clips.get(&clip))
            .map(|clip| clip.track_id)
            .or_else(|| selection.primary_track());
        let active_device = selection.primary_device();
        let time = selection.time.as_ref().map(|time| CommandTimeSelection {
            range: time.range,
            tracks: time.tracks.iter().copied().collect(),
        });
        CommandContext {
            revision: doc.revision,
            focus,
            selection: CommandSelection {
                tracks: selection.tracks.iter().copied().collect(),
                clips: selection.clips.iter().copied().collect(),
                devices: selection.devices.iter().copied().collect(),
                time,
                notes,
                slices,
            },
            active_track,
            active_clip,
            active_device,
            playhead: runtime.playhead,
            tempo_bpm: doc.meta.tempo_bpm,
            sample_rate: runtime.dsp.sample_rate,
            snap_enabled: self.state.snap.enabled,
        }
    }

    pub(crate) fn command_palette_window(
        &mut self,
        ctx: &egui::Context,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) {
        let Some(mut state) = self.command_palette.take() else {
            return;
        };
        let context = self.command_context(doc, runtime);
        let mut close = false;
        let mut run_builtin = None;
        let mut run_extension = None;
        let commands: Vec<_> = self
            .commands
            .sorted()
            .into_iter()
            .filter(|command| command.enabled(&context))
            .filter(|command| matches_query(&state.query, command))
            .cloned()
            .collect();
        if commands.is_empty() {
            state.selected = 0;
        } else {
            state.selected = state.selected.min(commands.len() - 1);
        }

        let window = egui::Window::new("Command Palette")
            .id(egui::Id::new("command-palette"))
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_TOP, [0.0, 72.0])
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .stroke(egui::Stroke::new(1.0_f32, theme::GRID_STRONG))
                    .inner_margin(egui::Margin::same(10))
                    .corner_radius(8.0),
            )
            .show(ctx, |ui| {
                ui.set_width(520.0);
                let response = ui.add(
                    TextEdit::singleline(&mut state.query)
                        .hint_text("Search commands")
                        .desired_width(f32::INFINITY),
                );
                if !state.focus_requested {
                    response.request_focus();
                    state.focus_requested = true;
                }
                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    close = true;
                }
                if !commands.is_empty() {
                    let mut delta = 0_i32;
                    if ui.input(|input| input.key_pressed(Key::ArrowDown)) {
                        delta += 1;
                    }
                    if ui.input(|input| input.key_pressed(Key::ArrowUp)) {
                        delta -= 1;
                    }
                    if delta != 0 {
                        let len = i32::try_from(commands.len()).unwrap_or(i32::MAX);
                        state.selected = usize::try_from(
                            (i32::try_from(state.selected).unwrap_or(0) + delta).rem_euclid(len),
                        )
                        .unwrap_or(0);
                    }
                    if ui.input(|input| input.key_pressed(Key::Enter)) {
                        let command = &commands[state.selected];
                        if let Some(builtin) = ShortcutCommand::from_id(&command.id) {
                            run_builtin = Some(builtin);
                        } else {
                            run_extension = Some(command.id.clone());
                        }
                    }
                }
                ui.add_space(6.0);
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("command-palette-list")
                    .max_height(300.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if commands.is_empty() {
                            ui.label(
                                RichText::new("No matching commands.").color(theme::TEXT_MUTED),
                            );
                        }
                        for (index, command) in commands.iter().enumerate() {
                            let selected = index == state.selected;
                            let binding = self
                                .shortcuts
                                .bindings_for(&command.id)
                                .iter()
                                .map(|chord| chord.label())
                                .collect::<Vec<_>>()
                                .join(", ");
                            let text = if binding.is_empty() {
                                format!("{}  ·  {}", command.title, command.category)
                            } else {
                                format!(
                                    "{}  ·  {}        {}",
                                    command.title, command.category, binding
                                )
                            };
                            let response = ui.selectable_label(selected, text);
                            if response.clicked() {
                                if let Some(builtin) = ShortcutCommand::from_id(&command.id) {
                                    run_builtin = Some(builtin);
                                } else {
                                    run_extension = Some(command.id.clone());
                                }
                            }
                        }
                    });
            });
        let _ = window;

        if close {
            self.command_palette = None;
        } else if let Some(command) = run_builtin {
            self.command_palette = None;
            self.pending_builtin_command = Some(command);
        } else if let Some(command_id) = run_extension {
            self.command_palette = None;
            self.emit(UiIntent::RunExtensionCommand {
                command_id,
                context,
            });
        } else {
            self.command_palette = Some(state);
        }
    }
}

fn matches_query(query: &str, command: &notwig_ext_api::CommandDescriptor) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let id = command.id.to_lowercase();
    command.title.to_lowercase().contains(&query)
        || command.category.to_lowercase().contains(&query)
        || command.description.to_lowercase().contains(&query)
        || id.contains(&query)
}
