use egui::{Align2, Color32, FontId, Pos2, Rect as EguiRect, Stroke as EguiStroke, StrokeKind};
use notwig_ext_api::{
    Align, Color, CursorIcon, DrawCommand, Drawer, Font, FontFamily, Modifiers, PanelAction, Point,
    PointerState, Rect, Response, Sense, Stroke, TextAlign, UiBackend, UiId, UiInput,
    VerticalAlign,
};

/// Records extension draw commands into an [`egui::Painter`].
///
/// Coordinates are panel-local; `origin` maps them to screen space.
pub(crate) struct EguiDrawer {
    painter: egui::Painter,
    origin: Pos2,
    clips: Vec<EguiRect>,
}

impl EguiDrawer {
    pub(crate) fn new(painter: egui::Painter, target: EguiRect) -> Self {
        let clip = painter.clip_rect().intersect(target);
        Self {
            painter,
            origin: target.min,
            clips: vec![clip],
        }
    }

    /// Maps a panel-local rect to screen space.
    pub(crate) fn screen_rect(&self, rect: Rect) -> EguiRect {
        EguiRect::from_min_max(self.screen_pos(rect.min), self.screen_pos(rect.max))
    }

    fn screen_pos(&self, point: Point) -> Pos2 {
        egui::pos2(self.origin.x + point.x, self.origin.y + point.y)
    }

    fn current_clip(&self) -> EguiRect {
        *self.clips.last().unwrap_or(&self.painter.clip_rect())
    }

    fn clipped_painter(&self) -> egui::Painter {
        self.painter.with_clip_rect(self.current_clip())
    }

    /// Replays a recorded command through this drawer.
    pub(crate) fn paint_command(&mut self, command: &DrawCommand) {
        match command {
            DrawCommand::PushClip { rect } => self.push_clip(*rect),
            DrawCommand::PopClip => self.pop_clip(),
            DrawCommand::FillRect {
                rect,
                radius,
                color,
            } => self.fill_rect(*rect, *radius, *color),
            DrawCommand::StrokeRect {
                rect,
                radius,
                width,
                color,
            } => self.stroke_rect(*rect, *radius, Stroke::new(*width, *color)),
            DrawCommand::Line {
                from,
                to,
                width,
                color,
            } => self.line(*from, *to, Stroke::new(*width, *color)),
            DrawCommand::CircleFill {
                center,
                radius,
                color,
            } => self.circle_filled(*center, *radius, *color),
            DrawCommand::CircleStroke {
                center,
                radius,
                width,
                color,
            } => self.circle_stroke(*center, *radius, Stroke::new(*width, *color)),
            DrawCommand::Text {
                pos,
                text,
                size,
                color,
                align,
                family,
                valign,
            } => self.text(
                *pos,
                text,
                Font::new(*family, *size),
                *color,
                Align::new(*align, *valign),
            ),
            DrawCommand::Path {
                points,
                closed,
                fill,
                stroke,
            } => self.path(points, *closed, *fill, *stroke),
        }
    }
}

impl Drawer for EguiDrawer {
    fn push_clip(&mut self, rect: Rect) {
        let clip = self.current_clip().intersect(self.screen_rect(rect));
        self.clips.push(clip);
    }

    fn pop_clip(&mut self) {
        if self.clips.len() > 1 {
            self.clips.pop();
        }
    }

    fn fill_rect(&mut self, rect: Rect, radius: f32, color: Color) {
        self.clipped_painter().rect_filled(
            self.screen_rect(rect),
            corner_radius(radius),
            to_color32(color),
        );
    }

    fn stroke_rect(&mut self, rect: Rect, radius: f32, stroke: Stroke) {
        self.clipped_painter().rect_stroke(
            self.screen_rect(rect),
            corner_radius(radius),
            EguiStroke::new(stroke.width, to_color32(stroke.color)),
            StrokeKind::Inside,
        );
    }

