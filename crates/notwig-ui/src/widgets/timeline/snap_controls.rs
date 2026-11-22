use egui::Align2;
use egui::{Align, Color32, FontId, Id, Layout, Rect, Response, RichText, Sense, Ui, Vec2};

use crate::icons::Icon;
use crate::theme;
use crate::widgets::{SNAP_LEVELS, SnapSettings, snap_step_label};

const CONTROL_HEIGHT: f32 = 16.0;
const SNAP_BUTTON_WIDTH: f32 = 20.0;
const GRID_BUTTON_WIDTH: f32 = 56.0;
const GAP: f32 = 4.0;

pub const CONTROLS_WIDTH: f32 = SNAP_BUTTON_WIDTH + GAP + GRID_BUTTON_WIDTH;

#[derive(Debug, Clone, Copy)]
pub struct SnapControlsRects {
    pub snap: Rect,
    pub grid: Rect,
}

pub fn show(
    ui: &mut Ui,
    rect: Rect,
    id_salt: &'static str,
    snap: &mut SnapSettings,
    pixels_per_beat: f32,
    beats_per_bar: u8,
) -> SnapControlsRects {
    let height = CONTROL_HEIGHT.min(rect.height()).max(14.0);
    let top = rect.center().y - height * 0.5;
    let snap_rect = Rect::from_min_size(
        egui::pos2(rect.right() - CONTROLS_WIDTH, top),
        Vec2::new(SNAP_BUTTON_WIDTH, height),
    );
    let grid_rect = Rect::from_min_size(
        egui::pos2(snap_rect.right() + GAP, top),
        Vec2::new(GRID_BUTTON_WIDTH, height),
    );

    let snap_response = icon_text_control(
        ui,
        snap_rect,
        Icon::Snap,
        snap.enabled,
        "Snap to grid (Shift bypasses)",
        id_salt,
    );
    if snap_response.clicked() {
        snap.enabled = !snap.enabled;
    }

    let label = snap_step_label(snap.step(pixels_per_beat, beats_per_bar), beats_per_bar);
    let grid_response = text_control(ui, grid_rect, &label, false, "Grid settings", id_salt);
    egui::Popup::from_toggle_button_response(&grid_response)
        .align(egui::RectAlign::TOP_END)
        .gap(4.0)
        .show(|ui| grid_popup(ui, id_salt, snap, pixels_per_beat, beats_per_bar));

    SnapControlsRects {
        snap: snap_rect,
        grid: grid_rect,
    }
}

fn grid_popup(
    ui: &mut Ui,
    id_salt: &'static str,
    snap: &mut SnapSettings,
    pixels_per_beat: f32,
    beats_per_bar: u8,
) {
    ui.set_min_width(176.0);
    ui.spacing_mut().item_spacing = Vec2::new(4.0, 5.0);
    ui.with_layout(Layout::top_down(Align::Center), |ui| {
        ui.label(theme::heading(ui.ctx(), "GRID"));
        if ui
            .add_sized(
                [ui.available_width(), 24.0],
                egui::Button::new(RichText::new("Adaptive").size(12.0)).selected(snap.adaptive),
            )
            .clicked()
        {
            snap.adaptive = !snap.adaptive;
        }
        if snap.adaptive {
            let step = snap.step(pixels_per_beat, beats_per_bar);
            ui.label(
                RichText::new(snap_step_label(step, beats_per_bar))
                    .monospace()
                    .size(15.0)
                    .color(theme::ACCENT),
            );
            ui.label(
                RichText::new("Spacing follows zoom")
                    .size(10.0)
                    .color(theme::TEXT_MUTED),
            );
        } else {
            egui::ComboBox::from_id_salt(("timeline-snap-level", id_salt))
                .selected_text(
                    RichText::new(snap.level.label())
                        .monospace()
                        .size(14.0)
                        .color(Color32::WHITE),
                )
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for level in SNAP_LEVELS {
                        if ui
                            .selectable_label(snap.level == *level, level.label())
                            .clicked()
                        {
                            snap.level = *level;
                        }
                    }
                });
        }
        ui.separator();
        ui.label(
            RichText::new("Shift or Alt bypasses")
                .size(10.0)
                .color(theme::TEXT_MUTED),
        );
    });
}

fn icon_text_control(
    ui: &mut Ui,
    rect: Rect,
    icon: Icon,
    selected: bool,
    tooltip: &str,
    id_salt: &'static str,
) -> Response {
    let response = ui
        .interact(
            rect,
            Id::new(("timeline-snap-toggle", id_salt)),
            Sense::click(),
        )
        .on_hover_text(tooltip);
    let color = control_color(response.hovered(), selected);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon.glyph(),
        theme::icon_font(ui.ctx(), 13.0),
        color,
    );
    response
}

fn text_control(
    ui: &mut Ui,
    rect: Rect,
    label: &str,
    selected: bool,
    tooltip: &str,
    id_salt: &'static str,
) -> Response {
    let response = ui
        .interact(
            rect,
            Id::new(("timeline-grid-settings", id_salt)),
            Sense::click(),
        )
        .on_hover_text(tooltip);
    let color = control_color(response.hovered(), selected);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::monospace(11.0),
        color,
    );
    response
}

fn control_color(hovered: bool, selected: bool) -> Color32 {
    if selected {
        theme::ACCENT
    } else if hovered {
        Color32::WHITE
    } else {
        theme::TEXT_MUTED
    }
}

#[doc(hidden)]
pub mod test_api {
    pub use super::{CONTROLS_WIDTH, SnapControlsRects, show};
}
