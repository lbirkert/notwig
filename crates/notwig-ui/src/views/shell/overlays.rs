use egui::{Color32, RichText};

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::state::NotificationKind;
use crate::theme;

use super::NotwigUi;

fn horizontal_indeterminate_bar(ui: &mut egui::Ui) {
    let width = ui.available_width().max(1.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 4.0), egui::Sense::hover());
    let painter = ui.painter();
    let radius = egui::CornerRadius::same(2);
    painter.rect_filled(rect, radius, theme::GRID);
    let segment = (width * 0.35).max(24.0).min(width);
    let travel = width + segment;
    let time = ui.ctx().input(|input| input.time) as f32;
    let phase = (time * 0.85).rem_euclid(1.0);
    let left = rect.left() - segment + travel * phase;
    let fill = egui::Rect::from_min_size(
        egui::pos2(left, rect.top()),
        egui::vec2(segment, rect.height()),
    );
    painter.rect_filled(fill.intersect(rect), radius, theme::ACCENT);
    ui.ctx().request_repaint();
}

impl NotwigUi {
    pub(super) fn notification_overlay(&mut self, ctx: &egui::Context) {
        self.state
            .expire_notifications(ctx.input(|input| input.time));
        if self.state.notifications.is_empty() {
            return;
        }
        let notes = self.state.notifications.clone();
        let offset_y = if self.render_progress.is_some() {
            118.0
        } else {
            12.0
        };
        let mut dismiss = None;
        egui::Area::new(egui::Id::new("notifications-overlay"))
            .anchor(egui::Align2::RIGHT_TOP, [-12.0, offset_y])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.set_width(300.0);
                for (index, note) in notes.iter().enumerate() {
                    let color = match note.kind {
                        NotificationKind::Info => theme::TEXT_MUTED,
                        NotificationKind::Warning => theme::ACCENT,
                        NotificationKind::Error => Color32::from_rgb(240, 120, 110),
                    };
                    egui::Frame::new()
                        .fill(theme::PANEL_LIGHT)
                        .stroke(egui::Stroke::new(1.0_f32, color))
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::Label::new(RichText::new(&note.message).color(color))
                                        .wrap(),
                                );
                                if ui.small_button("×").clicked() {
                                    dismiss = Some(index);
                                }
                            });
                            if note.indeterminate {
                                horizontal_indeterminate_bar(ui);
                            } else if let Some(progress) = note.progress {
                                ui.add(
                                    egui::ProgressBar::new(progress.clamp(0.0, 1.0)).animate(true),
                                );
                            }
                        });
                    ui.add_space(6.0);
                }
            });
        if let Some(index) = dismiss {
            self.state.remove_notification(index);
        }
    }

    pub(super) fn engine_crash_overlay(&mut self, ctx: &egui::Context) {
        let mut restart = false;
        egui::Area::new(egui::Id::new("engine-crash"))
            .anchor(egui::Align2::CENTER_TOP, [0.0, 54.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgb(58, 26, 28))
                    .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(240, 120, 110)))
                    .inner_margin(egui::Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(
                                    "The audio engine crashed. Playback is stopped to protect the app.",
                                )
                                .color(Color32::from_rgb(255, 200, 195)),
                            );
                            if ui.button("Restart engine").clicked() {
                                restart = true;
                            }
                        });
                    });
            });
        if restart {
            self.emit(UiIntent::RestartAudioEngine);
        }
    }

    pub(super) fn audio_device_lost_overlay(&mut self, ctx: &egui::Context, runtime: &RuntimeView) {
        let mut retry = false;
        let mut settings = false;
        egui::Area::new(egui::Id::new("audio-device-lost"))
            .anchor(egui::Align2::CENTER_TOP, [0.0, 54.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgb(58, 48, 24))
                    .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(238, 196, 96)))
                    .inner_margin(egui::Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(
                                        runtime
                                            .audio_device_lost_message
                                            .as_deref()
                                            .unwrap_or("The audio output device is unavailable."),
                                    )
                                    .color(Color32::from_rgb(255, 232, 178)),
                                );
                                ui.label(
                                    RichText::new(
                                        "Playback is paused. Reconnect the device or choose another output; Notwig retries automatically.",
                                    )
                                    .small()
                                    .color(Color32::from_rgb(224, 206, 160)),
                                );
                            });
                            if ui.button("Retry now").clicked() {
                                retry = true;
                            }
                            if ui.button("Audio settings…").clicked() {
                                settings = true;
                            }
                        });
                    });
            });
        if retry {
            self.emit(UiIntent::RestartAudioEngine);
        }
        if settings {
            self.open_settings(runtime);
        }
    }

    pub(super) fn render_progress_overlay(&mut self, ctx: &egui::Context, progress: f32) {
        let mut cancel = false;
        egui::Area::new(egui::Id::new("render-progress"))
            .anchor(egui::Align2::RIGHT_TOP, [-12.0, 12.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::PANEL_LIGHT)
                    .stroke(egui::Stroke::new(1.0_f32, theme::GRID_STRONG))
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        ui.set_width(260.0);
                        ui.label(RichText::new("Rendering mixdown").strong());
                        ui.add(egui::ProgressBar::new(progress.clamp(0.0, 1.0)).show_percentage());
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
            });
        if cancel {
            self.emit(UiIntent::CancelRender);
        }
    }

    pub(super) fn notifications_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_notifications;
        egui::Window::new("Notifications")
            .open(&mut open)
            .default_width(360.0)
            .show(ctx, |ui| {
                if self.state.notifications.is_empty() {
                    ui.label("No notifications.");
                }
                for note in &self.state.notifications {
                    let color = match note.kind {
                        NotificationKind::Info => theme::TEXT_MUTED,
                        NotificationKind::Warning => theme::ACCENT,
                        NotificationKind::Error => egui::Color32::from_rgb(240, 120, 110),
                    };
                    ui.label(RichText::new(&note.message).color(color));
                }
                if ui.button("Clear").clicked() {
                    self.emit(UiIntent::ClearNotifications);
                }
            });
        self.show_notifications = open;
    }
}