    fn line(&mut self, from: Point, to: Point, stroke: Stroke) {
        self.clipped_painter().line_segment(
            [self.screen_pos(from), self.screen_pos(to)],
            EguiStroke::new(stroke.width, to_color32(stroke.color)),
        );
    }

    fn circle_filled(&mut self, center: Point, radius: f32, color: Color) {
        self.clipped_painter()
            .circle_filled(self.screen_pos(center), radius, to_color32(color));
    }

    fn circle_stroke(&mut self, center: Point, radius: f32, stroke: Stroke) {
        self.clipped_painter().circle_stroke(
            self.screen_pos(center),
            radius,
            EguiStroke::new(stroke.width, to_color32(stroke.color)),
        );
    }

    fn text(&mut self, pos: Point, text: &str, font: Font, color: Color, align: Align) {
        self.clipped_painter().text(
            self.screen_pos(pos),
            align2(align),
            text,
            font_id(font),
            to_color32(color),
        );
    }

    fn path(
        &mut self,
        points: &[Point],
        closed: bool,
        fill: Option<Color>,
        stroke: Option<Stroke>,
    ) {
        let points: Vec<Pos2> = points.iter().map(|point| self.screen_pos(*point)).collect();
        let painter = self.clipped_painter();
        if closed {
            if let Some(fill) = fill {
                if points.len() >= 3 {
                    painter.add(egui::Shape::convex_polygon(
                        points.clone(),
                        to_color32(fill),
                        EguiStroke::NONE,
                    ));
                }
            }
        }
        if let Some(stroke) = stroke {
            painter.add(egui::Shape::line(
                points,
                EguiStroke::new(stroke.width, to_color32(stroke.color)),
            ));
        }
    }
}

/// An [`UiBackend`] that draws directly through an [`egui::Ui`].
#[allow(dead_code)]
pub(crate) struct EguiUi<'a> {
    ui: &'a mut egui::Ui,
    drawer: EguiDrawer,
    input: UiInput,
    actions: Vec<PanelAction>,
}

#[allow(dead_code)]
impl<'a> EguiUi<'a> {
    pub(crate) fn new(ui: &'a mut egui::Ui, target: EguiRect) -> Self {
        let input = ui_input(ui, target);
        let drawer = EguiDrawer::new(ui.painter().clone(), target);
        Self {
            ui,
            drawer,
            input,
            actions: Vec::new(),
        }
    }

    pub(crate) fn take_actions(&mut self) -> Vec<PanelAction> {
        std::mem::take(&mut self.actions)
    }
}

impl UiBackend for EguiUi<'_> {
    fn reserve(&mut self, rect: Rect) {
        self.ui
            .advance_cursor_after_rect(self.drawer.screen_rect(rect));
    }

    fn interact(&mut self, id: UiId, rect: Rect, _clip: Rect, sense: Sense) -> Response {
        let screen = self.drawer.screen_rect(rect);
        let response = self.ui.interact(
            screen,
            egui::Id::new(("notwig-ext-ui", id)),
            to_egui_sense(sense),
        );
        Response {
            hovered: response.hovered(),
            clicked: response.clicked(),
            double_clicked: response.double_clicked(),
            secondary_clicked: response.secondary_clicked(),
            dragged: response.dragged(),
            drag_started: response.drag_started(),
            drag_stopped: response.drag_stopped(),
            drag_delta: to_point(response.drag_delta()),
            contains_pointer: response.contains_pointer(),
        }
    }

    fn painter(&mut self) -> &mut dyn Drawer {
        &mut self.drawer
    }

    fn input(&self) -> &UiInput {
        &self.input
    }

    fn set_cursor(&mut self, cursor: CursorIcon) {
        self.ui.ctx().set_cursor_icon(to_egui_cursor(cursor));
    }

    fn request_repaint(&mut self) {
        self.ui.ctx().request_repaint();
    }

    fn emit(&mut self, action: PanelAction) {
        self.actions.push(action);
    }
}

