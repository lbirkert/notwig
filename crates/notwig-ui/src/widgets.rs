use egui::{Color32, CornerRadius, Ui};
use notwig_core::{Db, TimeMus};
use notwig_dsp::Unit;

use crate::theme;

pub fn snap_16(time: TimeMus, bypass: bool) -> TimeMus {
    const GRID: f64 = 16.0;
    if bypass {
        return time;
    }
    let beats = time.as_f64();
    let snapped = (beats * GRID).round() / GRID;
    TimeMus::new((snapped * 1_000_000.0).round() as i64, 1_000_000)
}

pub fn format_beats(time: TimeMus, beats_per_bar: u8) -> String {
    let beats = time.as_f64().max(0.0);
    let bar = (beats / f64::from(beats_per_bar.max(1))).floor() as u64 + 1;
    let beat_in_bar = beats % f64::from(beats_per_bar.max(1));
    let beat = beat_in_bar.floor() as u64 + 1;
    let sixteenth = ((beat_in_bar.fract()) * 4.0).floor() as u64 + 1;
    format!("{bar}.{beat}.{sixteenth}")
}

pub fn format_db(value: Db) -> String {
    if value.0 <= -95.0 {
        "-inf".into()
    } else {
        format!("{:+.1} dB", value.0)
    }
}

pub fn format_param(unit: Unit, value: f64) -> String {
    match unit {
        Unit::Decibels => format!("{value:+.2} dB"),
        Unit::Hertz => {
            if value >= 1000.0 {
                format!("{:.2} kHz", value / 1000.0)
            } else {
                format!("{value:.1} Hz")
            }
        }
        Unit::Percent => format!("{:.0}%", value * 100.0),
        Unit::Seconds => format!("{value:.3} s"),
        Unit::Semitones => format!("{value:+.1} st"),
        Unit::None => format!("{value:.2}"),
    }
}

pub fn small_toggle(ui: &mut Ui, label: &str, active: &mut bool, color: Color32) -> bool {
    let mut changed = false;
    let text = egui::RichText::new(label).color(if *active {
        Color32::BLACK
    } else {
        theme::TEXT_MUTED
    });
    let button = egui::Button::new(text)
        .fill(if *active { color } else { theme::PANEL_LIGHT })
        .corner_radius(CornerRadius::same(3));
    if ui.add_sized([18.0, 16.0], button).clicked() {
        *active = !*active;
        changed = true;
    }
    changed
}
