use egui::{Align2, Color32, Id, Rect, Sense, Stroke, Ui};
use notwig_core::{RangeMus, TimeMus};

use crate::theme;
use crate::timeline::TimelineGrid;

#[path = "loop.rs"]
mod r#loop;
mod paint;

use self::r#loop::loop_rect;
pub(super) use self::r#loop::{LoopEdge, loop_edge_range};
pub use self::paint::draw_playhead;
use self::paint::{draw_beats, draw_loop, draw_play_start, fill_range};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BeatStripAction {
    Locate(TimeMus),
    SetLoop(RangeMus),
}

pub struct BeatStrip<'a> {
    pub grid: TimelineGrid,
    pub header_width: f32,
    pub header_label: Option<&'a str>,
    pub loop_range: Option<RangeMus>,
    pub time_selection: Option<RangeMus>,
    pub play_start: TimeMus,
    pub playhead: TimeMus,
    pub show_play_start: bool,
    /// Space kept free at the bottom of the strip for clip arrow tips.
    pub loop_bottom_inset: f32,
}

impl BeatStrip<'_> {
    pub fn show(self, ui: &mut Ui, rect: Rect, id: Id) -> Option<BeatStripAction> {
        let content_left = rect.left() + self.header_width;
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, egui::CornerRadius::ZERO, theme::PANEL_LIGHT);
        let response = ui.interact(rect, id, Sense::click_and_drag());

        if self.header_width > 0.0 {
            painter.line_segment(
                [
                    egui::pos2(content_left, rect.top()),
                    egui::pos2(content_left, rect.bottom()),
                ],
                Stroke::new(1.0_f32, theme::GRID_STRONG),
            );
            if let Some(label) = self.header_label {
                painter.text(
                    egui::pos2(rect.left() + 10.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    theme::heading_font(ui.ctx()),
                    theme::HEADING,
                );
            }
        }

        if let Some(range) = self.time_selection {
            fill_range(
                &painter,
                rect,
                content_left,
                self.grid,
                range,
                Color32::from_rgba_unmultiplied(255, 210, 120, 34),
            );
        }

        draw_beats(&painter, rect, content_left, self.grid);

        let mut action = None;
        let mut loop_preview = self.loop_range;
        let mut loop_dragging = false;
        if let Some(range) = self.loop_range {
            let (preview, committed) = self.loop_interaction(ui, rect, id, content_left, range);
            loop_dragging = preview.is_some();
            if let Some(preview) = preview {
                loop_preview = Some(preview);
            }
            if let Some(committed) = committed {
                action = Some(BeatStripAction::SetLoop(committed));
            }
        }

        if let Some(range) = loop_preview {
            let loop_rect = loop_rect(rect, content_left, self.grid, range, self.loop_bottom_inset);
            draw_loop(&painter, loop_rect);
        }

        if self.show_play_start {
            draw_play_start(&painter, rect, content_left, self.grid, self.play_start);
        }
        draw_playhead(&painter, rect, content_left, self.grid, self.playhead);

        if action.is_none()
            && !loop_dragging
            && (response.clicked() || response.dragged())
            && !response
                .interact_pointer_pos()
                .is_some_and(|pos| self.pointer_inside_loop(rect, content_left, pos))
        {
            action = response
                .interact_pointer_pos()
                .filter(|pos| pos.x > content_left)
                .map(|pos| {
                    let raw = self.grid.beat_for_x(pos.x, content_left);
                    let bypass = ui.input(|input| self.grid.bypassed(input.modifiers));
                    BeatStripAction::Locate(self.grid.snap(raw, bypass))
                });
        }
        action
    }
}
