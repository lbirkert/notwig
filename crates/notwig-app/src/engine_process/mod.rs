mod child;
mod parent;
mod protocol;

pub(crate) use parent::{EngineClient, EngineClientHandle, EngineEvent};
pub(crate) use protocol::WireBuild;

/// Runs the native audio engine in its own process.
///
/// Invoked by the helper `engine-host` executable mode. The process owns CPAL,
/// the engine, plugin DSP instances, and plugin editor windows.
pub fn run_engine_host() -> Result<(), String> {
    child::run_engine_host()
}
