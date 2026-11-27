//! WASM instance lifecycle: instantiation and per-export ABI calls.

use notwig_core::ParamValue;
use notwig_ext_api::{
    ABI_VERSION, AudioSubEditorContext, AudioSubEditorFrame, CommandContext, CommandEffect,
    DeviceDrawContext, MAX_COMMAND_EDITS, MAX_OUTPUT_BYTES, MAX_PANEL_FRAME_BYTES, PanelFrame,
};
use wasmtime::{Engine, Instance, Linker, Memory, Module, Store, StoreLimitsBuilder, TypedFunc};

use super::host::{self, HostState};
use super::status::{check_status, param_wire};
use super::{FUEL_PER_CALL, WASM_MEMORY_LIMIT};
use crate::error::{ExtensionError, wasm_err};

type ProcessFn = TypedFunc<(i32, i32, i32, i32, i32, i64, i32), i32>;

pub(crate) struct ExtensionInstance {
    pub(super) store: Store<HostState>,
    pub(super) memory: Memory,
    pub(super) alloc: TypedFunc<i32, i32>,
    pub(super) free: TypedFunc<(i32, i32), ()>,
    describe: TypedFunc<(i32, i32), i64>,
    create: TypedFunc<(i32, i32), i32>,
    destroy: TypedFunc<i32, ()>,
    prepare: TypedFunc<(i32, i32, i32, i32), i32>,
    set_param: TypedFunc<(i32, i32, i32, i32, f64), i32>,
    process: ProcessFn,
    latency: TypedFunc<i32, i32>,
    draw: TypedFunc<(i32, i32, i32), i64>,
    sub_editor_create: Option<TypedFunc<(i32, i32), i32>>,
    sub_editor_destroy: Option<TypedFunc<i32, ()>>,
    sub_editor_draw: Option<TypedFunc<(i32, i32, i32), i64>>,
    command: Option<TypedFunc<(i32, i32, i32, i32), i64>>,
}

