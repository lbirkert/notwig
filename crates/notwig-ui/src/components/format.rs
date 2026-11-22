use notwig_core::Db;
use notwig_core::dsp::Unit;

/// Formats a decibel value for compact display.
#[must_use]
pub fn format_db(value: Db) -> String {
    if value.0 <= -95.0 {
        "-inf".into()
    } else {
        format!("{:+.1} dB", value.0)
    }
}

/// Formats a parameter value using its display unit.
#[must_use]
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
