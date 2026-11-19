use egui::{Color32, Rect, RichText, Sense, Stroke, StrokeKind, pos2};

use crate::runtime::{DspAnalysisView, RuntimeView};
use crate::shell::NotwigUi;
use crate::theme;

const BARS: usize = 200;
const DSP_BLUE: Color32 = Color32::from_rgb(80, 145, 230);
const CHART_BG: Color32 = Color32::from_rgb(7, 8, 11);
const CHART_GRID: Color32 = Color32::from_rgb(20, 22, 28);

impl NotwigUi {
    pub(crate) fn dsp_window(&mut self, ctx: &egui::Context, runtime: &RuntimeView) {
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.dsp_open = false;
            return;
        }
        let mut open = self.dsp_open;
        let size = egui::vec2(900.0, 450.0);
        let default_pos = ctx.screen_rect().center() - size * 0.5;
        egui::Window::new("DSP Load")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .movable(true)
            .default_size(size)
            .default_pos(default_pos)
            .min_size([900.0, 450.0])
            .show(ctx, |ui| {
                dsp_content(ui, &runtime.dsp);
            });
        self.dsp_open = open;
    }
}

fn dsp_content(ui: &mut egui::Ui, dsp: &DspAnalysisView) {
    let chart_height = (ui.available_height() - 130.0).max(140.0);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), chart_height),
        Sense::hover(),
    );
    draw_chart(ui, rect, dsp);
    ui.add_space(12.0);
    draw_stats(ui, dsp);
}

fn draw_chart(ui: &egui::Ui, rect: Rect, dsp: &DspAnalysisView) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::CornerRadius::same(2), CHART_BG);

    let grid = Stroke::new(1.0_f32, CHART_GRID);
    for percent in [25.0_f32, 50.0, 75.0, 100.0] {
        let y = rect.bottom() - rect.height() * percent / 100.0;
        painter.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], grid);
    }
    painter.line_segment(
        [
            pos2(rect.left(), rect.bottom() - 0.5),
            pos2(rect.right(), rect.bottom() - 0.5),
        ],
        Stroke::new(1.0_f32, theme::GRID),
    );
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(2),
        Stroke::new(1.0_f32, theme::GRID),
        StrokeKind::Inside,
    );

    let count = dsp.history.len().min(BARS);
    if count == 0 {
        return;
    }
    let start = dsp.history.len() - count;
    let slot = rect.width() / BARS as f32;
    let bar_width = (slot * 0.72).max(1.0);
    let inset = (slot - bar_width) * 0.5;
    for (index, value) in dsp.history[start..].iter().enumerate() {
        let right = rect.right() - (count - 1 - index) as f32 * slot - inset;
        let left = right - bar_width;
        let height = rect.height() * (value / 100.0).clamp(0.0, 1.0);
        if height < 0.5 {
            continue;
        }
        let bar = Rect::from_min_max(
            pos2(left, rect.bottom() - height),
            pos2(right, rect.bottom() - 1.0),
        );
        painter.rect_filled(bar, egui::CornerRadius::ZERO, DSP_BLUE);
    }
}

fn draw_stats(ui: &mut egui::Ui, dsp: &DspAnalysisView) {
    let stats = [
        ("Block", format!("{} samples", dsp.block_size)),
        ("Samplerate", format!("{} Hz", dsp.sample_rate)),
        ("Deadline", format!("{:.3} ms", dsp.deadline_ms)),
        ("Load (AVG)", format!("{:.3} ms", dsp.load_avg_ms)),
        ("Load (MAX)", format!("{:.3} ms", dsp.load_max_ms)),
        (
            "Period jitter",
            format!("{:.2} %", dsp.period_jitter_percent),
        ),
        (
            "Audio Input latency",
            format!("{} samples", dsp.input_latency_samples),
        ),
        (
            "Audio Output latency",
            format!("{} samples", dsp.output_latency_samples),
        ),
        ("Timing", dsp.timing.clone()),
    ];
    egui::Frame::new()
        .fill(Color32::from_rgb(16, 17, 21))
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (label, value) in stats {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(label).small().color(theme::TEXT_MUTED));
                        ui.label(RichText::new(value).monospace().strong());
                    });
                    ui.add_space(16.0);
                }
            });
        });
}
