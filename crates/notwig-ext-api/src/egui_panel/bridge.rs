use notwig_core::{ParamId, ParamValue};

use crate::context::ThemeSnapshot;
use crate::geometry::Color;
use crate::panel::PanelAction;
use crate::cursor::CursorIcon;

use super::{
    EguiEvent, EguiFrame, EguiImage, EguiInput, EguiMesh, EguiModifiers, EguiPointerButton,
    EguiPrimitive, EguiTextureDelta, EguiTextureOptions, EguiVertex,
};

#[derive(Default, Clone)]
struct ActionBuffer(Vec<PanelAction>);

fn action_id() -> egui::Id {
    egui::Id::new("notwig.egui.actions")
}

/// Applies the host theme to a guest egui context.
pub fn apply_theme(context: &egui::Context, theme: &ThemeSnapshot) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = to_color32(theme.panel);
    visuals.window_fill = to_color32(theme.panel);
    visuals.faint_bg_color = to_color32(theme.panel_light);
    visuals.extreme_bg_color = to_color32(theme.lane);
    visuals.override_text_color = Some(to_color32(theme.text));
    visuals.selection.bg_fill = to_color32(theme.selection);
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, to_color32(theme.text));
    visuals.hyperlink_color = to_color32(theme.accent);
    visuals.widgets.noninteractive.bg_fill = to_color32(theme.panel_light);
    visuals.widgets.noninteractive.fg_stroke =
        egui::Stroke::new(1.0_f32, to_color32(theme.text_muted));
    visuals.widgets.inactive.bg_fill = to_color32(theme.panel_light);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, to_color32(theme.text));
    visuals.widgets.hovered.bg_fill = to_color32(theme.grid_strong);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, to_color32(theme.text));
    visuals.widgets.active.bg_fill = to_color32(theme.accent);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, to_color32(theme.panel));
    context.set_visuals(visuals);
}

/// Runs one guest egui frame and converts it to the wire format.
pub fn run_frame(
    context: &egui::Context,
    input: EguiInput,
    theme: &ThemeSnapshot,
    mut draw: impl FnMut(&mut egui::Ui),
) -> EguiFrame {
    apply_theme(context, theme);
    let raw = raw_input(&input);
    let output = context.run(raw, |ctx| {
        ctx.data_mut(|data| data.insert_temp(action_id(), ActionBuffer::default()));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| draw(ui));
    });
    let actions = context
        .data_mut(|data| data.remove_temp::<ActionBuffer>(action_id()))
        .unwrap_or_default()
        .0;
    frame_from_output(context, output, actions)
}

/// Queues a normalized parameter change from guest UI code.
pub fn set_param_normalized(ui: &egui::Ui, param_id: ParamId, norm: f32) {
    ui.ctx().data_mut(|data| {
        data.get_temp_mut_or_default::<ActionBuffer>(action_id())
            .0
            .push(PanelAction::SetParamNormalized { param_id, norm });
    });
}

/// Queues an exact parameter change from guest UI code.
pub fn set_param_value(ui: &egui::Ui, param_id: ParamId, value: ParamValue) {
    ui.ctx().data_mut(|data| {
        data.get_temp_mut_or_default::<ActionBuffer>(action_id())
            .0
            .push(PanelAction::SetParamValue { param_id, value });
    });
}

fn raw_input(input: &EguiInput) -> egui::RawInput {
    let mut raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(input.width, input.height),
        )),
        max_texture_side: Some(input.max_texture_side as usize),
        time: input.time,
        modifiers: to_modifiers(input.modifiers),
        focused: input.focused,
        ..Default::default()
    };
    if let Some(viewport) = raw.viewports.get_mut(&egui::ViewportId::ROOT) {
        viewport.native_pixels_per_point = Some(input.pixels_per_point);
    }
    raw.events = input.events.iter().filter_map(to_event).collect();
    raw
}

