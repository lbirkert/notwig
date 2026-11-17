# Plugin Hosting (`notwig-plugins`, v0)

## 1. Abstraction
- Format-agnostic host interface `HostAdapter<F>`; DAW core/engine integrate via adapter/device contracts, not format APIs.
- Plugin instances surface as devices with `typeId` namespaced by format+uid (e.g. `vst3.<uid>`, `au.<uid>`).
- `notwig-plugins` MAY depend on [truce-audio/truce-rack](https://github.com/truce-audio/truce-rack) (VST3/AU/CLAP) for plugin loading; the `HostAdapter` boundary keeps the dependency swappable and prevents format APIs leaking into core/engine. If adopted, no clean-room reimplementation of VST3/AU loading plumbing is required.

## 2. Supported Formats & Platforms (v0)
- **Primary:** VST3 (cross-platform), Audio Units (AU, macOS).
- **Fallback/compat:** VST2 (where legally/technically available) for legacy plugins.
- **Platform:** macOS first; adapters keep Windows/Linux portable. Feature flags gate formats.
- In-process hosting only in v0. Out-of-process sandboxing/crash isolation deferred (boundaries kept separable).
- Instrument tracks use hosted instruments only; v0 ships no stock instrument. Clip playback supplies notes only (no channel/CC/PB/PC/sysex).
- **Candidate plugin-hosting dependency:**
  - [truce-audio/truce-rack](https://github.com/truce-audio/truce-rack) — VST3, AU, and CLAP plugin loading. MAY be used as the dependency behind the `HostAdapter` for VST3 and macOS AU. Subject to license/API review. CLAP remains out of v0 scope.
  - [hikarucorporation/hikaru-openlive](https://github.com/hikarucorporation/hikaru-openlive) — Bitwig-inspired UI/workflow reference (ideas only); v0 targets a smaller, higher-quality subset.

## 3. Lifecycle & Threading
- **Non-RT:** discovery/scan, validation, load, instantiate, describe ports/params/latency, state/preset (de)serialize, editor open/close, unload. Run on background/control threads except where platform API requires the main thread.
- **RT (engine callback):** `process`, RT-safe param/event delivery, transport queries as supported. No scan/load/editor/GUI/alloc/locks.
- **Editor:** non-RT; on macOS editors requiring the main thread are created, pumped, and destroyed on the UI/main thread. v0 uses floating OS windows (no embedded editor hosting).
- Host provides timing/transport, `sr`/block/layout notifications, buffer/event routing, latency reporting for PDC.

## 4. Capabilities & Negotiation
- Audio I/O, note I/O (as supported), parameter enumeration with mapping to `ParamSpec`, parameter value application.
- State save/restore (opaque blob keyed by format/plugin/version), preset load/save if supported.
- Latency reporting required for PDC when non-zero; the engine owns the runtime latency value.
- Unsupported feature => explicit capability flag/negotiation result and graceful degradation (not silent failure).

## 5. Discovery & Scan Policy (v0)
- Scan paths come from user settings (platform config dir) plus standard VST3/AU system locations.
- Scan/validate off-RT with per-plugin timeout; cache results keyed by format + binary path + mtime/size/hash + host ABI in the user cache dir (not the project bundle).
- Blacklist after repeated validation crashes/timeouts; duplicates resolved deterministically by `(format, path)` and conflicts reported.
- No out-of-process sandbox: a crashing plugin crashes the app; the scan blacklist prevents repeated startup crashes.

## 6. Safety, Errors, and State
- Plugins are treated as untrusted for RT behavior; the host enforces RT-safe bridges but cannot guarantee in-process plugin internals.
- Scan/instantiate/process-setup errors are isolated from the RT thread and reported to Core/UI; the engine avoids callback instability.
- Plugin state blobs are stored in the bundle `state/` area and referenced by `DeviceInstance.stateRef`; versioned/opaque per `(format, pluginId, pluginVersion)` with best-effort compatibility. Incompatible state => load warning/error with safe fallback.
- Missing/unloadable plugin instances are retained unresolved and flagged. Audio FX behave as latency-matched bypass; instruments are silent. The callback continues; relink/reload restores the device after validation.
