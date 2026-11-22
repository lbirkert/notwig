//! Reusable egui controls shared by the app UI and extension panels.
//!
//! These components depend only on `egui` and `notwig-core`, so extension
//! devices can link them without pulling in the application shell.

mod controls;
mod fader;
mod format;
mod knob;
mod meter;
mod value_drag;

pub use controls::{button, divider, dropdown, label, section, toggle};
pub use fader::{DB_MAX, DB_MIN, volume_fader};
pub use format::{format_db, format_param};
pub use knob::knob;
pub use meter::{draw_stereo_meter, draw_stereo_meter_with_peak};
pub use value_drag::value_drag;
