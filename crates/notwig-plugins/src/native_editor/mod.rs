#![allow(unsafe_code)]

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
mod appkit;
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
mod space_monitor;

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
pub use appkit::{prepare_window_host, pump_window_host};
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
pub use stub::{NativePluginWindow, prepare_window_host, pump_window_host};
