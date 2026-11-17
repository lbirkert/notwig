#![forbid(unsafe_code)]

pub mod devices;
pub mod graph;
pub mod params;
pub mod registry;
pub mod stock;

pub use devices::{
    DeviceProcessor, DeviceSpec, EngineCaps, Latency, NoteEvent, NoteKind, Port, PortDir,
    ProcessContext, ProcessStatus, RenderConfig, SignalType, TransportInfo,
};
pub use graph::{CompiledPlan, GraphError, PlanDevice, PlanSend, PlanTrack, compile};
pub use notwig_core::DeviceKind;
pub use params::{
    NormMap, ParamFlags, ParamKind, ParamSet, ParamSpec, RangeSpec, SmoothingSpec, Unit, Value,
};
pub use registry::DeviceRegistry;
pub use stock::{
    eq_magnitude_response, instantiate_stock, instantiate_stock_params, stock_registry,
};
