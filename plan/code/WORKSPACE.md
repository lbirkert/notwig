# Cargo Workspace Contracts

## 1. Language & Workspace
- Language: Rust (stable toolchain; edition set by workspace manifest).
- Single Cargo workspace at repository root: `W = (root, Members, Resolver, Profiles)`.
- Production code in workspace member crates under `crates/<name>/`.

## 2. Member Crates (v0)
`Members = { notwig-core, notwig-project, notwig-dsp, notwig-engine, notwig-plugins, notwig-ui, notwig-app }`
- Paths: `crates/notwig-core` ... `crates/notwig-app`.
- `notwig-app` is the binary/entry crate; others libraries unless specified.

## 3. Dependency Constraints
`D ⊆ Members × Members` MUST be acyclic.

Allowed direction (`A -> B`):
- `notwig-app -> {notwig-ui, notwig-core, notwig-project, notwig-engine, notwig-plugins, notwig-dsp}`
- `notwig-ui -> {notwig-core, notwig-project, notwig-dsp}`
- `notwig-engine -> {notwig-core, notwig-dsp, notwig-plugins}`
- `notwig-plugins -> {notwig-dsp, notwig-core}`
- `notwig-project -> {notwig-core}`
- `notwig-dsp -> {notwig-core}`
- `notwig-core -> ∅` (workspace-internal)

Forbidden: `notwig-core` depending on any other member; `notwig-ui` depending on engine RT internals or doing audio callback work; any cycle.

## 4. External Dependency Policy
- Prefer mature crates for audio IO, MIDI, serialization, plugin hosting (VST3/AU primary, VST2 fallback), and Faust/libfaust integration, subject to license/API/RT-safety evaluation.
- Candidate plugin-hosting dependency: [truce-audio/truce-rack](https://github.com/truce-audio/truce-rack) (VST3/AU/CLAP). Adopt only after license/API/RT-safety review; confine it behind `notwig-plugins`' `HostAdapter`.
- RT-path code MUST restrict dependencies to RT-safe usage or confine them to non-RT boundaries.
- Prefer workspace-level dependency/version management for consistency.

## 5. Features & Profiles
- Feature flags gate optional formats/backends/tools without breaking core contracts.
- Profiles: `dev`, `release` (+ test/bench as needed). Correctness/RT-safety prioritized.
- DSP compile caches and scan caches are regenerable (see crate plans).

## 6. Platform & Format Features (v0)
- macOS first; Windows/Linux deferred. Platform-specific audio/plugin code sits behind traits/adapters so RT code stays platform-agnostic.
- Feature flags gate optional formats/backends: `au` (macOS), `vst3`, `vst2` (where available), and Faust/DSP compilation.
- `wgpu`/`egui` UI and the RT engine build on macOS; no OS-specific types cross crate boundaries.

## 7. Mapping to Planning
- Detailed per-crate contracts: `plan/crates/notwig-*/`
- Workspace-level boundaries: this file and `CRATES.md`.
