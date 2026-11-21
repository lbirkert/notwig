use notwig_core::TimeMus;

/// Note-value relative to a whole note (e.g. `1/16` is a sixteenth note).
///
/// `TimeMus` counts quarter notes, so a whole note is four beats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SnapLevel {
    numerator: u32,
    denominator: u32,
}

impl SnapLevel {
    #[must_use]
    pub const fn new(numerator: u32, denominator: u32) -> Self {
        Self {
            numerator,
            denominator,
        }
    }

    /// Length in beats (quarter notes).
    #[must_use]
    pub fn beats(self) -> f64 {
        4.0 * f64::from(self.numerator) / f64::from(self.denominator)
    }

    #[must_use]
    pub fn step(self) -> TimeMus {
        beats_to_time(self.beats())
    }

    #[must_use]
    pub fn label(self) -> String {
        format!("{}/{}", self.numerator, self.denominator)
    }

    #[must_use]
    pub fn from_beats(beats: f64) -> Option<Self> {
        let step = beats_to_time(beats);
        SNAP_LEVELS
            .iter()
            .copied()
            .find(|level| level.step() == step)
    }
}

/// Selectable snap levels, ordered from the coarsest to the finest.
pub const SNAP_LEVELS: &[SnapLevel] = &[
    SnapLevel::new(2, 1),
    SnapLevel::new(1, 1),
    SnapLevel::new(1, 2),
    SnapLevel::new(1, 4),
    SnapLevel::new(1, 8),
    SnapLevel::new(1, 16),
    SnapLevel::new(1, 32),
    SnapLevel::new(1, 64),
    SnapLevel::new(1, 128),
    SnapLevel::new(1, 256),
    SnapLevel::new(1, 512),
    SnapLevel::new(1, 1024),
    SnapLevel::new(1, 2048),
    SnapLevel::new(1, 4096),
    SnapLevel::new(1, 8192),
    SnapLevel::new(1, 16384),
    SnapLevel::new(1, 32768),
    SnapLevel::new(1, 65536),
];

/// Minimum spacing, in pixels, for an adaptive grid line.
///
/// This is deliberately larger than the old 12 px threshold so adaptive mode
/// shows fewer, larger subdivisions at the same zoom level.
const ADAPTIVE_MIN_PIXELS: f32 = 40.0;

/// Global snap configuration shared by every timeline editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapSettings {
    pub enabled: bool,
    pub adaptive: bool,
    pub level: SnapLevel,
}

impl Default for SnapSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            adaptive: true,
            level: SnapLevel::new(1, 16),
        }
    }
}

impl SnapSettings {
    #[must_use]
    pub fn step(self, pixels_per_beat: f32, beats_per_bar: u8) -> TimeMus {
        if self.adaptive {
            adaptive_step(pixels_per_beat, beats_per_bar)
        } else {
            self.level.step()
        }
    }

    /// Step used to paint the grid. Fine manual levels are snapped exactly,
    /// but the visible grid stays readable instead of drawing thousands of
    /// lines.
    #[must_use]
    pub fn display_step(self, pixels_per_beat: f32, beats_per_bar: u8) -> TimeMus {
        self.step(pixels_per_beat, beats_per_bar)
            .max(adaptive_step(pixels_per_beat, beats_per_bar))
    }

    #[must_use]
    pub fn snap(
        self,
        time: TimeMus,
        pixels_per_beat: f32,
        beats_per_bar: u8,
        bypass: bool,
    ) -> TimeMus {
        if !self.enabled || bypass {
            return time;
        }
        snap_time(time, self.step(pixels_per_beat, beats_per_bar), false)
    }

    /// Returns `true` when a gesture must not snap.
    ///
    /// Shift is the universal bypass; Alt remains supported because the clip
    /// and audio editors already document it. Turning snap off disables it for
    /// every editor at once.
    #[must_use]
    pub fn bypassed(self, modifiers: egui::Modifiers) -> bool {
        !self.enabled || modifiers.shift || modifiers.alt
    }
}

/// Chooses the finest grid division that still leaves at least
/// [`ADAPTIVE_MIN_PIXELS`] between lines.
#[must_use]
pub fn adaptive_step(pixels_per_beat: f32, beats_per_bar: u8) -> TimeMus {
    let ppb = pixels_per_beat.max(0.0);
    let bar = f64::from(beats_per_bar.max(1));
    let mut candidates: Vec<f64> = SNAP_LEVELS.iter().map(|level| level.beats()).collect();
    for bars in [1.0_f64, 2.0, 4.0, 8.0] {
        candidates.push(bar * bars);
    }
    candidates.sort_by(f64::total_cmp);
    let chosen = candidates
        .iter()
        .copied()
        .find(|step| *step as f32 * ppb >= ADAPTIVE_MIN_PIXELS)
        .unwrap_or_else(|| *candidates.last().expect("snap candidates are never empty"));
    beats_to_time(chosen)
}

#[must_use]
pub fn snap_step(pixels_per_beat: f32, beats_per_bar: u8) -> TimeMus {
    adaptive_step(pixels_per_beat, beats_per_bar)
}

#[must_use]
pub fn snap_time(time: TimeMus, step: TimeMus, bypass: bool) -> TimeMus {
    if bypass || step.is_zero() {
        return time;
    }
    let beats = time.as_f64();
    let step_beats = step.as_f64();
    let snapped = (beats / step_beats).round() * step_beats;
    beats_to_time(snapped)
}

#[must_use]
pub fn snap_step_label(step: TimeMus, beats_per_bar: u8) -> String {
    let beats = step.as_f64();
    if let Some(level) = SnapLevel::from_beats(beats) {
        return level.label();
    }
    let bar = f64::from(beats_per_bar.max(1));
    let bars = (beats / bar).round();
    if bars >= 1.0 && (beats - bar * bars).abs() < 1e-6 {
        if (bars - 1.0).abs() < 1e-6 {
            "1 bar".into()
        } else {
            format!("{} bars", bars as i64)
        }
    } else {
        format!("{beats:.3} beats")
    }
}

fn beats_to_time(beats: f64) -> TimeMus {
    TimeMus::new((beats * 1_000_000.0).round() as i64, 1_000_000)
}