/// Builds a panel-local input snapshot from the current egui frame.
pub(crate) fn ui_input(ui: &egui::Ui, target: EguiRect) -> UiInput {
    let (pointer, modifiers, scroll, time, focused) = ui.input(|input| {
        let pointer = PointerState {
            pos: input
                .pointer
                .hover_pos()
                .map(|pos| to_point(pos - target.min)),
            delta: to_point(input.pointer.delta()),
            primary_down: input.pointer.primary_down(),
            primary_pressed: input.pointer.primary_pressed(),
            primary_released: input.pointer.primary_released(),
            secondary_down: input.pointer.secondary_down(),
            secondary_pressed: input.pointer.secondary_pressed(),
            secondary_released: input.pointer.secondary_released(),
        };
        let modifiers = Modifiers {
            alt: input.modifiers.alt,
            ctrl: input.modifiers.ctrl,
            shift: input.modifiers.shift,
            command: input.modifiers.command,
        };
        let scroll = to_point(input.raw_scroll_delta);
        (pointer, modifiers, scroll, input.time, input.focused)
    });
    UiInput {
        pointer,
        modifiers,
        scroll,
        time,
        focused,
        events: Vec::new(),
    }
}

/// Maps a neutral cursor request to egui's cursor enum.
pub(crate) fn to_egui_cursor(cursor: CursorIcon) -> egui::CursorIcon {
    match cursor {
        CursorIcon::Default => egui::CursorIcon::Default,
        CursorIcon::PointingHand => egui::CursorIcon::PointingHand,
        CursorIcon::ResizeVertical => egui::CursorIcon::ResizeVertical,
        CursorIcon::ResizeHorizontal => egui::CursorIcon::ResizeHorizontal,
        CursorIcon::Text => egui::CursorIcon::Text,
        CursorIcon::Grab => egui::CursorIcon::Grab,
        CursorIcon::Grabbing => egui::CursorIcon::Grabbing,
    }
}

#[allow(dead_code)]
fn to_egui_sense(sense: Sense) -> egui::Sense {
    match (sense.click, sense.drag) {
        (true, true) => egui::Sense::click_and_drag(),
        (true, false) => egui::Sense::click(),
        (false, true) => egui::Sense::drag(),
        (false, false) => egui::Sense::hover(),
    }
}

fn to_point(vec: egui::Vec2) -> Point {
    Point::new(vec.x, vec.y)
}

fn to_color32(color: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
}

fn corner_radius(radius: f32) -> egui::CornerRadius {
    egui::CornerRadius::same(radius.clamp(0.0, 255.0) as u8)
}

fn font_id(font: Font) -> FontId {
    match font.family {
        FontFamily::Proportional => FontId::proportional(font.size),
        FontFamily::Monospace => FontId::monospace(font.size),
    }
}

fn align2(align: Align) -> Align2 {
    match (align.x, align.y) {
        (TextAlign::Left, VerticalAlign::Top) => Align2::LEFT_TOP,
        (TextAlign::Left, VerticalAlign::Center) => Align2::LEFT_CENTER,
        (TextAlign::Left, VerticalAlign::Bottom) => Align2::LEFT_BOTTOM,
        (TextAlign::Center, VerticalAlign::Top) => Align2::CENTER_TOP,
        (TextAlign::Center, VerticalAlign::Center) => Align2::CENTER_CENTER,
        (TextAlign::Center, VerticalAlign::Bottom) => Align2::CENTER_BOTTOM,
        (TextAlign::Right, VerticalAlign::Top) => Align2::RIGHT_TOP,
        (TextAlign::Right, VerticalAlign::Center) => Align2::RIGHT_CENTER,
        (TextAlign::Right, VerticalAlign::Bottom) => Align2::RIGHT_BOTTOM,
    }
}
