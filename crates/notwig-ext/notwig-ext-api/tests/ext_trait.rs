use std::sync::Arc;

use notwig_ext_api::{
    CoreExtensionRegistry, Extension, ExtensionCall, ExtensionContext, ExtensionError,
    ExtensionPoint, ExtensionTrait, RemoteTarget, RpcTransport, SharedRpcTransport,
    notwig_ext_trait,
};

#[notwig_ext_trait(id = "test.echo", version = 1)]
pub trait Echo {
    fn echo(&self, context: &ExtensionContext<'_>, value: String)
    -> Result<String, ExtensionError>;
    fn fail(&self, context: &ExtensionContext<'_>, message: String) -> Result<(), ExtensionError>;
    fn ping(&self, context: &ExtensionContext<'_>) -> Result<u32, ExtensionError>;
}

const ECHO_POINT: ExtensionPoint<dyn Echo> = ExtensionPoint::new("test.echo.point");

#[derive(ExtensionCall)]
#[extension_call(Echo)]
struct Echoer;

#[notwig_ext_trait(id = "test.debug.echo", version = 1)]
pub trait DebugEcho: std::fmt::Debug {
    fn value(&self, context: &ExtensionContext<'_>) -> Result<u32, ExtensionError>;
}

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
    extension.register_extension(ECHO_POINT, Echoer);
    let calls = extension.extension_calls(ECHO_POINT.id());
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].interface_id(), EchoHandle::INTERFACE_ID);
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
    let debug_handle = DebugEchoHandle::new(
        Arc::new(TestTransport {
            dispatcher: Arc::new(Echoer),
        }),
        RemoteTarget::new("test.debug", 1),
    );
    assert!(format!("{debug_handle:?}").contains("DebugEchoHandle"));
    assert_eq!(
        handle.fail(&context, "nope".into()).unwrap_err(),
        ExtensionError::new("nope")
    );
    assert_eq!(handle.ping(&context).unwrap(), 42);
}

#[test]
fn generated_interface_bridge_creates_handles_and_processes_calls() {
    let transport: SharedRpcTransport = Arc::new(TestTransport {
        dispatcher: Arc::new(Echoer),
    });
    let handle =
        <dyn Echo as ExtensionTrait>::create_handle(transport, RemoteTarget::new("test.echoer", 7));
    let registry = CoreExtensionRegistry::new();
    let context = ExtensionContext::new(&registry);
    assert_eq!(
        <dyn Echo as ExtensionTrait>::INTERFACE_ID,
        EchoHandle::INTERFACE_ID
    );
    assert_eq!(handle.echo(&context, "hi".into()).unwrap(), "echo:hi");

    let request = EchoRequest::echo {
        value: "direct".into(),
    };
    let payload = notwig_ext_api::postcard::to_allocvec(&request).unwrap();
    let mut call = Echoer;
    let response = call.process(&context, "echo", &payload).unwrap();
    let response: EchoResponse = notwig_ext_api::postcard::from_bytes(&response).unwrap();
    assert!(matches!(response, EchoResponse::echo(Ok(value)) if value == "echo:direct"));
}
