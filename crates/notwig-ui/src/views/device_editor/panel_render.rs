use std::time::Duration;

use egui::{Pos2, Rect, Vec2};
use notwig_core::DeviceInstance;
use notwig_core::dsp::DeviceSpec;
use notwig_ext_api::{
    CursorIcon, PanelEvent, PanelFrame, PanelInput, PanelModifiers, PanelPointerButton,
};

use crate::shell::NotwigUi;

pub(crate) fn panel_input(ui: &egui::Ui, target: Rect, size: Vec2, focused: bool) -> PanelInput {
    let (events, modifiers, time, max_texture_side) = ui.input(|input| {
        let mut events = Vec::new();
        if let Some(pos) = input.pointer.hover_pos() {
            events.push(PanelEvent::PointerMoved {
                pos: local_point(pos, target),
            });
        } else {
            events.push(PanelEvent::PointerGone);
        }
        // Pointer events always forward; keyboard, text, and clipboard events
        // are gated on focus so a background panel cannot steal input.
        for event in &input.events {
            match event {
                egui::Event::PointerMoved(pos) => events.push(PanelEvent::PointerMoved {
                    pos: local_point(*pos, target),
                }),
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers,
                } => events.push(PanelEvent::PointerButton {
                    pos: local_point(*pos, target),
                    button: pointer_button(*button),
                    pressed: *pressed,
                    modifiers: modifiers_of(*modifiers),
                }),
                egui::Event::PointerGone => events.push(PanelEvent::PointerGone),
                egui::Event::MouseWheel {
                    delta, modifiers, ..
                } => events.push(PanelEvent::MouseWheel {
                    delta: [delta.x, delta.y],
                    modifiers: modifiers_of(*modifiers),
                }),
                egui::Event::Key {
                    key,
                    pressed,
                    repeat,
                    modifiers,
                    ..
                } if focused => events.push(PanelEvent::Key {
                    key: key.name().to_owned(),
                    pressed: *pressed,
                    repeat: *repeat,
                    modifiers: modifiers_of(*modifiers),
                }),
                egui::Event::Text(text) if focused => {
                    events.push(PanelEvent::Text(text.clone()));
                }
                egui::Event::Copy if focused => events.push(PanelEvent::Copy),
                egui::Event::Cut if focused => events.push(PanelEvent::Cut),
                egui::Event::Paste(text) if focused => {
                    events.push(PanelEvent::Paste(text.clone()));
                }
                egui::Event::WindowFocused(focused) => {
                    events.push(PanelEvent::WindowFocused(*focused));
                }
                _ => {}
            }
        }
        (events, input.modifiers, input.time, input.max_texture_side)
    });
    PanelInput {
        width: size.x,
        height: size.y,
        pixels_per_point: ui.ctx().pixels_per_point(),
        time: Some(time),
        focused,
        // Clamp the adapter's ceiling to the panel texture limit so extensions
        // cannot request textures the renderer cannot allocate.
        max_texture_side: u32::try_from(max_texture_side)
            .unwrap_or(4096)
            .clamp(1, notwig_ext_api::panel_frame::MAX_PANEL_TEXTURE_DIM),
        modifiers: modifiers_of(modifiers),
        events,
    }
}

pub(super) fn paint_panel_frame(
    state: &mut NotwigUi,
    ui: &mut egui::Ui,
    device: &DeviceInstance,
    spec: &DeviceSpec,
    frame: &PanelFrame,
    target: Rect,
) {
    paint_panel_frame_keyed(state, ui, &device.id.to_string(), frame, target);
    state.apply_panel_actions(ui, device, spec, &frame.actions);
}

pub(crate) fn paint_panel_frame_keyed(
    state: &mut NotwigUi,
    ui: &mut egui::Ui,
    key: &str,
    frame: &PanelFrame,
    target: Rect,
) {
    if frame.validate().is_err() {
        return;
    }
    let origin = target.min.to_vec2();
    let panel_clip = target.intersect(ui.clip_rect());
    {
        let textures = state
            .extension_panel_textures
            .entry(key.to_owned())
            .or_default();
        for delta in &frame.textures {
            let image = delta.image.to_color_image();
            let options = delta.options.to_texture_options();
            if let Some([x, y]) = delta.pos {
                if let Some(handle) = textures.get_mut(&delta.id) {
                    let [width, height] = handle.size();
                    let patch_width = usize::try_from(delta.image.width).unwrap_or(usize::MAX);
                    let patch_height = usize::try_from(delta.image.height).unwrap_or(usize::MAX);
                    let x = x as usize;
                    let y = y as usize;
                    if x.saturating_add(patch_width) <= width
                        && y.saturating_add(patch_height) <= height
                    {
                        handle.set_partial([x, y], image, options);
                    }
                }
            } else if let Some(handle) = textures.get_mut(&delta.id) {
                handle.set(image, options);
            } else {
                let name = format!("extension-egui-{key}-{}", delta.id);
                let handle = ui.ctx().load_texture(name, image, options);
                textures.insert(delta.id, handle);
            }
        }
    }

    for primitive in &frame.primitives {
        let Some(texture) = state
            .extension_panel_textures
            .get(key)
            .and_then(|textures| textures.get(&primitive.mesh.texture_id))
        else {
            continue;
        };
        let clip = Rect::from_min_max(
            egui::pos2(primitive.clip_rect[0], primitive.clip_rect[1]),
            egui::pos2(primitive.clip_rect[2], primitive.clip_rect[3]),
        )
        .translate(origin)
        .intersect(panel_clip);
        let mesh = primitive.mesh.to_panel_mesh(texture.id(), origin);
        ui.painter()
            .with_clip_rect(clip)
            .add(egui::Shape::mesh(mesh));
    }
    if let Some(textures) = state.extension_panel_textures.get_mut(key) {
        for id in &frame.free_textures {
            textures.remove(id);
        }
    }

    if let Some(cursor) = frame.cursor {
        ui.ctx().set_cursor_icon(to_cursor(cursor));
    }
    if let Some(text) = &frame.copy_text {
        ui.ctx().copy_text(text.clone());
    }
    if let Some(repaint_after_ms) = frame.repaint_after_ms {
        ui.ctx()
            .request_repaint_after(Duration::from_millis(u64::from(repaint_after_ms)));
    }
}

pub(super) fn to_cursor(cursor: CursorIcon) -> egui::CursorIcon {
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

pub(crate) fn local_point(pos: Pos2, target: Rect) -> [f32; 2] {
    // Extension panels work in a local space with (0, 0) at the target's
    // top-left, matching the reported `width`/`height`.
    [pos.x - target.min.x, pos.y - target.min.y]
}

fn modifiers_of(modifiers: egui::Modifiers) -> PanelModifiers {
    PanelModifiers {
        alt: modifiers.alt,
        ctrl: modifiers.ctrl,
        shift: modifiers.shift,
        mac_cmd: modifiers.mac_cmd,
        command: modifiers.command,
    }
}

fn pointer_button(button: egui::PointerButton) -> PanelPointerButton {
    match button {
        egui::PointerButton::Primary => PanelPointerButton::Primary,
        egui::PointerButton::Secondary => PanelPointerButton::Secondary,
        egui::PointerButton::Middle | egui::PointerButton::Extra1 | egui::PointerButton::Extra2 => {
            PanelPointerButton::Middle
        }
    }
}
