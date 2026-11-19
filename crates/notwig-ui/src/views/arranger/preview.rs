use egui::{Id, Rect, Stroke};
use notwig_core::{ProjectDoc, TrackId};

use crate::shell::NotwigUi;
use crate::theme;

use super::ROW_H;

impl NotwigUi {
    pub(super) fn draw_drop_preview(&self, ui: &mut egui::Ui, doc: &ProjectDoc, header_w: f32) {
        let Some(preview) = &self.drag_preview else {
            return;
        };
        let painter = ui.painter().clone();
        let order: Vec<TrackId> = doc.track_order.depth_first();
        let lane = match preview.target {
            crate::intent::DropAudioTarget::Track(track_id) => self.row_rect(track_id).map(|row| {
                Rect::from_min_max(
                    egui::pos2(row.left() + header_w, row.top() + 3.0),
                    egui::pos2(row.right() - 1.0, row.bottom() - 3.0),
                )
            }),
            crate::intent::DropAudioTarget::ClipEditor { .. } => None,
            crate::intent::DropAudioTarget::NewTrack { index } => {
                let y = order
                    .get(index)
                    .and_then(|id| self.row_rect(*id))
                    .map(|row| row.top())
                    .or_else(|| {
                        self.track_rows
                            .values()
                            .chain(self.last_track_rows.values())
                            .map(egui::Rect::bottom)
                            .reduce(f32::max)
                    })
                    .unwrap_or(ui.max_rect().top());
                painter.line_segment(
                    [
                        egui::pos2(ui.max_rect().left(), y),
                        egui::pos2(ui.max_rect().right(), y),
                    ],
                    Stroke::new(2.0_f32, theme::ACCENT),
                );
                Some(Rect::from_min_max(
                    egui::pos2(ui.max_rect().left() + header_w, y + 4.0),
                    egui::pos2(ui.max_rect().right() - 1.0, y + 4.0 + ROW_H - 8.0),
                ))
            }
        };
        let Some(lane) = lane else {
            return;
        };
        let x1 = self
            .state
            .viewport
            .beat_to_x(preview.at, self.timeline_left);
        let end = preview.at + preview.duration;
        let x2 = self.state.viewport.beat_to_x(end, self.timeline_left);
        let rect = Rect::from_min_max(
            egui::pos2(x1.max(lane.left()), lane.top()),
            egui::pos2(x2.max(x1 + 8.0).min(lane.right()), lane.bottom()),
        );
        crate::timeline::draw_drop_preview(&painter, rect, &preview.path);
    }

    pub(super) fn timeline_scrollbar(
        &mut self,
        ui: &mut egui::Ui,
        bar_rect: Rect,
        doc: &ProjectDoc,
        header_w: f32,
        ppb: f32,
    ) {
        let track = Rect::from_min_max(
            egui::pos2(bar_rect.left() + header_w + 4.0, bar_rect.top() + 3.0),
            egui::pos2(bar_rect.right() - 4.0, bar_rect.bottom() - 3.0),
        );
        let content_end = doc
            .arranger
            .clips
            .values()
            .map(|clip| clip.range.end.as_f64())
            .fold(8.0_f64, f64::max)
            + 4.0;
        let visible = f64::from(((bar_rect.right() - bar_rect.left() - header_w) / ppb).max(1.0));
        if let Some(start) = crate::timeline::horizontal_scrollbar(
            ui,
            bar_rect,
            track,
            content_end,
            visible,
            self.state.viewport.start,
            Id::new("arranger_hscroll"),
        ) {
            self.state.viewport.start = start;
        }
    }
}
