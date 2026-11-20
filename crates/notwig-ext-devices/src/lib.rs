use notwig_ext_api::{Extension, notwig_extension};

mod limiter;

pub use limiter::{LimiterDevice, TYPE_ID, release_id, threshold_id};

#[notwig_extension]
pub fn extension(extension: &mut Extension) {
    extension.register_device::<LimiterDevice>();
}
