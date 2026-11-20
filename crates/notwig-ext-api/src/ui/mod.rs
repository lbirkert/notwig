mod drawer;
mod record;
mod widgets;

pub use drawer::{Align, Drawer, Font};
pub use record::RecordDrawer;
pub use widgets::{
    METER_MIN_DB, button, divider, dropdown, knob, label, paint_button, paint_divider,
    paint_dropdown, paint_knob, paint_label, paint_section, paint_stereo_meter, paint_toggle,
    section, stereo_meter, toggle,
};

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::context::{DeviceMeterSnapshot, ThemeSnapshot};
use crate::draw::{FontFamily, MAX_ACTIONS, VerticalAlign};
use crate::geometry::{Point, Rect, Size};
use crate::panel::{PanelAction, PanelCommand, PanelDraw};

const DRAG_SLOP: f32 = 3.0;
const DOUBLE_CLICK_TIME: f64 = 0.3;
const DOUBLE_CLICK_DISTANCE: f32 = 6.0;

/// Stable widget identity chosen by the extension or UI code.
///
/// IDs are not persisted. They only need to stay stable across frames so
/// hover, focus, and drag state can be retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UiId(u64);

impl UiId {
    /// Creates an ID from a raw value.
    #[must_use]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// Creates an ID by hashing a stable source value.
    #[must_use]
    pub fn from_hash(source: impl Hash) -> Self {
        let mut hasher = DefaultHasher::new();
        source.hash(&mut hasher);
        Self(hasher.finish())
    }
}

/// Keyboard modifier state for the current frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Modifiers {
    /// Alt/Option is held.
    pub alt: bool,
    /// Control is held.
    pub ctrl: bool,
    /// Shift is held.
    pub shift: bool,
    /// Command on macOS, Control elsewhere.
    pub command: bool,
}

/// Pointer state in panel-local coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct PointerState {
    /// Current pointer position, if the pointer is over the panel.
    pub pos: Option<Point>,
    /// Pointer movement since the previous frame.
    pub delta: Point,
    /// Primary button is held.
    pub primary_down: bool,
    /// Primary button was pressed this frame.
    pub primary_pressed: bool,
    /// Primary button was released this frame.
    pub primary_released: bool,
    /// Secondary button is held.
    pub secondary_down: bool,
    /// Secondary button was pressed this frame.
    pub secondary_pressed: bool,
    /// Secondary button was released this frame.
    pub secondary_released: bool,
}

/// A bounded input event reserved for future keyboard and focus support.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum UiEvent {
    /// A key transition. `key` is a host-defined stable key code.
    Key {
        /// Host-defined key code.
        key: u32,
        /// Whether the key was pressed or released.
        pressed: bool,
        /// Whether this is an auto-repeat.
        repeat: bool,
    },
    /// Committed text input.
    Text(String),
    /// Focus entered or left the panel.
    Focus(bool),
}

/// Per-frame input snapshot passed to an extension panel.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct UiInput {
    /// Pointer state.
    pub pointer: PointerState,
    /// Modifier state.
    pub modifiers: Modifiers,
    /// Scroll delta in panel-local points.
    pub scroll: Point,
    /// Monotonic frame time in seconds.
    pub time: f64,
    /// Whether the panel currently has host focus.
    pub focused: bool,
    /// Bounded raw events. Empty in the pointer-only v1 interaction model.
    #[serde(default)]
    pub events: Vec<UiEvent>,
}

/// Declares which pointer interactions a widget accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Sense {
    /// Respond to clicks.
    pub click: bool,
    /// Respond to drags.
    pub drag: bool,
    /// Respond to hover.
    pub hover: bool,
}

impl Sense {
    /// Hover-only sense.
    #[must_use]
    pub const fn hover() -> Self {
        Self {
            click: false,
            drag: false,
            hover: true,
        }
    }

    /// Click sense, including hover.
    #[must_use]
    pub const fn click() -> Self {
        Self {
            click: true,
            drag: false,
            hover: true,
        }
    }

