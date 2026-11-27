use std::sync::Arc;

use notwig_ext_api::{
    CoreExtensionRegistry, Extension, ExtensionContext, ExtensionError, ExtensionInstance,
    ExtensionPoint, RemoteTarget, RpcTransport, SharedRpcTransport, notwig_ext_trait,
    notwig_ext_trait_impl,
};

#[notwig_ext_trait(id = "test.echo", version = 1)]
pub trait Echo {
    fn echo(&self, context: &ExtensionContext<'_>, value: String)
    -> Result<String, ExtensionError>;
    fn fail(&self, context: &ExtensionContext<'_>, message: String) -> Result<(), ExtensionError>;
    fn ping(&self, context: &ExtensionContext<'_>) -> Result<u32, ExtensionError>;
}

const ECHO_POINT: ExtensionPoint<dyn Echo> = ExtensionPoint::new("test.echo.point");

struct Echoer;

#[notwig_ext_trait_impl(Echo)]
impl Echo for Echoer {
    fn echo(
        &self,
        _context: &ExtensionContext<'_>,
        value: String,
    ) -> Result<String, ExtensionError> {
        Ok(format!("echo:{value}"))
    }

    fn fail(&self, _context: &ExtensionContext<'_>, message: String) -> Result<(), ExtensionError> {
        Err(ExtensionError::new(message))
    }

    fn ping(&self, _context: &ExtensionContext<'_>) -> Result<u32, ExtensionError> {
        Ok(42)
    }
}

struct TestTransport {
    dispatcher: Arc<dyn EchoDispatcher>,
}

impl RpcTransport for TestTransport {
    fn invoke(
        &self,
        target: &RemoteTarget,
        context: &ExtensionContext<'_>,
        interface_id: &'static str,
        interface_version: u32,
        method: &'static str,
        payload: &[u8],
    ) -> Result<Vec<u8>, ExtensionError> {
        assert_eq!(target.implementation_id, "test.echoer");
        assert_eq!(interface_id, EchoHandle::INTERFACE_ID);
        assert_eq!(interface_version, EchoHandle::INTERFACE_VERSION);
        assert!(EchoHandle::METHOD_IDS.contains(&method));
        self.dispatcher.dispatch_extension(context, method, payload)
    }
}

#[test]
fn generated_registration_keeps_the_instance_inside_the_guest() {
    let mut extension = Extension::new();
    Echoer.register_instance(&mut extension, ECHO_POINT.id());
    assert!(
        extension
            .typed_interface(ECHO_POINT.id(), std::any::type_name::<Echoer>())
            .is_some()
    );
}

#[test]
fn generated_handle_round_trips_method_calls() {
    let transport: SharedRpcTransport = Arc::new(TestTransport {
        dispatcher: Arc::new(Echoer),
    });
    let handle = EchoHandle::new(transport, RemoteTarget::new("test.echoer", 7));
    let registry = CoreExtensionRegistry::new();
    let context = ExtensionContext::new(&registry);
    assert_eq!(handle.echo(&context, "hi".into()).unwrap(), "echo:hi");
    assert_eq!(
        handle.fail(&context, "nope".into()).unwrap_err(),
        ExtensionError::new("nope")
    );
    assert_eq!(handle.ping(&context).unwrap(), 42);
}
