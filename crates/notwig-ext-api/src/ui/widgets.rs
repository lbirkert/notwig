use crate::draw::FontFamily;
use crate::geometry::{Color, Point, Rect, Size, Stroke};

use super::drawer::{Align, Drawer, Font};
use super::{CursorIcon, Response, Sense, UiContext, UiId};
use crate::context::ThemeSnapshot;

/// Lowest level, in dBFS, shown by [`stereo_meter`].
pub const METER_MIN_DB: f32 = -60.0;

/// Draws a standard button into a rectangle.
pub fn paint_button(
    drawer: &mut dyn Drawer,
    theme: &ThemeSnapshot,
    rect: Rect,
    text: &str,
    response: &Response,
) {
    let fill = if response.hovered() {
        theme.grid_strong
    } else {
        theme.panel_light
    };
    drawer.fill_rect(rect, 4.0, fill);
    drawer.stroke_rect(rect, 4.0, Stroke::new(1.0, theme.grid));
    drawer.text(
        rect.center(),
        text,
        Font::new(FontFamily::Proportional, 11.0),
        theme.text,
        Align::center(),
    );
}

/// Draws a toggle button into a rectangle.
pub fn paint_toggle(
    drawer: &mut dyn Drawer,
    theme: &ThemeSnapshot,
    rect: Rect,
    active: bool,
    label: &str,
) {
    drawer.fill_rect(
        rect,
        4.0,
        if active {
            theme.accent
        } else {
            theme.panel_light
        },
    );
    drawer.text(
        rect.center(),
        label,
        Font::new(FontFamily::Proportional, 10.0),
        if active {
            Color::BLACK
        } else {
            theme.text_muted
        },
        Align::center(),
    );
}

/// Draws a rotary knob into a rectangle.
pub fn paint_knob(
    drawer: &mut dyn Drawer,
    theme: &ThemeSnapshot,
    rect: Rect,
    value_norm: f32,
    bipolar: bool,
) {
    let center = rect.center();
    let radius = 15.0;
    drawer.circle_filled(center, radius + 3.0, Color::rgba(29, 31, 37, 255));
    drawer.circle_stroke(
        center,
        radius,
        Stroke::new(3.0, Color::rgba(73, 77, 88, 255)),
    );
    let start = std::f32::consts::PI * 0.75;
    let end = std::f32::consts::PI * 2.25;
    let angle = start + (end - start) * value_norm.clamp(0.0, 1.0);
    let arc_start = if bipolar {
        start + (end - start) * 0.5
    } else {
        start
    };
    let steps = 24;
    let mut points = Vec::with_capacity(steps + 1);
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let angle = arc_start + (angle - arc_start) * t;
        points.push(Point::new(
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        ));
    }
    drawer.path(&points, false, None, Some(Stroke::new(3.0, theme.accent)));
    drawer.line(
        center,
        Point::new(
            center.x + (radius - 3.0) * angle.cos(),
            center.y + (radius - 3.0) * angle.sin(),
        ),
        Stroke::new(2.0, Color::WHITE),
    );
    if bipolar {
        drawer.line(
            Point::new(center.x, rect.min.y - 1.0),
            Point::new(center.x, rect.min.y + 1.0),
            Stroke::new(1.5, theme.text_muted),
        );
    }
}

/// Paints a closed dropdown header into a rectangle.
pub fn paint_dropdown(
    drawer: &mut dyn Drawer,
    theme: &ThemeSnapshot,
    rect: Rect,
    text: &str,
    open: bool,
) {
    drawer.fill_rect(
        rect,
        4.0,
        if open {
            theme.grid_strong
        } else {
            theme.panel_light
        },
    );
    drawer.stroke_rect(rect, 4.0, Stroke::new(1.0, theme.grid));
    drawer.text(
        Point::new(rect.min.x + 7.0, rect.center().y),
        text,
        Font::new(FontFamily::Proportional, 11.0),
        theme.text,
        Align::new(
            crate::geometry::TextAlign::Left,
            crate::draw::VerticalAlign::Center,
        ),
    );
    let arrow_x = rect.max.x - 11.0;
    let arrow_y = rect.center().y;
    drawer.path(
        &[
            Point::new(arrow_x - 4.0, arrow_y - 2.0),
            Point::new(arrow_x, arrow_y + 2.0),
            Point::new(arrow_x + 4.0, arrow_y - 2.0),
        ],
        true,
        Some(theme.text_muted),
        None,
    );
}

fn paint_dropdown_item(
    drawer: &mut dyn Drawer,
    theme: &ThemeSnapshot,
    rect: Rect,
    text: &str,
    selected: bool,
    hovered: bool,
) {
    let fill = if hovered {
        theme.grid_strong
    } else if selected {
        theme.panel
    } else {
        theme.panel_light
    };
    drawer.fill_rect(rect, 0.0, fill);
    drawer.text(
        Point::new(rect.min.x + 7.0, rect.center().y),
        text,
        Font::new(FontFamily::Proportional, 11.0),
        if selected { theme.accent } else { theme.text },
        Align::new(
            crate::geometry::TextAlign::Left,
            crate::draw::VerticalAlign::Center,
        ),
    );
}

