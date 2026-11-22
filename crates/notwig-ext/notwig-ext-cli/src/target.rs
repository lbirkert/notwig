use notwig_ext_api::target;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildTarget {
    pub canonical: &'static str,
    pub rust_triple: &'static str,
    pub extension: &'static str,
    pub lib_prefix: &'static str,
}

impl BuildTarget {
    #[must_use]
    pub fn artifact_name(self, stem: &str) -> String {
        format!("{}{stem}.{}", self.lib_prefix, self.extension)
    }

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

#[must_use]
pub fn resolve(raw: &str) -> Option<BuildTarget> {
    let canonical = target::canonical(raw)?;
    TARGETS
        .iter()
        .copied()
        .find(|target| target.canonical == canonical)
}

#[must_use]
pub fn all() -> &'static [BuildTarget] {
    &TARGETS
}
