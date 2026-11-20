//! Native module ownership for VST 2 plugins.
//!
//! On macOS a `.vst` entry is usually a `CFBundle` directory. Loading the
//! binary directly with `dlopen` leaves the bundle unregistered with
//! CoreFoundation, which breaks plugins that query their bundle resources
//! during entry. Use `CFBundleLoadExecutable` for bundle directories and
//! `libloading` for plain shared libraries.

use crate::abi::Entry;
use notwig_rack_core::error::{Error, Result};
use std::path::{Path, PathBuf};

pub(crate) struct LoadedModule {
    #[cfg(target_os = "macos")]
    inner: Inner,
    #[cfg(not(target_os = "macos"))]
    library: libloading::Library,
}

#[cfg(not(target_os = "macos"))]
impl LoadedModule {
    pub(crate) unsafe fn open(_plugin_path: &Path, binary: &Path) -> Result<Self> {
        let library =
            unsafe { libloading::Library::new(binary) }.map_err(|err| Error::LoadFailed {
                path: binary.to_path_buf(),
                reason: format!("dlopen: {err}"),
            })?;
        Ok(Self { library })
    }

    pub(crate) unsafe fn entry(&self) -> Result<Entry> {
        let symbol: libloading::Symbol<'_, Entry> = unsafe {
            self.library
                .get(b"VSTPluginMain\0")
                .or_else(|_| self.library.get(b"main\0"))
        }
        .map_err(|err| Error::LoadFailed {
            path: PathBuf::new(),
            reason: format!("missing VSTPluginMain/main: {err}"),
        })?;
        Ok(*symbol)
    }
}

#[cfg(target_os = "macos")]
enum Inner {
    Bundle(mac::MacBundle),
    Library(libloading::Library),
}

#[cfg(target_os = "macos")]
impl LoadedModule {
    pub(crate) unsafe fn open(plugin_path: &Path, binary: &Path) -> Result<Self> {
        if plugin_path.is_dir() {
            return Ok(Self {
                inner: Inner::Bundle(mac::MacBundle::open(plugin_path)?),
            });
        }
        let library =
            unsafe { libloading::Library::new(binary) }.map_err(|err| Error::LoadFailed {
                path: binary.to_path_buf(),
                reason: format!("dlopen: {err}"),
            })?;
        Ok(Self {
            inner: Inner::Library(library),
        })
    }

    pub(crate) unsafe fn entry(&self) -> Result<Entry> {
        match &self.inner {
            Inner::Bundle(bundle) => {
                let pointer = unsafe { bundle.function_ptr(b"VSTPluginMain\0") }
                    .or_else(|| unsafe { bundle.function_ptr(b"main\0") });
                let pointer = pointer.ok_or_else(|| Error::LoadFailed {
                    path: bundle.path().to_path_buf(),
                    reason: "missing VSTPluginMain/main".into(),
                })?;
                Ok(unsafe { std::mem::transmute::<*mut std::ffi::c_void, Entry>(pointer) })
            }
            Inner::Library(library) => {
                let symbol: libloading::Symbol<'_, Entry> = unsafe {
                    library
                        .get(b"VSTPluginMain\0")
                        .or_else(|_| library.get(b"main\0"))
                }
                .map_err(|err| Error::LoadFailed {
                    path: PathBuf::new(),
                    reason: format!("missing VSTPluginMain/main: {err}"),
                })?;
                Ok(*symbol)
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use core_foundation::base::TCFType;
    use core_foundation::bundle::CFBundle;
    use core_foundation::string::CFString;
    use core_foundation::url::{CFURL, kCFURLPOSIXPathStyle};
    use notwig_rack_core::error::{Error, Result};
    use std::path::{Path, PathBuf};

    pub(super) struct MacBundle {
        bundle: CFBundle,
        path: PathBuf,
    }

    impl MacBundle {
        pub(super) fn open(bundle_path: &Path) -> Result<Self> {
            let path_str = bundle_path.to_str().ok_or_else(|| Error::LoadFailed {
                path: bundle_path.to_path_buf(),
                reason: "bundle path is not valid UTF-8".into(),
            })?;
            let cf_path = CFString::new(path_str);
            let url = unsafe {
                use core_foundation_sys::base::kCFAllocatorDefault;
                use core_foundation_sys::url::CFURLCreateWithFileSystemPath;
                let raw = CFURLCreateWithFileSystemPath(
                    kCFAllocatorDefault,
                    cf_path.as_concrete_TypeRef(),
                    kCFURLPOSIXPathStyle,
                    1,
                );
                if raw.is_null() {
                    return Err(Error::LoadFailed {
                        path: bundle_path.to_path_buf(),
                        reason: "CFURLCreateWithFileSystemPath returned NULL".into(),
                    });
                }
                CFURL::wrap_under_create_rule(raw)
            };

            let bundle = CFBundle::new(url).ok_or_else(|| Error::LoadFailed {
                path: bundle_path.to_path_buf(),
                reason: "CFBundleCreate returned NULL".into(),
            })?;

            let loaded = unsafe {
                use core_foundation_sys::bundle::CFBundleLoadExecutable;
                CFBundleLoadExecutable(bundle.as_concrete_TypeRef()) != 0
            };
            if !loaded {
                return Err(Error::LoadFailed {
                    path: bundle_path.to_path_buf(),
                    reason: "CFBundleLoadExecutable failed".into(),
                });
            }

            Ok(Self {
                bundle,
                path: bundle_path.to_path_buf(),
            })
        }

        pub(super) fn path(&self) -> &Path {
            &self.path
        }

        pub(super) unsafe fn function_ptr(&self, name: &[u8]) -> Option<*mut std::ffi::c_void> {
            let name = match name.split_last() {
                Some((0, rest)) => rest,
                _ => name,
            };
            let name_str = std::str::from_utf8(name).ok()?;
            let cf_name = CFString::new(name_str);
            let pointer = unsafe {
                use core_foundation_sys::bundle::CFBundleGetFunctionPointerForName;
                CFBundleGetFunctionPointerForName(
                    self.bundle.as_concrete_TypeRef(),
                    cf_name.as_concrete_TypeRef(),
                )
            };
            if pointer.is_null() {
                None
            } else {
                Some(pointer.cast_mut().cast())
            }
        }
    }
}
