mod sample_format;
mod service;
mod settings;
mod snapshot;
mod stream;

pub use service::AudioService;
pub use settings::{
    AudioSettings, SYSTEM_OUT_DEVICE_ID, resolve_output_device_name, settings_view,
};
pub use snapshot::AudioSnapshot;
pub use stream::{is_device_change_error, is_device_loss_error};

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use sample_format::with_input_sample_format;
pub(crate) use sample_format::with_output_sample_format;
pub(crate) use settings::default_driver_name;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use settings::{resolve_device, resolve_host, resolve_input_device};
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use snapshot::{DspSample, update_snapshot_at};
