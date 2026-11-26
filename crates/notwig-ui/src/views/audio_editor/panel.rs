use egui::{Color32, CornerRadius, Id, Rect, Sense, Vec2};
use notwig_core::{ClipPayload, ProjectDoc, TimeMus};

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::state::Viewport;
use crate::theme;
use crate::timeline::{ClipChrome, ClipChromeDrag, ClipChromeEdge, TimelineGrid};

mod range_selection;
mod slice_gestures;
mod slice_paint;

use range_selection::{draw_selection, handle_background, handle_bar_drag, preview_bar_drag};
use slice_gestures::handle_slice;
use slice_paint::draw_slice;

use super::SCROLLBAR_H;
use super::actions::handle_clip_editor_actions;
use super::geometry::{group_move_shift, group_starts, slice_geometry};
use super::model::{editor_slices, source_duration};
use super::navigation::{clip_editor_navigation, clip_editor_scrollbar};
use super::state::{AudioSubEditor, ClipEdge, ClipEdgeDrag};

const CLIP_HEADER_W: f32 = 120.0;
const RULER_H: f32 = 38.0;

pub(super) fn playhead_for_strip(runtime: &RuntimeView) -> TimeMus {
    crate::timeline::playhead_for_strip(runtime)
}

pub(super) fn event_color(doc: &ProjectDoc, clip: &notwig_core::ClipEntity) -> Color32 {
    crate::timeline::clip_color(doc, clip)
}

