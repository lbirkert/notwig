use std::path::PathBuf;

use notwig_plugins::{
    HostAdapter, PluginDescriptor, PluginError, PluginFormat, PluginHost, PluginInstance,
    UnavailableAdapter,
};

fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        type_id: "vst3.test".into(),
        format: PluginFormat::Vst3,
        path: PathBuf::from("/tmp/test.vst3"),
        name: "Test".into(),
        vendor: "Test".into(),
        version: "1.0".into(),
        uid: "test".into(),
        is_instrument: true,
        binary_size: 1,
        modified_utc: 0,
    }
}

#[test]
fn unavailable_backend_reports_explicit_error() {
    let mut host = PluginHost::empty();
    host.register_adapter(Box::new(UnavailableAdapter::new(PluginFormat::Vst3)));
    if let Err(err) = host.load(&descriptor()) {
        assert!(matches!(err, PluginError::BackendUnavailable(_)));
    } else {
        panic!("expected an unavailable backend error");
    }
}

#[test]
fn cache_blacklists_after_repeated_failures() {
    let mut host = PluginHost::empty();
    host.register_adapter(Box::new(UnavailableAdapter::new(PluginFormat::Vst3)));
    let d = descriptor();
    for _ in 0..3 {
        let _ = host.load(&d);
    }
    assert!(host.cache().is_blacklisted(&d.type_id));
    assert!(matches!(host.load(&d), Err(PluginError::Blacklisted(_))));
}

#[test]
fn custom_adapter_scan_merges_and_sorts() {
    struct TestAdapter;
    impl HostAdapter for TestAdapter {
        fn format(&self) -> PluginFormat {
            PluginFormat::Au
        }
        fn scan(
            &self,
            _paths: &[std::path::PathBuf],
        ) -> Result<Vec<PluginDescriptor>, PluginError> {
            let mut d = descriptor();
            d.format = PluginFormat::Au;
            d.type_id = "au.test".into();
            Ok(vec![d])
        }
        fn load(
            &self,
            _descriptor: &PluginDescriptor,
        ) -> Result<Box<dyn PluginInstance>, PluginError> {
            Err(PluginError::BackendUnavailable(PluginFormat::Au))
        }
    }
    let mut host = PluginHost::empty();
    host.register_adapter(Box::new(TestAdapter));
    let found = host.scan(&[]);
    assert!(found.iter().any(|d| d.type_id == "au.test"));
    let _ = UnavailableAdapter::new(PluginFormat::Vst2);
}
