use notwig_ext_api::target;

/// One supported build target and its Cargo artifact naming.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildTarget {
    /// Canonical Notwig target identifier.
    pub canonical: &'static str,
    /// Rust target triple passed to Cargo and `rustup`.
    pub rust_triple: &'static str,
    /// Platform artifact extension without the dot.
    pub extension: &'static str,
    /// Platform library prefix (`lib` or empty).
    pub lib_prefix: &'static str,
}

impl BuildTarget {
    /// Returns the Cargo artifact file name for `stem`.
    #[must_use]
    pub fn artifact_name(self, stem: &str) -> String {
        format!("{}{stem}.{}", self.lib_prefix, self.extension)
    }

    /// Returns the packaged entrypoint file name for `stem`.
    #[must_use]
    pub fn entrypoint_name(self, stem: &str) -> String {
        format!("{stem}.{}", self.extension)
    }
}

const TARGETS: [BuildTarget; 7] = [
    BuildTarget {
        canonical: target::MACOS_AARCH64,
        rust_triple: "aarch64-apple-darwin",
        extension: "dylib",
        lib_prefix: "lib",
    },
    BuildTarget {
        canonical: target::MACOS_X86_64,
        rust_triple: "x86_64-apple-darwin",
        extension: "dylib",
        lib_prefix: "lib",
    },
    BuildTarget {
        canonical: target::LINUX_X86_64,
        rust_triple: "x86_64-unknown-linux-gnu",
        extension: "so",
        lib_prefix: "lib",
    },
    BuildTarget {
        canonical: target::LINUX_AARCH64,
        rust_triple: "aarch64-unknown-linux-gnu",
        extension: "so",
        lib_prefix: "lib",
    },
    BuildTarget {
        canonical: target::WINDOWS_X86_64,
        rust_triple: "x86_64-pc-windows-msvc",
        extension: "dll",
        lib_prefix: "",
    },
    BuildTarget {
        canonical: target::WINDOWS_AARCH64,
        rust_triple: "aarch64-pc-windows-msvc",
        extension: "dll",
        lib_prefix: "",
    },
    BuildTarget {
        canonical: target::WASM,
        rust_triple: "wasm32-unknown-unknown",
        extension: "wasm",
        lib_prefix: "",
    },
];

/// Resolves a target identifier, alias, or Rust triple to a [`BuildTarget`].
#[must_use]
pub fn resolve(raw: &str) -> Option<BuildTarget> {
    let canonical = target::canonical(raw)?;
    TARGETS
        .iter()
        .copied()
        .find(|target| target.canonical == canonical)
}

/// Returns every supported build target.
#[must_use]
pub fn all() -> &'static [BuildTarget] {
    &TARGETS
}
