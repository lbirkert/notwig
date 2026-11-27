use std::sync::Arc;

use notwig_ext_api::{
    Cardinality, CoreExtensionRegistry, EP_NOTWIG_CLIPBOARD, ExtensionPoint, RegistryError,
};

trait TestExt: Send + Sync {
    fn value(&self) -> &'static str;
}

struct A;
impl TestExt for A {
    fn value(&self) -> &'static str {
        "a"
    }
}

struct B;
impl TestExt for B {
    fn value(&self) -> &'static str {
        "b"
    }
}

const MANY: ExtensionPoint<dyn TestExt> = ExtensionPoint::many("test.many");
const SINGLE: ExtensionPoint<dyn TestExt> = ExtensionPoint::single("test.single");

#[test]
fn many_point_returns_all_and_get_extension_rejects_multiple() {
    let mut registry = CoreExtensionRegistry::new();
    registry.register(MANY, "a", Arc::new(A)).unwrap();
    registry.register(MANY, "b", Arc::new(B)).unwrap();
    let values: Vec<_> = registry
        .get_extensions(MANY)
        .iter()
        .map(|ext| ext.value())
        .collect();
    assert_eq!(values, vec!["a", "b"]);
    assert!(matches!(
        registry.try_get_extension(MANY),
        Err(RegistryError::Cardinality { count: 2, .. })
    ));
}

#[test]
fn single_point_selects_exactly_one_active_implementation() {
    let mut registry = CoreExtensionRegistry::new();
    registry.register(SINGLE, "a", Arc::new(A)).unwrap();
    assert_eq!(registry.get_extension(SINGLE).value(), "a");
    registry.register(SINGLE, "b", Arc::new(B)).unwrap();
    assert_eq!(registry.get_extension(SINGLE).value(), "a");
    registry.set_active(SINGLE, "b").unwrap();
    assert_eq!(registry.get_extension(SINGLE).value(), "b");
}

#[test]
fn clipboard_point_is_single_cardinality() {
    assert_eq!(EP_NOTWIG_CLIPBOARD.cardinality(), Cardinality::Single);
}
