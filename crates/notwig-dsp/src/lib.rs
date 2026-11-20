#![forbid(unsafe_code)]

pub mod devices;
pub mod factory;
pub mod graph;
pub mod params;
pub mod registry;
pub mod smoothing;
pub mod stock;

pub use devices::{
    DeviceProcessor, DeviceSpec, DeviceUiSpec, EngineCaps, Latency, NoteEvent, NoteKind, Port,
    PortDir, ProcessContext, ProcessStatus, RenderConfig, SignalType, TransportInfo,
};
pub use factory::{CompositeDeviceFactory, DeviceFactory, StockDeviceFactory};
pub use graph::{CompiledPlan, GraphError, PlanDevice, PlanSend, PlanTrack, compile};
pub use notwig_core::DeviceKind;
pub use params::{
    NormMap, ParamFlags, ParamKind, ParamSet, ParamSpec, RangeSpec, SmoothingSpec, Unit, Value,
};
pub use registry::DeviceRegistry;
pub use stock::{
    eq_magnitude_response, instantiate_stock, instantiate_stock_params, stock_registry,
};
