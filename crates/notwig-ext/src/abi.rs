use notwig_core::ParamValue;
use notwig_ext_api::{
    ABI_VERSION, AbiStatus, MAX_OUTPUT_BYTES, PanelDraw, TrustLevel, unpack_ptr_len,
};
use wasmtime::{
    Caller, Config, Engine, Instance, Linker, Memory, Module, OptLevel, Store, StoreLimits,
    StoreLimitsBuilder, TypedFunc,
};

use crate::error::{ExtensionError, wasm_err};

const FUEL_PER_CALL: u64 = 20_000_000;
const MAX_INPUT_BYTES: usize = 1024 * 1024;

type ProcessFn = TypedFunc<(i32, i32, i32, i32, i32, i64, i32), i32>;

#[derive(Debug)]
pub(crate) struct ExtensionEngines {
    fast: Engine,
    limited: Engine,
}

impl ExtensionEngines {
    pub(crate) fn new() -> Result<Self, ExtensionError> {
        Ok(Self {
            fast: make_engine(false)?,
            limited: make_engine(true)?,
        })
    }

    pub(crate) fn engine(&self, trust: TrustLevel) -> &Engine {
        if trust == TrustLevel::Trusted {
            &self.fast
        } else {
            &self.limited
        }
    }
}

fn make_engine(consume_fuel: bool) -> Result<Engine, ExtensionError> {
    let mut config = Config::new();
    config.consume_fuel(consume_fuel);
    config.cranelift_opt_level(OptLevel::Speed);
    Engine::new(&config).map_err(wasm_err)
}

pub(crate) struct HostState {
    pub(crate) extension_id: String,
    pub(crate) limits: StoreLimits,
}

pub(crate) struct ExtensionInstance {
    store: Store<HostState>,
    memory: Memory,
    alloc: TypedFunc<i32, i32>,
    free: TypedFunc<(i32, i32), ()>,
    describe: TypedFunc<(i32, i32), i64>,
    create: TypedFunc<(i32, i32), i32>,
    destroy: TypedFunc<i32, ()>,
    prepare: TypedFunc<(i32, i32, i32, i32), i32>,
    set_param: TypedFunc<(i32, i32, i32, i32, f64), i32>,
    process: ProcessFn,
    latency: TypedFunc<i32, i32>,
    draw: TypedFunc<(i32, i32, i32), i64>,
    trust: TrustLevel,
}