    /// Drag sense, including hover.
    #[must_use]
    pub const fn drag() -> Self {
        Self {
            click: false,
            drag: true,
            hover: true,
        }
    }

    /// Click-and-drag sense, including hover.
    #[must_use]
    pub const fn click_and_drag() -> Self {
        Self {
            click: true,
            drag: true,
            hover: true,
        }
    }
}

/// The result of interacting with a widget during one frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Response {
    /// The widget is hovered and no other widget owns the pointer.
    pub hovered: bool,
    /// The primary button was clicked on the widget this frame.
    pub clicked: bool,
    /// The primary button produced a double-click on the widget this frame.
    pub double_clicked: bool,
    /// The secondary button was clicked on the widget this frame.
    pub secondary_clicked: bool,
    /// The widget is being dragged.
    pub dragged: bool,
    /// A drag started this frame.
    pub drag_started: bool,
    /// A drag ended this frame.
    pub drag_stopped: bool,
    /// Pointer movement since the previous frame while dragging.
    pub drag_delta: Point,
    /// The pointer is inside the widget rectangle and clip region.
    pub contains_pointer: bool,
}

impl Response {
    /// Returns whether the widget is hovered.
    #[must_use]
    pub const fn hovered(self) -> bool {
        self.hovered
    }

    /// Returns whether the widget was clicked.
    #[must_use]
    pub const fn clicked(self) -> bool {
        self.clicked
    }

    /// Returns whether the widget was double-clicked.
    #[must_use]
    pub const fn double_clicked(self) -> bool {
        self.double_clicked
    }

    /// Returns whether the widget is being dragged.
    #[must_use]
    pub const fn dragged(self) -> bool {
        self.dragged
    }

    /// Returns whether a drag started this frame.
    #[must_use]
    pub const fn drag_started(self) -> bool {
        self.drag_started
    }

    /// Returns whether a drag ended this frame.
    #[must_use]
    pub const fn drag_stopped(self) -> bool {
        self.drag_stopped
    }

    /// Returns the pointer movement while dragging.
    #[must_use]
    pub const fn drag_delta(self) -> Point {
        self.drag_delta
    }

    /// Returns whether the pointer is inside the widget.
    #[must_use]
    pub const fn contains_pointer(self) -> bool {
        self.contains_pointer
    }
}

/// A neutral cursor request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CursorIcon {
    /// The platform default cursor.
    #[default]
    Default,
    /// A pointing hand, for clickable controls.
    PointingHand,
    /// Vertical resize, for vertical drag controls.
    ResizeVertical,
    /// Horizontal resize, for horizontal drag controls.
    ResizeHorizontal,
    /// Text input cursor.
    Text,
    /// Open-hand grab.
    Grab,
    /// Closed-hand grabbing.
    Grabbing,
}

/// Shared style values for panel widgets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    /// Theme colors.
    pub theme: ThemeSnapshot,
    /// Spacing between allocated widgets.
    pub spacing: Size,
    /// Default text size.
    pub text_size: f32,
    /// Small/label text size.
    pub small_text_size: f32,
    /// Default button size.
    pub button_size: Size,
    /// Default knob cell size.
    pub knob_size: Size,
}

impl Style {
    /// Creates a style from theme colors using the default metrics.
    #[must_use]
    pub const fn from_theme(theme: ThemeSnapshot) -> Self {
        Self {
            theme,
            spacing: Size::new(6.0, 4.0),
            text_size: 11.0,
            small_text_size: 9.0,
            button_size: Size::new(72.0, 22.0),
            knob_size: Size::new(78.0, 84.0),
        }
    }
}

impl Default for Style {
    fn default() -> Self {
        Self::from_theme(ThemeSnapshot::default())
    }
}

#[derive(Debug, Clone, Copy)]
struct HitRect {
    id: UiId,
    rect: Rect,
    clip: Rect,
    sense: Sense,
}

#[derive(Debug, Clone, Copy)]
struct DragState {
    id: UiId,
    press_pos: Point,
    last_pos: Point,
    dragging: bool,
}

#[derive(Debug, Clone, Copy)]
struct ClickRecord {
    id: UiId,
    time: f64,
    pos: Point,
}

