//! Example preload point.

use notwig_ext_api::ExtensionPoint;

/// Example point used to exercise the preload phase.
pub trait ExamplePoint {}

/// Stable id for the example preload point.
pub const EXAMPLE_POINT: ExtensionPoint<dyn ExamplePoint> =
    ExtensionPoint::new("com.notwig.ext.example.point");