impl ExtensionInstance {
    pub(crate) fn instantiate(
        engine: &Engine,
        module: &Module,
        extension_id: &str,
        trust: TrustLevel,
    ) -> Result<Self, ExtensionError> {
        let memory_limit = match trust {
            TrustLevel::Trusted => 64 * 1024 * 1024,
            TrustLevel::Limited => 32 * 1024 * 1024,
            TrustLevel::DrawOnly => 16 * 1024 * 1024,
        };
        let limits = StoreLimitsBuilder::new().memory_size(memory_limit).build();
        let mut store = Store::new(
            engine,
            HostState {
                extension_id: extension_id.to_owned(),
                limits,
            },
        );
        store.limiter(|state| &mut state.limits);
        if trust != TrustLevel::Trusted {
            store.set_fuel(FUEL_PER_CALL).map_err(wasm_err)?;
        }

        let mut linker = Linker::new(engine);
        linker
            .func_wrap(
                "notwig.host",
                "log",
                |mut caller: Caller<'_, HostState>, level: i32, ptr: i32, len: i32| {
                    if let Some(text) = caller_string(&mut caller, ptr, len) {
                        match level {
                            0 => tracing::debug!(extension = %caller.data().extension_id, "{text}"),
                            1 => tracing::info!(extension = %caller.data().extension_id, "{text}"),
                            _ => tracing::warn!(extension = %caller.data().extension_id, "{text}"),
                        }
                    }
                },
            )
            .map_err(wasm_err)?;
        linker
            .func_wrap(
                "notwig.host",
                "notify",
                |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| {
                    if let Some(text) = caller_string(&mut caller, ptr, len) {
                        tracing::info!(
                            extension = %caller.data().extension_id,
                            "{text}"
                        );
                    }
                },
            )
            .map_err(wasm_err)?;
        let instance = linker.instantiate(&mut store, module).map_err(wasm_err)?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| ExtensionError::MissingExport("memory".into()))?;

        let abi_version = instance
            .get_typed_func::<(), i32>(&mut store, "notwig_abi_version")
            .map_err(|_| ExtensionError::MissingExport("notwig_abi_version".into()))?;
        let version = abi_version.call(&mut store, ()).map_err(wasm_err)?;
        let expected = i32::try_from(ABI_VERSION)
            .map_err(|_| ExtensionError::Abi("ABI version does not fit in i32".into()))?;
        if version != expected {
            return Err(ExtensionError::Abi(format!(
                "extension ABI {version} does not match host {ABI_VERSION}"
            )));
        }

        let alloc = get_func(&instance, &mut store, "notwig_alloc")?;
        let free = get_func(&instance, &mut store, "notwig_free")?;
        let describe = get_func(&instance, &mut store, "notwig_device_describe")?;
        let create = get_func(&instance, &mut store, "notwig_device_create")?;
        let destroy = get_func(&instance, &mut store, "notwig_device_destroy")?;
        let prepare = get_func(&instance, &mut store, "notwig_device_prepare")?;
        let set_param = get_func(&instance, &mut store, "notwig_device_set_param")?;
        let process = get_func(&instance, &mut store, "notwig_device_process")?;
        let latency = get_func(&instance, &mut store, "notwig_device_latency")?;
        let draw = get_func(&instance, &mut store, "notwig_device_draw")?;

        Ok(Self {
            store,
            memory,
            alloc,
            free,
            describe,
            create,
            destroy,
            prepare,
            set_param,
            process,
            latency,
            draw,
            trust,
        })
    }

    fn reset_fuel(&mut self) -> Result<(), ExtensionError> {
        if self.trust == TrustLevel::Trusted {
            return Ok(());
        }
        self.store.set_fuel(FUEL_PER_CALL).map_err(wasm_err)
    }

    pub(crate) fn describe(
        &mut self,
        type_id: &str,
    ) -> Result<notwig_dsp::DeviceSpec, ExtensionError> {
        let (ptr, len) = self.write_input(type_id.as_bytes())?;
        self.reset_fuel()?;
        let packed = self
            .describe
            .call(&mut self.store, (ptr, len))
            .map_err(wasm_err)?;
        self.free_input(ptr, len);
        let json = self.read_packed_json(packed)?;
        serde_json::from_slice(&json)
            .map_err(|error| ExtensionError::Abi(format!("parse device spec: {error}")))
    }

    pub(crate) fn create_device(&mut self, type_id: &str) -> Result<i32, ExtensionError> {
        let (ptr, len) = self.write_input(type_id.as_bytes())?;
        self.reset_fuel()?;
        let handle = self
            .create
            .call(&mut self.store, (ptr, len))
            .map_err(wasm_err)?;
        self.free_input(ptr, len);
        if handle < 0 {
            return Err(ExtensionError::InvalidHandle);
        }
        Ok(handle)
    }

    pub(crate) fn destroy_device(&mut self, handle: i32) {
        let _ = self.destroy.call(&mut self.store, handle);
    }

    pub(crate) fn prepare_device(
        &mut self,
        handle: i32,
        sample_rate: u32,
        max_block: usize,
        channels: u16,
    ) -> Result<(), ExtensionError> {
        self.reset_fuel()?;
        let sample_rate = i32::try_from(sample_rate)
            .map_err(|_| ExtensionError::Abi("sample rate does not fit in i32".into()))?;
        let max_block = i32::try_from(max_block)
            .map_err(|_| ExtensionError::Abi("block size does not fit in i32".into()))?;
        let status = self
            .prepare
            .call(
                &mut self.store,
                (handle, sample_rate, max_block, i32::from(channels)),
            )
            .map_err(wasm_err)?;
        check_status(status)
    }

    pub(crate) fn set_param(
        &mut self,
        handle: i32,
        param_id: notwig_core::ParamId,
        value: &ParamValue,
    ) -> Result<(), ExtensionError> {
        let id = param_id.to_string();
        let (ptr, len) = self.write_input(id.as_bytes())?;
        let (tag, raw) = param_wire(value);
        self.reset_fuel()?;
        let status = self
            .set_param
            .call(&mut self.store, (handle, ptr, len, tag, raw))
            .map_err(wasm_err)?;
        self.free_input(ptr, len);
        check_status(status)
    }

    pub(crate) fn process(
        &mut self,
        handle: i32,
        audio_ptr: i32,
        frames: usize,
        events_ptr: i32,
        event_count: usize,
        sample_position: u64,
        playing: bool,
    ) -> Result<i32, ExtensionError> {
        self.reset_fuel()?;
        let frames = i32::try_from(frames)
            .map_err(|_| ExtensionError::Abi("frame count does not fit in i32".into()))?;
        let event_count = i32::try_from(event_count)
            .map_err(|_| ExtensionError::Abi("event count does not fit in i32".into()))?;
        let sample_position = i64::try_from(sample_position)
            .map_err(|_| ExtensionError::Abi("sample position does not fit in i64".into()))?;
        self.process
            .call(
                &mut self.store,
                (
                    handle,
                    audio_ptr,
                    frames,
                    events_ptr,
                    event_count,
                    sample_position,
                    i32::from(playing),
                ),
            )
            .map_err(wasm_err)
    }

    pub(crate) fn latency(&mut self, handle: i32) -> u32 {
        let _ = self.reset_fuel();
        self.latency
            .call(&mut self.store, handle)
            .ok()
            .and_then(|samples| u32::try_from(samples).ok())
            .unwrap_or(0)
    }

    pub(crate) fn draw_panel(
        &mut self,
        handle: i32,
        context: &notwig_ext_api::DeviceDrawContext,
    ) -> Result<PanelDraw, ExtensionError> {
        let json = serde_json::to_vec(context)
            .map_err(|error| ExtensionError::Abi(format!("encode draw context: {error}")))?;
        let (ptr, len) = self.write_input(&json)?;
        self.reset_fuel()?;
        let packed = self
            .draw
            .call(&mut self.store, (handle, ptr, len))
            .map_err(wasm_err)?;
        self.free_input(ptr, len);
        let json = self.read_packed_json(packed)?;
        let panel: PanelDraw = serde_json::from_slice(&json)
            .map_err(|error| ExtensionError::Abi(format!("parse panel draw: {error}")))?;
        panel
            .validate()
            .map_err(|error| ExtensionError::Abi(error.to_string()))?;
        Ok(panel)
    }

    pub(crate) fn alloc_audio(&mut self, bytes: usize) -> Result<i32, ExtensionError> {
        let len = i32::try_from(bytes)
            .map_err(|_| ExtensionError::Abi("audio buffer is too large".into()))?;
        let ptr = self.alloc.call(&mut self.store, len).map_err(wasm_err)?;
        if ptr < 0 {
            return Err(ExtensionError::BudgetExceeded);
        }
        Ok(ptr)
    }

    pub(crate) fn write_bytes(&mut self, ptr: i32, bytes: &[u8]) -> Result<(), ExtensionError> {
        let start =
            usize::try_from(ptr).map_err(|_| ExtensionError::Abi("negative pointer".into()))?;
        self.memory
            .write(&mut self.store, start, bytes)
            .map_err(wasm_err)
    }

    pub(crate) fn read_into(&self, ptr: i32, out: &mut [u8]) -> Result<(), ExtensionError> {
        let start =
            usize::try_from(ptr).map_err(|_| ExtensionError::Abi("negative pointer".into()))?;
        let end = start
            .checked_add(out.len())
            .ok_or_else(|| ExtensionError::Abi("pointer overflow".into()))?;
        if end > self.memory.data_size(&self.store) {
            return Err(ExtensionError::Abi("out-of-bounds read".into()));
        }
        self.memory.read(&self.store, start, out).map_err(wasm_err)
    }

    pub(crate) fn read_bytes(&self, ptr: i32, len: i32) -> Result<Vec<u8>, ExtensionError> {
        let start =
            usize::try_from(ptr).map_err(|_| ExtensionError::Abi("negative pointer".into()))?;
        let len =
            usize::try_from(len).map_err(|_| ExtensionError::Abi("negative length".into()))?;
        let end = start
            .checked_add(len)
            .ok_or_else(|| ExtensionError::Abi("pointer overflow".into()))?;
        let data = self.memory.data(&self.store);
        if end > data.len() {
            return Err(ExtensionError::Abi("out-of-bounds read".into()));
        }
        Ok(data[start..end].to_vec())
    }

    fn write_input(&mut self, bytes: &[u8]) -> Result<(i32, i32), ExtensionError> {
        if bytes.len() > MAX_INPUT_BYTES {
            return Err(ExtensionError::Abi("input is too large".into()));
        }
        let len = i32::try_from(bytes.len())
            .map_err(|_| ExtensionError::Abi("input is too large".into()))?;
        let ptr = self.alloc.call(&mut self.store, len).map_err(wasm_err)?;
        if ptr < 0 {
            return Err(ExtensionError::BudgetExceeded);
        }
        self.write_bytes(ptr, bytes)?;
        Ok((ptr, len))
    }

    fn free_input(&mut self, ptr: i32, len: i32) {
        let _ = self.free.call(&mut self.store, (ptr, len));
    }

    fn read_packed_json(&mut self, packed: i64) -> Result<Vec<u8>, ExtensionError> {
        let (ptr, len) = unpack_ptr_len(packed as u64);
        let ptr = i32::try_from(ptr)
            .map_err(|_| ExtensionError::Abi("negative result pointer".into()))?;
        let len =
            i32::try_from(len).map_err(|_| ExtensionError::Abi("negative result length".into()))?;
        if len == 0 {
            return Err(ExtensionError::Abi("empty result".into()));
        }
        if usize::try_from(len).is_ok_and(|len| len > MAX_OUTPUT_BYTES) {
            self.free_input(ptr, len);
            return Err(ExtensionError::Abi("result is too large".into()));
        }
        let bytes = self.read_bytes(ptr, len)?;
        self.free_input(ptr, len);
        Ok(bytes)
    }
}