/// Draws a dropdown header at `rect` and its option list when open.
///
/// Call this after the rest of the panel so the option list paints and
/// hit-tests above other widgets. The caller owns the `open` flag and the
/// retained header rect. Returns the newly selected index when clicked.
pub fn dropdown(
    ui: &mut UiContext<'_>,
    id: UiId,
    rect: Rect,
    selected: usize,
    options: &[&str],
    open: &mut bool,
) -> Option<usize> {
    let theme = ui.theme();
    paint_dropdown(
        ui.painter(),
        &theme,
        rect,
        options.get(selected).copied().unwrap_or(""),
        *open,
    );

    if !*open {
        let response = ui.interact(id, rect, Sense::click());
        if response.hovered() {
            ui.set_cursor(CursorIcon::PointingHand);
        }
        if response.clicked() {
            *open = true;
        }
        return None;
    }

    let backdrop = ui.interact(
        ui.id(("dropdown-backdrop", id)),
        ui.clip_rect(),
        Sense::click(),
    );
    let header = ui.interact(id, rect, Sense::click());
    let item_height = 20.0;
    let mut chosen = None;
    for (index, option) in options.iter().enumerate() {
        let item_rect = Rect::from_min_size(
            Point::new(rect.min.x, rect.max.y + index as f32 * item_height),
            Size::new(rect.width(), item_height),
        );
        let response = ui.interact(
            ui.id(("dropdown-item", id, index)),
            item_rect,
            Sense::click(),
        );
        paint_dropdown_item(
            ui.painter(),
            &theme,
            item_rect,
            option,
            index == selected,
            response.hovered(),
        );
        if response.hovered() {
            ui.set_cursor(CursorIcon::PointingHand);
        }
        if response.clicked() {
            chosen = Some(index);
        }
    }

    if chosen.is_some() || header.clicked() || backdrop.clicked() {
        *open = false;
    }
    chosen
}

/// Draws a stereo peak meter from linear peak amplitudes.
///
/// `peak_hold` draws a thin hold line when it is greater than zero. The meter
/// uses green, amber, and red zones and the same -60 dBFS floor as the host
/// track meters.
pub fn paint_stereo_meter(
    drawer: &mut dyn Drawer,
    theme: &ThemeSnapshot,
    rect: Rect,
    peak_left: f32,
    peak_right: f32,
    peak_hold: f32,
) {
    let gap = 2.0;
    let bar_width = ((rect.width() - gap) * 0.5).max(1.0);
    for (index, peak) in [peak_left, peak_right].into_iter().enumerate() {
        let x = rect.min.x + index as f32 * (bar_width + gap);
        let bar = Rect::new(
            Point::new(x, rect.min.y),
            Point::new(x + bar_width, rect.max.y),
        );
        drawer.fill_rect(bar, 1.0, theme.panel);
        let level = meter_level(peak);
        let top = bar.max.y - bar.height() * level;
        let visible = Rect::new(Point::new(x, top), Point::new(x + bar_width, bar.max.y));
        let green = Rect::new(
            Point::new(x, bar.min.y + bar.height() * 0.25),
            Point::new(x + bar_width, bar.max.y),
        );
        let yellow = Rect::new(
            Point::new(x, bar.min.y + bar.height() * 0.08),
            Point::new(x + bar_width, bar.min.y + bar.height() * 0.25),
        );
        let red = Rect::new(
            Point::new(x, bar.min.y),
            Point::new(x + bar_width, bar.min.y + bar.height() * 0.08),
        );
        for (zone, color) in [
            (green, Color::rgb(91, 196, 88)),
            (yellow, Color::rgb(218, 165, 61)),
            (red, Color::rgb(224, 80, 66)),
        ] {
            let clipped = zone.intersect(visible);
            if clipped.is_ordered() && clipped.height() > 0.0 {
                drawer.fill_rect(clipped, 0.0, color);
            }
        }
    }
    if peak_hold > 0.0 {
        let y = rect.max.y - rect.height() * meter_level(peak_hold);
        drawer.line(
            Point::new(rect.min.x, y),
            Point::new(rect.max.x, y),
            Stroke::new(1.0, theme.text),
        );
    }
    drawer.stroke_rect(rect, 1.0, Stroke::new(1.0, theme.grid));
}

fn meter_level(peak: f32) -> f32 {
    let db = 20.0 * peak.max(1e-6).log10();
    ((db - METER_MIN_DB) / (0.0 - METER_MIN_DB)).clamp(0.0, 1.0)
}

