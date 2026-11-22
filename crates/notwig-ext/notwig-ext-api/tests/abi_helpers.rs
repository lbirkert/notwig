use notwig_ext_api::abi::{AbiStatus, DeviceStore};

static STORE: DeviceStore<u32> = DeviceStore::new();

#[test]
fn device_store_creates_mutates_and_removes_handles() {
    let handle = STORE.create(7);
    assert_eq!(STORE.with(handle, |value| *value), Ok(7));

    STORE.with(handle, |value| *value += 1).unwrap();
    assert_eq!(STORE.remove(handle), Some(8));
    assert_eq!(
        STORE.with(handle, |value| *value),
        Err(AbiStatus::UnknownDevice)
    );
}

#[test]
fn device_store_handles_are_not_reused() {
    let first = STORE.create(1);
    let second = STORE.create(2);
    assert!(second > first);
    assert_eq!(STORE.remove(first), Some(1));
    let third = STORE.create(3);
    assert!(third > second);
    assert_eq!(STORE.remove(second), Some(2));
    assert_eq!(STORE.remove(third), Some(3));
}
