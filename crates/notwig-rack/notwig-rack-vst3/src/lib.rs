//! VST3 host implementation for the notwig-rack framework.
//!
//! Built on the community `vst3` Rust bindings — no Steinberg SDK
//! submodule, no cmake. A fresh checkout builds in seconds.
//!
//! # Lifecycle
//!
//! Each `.vst3` bundle exports `GetPluginFactory` (and on macOS
//! `bundleEntry` / `bundleExit`). Loading walks:
//!
//! 1. `dlopen` the binary inside the bundle.
//! 2. `bundleEntry()` on macOS (no-op elsewhere).
//! 3. `GetPluginFactory()` → `IPluginFactory`.
//! 4. `createInstance(cid, IComponent::IID)` → `IComponent`.
//! 5. `IPluginBase::initialize(host_context)` on the component.
//! 6. Cast to `IAudioProcessor`.
//! 7. `IComponent::getControllerClassId(&mut cid2)` — if separate,
//!    `factory.createInstance(cid2, IEditController::IID)` and
//!    initialize it. Otherwise cast the component itself.
//! 8. Connect the component and controller `IConnectionPoint`s
//!    (separate-controller case only).
//!
//! Activate calls `setBusArrangements`, `setupProcessing`,
//! `setActive(true)`, `setProcessing(true)`. Deactivate reverses.

mod editor;
mod events;
mod module;
mod params;
mod plugin;
mod process;
mod scanner;
mod stream;

pub use plugin::Vst3Plugin;
pub use scanner::{Vst3Scanner, default_vst3_paths};

/// Format identifier used on returned [`PluginInfo`].
pub const FORMAT: &str = "vst3";

/// Bundle directory suffix every VST3 plugin uses.
pub const VST3_EXTENSION: &str = ".vst3";

#[cfg(test)]
use scanner::{char8_array_to_string, hex_to_tuid, tuid_to_hex};
#[cfg(test)]
use vst3::Steinberg::TUID;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tuid_hex_roundtrip() {
        let cid: TUID = [
            0x01, 0x23, 0x45, 0x67, -0x10, -0x10, -0x10, -0x10, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let hex = tuid_to_hex(&cid);
        let parsed = hex_to_tuid(&hex).expect("parse");
        assert_eq!(parsed, cid);
    }

    #[test]
    fn char8_skips_trailing_nul() {
        let mut arr = [0i8; 16];
        for (i, &b) in b"Hello".iter().enumerate() {
            // ASCII byte → i8 always fits losslessly; the cast is
            // bit-pattern-identical for `b` < 128.
            arr[i] = i8::try_from(b).expect("ASCII byte fits in i8");
        }
        assert_eq!(char8_array_to_string(&arr), "Hello");
    }
}
