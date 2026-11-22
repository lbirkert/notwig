#[cfg(not(target_arch = "wasm32"))]
pub fn init() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_writer(std::io::stderr)
        .try_init();
}

#[cfg(target_arch = "wasm32")]
pub fn init() {
    tracing_wasm::set_as_global_default();
}
