use cpal::ErrorKind;
use notwig_app::audio::{is_device_change_error, is_device_loss_error};

#[test]
fn classifies_device_loss_and_default_output_changes() {
    for kind in [
        ErrorKind::DeviceNotAvailable,
        ErrorKind::StreamInvalidated,
        ErrorKind::HostUnavailable,
    ] {
        assert!(is_device_loss_error(kind), "{kind:?} should be device loss");
        assert!(
            !is_device_change_error(kind),
            "{kind:?} should not be a device change"
        );
    }

    assert!(is_device_change_error(ErrorKind::DeviceChanged));
    assert!(!is_device_loss_error(ErrorKind::DeviceChanged));
}

#[test]
fn leaves_non_loss_errors_for_normal_error_reporting() {
    for kind in [
        ErrorKind::Xrun,
        ErrorKind::DeviceBusy,
        ErrorKind::PermissionDenied,
        ErrorKind::UnsupportedConfig,
        ErrorKind::BackendError,
        ErrorKind::Other,
    ] {
        assert!(!is_device_loss_error(kind), "{kind:?} should not be loss");
        assert!(
            !is_device_change_error(kind),
            "{kind:?} should not be a device change"
        );
    }
}
