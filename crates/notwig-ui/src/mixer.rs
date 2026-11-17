use egui::{Align, Color32, Layout, RichText, Vec2};
use notwig_core::{Db, Pan, ProjectDoc, TrackId};

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::theme;
use crate::widgets::{format_db, meter};

impl NotwigUi {
    pub(crate) fn mixer_view(
        &mut self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) {
        ui.add_space(4.0);
        egui::ScrollArea::horizontal()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    for track_id in doc.track_order.depth_first() {
                        self.mixer_strip(ui, doc, track_id, runtime);
                        ui.separator();
                    }
                    ui.vertical(|ui| {
                        ui.add_space(8.0);
                        ui.label(RichText::new("Add track").small().color(theme::TEXT_MUTED));
                        if ui.button("+ Audio").clicked() {
                            self.emit(UiIntent::CreateTrack(notwig_core::TrackKind::Audio));
                        }
                        if ui.button("+ Instrument").clicked() {
                            self.emit(UiIntent::CreateTrack(notwig_core::TrackKind::Instrument));
                        }
                        if ui.button("+ Group").clicked() {
                            self.emit(UiIntent::CreateTrack(notwig_core::TrackKind::Group));
                        }
                    });
                });
            });
    }

    fn mixer_strip(
        &mut self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        track_id: TrackId,
        runtime: &RuntimeView,
    ) {
        let Some(track) = doc.tracks.get(&track_id) else {
            return;
        };
        let width = 104.0;
        ui.allocate_ui_with_layout(
            Vec2::new(width, ui.available_height().max(260.0)),
            Layout::top_down(Align::Center),
            |ui| {
                ui.set_width(width - 10.0);
                ui.add_space(4.0);
                let selected = self.state.selection.tracks.contains(&track_id)
                    || self.state.selection.primary_track == Some(track_id);
                let name = if selected {
                    RichText::new(&track.name).strong().color(theme::ACCENT)
                } else {
                    RichText::new(&track.name)
                };
                if ui
                    .add(egui::Label::new(name).sense(egui::Sense::click()))
                    .clicked()
                {
                    self.state.selection.select_only_track(track_id);
                    self.emit(UiIntent::SelectTrack(track_id));
                }
                ui.label(
                    RichText::new(track.kind.name().to_uppercase())
                        .small()
                        .color(theme::TEXT_MUTED),
                );
                ui.add_space(6.0);

                meter(
                    ui,
                    runtime.meters.get(&track_id).copied().unwrap_or_default(),
                    false,
                );
                ui.add_space(6.0);

                let mut volume = track.mixer.volume_db.0;
                if ui
                    .add(
                        egui::Slider::new(&mut volume, -60.0..=6.0)
                            .orientation(egui::SliderOrientation::Vertical)
                            .show_value(false),
                    )
                    .changed()
                {
                    self.emit(UiIntent::SetTrackMixer {
                        track_id,
                        volume_db: Some(Db(volume)),
                        pan: None,
                        mute: None,
                        solo: None,
                    });
                }
                ui.label(
                    RichText::new(format_db(Db(volume)))
                        .small()
                        .monospace()
                        .color(theme::TEXT_MUTED),
                );

                let mut pan = track.mixer.pan.0;
                if ui
                    .add(egui::Slider::new(&mut pan, -1.0..=1.0).show_value(false))
                    .changed()
                {
                    self.emit(UiIntent::SetTrackMixer {
                        track_id,
                        volume_db: None,
                        pan: Some(Pan(pan)),
                        mute: None,
                        solo: None,
                    });
                }
                ui.label(
                    RichText::new(format!("PAN {pan:+.2}"))
                        .small()
                        .monospace()
                        .color(theme::TEXT_MUTED),
                );

                ui.horizontal(|ui| {
                    let mut mute = track.mixer.mute;
                    if ui
                        .add(
                            egui::Button::new("M")
                                .fill(if mute {
                                    theme::PLAYHEAD
                                } else {
                                    theme::PANEL_LIGHT
                                })
                                .min_size(Vec2::new(26.0, 20.0)),
                        )
                        .clicked()
                    {
                        mute = !mute;
                        self.emit(UiIntent::SetTrackMixer {
                            track_id,
                            volume_db: None,
                            pan: None,
                            mute: Some(mute),
                            solo: None,
                        });
                    }
                    let mut solo = track.mixer.solo;
                    if ui
                        .add(
                            egui::Button::new("S")
                                .fill(if solo {
                                    theme::ACCENT
                                } else {
                                    theme::PANEL_LIGHT
                                })
                                .min_size(Vec2::new(26.0, 20.0)),
                        )
                        .clicked()
                    {
                        solo = !solo;
                        self.emit(UiIntent::SetTrackMixer {
                            track_id,
                            volume_db: None,
                            pan: None,
                            mute: None,
                            solo: Some(solo),
                        });
                    }
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new(if track.kind == notwig_core::TrackKind::Master {
                        "MASTER"
                    } else {
                        "OUT"
                    })
                    .small()
                    .color(if track.kind == notwig_core::TrackKind::Master {
                        theme::ACCENT
                    } else {
                        Color32::DARK_GRAY
                    }),
                );
            },
        );
    }
}