/// Allocates and draws a stereo peak meter.
pub fn stereo_meter(
    ui: &mut UiContext<'_>,
    size: Size,
    peak_left: f32,
    peak_right: f32,
    peak_hold: f32,
) -> Rect {
    let rect = ui.allocate(size);
    let theme = ui.theme();
    paint_stereo_meter(ui.painter(), &theme, rect, peak_left, peak_right, peak_hold);
    rect
}

/// Draws a text label placed at the top-left of a rectangle.
pub fn paint_label(drawer: &mut dyn Drawer, theme: &ThemeSnapshot, rect: Rect, text: &str) {
    drawer.text(
        rect.min,
        text,
        Font::new(FontFamily::Proportional, 11.0),
        theme.text_muted,
        Align::left_top(),
    );
}

/// Draws a section header placed at the top-left of a rectangle.
pub fn paint_section(drawer: &mut dyn Drawer, theme: &ThemeSnapshot, rect: Rect, title: &str) {
    drawer.text(
        rect.min,
        &title.to_uppercase(),
        Font::new(FontFamily::Monospace, 9.0),
        theme.text_muted,
        Align::left_top(),
    );
}

/// Draws a horizontal divider.
pub fn paint_divider(drawer: &mut dyn Drawer, theme: &ThemeSnapshot, rect: Rect) {
    drawer.line(
        Point::new(rect.min.x, rect.center().y),
        Point::new(rect.max.x, rect.center().y),
        Stroke::new(1.0, theme.grid),
    );
}

/// Allocates and draws a standard clickable button.
///
/// Use [`Response::clicked`] to react in the same frame:
///
/// ```ignore
/// if ui.button("Reset").clicked() {
///     ui.set_param_value(param_id, ParamValue::F64(0.0));
/// }
/// ```
pub fn button(ui: &mut UiContext<'_>, text: &str) -> Response {
    let id = ui.id(("button", text));
    let rect = ui.allocate(ui.style().button_size);
    let response = ui.interact(id, rect, Sense::click());
    if response.hovered() {
        ui.set_cursor(CursorIcon::PointingHand);
    }
    let theme = ui.theme();
    paint_button(ui.painter(), &theme, rect, text, &response);
    response
}

/// Allocates and draws a text label.
pub fn label(ui: &mut UiContext<'_>, text: &str) -> Rect {
    let rect = ui.allocate(Size::new(ui.available_rect().width(), 14.0));
    let theme = ui.theme();
    paint_label(ui.painter(), &theme, rect, text);
    rect
}

/// Allocates and draws a section header.
pub fn section(ui: &mut UiContext<'_>, title: &str) -> Rect {
    let rect = ui.allocate(Size::new(ui.available_rect().width(), 14.0));
    let theme = ui.theme();
    paint_section(ui.painter(), &theme, rect, title);
    rect
}

/// Allocates and draws a horizontal divider.
pub fn divider(ui: &mut UiContext<'_>) -> Rect {
    let rect = ui.allocate(Size::new(ui.available_rect().width(), 1.0));
    let theme = ui.theme();
    paint_divider(ui.painter(), &theme, rect);
    rect
}

/// Draws and interacts with a toggle button.
///
/// Returns `Some(new_value)` when the toggle was clicked this frame.
pub fn toggle(
    ui: &mut UiContext<'_>,
    id: UiId,
    rect: Rect,
    active: bool,
    text: &str,
) -> Option<bool> {
    let response = ui.interact(id, rect, Sense::click());
    if response.hovered() {
        ui.set_cursor(CursorIcon::PointingHand);
    }
    let theme = ui.theme();
    paint_toggle(ui.painter(), &theme, rect, active, text);
    response.clicked().then_some(!active)
}

/// Draws and interacts with a rotary knob.
///
/// Returns `Some(new_normalized_value)` when the knob changed this frame.
pub fn knob(
    ui: &mut UiContext<'_>,
    id: UiId,
    rect: Rect,
    value_norm: f32,
    default_norm: f32,
    bipolar: bool,
    label: &str,
) -> Option<f32> {
    let response = ui.interact(id, rect, Sense::click_and_drag());
    let mut norm = value_norm;
    if response.dragged() {
        let fine = ui.input().modifiers.shift;
        let speed = if fine { 0.001 } else { 0.005 };
        norm = (norm - response.drag_delta().y * speed).clamp(0.0, 1.0);
    }
    if response.double_clicked() {
        norm = default_norm;
    }
    if response.hovered() || response.dragged() {
        ui.set_cursor(CursorIcon::ResizeVertical);
    }
    let theme = ui.theme();
    paint_knob(ui.painter(), &theme, rect, norm, bipolar);
    paint_label(
        ui.painter(),
        &theme,
        Rect::from_min_size(
            Point::new(rect.min.x, rect.max.y - 14.0),
            Size::new(rect.width(), 14.0),
        ),
        label,
    );
    ((norm - value_norm).abs() > f32::EPSILON).then_some(norm)
}
