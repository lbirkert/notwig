use crate::shell::NotwigUi;
use crate::timeline::interaction::timeline_navigation;

impl NotwigUi {
    pub(super) fn handle_navigation(&mut self, ui: &mut egui::Ui) {
        timeline_navigation(
            ui,
            self.arranger_rect,
            self.timeline_left,
            &mut self.state.viewport,
        );
    }
}