fn to_event(event: &EguiEvent) -> Option<egui::Event> {
    match event {
        EguiEvent::PointerMoved { pos } => Some(egui::Event::PointerMoved(to_pos(*pos))),
        EguiEvent::PointerButton {
            pos,
            button,
            pressed,
            modifiers,
        } => Some(egui::Event::PointerButton {
            pos: to_pos(*pos),
            button: to_button(*button),
            pressed: *pressed,
            modifiers: to_modifiers(*modifiers),
        }),
        EguiEvent::PointerGone => Some(egui::Event::PointerGone),
        EguiEvent::MouseWheel { delta, modifiers } => Some(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(delta[0], delta[1]),
            modifiers: to_modifiers(*modifiers),
        }),
        EguiEvent::Key {
            key,
            pressed,
            repeat,
            modifiers,
        } => Some(egui::Event::Key {
            key: egui::Key::from_name(key)?,
            physical_key: None,
            pressed: *pressed,
            repeat: *repeat,
            modifiers: to_modifiers(*modifiers),
        }),
        EguiEvent::Text(text) => Some(egui::Event::Text(text.clone())),
        EguiEvent::Copy => Some(egui::Event::Copy),
        EguiEvent::Cut => Some(egui::Event::Cut),
        EguiEvent::Paste(text) => Some(egui::Event::Paste(text.clone())),
        EguiEvent::WindowFocused(focused) => Some(egui::Event::WindowFocused(*focused)),
    }
}

fn to_button(button: EguiPointerButton) -> egui::PointerButton {
    match button {
        EguiPointerButton::Primary => egui::PointerButton::Primary,
        EguiPointerButton::Secondary => egui::PointerButton::Secondary,
        EguiPointerButton::Middle => egui::PointerButton::Middle,
    }
}

fn to_modifiers(modifiers: EguiModifiers) -> egui::Modifiers {
    egui::Modifiers {
        alt: modifiers.alt,
        ctrl: modifiers.ctrl,
        shift: modifiers.shift,
        mac_cmd: modifiers.mac_cmd,
        command: modifiers.command,
    }
}

fn to_pos(pos: [f32; 2]) -> egui::Pos2 {
    egui::pos2(pos[0], pos[1])
}

fn to_color32(color: Color) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
}

fn frame_from_output(
    context: &egui::Context,
    output: egui::FullOutput,
    actions: Vec<PanelAction>,
) -> EguiFrame {
    let mut primitives = Vec::new();
    for primitive in context.tessellate(output.shapes, output.pixels_per_point) {
        let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive else {
            continue;
        };
        let Some(mesh) = mesh_to_wire(&mesh) else {
            continue;
        };
        primitives.push(EguiPrimitive {
            clip_rect: [
                primitive.clip_rect.min.x,
                primitive.clip_rect.min.y,
                primitive.clip_rect.max.x,
                primitive.clip_rect.max.y,
            ],
            mesh,
        });
    }

    let textures = output
        .textures_delta
        .set
        .into_iter()
        .filter_map(|(id, delta)| match id {
            egui::TextureId::Managed(id) => texture_delta_to_wire(id, delta),
            egui::TextureId::User(_) => None,
        })
        .collect();
    let free_textures = output
        .textures_delta
        .free
        .into_iter()
        .filter_map(|id| match id {
            egui::TextureId::Managed(id) => Some(id),
            egui::TextureId::User(_) => None,
        })
        .collect();
    let repaint_after_ms = output
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .and_then(|viewport| u32::try_from(viewport.repaint_delay.as_millis()).ok());
    let copy_text = output
        .platform_output
        .commands
        .iter()
        .find_map(|command| match command {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        });

    EguiFrame {
        primitives,
        textures,
        free_textures,
        actions,
        cursor: cursor_from_egui(output.platform_output.cursor_icon),
        wants_keyboard: context.wants_keyboard_input(),
        wants_pointer: context.wants_pointer_input(),
        copy_text,
        repaint_after_ms,
    }
}

fn mesh_to_wire(mesh: &egui::epaint::Mesh) -> Option<EguiMesh> {
    let egui::TextureId::Managed(texture_id) = mesh.texture_id else {
        return None;
    };
    Some(EguiMesh {
        indices: mesh.indices.clone(),
        vertices: mesh
            .vertices
            .iter()
            .map(|vertex| EguiVertex {
                pos: [vertex.pos.x, vertex.pos.y],
                uv: [vertex.uv.x, vertex.uv.y],
                color: vertex.color.to_array(),
            })
            .collect(),
        texture_id,
    })
}