#[derive(Debug, Clone, Copy)]
struct ReleaseState {
    id: UiId,
    clicked: bool,
    double_clicked: bool,
}

/// Per-device interaction memory retained across frames.
///
/// The SDK owns one `UiMemory` per device instance. Extensions do not need to
/// manage it directly; the extension runtime passes it to
/// [`RecordUi`].
#[derive(Debug, Default)]
pub struct UiMemory {
    hovered: Option<UiId>,
    active: Option<UiId>,
    focus: Option<UiId>,
    drag: Option<DragState>,
    last_click: Option<ClickRecord>,
    release: Option<ReleaseState>,
    drag_started: Option<UiId>,
    drag_stopped: Option<UiId>,
    hit_rects: Vec<HitRect>,
    next_hit_rects: Vec<HitRect>,
}

impl UiMemory {
    /// Creates empty interaction memory.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            hovered: None,
            active: None,
            focus: None,
            drag: None,
            last_click: None,
            release: None,
            drag_started: None,
            drag_stopped: None,
            hit_rects: Vec::new(),
            next_hit_rects: Vec::new(),
        }
    }

    /// Returns the widget that currently owns keyboard focus, if any.
    #[must_use]
    pub const fn focus(&self) -> Option<UiId> {
        self.focus
    }

    fn begin_frame(&mut self, input: &UiInput) {
        self.release = None;
        self.drag_started = None;
        self.drag_stopped = None;
        self.hit_rects = std::mem::take(&mut self.next_hit_rects);
        self.next_hit_rects.clear();

        let pos = input.pointer.pos;
        self.hovered = pos.and_then(|pos| {
            self.hit_rects
                .iter()
                .rev()
                .find(|hit| hit.matches(pos))
                .map(|hit| hit.id)
        });

        if input.pointer.primary_pressed && self.active.is_none() {
            if let Some(id) = self.hovered {
                let pos = pos.unwrap_or_default();
                self.active = Some(id);
                self.focus = Some(id);
                self.drag = Some(DragState {
                    id,
                    press_pos: pos,
                    last_pos: pos,
                    dragging: false,
                });
            } else {
                self.focus = None;
            }
        }

        if let Some(active) = self.active {
            if input.pointer.primary_down {
                if let Some(drag) = self.drag.as_mut() {
                    if drag.id == active {
                        let pos = pos.unwrap_or(drag.last_pos);
                        if !drag.dragging && pos.distance(drag.press_pos) > DRAG_SLOP {
                            drag.dragging = true;
                            self.drag_started = Some(active);
                        }
                        drag.last_pos = pos;
                    }
                }
            }

            if input.pointer.primary_released {
                let hit = self
                    .hit_rects
                    .iter()
                    .rev()
                    .find(|hit| hit.id == active)
                    .copied();
                let dragging = self
                    .drag
                    .is_some_and(|drag| drag.id == active && drag.dragging);
                let inside = pos.zip(hit).is_some_and(|(pos, hit)| hit.matches(pos));
                let clicked = !dragging && inside;
                let mut double_clicked = false;
                if clicked {
                    let pos = pos.unwrap_or_default();
                    if let Some(last) = self.last_click {
                        if last.id == active
                            && (input.time - last.time) <= DOUBLE_CLICK_TIME
                            && pos.distance(last.pos) <= DOUBLE_CLICK_DISTANCE
                        {
                            double_clicked = true;
                            self.last_click = None;
                        } else {
                            self.last_click = Some(ClickRecord {
                                id: active,
                                time: input.time,
                                pos,
                            });
                        }
                    } else {
                        self.last_click = Some(ClickRecord {
                            id: active,
                            time: input.time,
                            pos,
                        });
                    }
                }
                self.release = Some(ReleaseState {
                    id: active,
                    clicked,
                    double_clicked,
                });
                if dragging {
                    self.drag_stopped = Some(active);
                }
                self.active = None;
                self.drag = None;
            }
        }
    }

    fn register(&mut self, id: UiId, rect: Rect, clip: Rect, sense: Sense) {
        self.next_hit_rects.push(HitRect {
            id,
            rect,
            clip,
            sense,
        });
    }

    fn response(&self, id: UiId, rect: Rect, clip: Rect, input: &UiInput) -> Response {
        let pos = input.pointer.pos;
        let contains_pointer = pos.is_some_and(|pos| rect.contains(pos) && clip.contains(pos));
        let active = self.active == Some(id);
        let hovered = active || (self.active.is_none() && self.hovered == Some(id));
        let release = self.release.filter(|release| release.id == id);
        let dragged = active && self.drag.is_some_and(|drag| drag.id == id && drag.dragging);
        let drag_delta = if active || self.drag_stopped == Some(id) {
            input.pointer.delta
        } else {
            Point::default()
        };
        Response {
            hovered,
            clicked: release.is_some_and(|release| release.clicked),
            double_clicked: release.is_some_and(|release| release.double_clicked),
            secondary_clicked: input.pointer.secondary_released
                && contains_pointer
                && self.active.is_none(),
            dragged,
            drag_started: self.drag_started == Some(id),
            drag_stopped: self.drag_stopped == Some(id),
            drag_delta,
            contains_pointer,
        }
    }
}

