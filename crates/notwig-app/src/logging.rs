#[cfg(not(target_arch = "wasm32"))]
use std::fs::OpenOptions;
#[cfg(not(target_arch = "wasm32"))]
use std::io::Write as _;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc;
#[cfg(target_arch = "wasm32")]
use std::sync::{Mutex, OnceLock};

use crate::clock::{SystemTime, UNIX_EPOCH};

/// Initializes logging for the current process.
///
/// The GUI and CLI write `app-<timestamp>-<pid>.log`, the supervised audio
/// engine writes `engine-<timestamp>-<pid>.log`, and internal helper
/// subprocesses (plugin scan/probe) write to stderr so the parent can forward
/// their output into the owning log through `tracing`. Browser builds buffer
/// the same app log under `<app>/logs` in OPFS.
#[cfg(not(target_arch = "wasm32"))]
pub fn init() {
    use tracing_subscriber::Layer;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let role = LogRole::detect();
    let console_default = if role == LogRole::Helper {
        "debug"
    } else {
        "info"
    };
    let console_filter = filter_from_env("RUST_LOG", console_default);
    let console_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_writer(std::io::stderr)
        .with_ansi(role != LogRole::Helper)
        .with_filter(console_filter);

    if role == LogRole::Helper {
        let _ = tracing_subscriber::registry()
            .with(console_layer)
            .try_init();
        return;
    }

    let file_filter = filter_from_env("NOTWIG_LOG", "debug");
    let mut log_path = None;
    let mut log_error = None;
    let file_layer = match open_log_file(role)
        .and_then(|(path, file)| NonBlockingLog::new(file).map(|writer| (path, writer)))
    {
        Ok((path, writer)) => {
            log_path = Some(path);
            Some(
                tracing_subscriber::fmt::layer()
                    .with_target(true)
                    .with_file(true)
                    .with_line_number(true)
                    .with_ansi(false)
                    .with_writer(writer)
                    .with_filter(file_filter),
            )
        }
        Err(error) => {
            log_error = Some(error);
            None
        }
    };

    let _ = tracing_subscriber::registry()
        .with(console_layer)
        .with(file_layer)
        .try_init();

    if let Some(path) = log_path {
        tracing::info!(path = %path.display(), "writing Notwig log file");
    }
    if let Some(error) = log_error {
        tracing::warn!(%error, "file logging is unavailable");
    }
}

#[cfg(target_arch = "wasm32")]
static WEB_LOG: OnceLock<Mutex<WebLogState>> = OnceLock::new();
#[cfg(target_arch = "wasm32")]
static WEB_FLUSHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(target_arch = "wasm32")]
const WEB_APP_DIR: &str = "notwig";
#[cfg(target_arch = "wasm32")]
const MAX_WEB_LOG_BUFFER: usize = 2 * 1024 * 1024;

#[cfg(target_arch = "wasm32")]
struct WebLogState {
    path: String,
    buffer: Vec<u8>,
}

#[cfg(target_arch = "wasm32")]
impl WebLogState {
    fn new() -> Self {
        Self {
            path: format!(
                "{WEB_APP_DIR}/logs/app-{}.log",
                timestamp_for_file_name(SystemTime::now())
            ),
            buffer: Vec::new(),
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn web_log_state() -> &'static Mutex<WebLogState> {
    WEB_LOG.get_or_init(|| Mutex::new(WebLogState::new()))
}

#[cfg(target_arch = "wasm32")]
pub fn init() {
    use tracing_subscriber::Layer;
    use tracing_subscriber::filter::LevelFilter;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let path = web_log_state().lock().ok().map(|state| state.path.clone());

    let mut config = tracing_wasm::WASMLayerConfigBuilder::new();
    config.set_max_level(tracing::Level::INFO);
    config.set_report_logs_in_timings(false);
    let console_layer = tracing_wasm::WASMLayer::new(config.build()).with_filter(LevelFilter::INFO);
    let file_layer = WebFileLayer.with_filter(LevelFilter::DEBUG);
    let _ = tracing_subscriber::registry()
        .with(console_layer)
        .with(file_layer)
        .try_init();

    if let Some(path) = path {
        tracing::info!(path = %path, "writing Notwig log file");
    }
}

/// Writes buffered browser log lines to OPFS.
///
/// The app frame loop calls this so formatting stays synchronous and cheap while
/// the asynchronous OPFS write runs in a local task.
#[cfg(target_arch = "wasm32")]
pub fn flush_web() {
    use std::sync::atomic::Ordering;

    if WEB_FLUSHING.swap(true, Ordering::AcqRel) {
        return;
    }
    let payload = match web_log_state().lock() {
        Ok(mut state) => {
            if state.buffer.is_empty() {
                WEB_FLUSHING.store(false, Ordering::Release);
                return;
            }
            Some((state.path.clone(), std::mem::take(&mut state.buffer)))
        }
        Err(_) => None,
    };
    let Some((path, bytes)) = payload else {
        WEB_FLUSHING.store(false, Ordering::Release);
        return;
    };
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(error) = crate::web::storage::append_opfs_file(&path, &bytes).await {
            let message = format!("notwig: could not write OPFS log file: {error}");
            web_sys::console::error_1(&wasm_bindgen::JsValue::from_str(&message));
        }
        WEB_FLUSHING.store(false, Ordering::Release);
    });
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy)]
struct WebFileLayer;

