use egui::{Rect, Sense, Vec2};
use notwig_core::{ProjectDoc, TimeMus, TrackId};

use crate::runtime::RuntimeView;
use crate::shell::{NotwigUi, track_depth};

use super::ROW_H;

mod drop;
mod header;
mod lane;
mod menu;

impl NotwigUi {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn track_row(
        &mut self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        track_id: TrackId,
        header_w: f32,
        ppb: f32,
        start: TimeMus,
        runtime: &RuntimeView,
        grid: TimeMus,
    ) {
        let Some(track) = doc.tracks.get(&track_id) else {
            return;
        };
        let snap = self.state.snap;
        let width = ui.available_width().max(320.0);
        let (row_rect, _) = ui.allocate_exact_size(Vec2::new(width, ROW_H), Sense::hover());
        self.track_rows.insert(track_id, row_rect);
        let depth = track_depth(doc, track_id) as f32;
        let header_left = row_rect.left() + (depth * 14.0).min(header_w * 0.35);
        let header_rect = Rect::from_min_max(
            egui::pos2(header_left, row_rect.top()),
            egui::pos2(
                row_rect.left() + header_w.min(row_rect.width()),
                row_rect.bottom(),
            ),
        );
        let lane_rect = Rect::from_min_max(
            egui::pos2(
                row_rect.left() + header_w.min(row_rect.width()),
                row_rect.top(),
            ),
            row_rect.max,
        );
        let selected_track = self.state.selection.tracks.contains(&track_id)
            || self.state.selection.tracks.primary() == Some(track_id);
        let painter = ui.painter().clone();
        let layout = header::HeaderLayout::new(doc, track, track_id, row_rect, header_rect, depth);
        self.draw_track_header(
            ui,
            &painter,
            doc,
            track,
            track_id,
            &layout,
            selected_track,
            runtime,
        );
        self.handle_track_header(ui, doc, track, track_id, &layout);
        let clip_ids = self.interact_track_lane(
            ui, doc, track, track_id, lane_rect, start, ppb, grid, snap, runtime,
        );
        self.draw_track_lane(
            ui,
            doc,
            &painter,
            track,
            track_id,
            &clip_ids,
            header_rect,
            lane_rect,
            row_rect,
            start,
            ppb,
            runtime,
            grid,
            snap,
        );
    }
}