impl HitRect {
    fn matches(self, pos: Point) -> bool {
        self.clip.contains(pos)
            && self.rect.contains(pos)
            && (self.sense.hover || self.sense.click || self.sense.drag)
    }
}

/// Low-level host operations behind [`UiContext`].
///
/// This trait is implemented by the extension recorder and by the egui host
/// adapter. Extension authors should use [`UiContext`] and shared widgets
/// instead of calling the backend directly.
pub trait UiBackend {
    /// Reserves layout space for a rect in the host UI.
    fn reserve(&mut self, rect: Rect);
    /// Registers and resolves interaction for a widget.
    fn interact(&mut self, id: UiId, rect: Rect, clip: Rect, sense: Sense) -> Response;
    /// Returns the primitive painter.
    fn painter(&mut self) -> &mut dyn Drawer;
    /// Returns the current input snapshot.
    fn input(&self) -> &UiInput;
    /// Requests a cursor icon.
    fn set_cursor(&mut self, cursor: CursorIcon);
    /// Requests another frame.
    fn request_repaint(&mut self);
    /// Emits a validated host action.
    fn emit(&mut self, action: PanelAction);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy)]
struct LayoutState {
    rect: Rect,
    start: Point,
    cursor: Point,
    used_max: Point,
    direction: Direction,
}

impl LayoutState {
    fn new(rect: Rect, direction: Direction) -> Self {
        Self {
            rect,
            start: rect.min,
            cursor: rect.min,
            used_max: rect.min,
            direction,
        }
    }

    fn used_rect(self) -> Rect {
        Rect::new(self.start, self.used_max)
    }
}

/// Immediate-mode panel UI context.
///
/// `UiContext` is the API extension developers use. It supports explicit
/// allocation, simple vertical/horizontal layout, shared widgets, interaction
/// responses, and host actions. It is backed by either [`RecordUi`] (inside a
/// WASM extension) or an egui adapter on the host.
pub struct UiContext<'a> {
    backend: &'a mut dyn UiBackend,
    layout: LayoutState,
    id_stack: Vec<u64>,
    style: Style,
    clips: Vec<Rect>,
    meter: DeviceMeterSnapshot,
}

impl<'a> UiContext<'a> {
    /// Creates a UI context for a panel target.
    #[must_use]
    pub fn new(backend: &'a mut dyn UiBackend, style: Style, target: Rect) -> Self {
        Self::new_with_meter(backend, style, target, DeviceMeterSnapshot::default())
    }

    /// Creates a UI context with the latest device meter snapshot.
    #[must_use]
    pub fn new_with_meter(
        backend: &'a mut dyn UiBackend,
        style: Style,
        target: Rect,
        meter: DeviceMeterSnapshot,
    ) -> Self {
        Self {
            backend,
            layout: LayoutState::new(target, Direction::Vertical),
            id_stack: Vec::new(),
            style,
            clips: vec![target],
            meter,
        }
    }

