use std::sync::Arc;

use notwig_ext_api::{
    ExtensionError, RemoteTarget, RpcTransport, SharedRpcTransport, notwig_ext_trait,
};

#[notwig_ext_trait(id = "test.echo", version = 1)]
pub trait Echo {
    fn echo(&self, value: String) -> Result<String, ExtensionError>;
    fn fail(&self, message: String) -> Result<(), ExtensionError>;
    fn ping(&self) -> Result<u32, ExtensionError>;
}

struct Echoer;

impl Echo for Echoer {
    fn echo(&self, value: String) -> Result<String, ExtensionError> {
        Ok(format!("echo:{value}"))
    }

    fn fail(&self, message: String) -> Result<(), ExtensionError> {
        Err(ExtensionError::new(message))
    }

    fn ping(&self) -> Result<u32, ExtensionError> {
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
        interface_id: &'static str,
        interface_version: u32,
        method: &'static str,
        payload: &[u8],
    ) -> Result<Vec<u8>, ExtensionError> {
        assert_eq!(target.implementation_id, "test.echoer");
        assert_eq!(interface_id, EchoHandle::INTERFACE_ID);
        assert_eq!(interface_version, EchoHandle::INTERFACE_VERSION);
        assert!(EchoHandle::METHOD_IDS.contains(&method));
        self.dispatcher.dispatch_extension(method, payload)
    }
}

#[test]
fn generated_handle_round_trips_method_calls() {
    let transport: SharedRpcTransport = Arc::new(TestTransport {
        dispatcher: Arc::new(Echoer),
    });
    let handle = EchoHandle::new(transport, RemoteTarget::new("test.echoer", 7));
    assert_eq!(handle.echo("hi".into()).unwrap(), "echo:hi");
    assert_eq!(
        handle.fail("nope".into()).unwrap_err(),
        ExtensionError::new("nope")
    );
    assert_eq!(handle.ping().unwrap(), 42);
}
