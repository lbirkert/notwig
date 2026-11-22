use std::path::PathBuf;

use notwig_plugins::{
    HostAdapter, PluginCapabilities, PluginDescriptor, PluginError, PluginFormat, PluginHost,
    PluginInstance, UnavailableAdapter,
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

#[test]
fn capabilities_default_to_no_latency_report() {
    let capabilities = PluginCapabilities::default();
    assert!(!capabilities.reports_latency);
    assert_eq!(capabilities.latency_samples, 0);
}

#[test]
fn probe_runs_before_in_process_load_and_is_cached() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingAdapter {
        loads: Arc<AtomicUsize>,
    }
    impl HostAdapter for CountingAdapter {
        fn format(&self) -> PluginFormat {
            PluginFormat::Vst3
        }
        fn scan(
            &self,
            _paths: &[std::path::PathBuf],
        ) -> Result<Vec<PluginDescriptor>, PluginError> {
            Ok(Vec::new())
        }
        fn load(
            &self,
            _descriptor: &PluginDescriptor,
        ) -> Result<Box<dyn PluginInstance>, PluginError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Err(PluginError::BackendUnavailable(PluginFormat::Vst3))
        }
    }
    struct CountingProbe {
        calls: Arc<AtomicUsize>,
    }
    impl notwig_plugins::PluginProbe for CountingProbe {
        fn probe(
            &self,
            _descriptor: &PluginDescriptor,
        ) -> Result<notwig_plugins::PluginMetadata, PluginError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(notwig_plugins::PluginMetadata {
                capabilities: PluginCapabilities::default(),
                params: Vec::new(),
            })
        }
    }

    let loads = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(AtomicUsize::new(0));
    let mut host = PluginHost::empty();
    host.register_adapter(Box::new(CountingAdapter {
        loads: Arc::clone(&loads),
    }));
    host.set_probe(Arc::new(CountingProbe {
        calls: Arc::clone(&calls),
    }));

    let metadata = host.probe_metadata(&descriptor()).unwrap();
    assert_eq!(metadata.params.len(), 0);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let _ = host.load(&descriptor());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(loads.load(Ordering::SeqCst), 1);
}

#[test]
fn failed_probe_prevents_in_process_load() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct PanicAdapter {
        loads: Arc<AtomicUsize>,
    }
    impl HostAdapter for PanicAdapter {
        fn format(&self) -> PluginFormat {
            PluginFormat::Vst3
        }
        fn scan(
            &self,
            _paths: &[std::path::PathBuf],
        ) -> Result<Vec<PluginDescriptor>, PluginError> {
            Ok(Vec::new())
        }
        fn load(
            &self,
            _descriptor: &PluginDescriptor,
        ) -> Result<Box<dyn PluginInstance>, PluginError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            panic!("this adapter must not run after a failed probe");
        }
    }
    struct FailingProbe;
    impl notwig_plugins::PluginProbe for FailingProbe {
        fn probe(
            &self,
            _descriptor: &PluginDescriptor,
        ) -> Result<notwig_plugins::PluginMetadata, PluginError> {
            Err(PluginError::Load("probe crashed".into()))
        }
    }

    let loads = Arc::new(AtomicUsize::new(0));
    let mut host = PluginHost::empty();
    host.register_adapter(Box::new(PanicAdapter {
        loads: Arc::clone(&loads),
    }));
    host.set_probe(Arc::new(FailingProbe));

    assert!(matches!(
        host.load(&descriptor()),
        Err(PluginError::Load(_))
    ));
    assert_eq!(loads.load(Ordering::SeqCst), 0);
}
