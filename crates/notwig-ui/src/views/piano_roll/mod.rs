pub(crate) mod actions;
pub(crate) mod geometry;
pub(crate) mod model;
pub(crate) mod navigation;
pub(crate) mod panel;
pub(crate) mod state;

pub use state::PianoRollState;

#[doc(hidden)]
pub mod test_api {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    pub use super::model::{
        MidiModel, NoteLowered, NoteSelection, Pitch, apply_note_lowered, create_note_clearing,
        default_note_end, insert_note_clearing, lower_note_plan,
    };

    pub fn pitch_for_y(y: f32, top: f32, scroll: f32) -> u8 {
        super::geometry::pitch_for_y(y, top, scroll)
    }

    pub fn y_for_pitch(pitch: u8, top: f32, scroll: f32) -> f32 {
        super::geometry::y_for_pitch(pitch, top, scroll)
    }

    pub const ROW_HEIGHT: f32 = super::geometry::ROW_HEIGHT;

    #[allow(clippy::too_many_arguments)]
    pub fn handle_piano_roll_actions(
        ui: &mut crate::shell::NotwigUi,
        ctx: &egui::Context,
        state: &mut PianoRollState,
        midi: &mut notwig_core::MidiClipData,
        clip: &notwig_core::ClipEntity,
        grid: notwig_core::TimeMus,
        playhead: notwig_core::TimeMus,
    ) {
        super::actions::handle_piano_roll_actions(ui, ctx, state, midi, clip, grid, playhead);
    }
}