    /// Returns the latest input/output peaks measured by the engine.
    #[must_use]
    pub const fn device_meter(&self) -> DeviceMeterSnapshot {
        self.meter
    }

    /// Returns the remaining panel-local space.
    #[must_use]
    pub fn available_rect(&self) -> Rect {
        Rect::new(self.layout.cursor, self.layout.rect.max)
    }

    /// Allocates a rect from the current layout cursor.
    pub fn allocate(&mut self, size: Size) -> Rect {
        let rect = Rect::from_min_size(self.layout.cursor, size);
        self.layout.used_max.x = self.layout.used_max.x.max(rect.max.x);
        self.layout.used_max.y = self.layout.used_max.y.max(rect.max.y);
        match self.layout.direction {
            Direction::Vertical => {
                self.layout.cursor.y += size.height + self.style.spacing.height;
            }
            Direction::Horizontal => {
                self.layout.cursor.x += size.width + self.style.spacing.width;
            }
        }
        self.backend.reserve(rect);
        rect
    }

    /// Runs `add` in a vertical sub-layout.
    pub fn vertical(&mut self, add: impl FnOnce(&mut UiContext<'_>)) {
        self.with_direction(Direction::Vertical, add);
    }

    /// Runs `add` in a horizontal sub-layout.
    pub fn horizontal(&mut self, add: impl FnOnce(&mut UiContext<'_>)) {
        self.with_direction(Direction::Horizontal, add);
    }

    fn with_direction(&mut self, direction: Direction, add: impl FnOnce(&mut UiContext<'_>)) {
        let rect = Rect::new(self.layout.cursor, self.layout.rect.max);
        let mut child = UiContext {
            backend: &mut *self.backend,
            layout: LayoutState::new(rect, direction),
            id_stack: self.id_stack.clone(),
            style: self.style,
            clips: self.clips.clone(),
            meter: self.meter,
        };
        let clip_count = child.clips.len();
        add(&mut child);
        while child.clips.len() > clip_count {
            child.pop_clip();
        }
        let used = child.layout.used_rect();
        self.layout.used_max.x = self.layout.used_max.x.max(used.max.x);
        self.layout.used_max.y = self.layout.used_max.y.max(used.max.y);
        match self.layout.direction {
            Direction::Vertical => {
                self.layout.cursor.y = used.max.y + self.style.spacing.height;
            }
            Direction::Horizontal => {
                self.layout.cursor.x = used.max.x + self.style.spacing.width;
            }
        }
    }

    /// Returns the primitive painter.
    pub fn painter(&mut self) -> &mut dyn Drawer {
        self.backend.painter()
    }

    /// Returns the current clip rectangle.
    #[must_use]
    pub fn clip_rect(&self) -> Rect {
        *self.clips.last().unwrap_or(&self.layout.rect)
    }

    /// Pushes an additional clip rectangle.
    pub fn push_clip(&mut self, rect: Rect) {
        let clip = self.clip_rect().intersect(rect);
        self.clips.push(clip);
        self.backend.painter().push_clip(rect);
    }

    /// Pops the most recent clip rectangle.
    pub fn pop_clip(&mut self) {
        if self.clips.len() > 1 {
            self.clips.pop();
            self.backend.painter().pop_clip();
        }
    }

    /// Returns the input snapshot for this frame.
    #[must_use]
    pub fn input(&self) -> &UiInput {
        self.backend.input()
    }

    /// Hashes `source` with the current ID stack.
    #[must_use]
    pub fn id(&self, source: impl Hash) -> UiId {
        let mut hasher = DefaultHasher::new();
        self.id_stack.hash(&mut hasher);
        source.hash(&mut hasher);
        UiId(hasher.finish())
    }

    /// Pushes a raw ID onto the ID stack.
    pub fn push_id(&mut self, id: UiId) {
        self.id_stack.push(id.0);
    }

    /// Pops the most recent ID.
    pub fn pop_id(&mut self) {
        self.id_stack.pop();
    }

    /// Registers interaction for a rect.
    pub fn interact(&mut self, id: UiId, rect: Rect, sense: Sense) -> Response {
        self.backend.interact(id, rect, self.clip_rect(), sense)
    }

    /// Returns shared style values.
    #[must_use]
    pub const fn style(&self) -> &Style {
        &self.style
    }

    /// Returns the current theme colors.
    #[must_use]
    pub const fn theme(&self) -> ThemeSnapshot {
        self.style.theme
    }

    /// Requests a normalized parameter change.
    pub fn set_param_normalized(&mut self, param_id: notwig_core::ParamId, norm: f32) {
        self.backend
            .emit(PanelAction::SetParamNormalized { param_id, norm });
    }

    /// Requests an exact parameter change.
    pub fn set_param_value(
        &mut self,
        param_id: notwig_core::ParamId,
        value: notwig_core::ParamValue,
    ) {
        self.backend
            .emit(PanelAction::SetParamValue { param_id, value });
    }

    /// Requests a cursor icon for the panel.
    pub fn set_cursor(&mut self, cursor: CursorIcon) {
        self.backend.set_cursor(cursor);
    }

    /// Requests another UI frame.
    pub fn request_repaint(&mut self) {
        self.backend.request_repaint();
    }

    /// Returns the default font size used by shared widgets.
    #[must_use]
    pub const fn text_size(&self) -> f32 {
        self.style.text_size
    }

    /// Returns the small font size used by shared widgets.
    #[must_use]
    pub const fn small_text_size(&self) -> f32 {
        self.style.small_text_size
    }

    /// Returns the default proportional font request.
    #[must_use]
    pub const fn font(&self) -> Font {
        Font::new(FontFamily::Proportional, self.style.text_size)
    }

    /// Returns the default monospace font request.
    #[must_use]
    pub const fn mono_font(&self) -> Font {
        Font::new(FontFamily::Monospace, self.style.small_text_size)
    }

    /// Returns a centered text anchor.
    #[must_use]
    pub const fn center_align(&self) -> Align {
        Align::new(crate::geometry::TextAlign::Center, VerticalAlign::Center)
    }
}

/// Extension-side [`UiBackend`] that records commands and actions.
///
/// Create one per frame with the host input and the device's retained
/// [`UiMemory`], then build a [`UiContext`] over it. The SDK runtime does this
/// automatically; extension authors normally only receive `&mut UiContext`.
pub struct RecordUi<'a> {
    draw: RecordDrawer,
    input: &'a UiInput,
    memory: &'a mut UiMemory,
    actions: Vec<PanelAction>,
}

impl<'a> RecordUi<'a> {
    /// Starts a frame with retained interaction memory.
    #[must_use]
    pub fn new(input: &'a UiInput, memory: &'a mut UiMemory) -> Self {
        memory.begin_frame(input);
        Self {
            draw: RecordDrawer::new(),
            input,
            memory,
            actions: Vec::new(),
        }
    }

    /// Finishes the frame and returns the command list plus actions.
    #[must_use]
    pub fn into_panel(self) -> PanelDraw {
        PanelDraw {
            commands: self
                .draw
                .into_commands()
                .into_iter()
                .map(PanelCommand::Draw)
                .collect(),
            actions: self.actions,
        }
    }
}

impl UiBackend for RecordUi<'_> {
    fn reserve(&mut self, _rect: Rect) {}

    fn interact(&mut self, id: UiId, rect: Rect, clip: Rect, sense: Sense) -> Response {
        self.memory.register(id, rect, clip, sense);
        self.memory.response(id, rect, clip, self.input)
    }

    fn painter(&mut self) -> &mut dyn Drawer {
        &mut self.draw
    }

    fn input(&self) -> &UiInput {
        self.input
    }

    fn set_cursor(&mut self, cursor: CursorIcon) {
        self.emit(PanelAction::SetCursor { cursor });
    }

    fn request_repaint(&mut self) {
        self.emit(PanelAction::RequestRepaint);
    }

    fn emit(&mut self, action: PanelAction) {
        if self.actions.len() < MAX_ACTIONS {
            self.actions.push(action);
        }
    }
}
