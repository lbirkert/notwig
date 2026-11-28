#[cfg(all(
    target_os = "macos",
    any(
        feature = "vst3",
        feature = "vst2",
        feature = "au",
        feature = "au3",
        feature = "clap",
        feature = "lv2"
    )
))]
mod editor;

#[cfg(all(
    target_os = "macos",
    any(
        feature = "vst3",
        feature = "vst2",
        feature = "au",
        feature = "au3",
        feature = "clap",
        feature = "lv2"
    )
))]
pub use editor::NativePluginWindow;

#[cfg(not(all(
    target_os = "macos",
    any(
        feature = "vst3",
        feature = "vst2",
        feature = "au",
        feature = "au3",
        feature = "clap",
        feature = "lv2"
    )
)))]
mod stub;
#[cfg(not(all(
    target_os = "macos",
    any(
        feature = "vst3",
        feature = "vst2",
        feature = "au",
        feature = "au3",
        feature = "clap",
        feature = "lv2"
    )
)))]
pub use stub::NativePluginWindow;

pub use notwig_rack_editor::{prepare_window_host, pump_window_host};