fn get_func<Params, Results>(
    instance: &Instance,
    store: &mut Store<HostState>,
    name: &str,
) -> Result<TypedFunc<Params, Results>, ExtensionError>
where
    Params: wasmtime::WasmParams,
    Results: wasmtime::WasmResults,
{
    instance
        .get_typed_func::<Params, Results>(store, name)
        .map_err(|_| ExtensionError::MissingExport(name.to_owned()))
}

fn param_wire(value: &ParamValue) -> (i32, f64) {
    match value {
        ParamValue::F64(value) => (0, *value),
        ParamValue::I64(value) => (1, *value as f64),
        ParamValue::Bool(value) => (2, f64::from(u8::from(*value))),
        ParamValue::EnumIndex(value) => (3, f64::from(*value)),
    }
}

fn check_status(status: i32) -> Result<(), ExtensionError> {
    match AbiStatus::from_i32(status) {
        Some(AbiStatus::Ok) => Ok(()),
        Some(AbiStatus::FuelExhausted) => Err(ExtensionError::BudgetExceeded),
        Some(other) => Err(ExtensionError::Abi(format!("extension returned {other:?}"))),
        None => Err(ExtensionError::Abi(format!(
            "extension returned unknown status {status}"
        ))),
    }
}

fn caller_string(caller: &mut Caller<'_, HostState>, ptr: i32, len: i32) -> Option<String> {
    let memory = caller.get_export("memory")?.into_memory()?;
    let start = usize::try_from(ptr).ok()?;
    let len = usize::try_from(len).ok()?;
    let end = start.checked_add(len)?;
    let data = memory.data(&*caller);
    if end > data.len() {
        return None;
    }
    Some(String::from_utf8_lossy(&data[start..end]).into_owned())
}
