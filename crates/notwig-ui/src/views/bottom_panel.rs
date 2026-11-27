use egui::{Sense, Vec2};
use notwig_core::ProjectDoc;
use notwig_ext_api::{BottomPanelContext, BottomPanelFrame, CommandEffect};

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::theme;
use crate::views::device_editor::panel_render::{paint_panel_frame_keyed, panel_input};
use crate::views::device_editor::theme_snapshot;

impl NotwigUi {
    pub(crate) fn extension_bottom_panel_panel(
        &mut self,
        ctx: &egui::Context,
        doc: &ProjectDoc,
        _runtime: &RuntimeView,
        panel_id: &str,
    ) {
        let open = crate::views::panel::bottom_panel_styled(
            ctx,
            crate::views::panel::PanelConfig {
                id: "extension_bottom_panel",
                resizable: true,
                default_height: 300.0,
                min_height: 100.0,
                max_height: 800.0,
                collapse_height: 160.0,
                handle_id: "extension-bottom-panel",
                handle_tooltip: "Drag down to close",
            },
            theme::PANEL,
            egui::Margin::same(0),
            |ui| {
                let size = ui.available_size();
                let minimum = Vec2::new(320.0, 100.0);
                let size = size.max(minimum);
                let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
                if response.clicked() {
                    response.request_focus();
                }
                let focused = response.has_focus();
                let key = format!("bottom-panel:{panel_id}");
                let input = panel_input(ui, rect, size, focused);
                let context = BottomPanelContext {
                    panel_id: panel_id.to_owned(),
                    revision: doc.revision,
                    input,
                    theme: theme_snapshot(),
                };
                let frame = self
                    .bottom_panel_host
                    .as_mut()
                    .and_then(|host| host.draw_bottom_panel(panel_id, &context));
                let frame = match frame {
                    Some(frame) if frame.validate().is_ok() => {
                        self.last_extension_frames
                            .insert(key.clone(), frame.panel.clone());
                        Some(frame)
                    }
                    _ => self.last_extension_frames.get(&key).cloned().map(|panel| {
                        BottomPanelFrame {
                            panel,
                            effect: CommandEffect::None,
                        }
                    }),
                };
                let Some(frame) = frame else {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            egui::RichText::new("Bottom panel unavailable.")
                                .color(theme::TEXT_MUTED),
                        );
                    });
                    return true;
                };
                paint_panel_frame_keyed(self, ui, &key, &frame.panel, rect);
                if focused {
                    self.extension_panel_keyboard = true;
                }
                if !matches!(frame.effect, CommandEffect::None) {
                    self.emit(UiIntent::ApplyExtensionEffect {
                        revision: doc.revision,
                        effect: frame.effect,
                    });
                }
                true
            },
        );
        if !open {
            self.extension_bottom_panel = None;
        }
    }
}