#[cfg(target_arch = "wasm32")]
impl<S> tracing_subscriber::Layer<S> for WebFileLayer
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let metadata = event.metadata();
        let mut visitor = WebLogVisitor::default();
        event.record(&mut visitor);
        let mut line = format!(
            "{} {:>5} {}",
            timestamp_for_log_line(SystemTime::now()),
            metadata.level(),
            metadata.target()
        );
        if !visitor.message.is_empty() {
            line.push(' ');
            line.push_str(&visitor.message);
        }
        line.push_str(&visitor.fields);
        if let Ok(mut state) = web_log_state().lock()
            && state.buffer.len() < MAX_WEB_LOG_BUFFER
        {
            state.buffer.extend_from_slice(line.as_bytes());
            state.buffer.push(b'\n');
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Default)]
struct WebLogVisitor {
    message: String,
    fields: String,
}

#[cfg(target_arch = "wasm32")]
impl WebLogVisitor {
    fn record(&mut self, field: &tracing::field::Field, value: &str) {
        use std::fmt::Write as _;

        if field.name() == "message" {
            if !self.message.is_empty() {
                self.message.push(' ');
            }
            self.message.push_str(value);
        } else {
            let _ = write!(self.fields, " {}={value}", field.name());
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl tracing::field::Visit for WebLogVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.record(field, value);
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let rendered = format!("{value:?}");
        self.record(field, &rendered);
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct NonBlockingLog {
    sender: mpsc::Sender<Vec<u8>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl NonBlockingLog {
    fn new(mut file: std::fs::File) -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel::<Vec<u8>>();
        std::thread::Builder::new()
            .name("notwig-log-writer".into())
            .spawn(move || {
                while let Ok(bytes) = receiver.recv() {
                    if file.write_all(&bytes).is_err() {
                        break;
                    }
                    let _ = file.flush();
                }
            })
            .map_err(|error| format!("start log writer: {error}"))?;
        Ok(Self { sender })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<'a> tracing_subscriber::fmt::writer::MakeWriter<'a> for NonBlockingLog {
    type Writer = NonBlockingLogWriter;

    fn make_writer(&'a self) -> Self::Writer {
        NonBlockingLogWriter {
            sender: self.sender.clone(),
            buffer: Vec::new(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct NonBlockingLogWriter {
    sender: mpsc::Sender<Vec<u8>>,
    buffer: Vec<u8>,
}

#[cfg(not(target_arch = "wasm32"))]
impl std::io::Write for NonBlockingLogWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for NonBlockingLogWriter {
    fn drop(&mut self) {
        if !self.buffer.is_empty() {
            let _ = self.sender.send(std::mem::take(&mut self.buffer));
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LogRole {
    App,
    Engine,
    Helper,
}

#[cfg(not(target_arch = "wasm32"))]
impl LogRole {
    fn detect() -> Self {
        match std::env::args_os()
            .nth(1)
            .and_then(|arg| arg.into_string().ok())
        {
            Some(command) if command == "engine-host" => Self::Engine,
            Some(command)
                if matches!(
                    command.as_str(),
                    "scan-plugins" | "load-plugin" | "probe-plugin"
                ) =>
            {
                Self::Helper
            }
            _ => Self::App,
        }
    }

    const fn file_prefix(self) -> &'static str {
        match self {
            Self::Engine => "engine",
            Self::App | Self::Helper => "app",
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn filter_from_env(variable: &str, default: &str) -> tracing_subscriber::EnvFilter {
    std::env::var(variable)
        .ok()
        .and_then(|value| tracing_subscriber::EnvFilter::try_new(value).ok())
        .unwrap_or_else(|| tracing_subscriber::EnvFilter::new(default))
}

#[cfg(not(target_arch = "wasm32"))]
fn open_log_file(role: LogRole) -> Result<(PathBuf, std::fs::File), String> {
    let dir = crate::app_paths::log_dir()
        .ok_or_else(|| "could not determine the Notwig application directory".to_string())?;
    std::fs::create_dir_all(&dir).map_err(|error| format!("create {}: {error}", dir.display()))?;
    let file_name = format!(
        "{}-{}-{}.log",
        role.file_prefix(),
        timestamp_for_file_name(SystemTime::now()),
        std::process::id()
    );
    let path = dir.join(file_name);
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("open {}: {error}", path.display()))?;
    Ok((path, file))
}

fn timestamp_for_file_name(now: SystemTime) -> String {
    let (year, month, day, hour, minute, second) = utc_parts(unix_seconds(now));
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}Z")
}

#[cfg(target_arch = "wasm32")]
fn timestamp_for_log_line(now: SystemTime) -> String {
    let (year, month, day, hour, minute, second) = utc_parts(unix_seconds(now));
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn unix_seconds(now: SystemTime) -> i64 {
    now.duration_since(UNIX_EPOCH).map_or(0, |duration| {
        i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
    })
}

/// Splits Unix seconds into UTC calendar fields using the civil-from-days
/// algorithm. Kept local so filename formatting does not add a date dependency.
fn utc_parts(seconds: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    (
        year,
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
        u32::try_from(seconds_of_day / 3_600).unwrap_or(0),
        u32::try_from(seconds_of_day % 3_600 / 60).unwrap_or(0),
        u32::try_from(seconds_of_day % 60).unwrap_or(0),
    )
}
