use std::sync::atomic::{AtomicBool, Ordering};

use notwig_ext_api::lifecycle;
use notwig_ext_api::{ExtensionPoint, PreloadBuilder};

trait TestInterface {}

const TEST_POINT: ExtensionPoint<dyn TestInterface> =
    ExtensionPoint::new("com.test.extension.lifecycle");

static UNLOADED: AtomicBool = AtomicBool::new(false);

#[test]
fn preload_then_load_keeps_declared_points() {
    lifecycle::run_preload(|builder: &mut PreloadBuilder| {
        builder.register_extension_point(TEST_POINT);
    })
    .expect("preload");

    lifecycle::run_load(|_| {}).expect("load");

    let loaded = lifecycle::ensure_loaded(|_| {});
    assert_eq!(
        loaded.extension_points().collect::<Vec<_>>(),
        vec![TEST_POINT.id()]
    );

    lifecycle::run_unload(|_| UNLOADED.store(true, Ordering::SeqCst));
    assert!(UNLOADED.load(Ordering::SeqCst));
}