impl NotwigUi {
    pub(crate) fn clip_editor_panel(
        &mut self,
        ctx: &egui::Context,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) {
        let Some(mut state) = self.clip_editor.clone() else {
            self.clip_editor_placeholder(ctx);
            return;
        };
        let Some(clip) = doc.arranger.clips.get(&state.clip_id).cloned() else {
            self.close_clip_editor();
            return;
        };
        let ClipPayload::Audio(audio) = &clip.payload else {
            self.close_clip_editor();
            return;
        };
        let mut audio = audio.clone();
        let map = doc.tempo_map();
        if !state.initialized {
            state.viewport.start = (clip.range.start - TimeMus::new(4, 1)).max(TimeMus::zero());
            state.initialized = true;
        }
        let clip_start = clip.range.start;
        let display_range = state.preview_range.unwrap_or(clip.range);
        let mut snap = self.state.snap;
        let extension_editors = self.extension_audio_sub_editors.clone();

        handle_clip_editor_actions(
            self,
            ctx,
            &mut state,
            &mut audio,
            &clip,
            &map,
            &runtime.clipboard,
            runtime.playhead,
        );

        let open = crate::views::panel::bottom_panel_styled(
            ctx,
            crate::views::panel::PanelConfig {
                id: "clip_editor_panel",
                resizable: true,
                default_height: 420.0,
                min_height: 120.0,
                max_height: 700.0,
                collapse_height: 180.0,
                handle_id: "clip-editor-panel",
                handle_tooltip: "Drag down to close",
            },
            theme::PANEL,
            egui::Margin::same(0),
            |ui| {
                let width = ui.available_width().max(320.0);
                let height = (ui.available_height() - SCROLLBAR_H).max(120.0);
                let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
                let header = Rect::from_min_max(
                    rect.min,
                    egui::pos2(rect.left() + CLIP_HEADER_W, rect.bottom()),
                );
                let content = Rect::from_min_max(egui::pos2(header.right(), rect.top()), rect.max);
                let ruler = Rect::from_min_size(content.min, Vec2::new(content.width(), RULER_H));
                let lanes =
                    Rect::from_min_max(egui::pos2(content.left(), ruler.bottom()), content.max);
                self.clip_editor_content_rect = content;
                if ui.rect_contains_pointer(rect) && ui.input(|input| input.pointer.any_pressed()) {
                    self.edit_focus = crate::state::EditFocus::ClipEditor;
                }

                clip_editor_navigation(&mut state, ui, content);

                let ppb = state
                    .viewport
                    .pixels_per_beat
                    .clamp(Viewport::MIN_PPB, Viewport::MAX_PPB);
                let bar_length = doc.meta.bar_length();
                let beat_length = doc.meta.beat_length();
                let grid = snap.step(ppb, bar_length);
                let timeline_grid =
                    TimelineGrid::new(state.viewport.start, ppb, bar_length, beat_length, snap);
                let clip_start_x = timeline_grid.x_for_beat(display_range.start, content.left());
                let clip_end_x = timeline_grid.x_for_beat(display_range.end, content.left());

                let painter = ui.painter().clone();
                painter.rect_filled(rect, CornerRadius::ZERO, theme::PANEL);
                painter.rect_filled(header, CornerRadius::ZERO, theme::PANEL_LIGHT);
                painter.rect_filled(lanes, CornerRadius::ZERO, theme::LANE);

                let strip_rect =
                    Rect::from_min_max(rect.min, egui::pos2(content.right(), rect.top() + RULER_H));
                let strip = crate::timeline::BeatStrip {
                    grid: timeline_grid,
                    header_width: CLIP_HEADER_W,
                    header_label: None,
                    loop_range: if doc.transport.loop_enabled {
                        doc.transport.loop_range
                    } else {
                        None
                    },
                    time_selection: state.bar_selection,
                    play_start: runtime.play_start,
                    playhead: playhead_for_strip(runtime),
                    show_play_start: true,
                    loop_bottom_inset: 14.0,
                };
                if let Some(action) = strip.show(
                    ui,
                    strip_rect,
                    Id::new(("clip-editor-ruler", state.clip_id)),
                ) {
                    match action {
                        crate::timeline::BeatStripAction::Locate(beat) => {
                            self.emit(UiIntent::SetPlayStart(beat));
                        }
                        crate::timeline::BeatStripAction::SetLoop(range) => {
                            self.emit(UiIntent::SetLoopRange(range));
                        }
                    }
                }
                let header_rect = Rect::from_min_max(
                    rect.min,
                    egui::pos2(rect.left() + CLIP_HEADER_W, rect.top() + RULER_H),
                );
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(header_rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    |ui| {
                        ui.add_space(8.0);
                        let active_label = state.sub_editor.extension_id().map_or_else(
                            || state.sub_editor.label().to_owned(),
                            |id| {
                                extension_editors
                                    .iter()
                                    .find(|editor| editor.id == id)
                                    .map_or_else(
                                        || "Extension".into(),
                                        |editor| editor.title.clone(),
                                    )
                            },
                        );
                        ui.menu_button(active_label, |ui| {
                            for editor in AudioSubEditor::ALL {
                                if ui
                                    .selectable_label(state.sub_editor == editor, editor.label())
                                    .clicked()
                                {
                                    state.switch_sub_editor(editor);
                                    ui.close();
                                }
                            }
                            if !extension_editors.is_empty() {
                                ui.separator();
                                for editor in &extension_editors {
                                    let selected =
                                        state.sub_editor.extension_id() == Some(editor.id.as_str());
                                    if ui.selectable_label(selected, &editor.title).clicked() {
                                        state.switch_sub_editor(AudioSubEditor::Extension(
                                            editor.id.clone(),
                                        ));
                                        ui.close();
                                    }
                                }
                            }
                        })
                        .response
                        .on_hover_text("Audio editor surface");
                    },
                );
                let clipped = painter.with_clip_rect(content);
                let bypass = snap.bypassed(ui.input(|input| input.modifiers));
                let slices = editor_slices(&clip, &audio);
                let slice_top = lanes.top() + 8.0;
                let slice_bottom = lanes.bottom() - 8.0;

                if clip_start_x > lanes.left() {
                    clipped.rect_filled(
                        Rect::from_min_max(lanes.min, egui::pos2(clip_start_x, lanes.bottom())),
                        CornerRadius::ZERO,
                        Color32::from_black_alpha(110),
                    );
                }
                if clip_end_x < lanes.right() {
                    clipped.rect_filled(
                        Rect::from_min_max(egui::pos2(clip_end_x, lanes.top()), lanes.max),
                        CornerRadius::ZERO,
                        Color32::from_black_alpha(110),
                    );
                }
                timeline_grid.draw(&clipped, lanes);

                if state.sub_editor == AudioSubEditor::AudioEvents {
                    let bg_resp = ui.interact(
                        lanes,
                        Id::new(("clip-editor-bg", state.clip_id)),
                        Sense::click_and_drag(),
                    );
                    handle_background(
                        ui,
                        &mut state,
                        &bg_resp,
                        timeline_grid,
                        content.left(),
                        snap,
                    );

                    let drag_pointer = ui
                        .input(|input| input.pointer.hover_pos())
                        .or_else(|| state.drag.map(|drag| drag.origin_pointer));
                    let drag_delta = state.drag.map_or(0.0, |drag| {
                        drag_pointer.map_or(0.0, |pointer| {
                            f64::from(pointer.x - drag.origin_pointer.x) / f64::from(ppb)
                        })
                    });
                    let starts = group_starts(&slices, &state.selected, state.drag);
                    let node_shift = state.drag.map_or(TimeMus::zero(), |drag| {
                        group_move_shift(
                            drag,
                            drag_delta,
                            clip_start,
                            grid,
                            bypass,
                            TimeMus::zero() - display_range.start,
                            &starts,
                        )
                    });
                    let bar_preview_shift = preview_bar_drag(&mut state, ui, grid, bypass, ppb);

                    handle_bar_drag(
                        self,
                        ui,
                        &mut state,
                        &mut audio,
                        &clip,
                        &map,
                        &slices,
                        timeline_grid,
                        content.left(),
                        lanes,
                        ppb,
                        grid,
                        bypass,
                    );

                    for slice in &slices {
                        let geometry = slice_geometry(
                            slice,
                            state.drag,
                            drag_delta,
                            node_shift,
                            &map,
                            clip_start,
                            grid,
                            bypass,
                            TimeMus::zero() - display_range.start,
                            state.selected.contains(&slice.id),
                        );
                        let start = if state
                            .bar_drag
                            .as_ref()
                            .is_some_and(|drag| drag.slices.contains(&slice.id))
                        {
                            slice.start + bar_preview_shift
                        } else {
                            geometry.start
                        };
                        let selected = state.selected.contains(&slice.id);
                        let paint = draw_slice(
                            &painter,
                            doc,
                            runtime,
                            &clip,
                            &audio,
                            slice,
                            &geometry,
                            start,
                            timeline_grid,
                            content.left(),
                            clip_start,
                            &map,
                            slice_top,
                            slice_bottom,
                            lanes,
                            selected,
                        );
                        handle_slice(
                            self,
                            ui,
                            &painter,
                            &mut state,
                            &mut audio,
                            &slices,
                            &map,
                            &clip,
                            slice,
                            &paint,
                            ppb,
                            grid,
                            snap,
                            timeline_grid,
                            content.left(),
                            bypass,
                        );
                    }
                } else if state.sub_editor == AudioSubEditor::Gain {
                    super::curve::draw_curve_sub_editor(
                        self,
                        ui,
                        &mut state,
                        &mut audio,
                        &clip,
                        runtime,
                        map,
                        timeline_grid,
                        lanes,
                        notwig_ext_api::CurveTarget::Gain,
                        super::curve::gain_style(),
                    );
                } else if let Some(spec) = state
                    .sub_editor
                    .extension_id()
                    .and_then(|id| extension_editors.iter().find(|editor| editor.id == id))
                    .and_then(notwig_ext_api::AudioSubEditorDescriptor::curve)
                {
                    super::curve::draw_curve_sub_editor(
                        self,
                        ui,
                        &mut state,
                        &mut audio,
                        &clip,
                        runtime,
                        map,
                        timeline_grid,
                        lanes,
                        spec.target,
                        super::curve::extension_style(spec),
                    );
                }

                if let Some(preview) = &self.drag_preview
                    && matches!(
                        preview.target,
                        crate::intent::DropAudioTarget::ClipEditor { clip_id }
                            if clip_id == state.clip_id
                    )
                {
                    let x1 = timeline_grid.x_for_beat(clip_start + preview.at, content.left());
                    let x2 = timeline_grid
                        .x_for_beat(clip_start + preview.at + preview.duration, content.left());
                    let rect = Rect::from_min_max(
                        egui::pos2(x1, slice_top),
                        egui::pos2(x2.max(x1 + 2.0), slice_bottom),
                    )
                    .intersect(lanes);
                    crate::timeline::draw_drop_preview(
                        &painter,
                        rect,
                        &preview.path,
                        preview.analyzing,
                    );
                }

                let mut chrome_drag = state.edge_drag.map(|drag| ClipChromeDrag {
                    edge: match drag.edge {
                        ClipEdge::Start => ClipChromeEdge::Start,
                        ClipEdge::End => ClipChromeEdge::End,
                    },
                    start_pointer: drag.start_pointer,
                    origin: drag.origin,
                });
                let was_dragging = chrome_drag.is_some();
                let chrome = crate::timeline::draw_clip_chrome(
                    ui,
                    &clipped,
                    ClipChrome {
                        grid: timeline_grid,
                        content,
                        ruler,
                        lanes,
                        range: display_range,
                        origin: clip.range,
                        id_salt: Id::new(("clip-editor-clip-edge", state.clip_id)),
                        ppb,
                        snap_step: grid,
                        bypass,
                        start_label: "",
                        end_label: "",
                        start_color: crate::timeline::clip_color(doc, &clip),
                        end_color: crate::timeline::clip_color(doc, &clip),
                    },
                    &mut chrome_drag,
                );
                state.edge_drag = chrome_drag.map(|drag| ClipEdgeDrag {
                    edge: match drag.edge {
                        ClipChromeEdge::Start => ClipEdge::Start,
                        ClipChromeEdge::End => ClipEdge::End,
                    },
                    start_pointer: drag.start_pointer,
                    origin: drag.origin,
                });
                if let Some(range) = chrome.preview {
                    state.preview_range = Some(range);
                }
                if was_dragging && state.edge_drag.is_none() {
                    state.preview_range = None;
                }
                if let Some(range) = chrome.committed {
                    self.emit(UiIntent::SetClipRange {
                        clip_id: state.clip_id,
                        range,
                    });
                }

                draw_selection(
                    &clipped,
                    &state,
                    timeline_grid,
                    content.left(),
                    lanes,
                    ruler,
                    content,
                );
                if runtime.playing {
                    crate::timeline::draw_playhead(
                        &clipped,
                        lanes,
                        content.left(),
                        timeline_grid,
                        runtime.playhead,
                    );
                }

                let event_end = slices
                    .iter()
                    .map(|slice| clip.range.start + slice.start + source_duration(&map, slice))
                    .max()
                    .unwrap_or(clip.range.end);
                let content_end = event_end.max(clip.range.end) + TimeMus::new(4, 1);
                clip_editor_scrollbar(
                    ui,
                    &mut state,
                    &mut snap,
                    rect,
                    content,
                    content_end,
                    ppb,
                    bar_length,
                );
                self.state.snap = snap;
                true
            },
        );

        if open {
            self.clip_editor = Some(state);
        } else {
            self.close_clip_editor();
        }
    }
}

impl NotwigUi {
    fn clip_editor_placeholder(&mut self, ctx: &egui::Context) {
        let open = crate::views::panel::bottom_panel(
            ctx,
            crate::views::panel::PanelConfig {
                id: "clip_editor_panel",
                resizable: true,
                default_height: 260.0,
                min_height: 80.0,
                max_height: 700.0,
                collapse_height: 120.0,
                handle_id: "clip-editor-panel",
                handle_tooltip: "Drag down to close",
            },
            |ui| {
                ui.label(
                    egui::RichText::new("Select an Arrangement Clip to edit.")
                        .color(theme::TEXT_MUTED),
                );
                true
            },
        );
        if !open {
            self.close_editor();
        }
    }
}