fn texture_delta_to_wire(id: u64, delta: egui::epaint::ImageDelta) -> Option<EguiTextureDelta> {
    let egui::epaint::ImageData::Color(image) = delta.image;
    let width = u32::try_from(image.size[0]).ok()?;
    let height = u32::try_from(image.size[1]).ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    let pos = delta
        .pos
        .and_then(|[x, y]| Some([u32::try_from(x).ok()?, u32::try_from(y).ok()?]));
    Some(EguiTextureDelta {
        id,
        pos,
        options: EguiTextureOptions {
            linear: matches!(delta.options.magnification, egui::TextureFilter::Linear),
            repeat: matches!(delta.options.wrap_mode, egui::TextureWrapMode::Repeat),
        },
        image: EguiImage {
            width,
            height,
            pixels: image
                .pixels
                .iter()
                .map(|pixel| u32::from_le_bytes(pixel.to_array()))
                .collect(),
        },
    })
}

fn cursor_from_egui(cursor: egui::CursorIcon) -> Option<CursorIcon> {
    match cursor {
        egui::CursorIcon::Default => Some(CursorIcon::Default),
        egui::CursorIcon::PointingHand => Some(CursorIcon::PointingHand),
        egui::CursorIcon::ResizeVertical => Some(CursorIcon::ResizeVertical),
        egui::CursorIcon::ResizeHorizontal => Some(CursorIcon::ResizeHorizontal),
        egui::CursorIcon::Text => Some(CursorIcon::Text),
        egui::CursorIcon::Grab => Some(CursorIcon::Grab),
        egui::CursorIcon::Grabbing => Some(CursorIcon::Grabbing),
        _ => None,
    }
}

impl EguiImage {
    /// Converts this wire image to an egui image.
    #[must_use]
    pub fn to_egui_color_image(&self) -> egui::ColorImage {
        let pixels = self
            .pixels
            .iter()
            .map(|pixel| {
                let [r, g, b, a] = pixel.to_le_bytes();
                egui::Color32::from_rgba_premultiplied(r, g, b, a)
            })
            .collect();
        egui::ColorImage {
            size: [self.width as usize, self.height as usize],
            source_size: egui::vec2(self.width as f32, self.height as f32),
            pixels,
        }
    }
}

impl EguiTextureOptions {
    /// Converts this wire option set to egui texture options.
    #[must_use]
    pub const fn to_egui(self) -> egui::TextureOptions {
        if self.repeat {
            egui::TextureOptions {
                magnification: if self.linear {
                    egui::TextureFilter::Linear
                } else {
                    egui::TextureFilter::Nearest
                },
                minification: if self.linear {
                    egui::TextureFilter::Linear
                } else {
                    egui::TextureFilter::Nearest
                },
                wrap_mode: egui::TextureWrapMode::Repeat,
                mipmap_mode: None,
            }
        } else if self.linear {
            egui::TextureOptions::LINEAR
        } else {
            egui::TextureOptions::NEAREST
        }
    }
}

impl EguiMesh {
    /// Converts this wire mesh to an egui mesh, offset into panel space.
    #[must_use]
    pub fn to_egui_mesh(
        &self,
        texture_id: egui::TextureId,
        offset: egui::Vec2,
    ) -> egui::epaint::Mesh {
        egui::epaint::Mesh {
            indices: self.indices.clone(),
            vertices: self
                .vertices
                .iter()
                .map(|vertex| egui::epaint::Vertex {
                    pos: egui::pos2(vertex.pos[0] + offset.x, vertex.pos[1] + offset.y),
                    uv: egui::pos2(vertex.uv[0], vertex.uv[1]),
                    color: egui::Color32::from_rgba_premultiplied(
                        vertex.color[0],
                        vertex.color[1],
                        vertex.color[2],
                        vertex.color[3],
                    ),
                })
                .collect(),
            texture_id,
        }
    }
}