impl ExtensionInstance {
    pub(crate) fn instantiate(
        engine: &Engine,
        module: &Module,
        extension_id: &str,
    ) -> Result<Self, ExtensionError> {
        let limits = StoreLimitsBuilder::new()
            .memory_size(WASM_MEMORY_LIMIT)
            .build();
        let mut store = Store::new(
            engine,
            HostState {
                extension_id: extension_id.to_owned(),
                limits,
            },
        );
        store.limiter(|state| &mut state.limits);
        store.set_fuel(FUEL_PER_CALL).map_err(wasm_err)?;

        let mut linker = Linker::new(engine);
        host::install(&mut linker)?;
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
        let sub_editor_create = instance
            .get_typed_func::<(i32, i32), i32>(&mut store, "notwig_audio_sub_editor_create")
            .ok();
        let sub_editor_destroy = instance
            .get_typed_func::<i32, ()>(&mut store, "notwig_audio_sub_editor_destroy")
            .ok();
        let sub_editor_draw = instance
            .get_typed_func::<(i32, i32, i32), i64>(&mut store, "notwig_audio_sub_editor_draw")
            .ok();
        let command = instance
            .get_typed_func::<(i32, i32, i32, i32), i64>(&mut store, "notwig_command_execute")
            .ok();

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
            sub_editor_create,
            sub_editor_destroy,
            sub_editor_draw,
            command,
        })
    }

    fn reset_fuel(&mut self) -> Result<(), ExtensionError> {
        self.store.set_fuel(FUEL_PER_CALL).map_err(wasm_err)
    }

    pub(crate) fn describe(
        &mut self,
        type_id: &str,
    ) -> Result<notwig_core::dsp::DeviceSpec, ExtensionError> {
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

    pub(crate) fn execute_command(
        &mut self,
        id: &str,
        context: &CommandContext,
    ) -> Result<CommandEffect, ExtensionError> {
        let Some(command) = self.command.clone() else {
            return Err(ExtensionError::MissingExport(
                "notwig_command_execute".into(),
            ));
        };
        let context = postcard::to_allocvec(context)
            .map_err(|error| ExtensionError::Abi(format!("encode command context: {error}")))?;
        let (command_ptr, command_len) = self.write_input(id.as_bytes())?;
        let (context_ptr, context_len) = match self.write_input(&context) {
            Ok(values) => values,
            Err(error) => {
                self.free_input(command_ptr, command_len);
                return Err(error);
            }
        };
        self.reset_fuel()?;
        let packed = command
            .call(
                &mut self.store,
                (command_ptr, command_len, context_ptr, context_len),
            )
            .map_err(wasm_err);
        self.free_input(context_ptr, context_len);
        self.free_input(command_ptr, command_len);
        let packed = packed?;
        let bytes = self.read_packed_bytes(packed, MAX_OUTPUT_BYTES)?;
        let effect: CommandEffect = postcard::from_bytes(&bytes)
            .map_err(|error| ExtensionError::Abi(format!("decode command effect: {error}")))?;
        if let CommandEffect::Apply { edits, .. } = &effect
            && edits.len() > MAX_COMMAND_EDITS
        {
            return Err(ExtensionError::Abi(
                "command returned too many edits".into(),
            ));
        }
        Ok(effect)
    }

    pub(crate) fn create_audio_sub_editor(
        &mut self,
        renderer: &str,
    ) -> Result<i32, ExtensionError> {
        let Some(create) = self.sub_editor_create.clone() else {
            return Err(ExtensionError::MissingExport(
                "notwig_audio_sub_editor_create".into(),
            ));
        };
        let (ptr, len) = self.write_input(renderer.as_bytes())?;
        self.reset_fuel()?;
        let handle = create.call(&mut self.store, (ptr, len)).map_err(wasm_err)?;
        self.free_input(ptr, len);
        if handle < 0 {
            return Err(ExtensionError::InvalidHandle);
        }
        Ok(handle)
    }

    pub(crate) fn destroy_audio_sub_editor(&mut self, handle: i32) {
        if let Some(destroy) = self.sub_editor_destroy.clone() {
            let _ = self.reset_fuel();
            let _ = destroy.call(&mut self.store, handle);
        }
    }

    pub(crate) fn draw_audio_sub_editor(
        &mut self,
        handle: i32,
        context: &AudioSubEditorContext,
    ) -> Result<AudioSubEditorFrame, ExtensionError> {
        let Some(draw) = self.sub_editor_draw.clone() else {
            return Err(ExtensionError::MissingExport(
                "notwig_audio_sub_editor_draw".into(),
            ));
        };
        let context = postcard::to_allocvec(context)
            .map_err(|error| ExtensionError::Abi(format!("encode sub-editor context: {error}")))?;
        let (ptr, len) = self.write_input(&context)?;
        self.reset_fuel()?;
        let packed = draw
            .call(&mut self.store, (handle, ptr, len))
            .map_err(wasm_err)?;
        self.free_input(ptr, len);
        let bytes = self.read_packed_bytes(packed, MAX_PANEL_FRAME_BYTES)?;
        let frame: AudioSubEditorFrame = postcard::from_bytes(&bytes)
            .map_err(|error| ExtensionError::Abi(format!("decode sub-editor frame: {error}")))?;
        frame.validate().map_err(ExtensionError::Abi)?;
        Ok(frame)
    }

    pub(crate) fn draw_panel(
        &mut self,
        handle: i32,
        context: &DeviceDrawContext,
    ) -> Result<PanelFrame, ExtensionError> {
        let context = postcard::to_allocvec(context)
            .map_err(|error| ExtensionError::Abi(format!("encode egui context: {error}")))?;
        let (ptr, len) = self.write_input(&context)?;
        self.reset_fuel()?;
        let packed = self
            .draw
            .call(&mut self.store, (handle, ptr, len))
            .map_err(wasm_err)?;
        self.free_input(ptr, len);
        let bytes = self.read_packed_bytes(packed, MAX_PANEL_FRAME_BYTES)?;
        let frame: PanelFrame = postcard::from_bytes(&bytes)
            .map_err(|error| ExtensionError::Abi(format!("decode egui frame: {error}")))?;
        frame
            .validate()
            .map_err(|error| ExtensionError::Abi(format!("invalid egui frame: {error}")))?;
        Ok(frame)
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
