# Extensions

Status: superseded. The authoritative design is
[`docs/03_development/19_extension_system.md`](../docs/03_development/19_extension_system.md).

The earlier v1 plan described JSON wire messages, command draw IR, and a single
`#[notwig_extension]` entrypoint. Those descriptions are obsolete. The current
system uses ABI version 9, preload/load/unload phases, runtime registration
manifests, typed `#[notwig_ext_trait]` interfaces with bitcode RPC, guest egui
panels, bottom panels, and a replaceable clipboard point.

Remaining extension-platform work:

- package-side typed RPC invoke export, typed manifest entries, and the host
  scheduler;
- many-cardinality handle targeting;
- replacement edges, user load order, generations, and stale-handle diagnostics;
- package clipboard providers;
- general arranger/clip/track drawing hooks.

Per-feature plans remain in `plan/EXTENSION_CHAINS.md`,
`plan/EXTENSION_DEVICES.md`, and `plan/MOVING_DEVICES.md`.
