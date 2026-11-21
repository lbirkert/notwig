mod index;
#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::ExtensionManager;
#[cfg(target_arch = "wasm32")]
pub(crate) use web::ExtensionManager;

#[allow(dead_code)]
pub(crate) enum ExtensionEvent {
    Loaded,
    Installed { name: String, version: String },
    Error(String),
}
