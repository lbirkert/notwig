//! Extension target identifiers and platform matching.
//!
//! `extension.json` entrypoints name a target such as `wasm` or
//! `macos-aarch64`. Manifests are enumerated in order, so a package can list
//! its preferred native build first and a portable WASM build last.

/// WebAssembly target, run by `wasmtime` on native and by the browser API on
/// the web.
pub const WASM: &str = "wasm";
/// macOS on Apple Silicon.
pub const MACOS_AARCH64: &str = "macos-aarch64";
/// macOS on Intel.
pub const MACOS_X86_64: &str = "macos-x86_64";
/// Linux on x86-64.
pub const LINUX_X86_64: &str = "linux-x86_64";
/// Linux on 64-bit ARM.
pub const LINUX_AARCH64: &str = "linux-aarch64";
/// Windows on x86-64.
pub const WINDOWS_X86_64: &str = "windows-x86_64";
/// Windows on 64-bit ARM.
pub const WINDOWS_AARCH64: &str = "windows-aarch64";

/// Every canonical target identifier.
pub const ALL: &[&str] = &[
    MACOS_AARCH64,
    MACOS_X86_64,
    LINUX_X86_64,
    LINUX_AARCH64,
    WINDOWS_X86_64,
    WINDOWS_AARCH64,
    WASM,
];

/// Returns the canonical identifier for a target, accepting common aliases
/// and Rust target triples.
#[must_use]
pub fn canonical(raw: &str) -> Option<&'static str> {
    match raw.trim() {
        WASM | "wasm32-unknown-unknown" | "wasm32" => Some(WASM),
        MACOS_AARCH64 | "macos-arm64" | "aarch64-apple-darwin" => Some(MACOS_AARCH64),
        MACOS_X86_64 | "macos-x64" | "x86_64-apple-darwin" => Some(MACOS_X86_64),
        LINUX_X86_64
        | "linux-x64_86"
        | "linux-x64"
        | "linux-amd64"
        | "x86_64-unknown-linux-gnu" => Some(LINUX_X86_64),
        LINUX_AARCH64 | "linux-arm64" | "aarch64-unknown-linux-gnu" => Some(LINUX_AARCH64),
        WINDOWS_X86_64
        | "windows-x64"
        | "windows-amd64"
        | "x86_64-pc-windows-msvc"
        | "x86_64-pc-windows-gnu" => Some(WINDOWS_X86_64),
        WINDOWS_AARCH64 | "windows-arm64" | "aarch64-pc-windows-msvc" => Some(WINDOWS_AARCH64),
        _ => None,
    }
}

/// Returns `true` when this build can load the target.
///
/// WASM is always supported: native uses `wasmtime`, the browser uses the
/// platform WebAssembly API. Native shared libraries are supported when the
/// host OS and architecture match.
#[must_use]
pub fn is_supported(raw: &str) -> bool {
    match canonical(raw) {
        Some(WASM) => true,
        Some(MACOS_AARCH64) => cfg!(all(target_os = "macos", target_arch = "aarch64")),
        Some(MACOS_X86_64) => cfg!(all(target_os = "macos", target_arch = "x86_64")),
        Some(LINUX_X86_64) => cfg!(all(target_os = "linux", target_arch = "x86_64")),
        Some(LINUX_AARCH64) => cfg!(all(target_os = "linux", target_arch = "aarch64")),
        Some(WINDOWS_X86_64) => cfg!(all(target_os = "windows", target_arch = "x86_64")),
        Some(WINDOWS_AARCH64) => cfg!(all(target_os = "windows", target_arch = "aarch64")),
        _ => false,
    }
}

/// Returns the canonical native target for this build, if any.
#[must_use]
pub const fn current() -> Option<&'static str> {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some(MACOS_AARCH64)
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some(MACOS_X86_64)
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some(LINUX_X86_64)
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some(LINUX_AARCH64)
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some(WINDOWS_X86_64)
    } else if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        Some(WINDOWS_AARCH64)
    } else {
        None
    }
}

/// Returns `true` when the target is a native shared library rather than WASM.
#[must_use]
pub fn is_native(raw: &str) -> bool {
    canonical(raw).is_some_and(|target| target != WASM)
}

/// Returns `true` for the portable WASM target.
#[must_use]
pub fn is_wasm(raw: &str) -> bool {
    canonical(raw) == Some(WASM)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_aliases_and_triples() {
        assert_eq!(canonical("macos-arm64"), Some(MACOS_AARCH64));
        assert_eq!(canonical("aarch64-apple-darwin"), Some(MACOS_AARCH64));
        assert_eq!(canonical("linux-x64_86"), Some(LINUX_X86_64));
        assert_eq!(canonical("wasm32-unknown-unknown"), Some(WASM));
        assert_eq!(canonical("unknown"), None);
    }

    #[test]
    fn wasm_is_always_supported() {
        assert!(is_supported(WASM));
        assert!(is_supported("wasm32-unknown-unknown"));
    }
}
