//! Audio Unit v2 host for the notwig-rack framework. Apple-only.
//!
//! Built on `objc2-audio-toolbox` — the modern Rust bindings
//! that replace the C++ `AudioToolbox` scanner the legacy rack 0.4
//! relied on. No cmake, no bridging-header step.
//!
//! # Status
//!
//! - **Scan** via `AudioComponentFindNext` across the AU v2 type
//!   families (Effect, `MusicDevice`, Generator, Mixer, `MusicEffect`,
//!   `MIDIProcessor`). Each match yields one [`PluginInfo`] with the
//!   AU's four-CC packed as a `"type:sub:mfr"` `unique_id`.
//! - **Load** via `AudioComponentInstanceNew`.
//! - **Process** via `AudioUnitRender` into a per-block
//!   `AudioBufferList` whose buffers point directly at the host's
//!   output planes (see [`AuPlugin::process`]).
//! - **MIDI** via `MusicDeviceMIDIEvent`. Routes channel-voice
//!   messages with sample-accurate offsets; raw bytes pass through
//!   the same call. Non-MIDI-accepting effects no-op silently.
//! - **Editor** via the `AUv2` Cocoa view (`kAudioUnitProperty_CocoaUI`).
//!
//! AU v3 plugins that set `kAudioComponentFlag_RequiresAsyncInstantiation`
//! still need the async path and are not yet loadable through this
//! crate; notwig-rack-au3 forwards them here regardless.

#![cfg(target_vendor = "apple")]

#[cfg(feature = "ara")]
mod ara;
mod editor;
mod params;
mod plugin;
mod process;
mod render;
mod scan;
mod transport;

pub use plugin::AuPlugin;
pub use scan::{AuScanner, FORMAT};

#[cfg(test)]
pub(crate) use objc2_audio_toolbox::AudioComponentDescription;
#[cfg(test)]
pub(crate) use scan::{format_unique_id, four_cc, parse_four_cc, parse_unique_id};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_cc_roundtrip() {
        let aufx = u32::from_be_bytes(*b"aufx");
        assert_eq!(four_cc(aufx), "aufx");
        assert_eq!(parse_four_cc("aufx"), Some(aufx));
    }

    #[test]
    fn unique_id_roundtrip() {
        let desc = AudioComponentDescription {
            componentType: u32::from_be_bytes(*b"aufx"),
            componentSubType: u32::from_be_bytes(*b"dely"),
            componentManufacturer: u32::from_be_bytes(*b"appl"),
            componentFlags: 0,
            componentFlagsMask: 0,
        };
        let id = format_unique_id(&desc);
        assert_eq!(id, "aufx:dely:appl");
        let parsed = parse_unique_id(&id).unwrap();
        assert_eq!(parsed.componentType, desc.componentType);
        assert_eq!(parsed.componentSubType, desc.componentSubType);
        assert_eq!(parsed.componentManufacturer, desc.componentManufacturer);
    }
}
