//! Stable facade for the native WASM extension host.
//!
//! The module tree separates engine setup, instance lifecycle, memory access,
//! host imports, and status/parameter decoding. Call sites keep using
//! [`ExtensionEngines`] and [`ExtensionInstance`] through this facade.

use wasmtime::{Config, Engine, OptLevel};

use crate::error::{ExtensionError, wasm_err};

mod host;
mod instance;
mod memory;
mod status;

pub(crate) use instance::ExtensionInstance;

const FUEL_PER_CALL: u64 = 20_000_000;
const WASM_MEMORY_LIMIT: usize = 64 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct ExtensionEngines {
    engine: Engine,
}

impl ExtensionEngines {
    pub(crate) fn new() -> Result<Self, ExtensionError> {
        Ok(Self {
            engine: make_engine()?,
        })
    }

    pub(crate) fn engine(&self) -> &Engine {
        &self.engine
    }
}

fn make_engine() -> Result<Engine, ExtensionError> {
    let mut config = Config::new();
    config.consume_fuel(true);
    config.cranelift_opt_level(OptLevel::Speed);
    Engine::new(&config).map_err(wasm_err)
}
