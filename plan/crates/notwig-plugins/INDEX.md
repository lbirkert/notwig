# `notwig-plugins` Planning

Format-agnostic plugin hosting crate (v0 in-process).

## Contracts
- [`PLUGINS.md`](./PLUGINS.md) — adapter abstraction, supported formats (v0), lifecycle, capabilities, state, threading/safety.

## Boundaries
- Depends on `notwig-dsp`, `notwig-core` (types/IDs).
- May depend on [truce-rack](https://github.com/truce-audio/truce-rack) (VST3/AU/CLAP loading) behind `HostAdapter`, subject to license/API/RT-safety review.
- Discovery/scan/load/editor non-RT; RT-safe interaction limited to process/param/event delivery per adapter contract.
