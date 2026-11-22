use notwig_ext_api::egui;
use egui::{Color32, CornerRadius, Rect, Stroke, StrokeKind};
use notwig_ui::palette;

/// Number of frames retained by [`History`].
pub const HISTORY_LEN: usize = 50;

/// Lowest level, in dBFS, shown by meters and history.
pub const METER_MIN_DB: f32 = -60.0;

const GRAPH_MIN_DB: f32 = -60.0;
const GRAPH_MAX_DB: f32 = 0.0;

fn signal_fill() -> Color32 {
    Color32::from_rgba_unmultiplied(96, 224, 184, 56)
}

const SIGNAL_STROKE: Color32 = Color32::from_rgb(96, 224, 184);

fn threshold_glow() -> Color32 {
    Color32::from_rgba_unmultiplied(86, 156, 255, 80)
}

const THRESHOLD_STROKE: Color32 = Color32::from_rgb(86, 156, 255);

fn gain_stroke() -> Color32 {
    Color32::from_rgba_unmultiplied(255, 190, 90, 220)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HistorySample {
    pub level_db: f32,
    pub gain_db: f32,
}

impl Default for HistorySample {
    fn default() -> Self {
        Self {
            level_db: GRAPH_MIN_DB,
            gain_db: 0.0,
        }
    }
}

/// Fixed-size ring of recent peak and gain values.
#[derive(Debug, Clone)]
pub struct History {
    samples: [HistorySample; HISTORY_LEN],
    next: usize,
    len: usize,
}

impl Default for History {
    fn default() -> Self {
        Self {
            samples: [HistorySample::default(); HISTORY_LEN],
            next: 0,
            len: 0,
        }
    }
}

impl History {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, sample: HistorySample) {
        self.samples[self.next] = sample;
        self.next = (self.next + 1) % HISTORY_LEN;
        self.len = (self.len + 1).min(HISTORY_LEN);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Iterates oldest to newest.
    pub fn iter(&self) -> impl Iterator<Item = HistorySample> + '_ {
        let start = (self.next + HISTORY_LEN - self.len) % HISTORY_LEN;
        (0..self.len).map(move |index| self.samples[(start + index) % HISTORY_LEN])
    }
}

/// How the threshold line is drawn across the history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThresholdCurve {
    /// A flat line at the configured threshold.
    Fixed,
    /// The threshold shifted down by the gain reduction at each point.
    FollowGain,
}

/// Converts a linear amplitude to dBFS with a -120 dB floor.
#[must_use]
pub fn linear_to_db(value: f32) -> f32 {
    20.0 * value.max(1e-6).log10()
}

/// Rises instantly to a new peak and falls by `decay_db` per frame otherwise.
#[must_use]
pub fn follow_peak(current_db: f32, target_db: f32, decay_db: f32) -> f32 {
    if target_db >= current_db {
        target_db
    } else {
        (current_db - decay_db).max(METER_MIN_DB)
    }
}

/// Paints a peak-history graph with a threshold overlay and optional gain line.
pub fn paint_history_graph(
    painter: &egui::Painter,
    rect: Rect,
    history: &History,
    threshold_db: f32,
    curve: ThresholdCurve,
    show_gain: bool,
) {
    let graph = rect.shrink(1.0);
    painter.rect_filled(rect, CornerRadius::same(4), palette::PANEL);
    painter.rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0_f32, palette::GRID),
        StrokeKind::Inside,
    );

    for db in [-48.0, -36.0, -24.0, -12.0] {
        let y = db_to_y(graph, db);
        painter.line_segment(
            [egui::pos2(graph.min.x, y), egui::pos2(graph.max.x, y)],
            Stroke::new(1.0_f32, palette::GRID),
        );
    }

    let mut samples = [HistorySample::default(); HISTORY_LEN];
    let offset = HISTORY_LEN - history.len();
    for (index, sample) in history.iter().enumerate() {
        samples[offset + index] = sample;
    }

    let step = graph.width() / (HISTORY_LEN - 1) as f32;
    let x_at = |slot: usize| graph.min.x + slot as f32 * step;

    let peak: Vec<egui::Pos2> = samples
        .iter()
        .enumerate()
        .map(|(index, sample)| egui::pos2(x_at(index), db_to_y(graph, sample.level_db)))
        .collect();
    for (index, point) in peak.iter().enumerate() {
        let left = if index == 0 {
            graph.min.x
        } else {
            f32::midpoint(x_at(index - 1), x_at(index))
        };
        let right = if index + 1 == peak.len() {
            graph.max.x
        } else {
            f32::midpoint(x_at(index), x_at(index + 1))
        };
        painter.rect_filled(
            Rect::from_min_max(egui::pos2(left, point.y), egui::pos2(right, graph.max.y)),
            CornerRadius::ZERO,
            signal_fill(),
        );
    }
    painter.add(egui::Shape::line(
        peak,
        Stroke::new(1.5_f32, SIGNAL_STROKE),
    ));

    let threshold: Vec<egui::Pos2> = samples
        .iter()
        .enumerate()
        .map(|(index, sample)| {
            let db = match curve {
                ThresholdCurve::Fixed => threshold_db,
                ThresholdCurve::FollowGain => threshold_db + sample.gain_db,
            };
            egui::pos2(x_at(index), db_to_y(graph, db))
        })
        .collect();
    painter.add(egui::Shape::line(
        threshold.clone(),
        Stroke::new(4.0_f32, threshold_glow()),
    ));
    painter.add(egui::Shape::line(
        threshold,
        Stroke::new(1.5_f32, THRESHOLD_STROKE),
    ));

    if show_gain {
        let gain: Vec<egui::Pos2> = samples
            .iter()
            .enumerate()
            .map(|(index, sample)| egui::pos2(x_at(index), db_to_y(graph, sample.gain_db)))
            .collect();
        painter.add(egui::Shape::line(gain, Stroke::new(1.0_f32, gain_stroke())));
    }
}

fn db_to_y(rect: Rect, db: f32) -> f32 {
    let norm = ((db - GRAPH_MIN_DB) / (GRAPH_MAX_DB - GRAPH_MIN_DB)).clamp(0.0, 1.0);
    rect.max.y - rect.height() * norm
}
