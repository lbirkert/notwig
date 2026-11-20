#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::ExtensionManager;
#[cfg(target_arch = "wasm32")]
pub(crate) use web::ExtensionManager;

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) enum ExtensionEvent {
    Installed { name: String, version: String },
    Error(String),
}
