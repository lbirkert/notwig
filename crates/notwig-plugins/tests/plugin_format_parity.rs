//! Cross-format parity scan for installed plugins.
//!
//! Run through `scripts/test-plugin-parity.sh`. The harness scans the
//! platform's installed plugin locations, groups descriptors that describe the
//! same plugin, then checks each group across the formats that are installed:
//!
//! - native editor window size and content (PNG captures),
//! - matched parameters randomized with one deterministic sequence,
//! - a generated stereo audio clip rendered through each format,
//! - rendered audio aligned by each format's reported latency.
//!
//! The harness intentionally runs on the process main thread because native
//! editor windows require it. It writes a JSON report and keeps PNG/WAV
//! artifacts for failed groups by default.
//!
//! `cargo test --workspace` invokes this target without plugin features; in
//! that case it prints a skip notice and exits successfully. The script sets
//! `NOTWIG_PLUGIN_PARITY=1` and enables the available format features.

#[cfg(all(
    target_os = "macos",
    any(
        feature = "vst3",
        feature = "vst2",
        feature = "clap",
        feature = "lv2",
        feature = "au",
        feature = "au3"
    )
))]
fn main() {
    macos::main();
}

#[cfg(not(all(
    target_os = "macos",
    any(
        feature = "vst3",
        feature = "vst2",
        feature = "clap",
        feature = "lv2",
        feature = "au",
        feature = "au3"
    )
)))]
fn main() {
    eprintln!(
        "plugin format parity requires macOS and at least one plugin format feature; skipping"
    );
}

#[cfg(all(
    target_os = "macos",
    any(
        feature = "vst3",
        feature = "vst2",
        feature = "clap",
        feature = "lv2",
        feature = "au",
        feature = "au3"
    )
))]
mod macos {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use notwig_core::Norm01;
    use notwig_core::dsp::{ParamSpec, ProcessContext, ProcessStatus, RenderConfig, TransportInfo};
    use notwig_plugins::{
        NativePluginWindow, PluginCapabilities, PluginDescriptor, PluginFormat, PluginHost,
        PluginInstance, prepare_window_host, pump_window_host,
    };
    use serde::{Deserialize, Serialize};

    const DEFAULT_SEED: u64 = 0x4e6f_7477_6967_0001;

    pub fn main() {
        if std::env::var_os("NOTWIG_PLUGIN_PARITY").is_none() {
            eprintln!(
                "skipping plugin format parity; run scripts/test-plugin-parity.sh or set NOTWIG_PLUGIN_PARITY=1"
            );
            return;
        }
        let options = match Options::parse(std::env::args().skip(1)) {
            Ok(options) => options,
            Err(error) => {
                eprintln!("plugin format parity: {error}");
                eprintln!("{}", Options::HELP);
                std::process::exit(2);
            }
        };
        if let (Some(group_path), Some(report_path)) =
            (&options.worker_group, &options.worker_report)
        {
            run_worker(&options, group_path, report_path);
            return;
        }
        match run(&options) {
            Ok(summary) => {
                eprintln!(
                    "plugin format parity: {} groups, {} passed, {} failed, {} skipped; report {}",
                    summary.groups,
                    summary.passed,
                    summary.failed,
                    summary.skipped,
                    options.report.display()
                );
                if summary.groups == 0 {
                    std::process::exit(2);
                }
                if summary.failed > 0 {
                    std::process::exit(1);
                }
            }
            Err(error) => {
                eprintln!("plugin format parity failed: {error}");
                std::process::exit(2);
            }
        }
    }

    fn run_worker(options: &Options, group_path: &Path, report_path: &Path) {
        prepare_window_host();
        let group: Group = if let Some(group) = std::fs::read(group_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        {
            group
        } else {
            eprintln!(
                "plugin format parity worker: could not read group {}",
                group_path.display()
            );
            std::process::exit(2);
        };
        let mut host = PluginHost::new();
        let report = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_group(&mut host, &group, options)
        })) {
            Ok(report) => report,
            Err(payload) => GroupReport::failed(
                &group,
                format!(
                    "plugin group panicked while under test: {}",
                    panic_message(&payload)
                ),
            ),
        };
        match serde_json::to_vec_pretty(&report) {
            Ok(bytes) => {
                if let Err(error) = std::fs::write(report_path, bytes) {
                    eprintln!(
                        "plugin format parity worker: write {}: {error}",
                        report_path.display()
                    );
                    std::process::exit(2);
                }
            }
            Err(error) => {
                eprintln!("plugin format parity worker: serialize report: {error}");
                std::process::exit(2);
            }
        }
    }

    fn run(options: &Options) -> Result<Summary, String> {
        if let Some(parent) = options.report.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!("create report directory {}: {error}", parent.display())
            })?;
        }
        let mut host = PluginHost::new();
        let descriptors = host.scan(&[]);
        let scanned_descriptors = descriptors.len();
        let groups = build_groups(descriptors, options);
        if groups.is_empty() {
            eprintln!("no plugin groups with at least two installed formats matched the filters");
            let report = Report {
                generated_utc: now_utc(),
                options: options.report_options(),
                scanned_descriptors,
                groups: Vec::new(),
            };
            write_report(&report, &options.report)?;
            return Ok(Summary::default());
        }
        eprintln!("scan found {} candidate plugin group(s)", groups.len());
        let mut report = Report {
            generated_utc: now_utc(),
            options: options.report_options(),
            scanned_descriptors,
            groups: Vec::new(),
        };
        let mut summary = Summary::default();
        for (index, group) in groups.iter().enumerate() {
            if let Some(max) = options.max_groups
                && index >= max
            {
                break;
            }
            eprintln!(
                "[{}/{}] {} — {}",
                index + 1,
                groups.len(),
                group.label(),
                group
                    .descriptors
                    .iter()
                    .map(|descriptor| descriptor.format.prefix())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let group_report = run_group_isolated(group, options, index);
            let status = group_report
                .get("status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("fail");
            match status {
                "pass" => summary.passed += 1,
                "fail" => summary.failed += 1,
                _ => summary.skipped += 1,
            }
            summary.groups += 1;
            eprintln!(
                "    {} ({} error(s), {} warning(s))",
                status,
                array_len(&group_report, "errors"),
                array_len(&group_report, "warnings")
            );
            report.groups.push(group_report);
            write_report(&report, &options.report)?;
        }
        write_report(&report, &options.report)?;
        Ok(summary)
    }

    // Each group runs in a fresh child process. Plugin editors are native code
    // and a single bad plugin can panic, segfault, or hang; isolating a group
    // keeps the rest of the scan alive and leaves the report for the failure.
    fn run_group_isolated(group: &Group, options: &Options, index: usize) -> serde_json::Value {
        let worker_dir =
            std::env::temp_dir().join(format!("notwig-parity-{}-{index}", std::process::id()));
        let group_path = worker_dir.join("group.json");
        let report_path = worker_dir.join("report.json");
        let result = (|| -> Result<serde_json::Value, String> {
            std::fs::create_dir_all(&worker_dir).map_err(|error| {
                format!("create worker directory {}: {error}", worker_dir.display())
            })?;
            let group_bytes =
                serde_json::to_vec(group).map_err(|error| format!("serialize group: {error}"))?;
            std::fs::write(&group_path, group_bytes)
                .map_err(|error| format!("write {}: {error}", group_path.display()))?;

            let executable = std::env::current_exe()
                .map_err(|error| format!("resolve current executable: {error}"))?;
            let mut command = Command::new(executable);
            command.args(std::env::args_os().skip(1));
            command.arg("--worker-group").arg(&group_path);
            command.arg("--worker-report").arg(&report_path);
            command.env("NOTWIG_PLUGIN_PARITY", "1");
            command.stdin(Stdio::null());
            command.stdout(Stdio::inherit());
            command.stderr(Stdio::inherit());
            let mut child = command
                .spawn()
                .map_err(|error| format!("spawn group worker: {error}"))?;

            let started = Instant::now();
            let status = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break Some(status),
                    Ok(None) => {}
                    Err(error) => return Err(format!("wait for group worker: {error}")),
                }
                if options.group_timeout_seconds > 0
                    && started.elapsed() >= Duration::from_secs(options.group_timeout_seconds)
                {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(Duration::from_millis(100));
            };

            let report = std::fs::read(&report_path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
            if let Some(report) = report {
                return Ok(report);
            }
            let error = match status {
                None => format!(
                    "group timed out after {} seconds",
                    options.group_timeout_seconds
                ),
                Some(status) => format!("group worker exited without a report ({status})"),
            };
            serde_json::to_value(GroupReport::failed(group, error))
                .map_err(|error| format!("serialize failed group: {error}"))
        })();
        let _ = std::fs::remove_dir_all(&worker_dir);
        result.unwrap_or_else(|error| {
            serde_json::to_value(GroupReport::failed(group, error))
                .unwrap_or(serde_json::Value::Null)
        })
    }

    fn array_len(value: &serde_json::Value, key: &str) -> usize {
        value
            .get(key)
            .and_then(serde_json::Value::as_array)
            .map_or(0, Vec::len)
    }

    #[derive(Default)]
    struct Summary {
        groups: usize,
        passed: usize,
        failed: usize,
        skipped: usize,
    }

    #[derive(Clone)]
    struct Options {
        sample_rate: u32,
        block_size: usize,
        seconds: f64,
        rounds: usize,
        seed: u64,
        audio_tolerance: f64,
        gui_mean_tolerance: f64,
        gui_pixel_threshold: u8,
        gui_max_fraction: f64,
        gui_settle_ms: u64,
        gui: bool,
        audio: bool,
        include_instruments: bool,
        ignore_latency: bool,
        keep_all_artifacts: bool,
        formats: Option<BTreeSet<PluginFormat>>,
        filter: Option<String>,
        max_groups: Option<usize>,
        group_timeout_seconds: u64,
        report: PathBuf,
        artifacts: PathBuf,
        worker_group: Option<PathBuf>,
        worker_report: Option<PathBuf>,
    }

    impl Default for Options {
        fn default() -> Self {
            let report = std::env::var_os("NOTWIG_PLUGIN_PARITY_REPORT").map_or_else(
                || PathBuf::from("target/plugin-parity/report.json"),
                PathBuf::from,
            );
            let artifacts = std::env::var_os("NOTWIG_PLUGIN_PARITY_ARTIFACTS").map_or_else(
                || {
                    report
                        .parent()
                        .map_or_else(|| PathBuf::from("target/plugin-parity"), Path::to_path_buf)
                },
                PathBuf::from,
            );
            Self {
                sample_rate: 48_000,
                block_size: 512,
                seconds: 1.0,
                rounds: 3,
                seed: DEFAULT_SEED,
                audio_tolerance: 2.0e-3,
                gui_mean_tolerance: 2.0,
                gui_pixel_threshold: 8,
                gui_max_fraction: 0.01,
                gui_settle_ms: 600,
                gui: true,
                audio: true,
                include_instruments: false,
                ignore_latency: false,
                keep_all_artifacts: false,
                formats: None,
                filter: None,
                max_groups: None,
                group_timeout_seconds: 180,
                report,
                artifacts,
                worker_group: None,
                worker_report: None,
            }
        }
    }

    impl Options {
        const HELP: &'static str = "\
Usage: plugin_format_parity [options]

Options:
  --filter=TEXT             Only groups whose name or vendor contains TEXT
  --formats=a,b,c           Restrict to formats (clap, vst3, vst2, lv2, au, au3)
  --seed=N                  Deterministic parameter seed (default: 0x4e6f747769670001)
  --rounds=N                Randomized renders per plugin group (default: 3)
  --seconds=N               Audio clip length per render (default: 1.0)
  --sample-rate=N           Render sample rate (default: 48000)
  --block-size=N            Render block size (default: 512)
  --audio-tolerance=F       Max absolute/RMS sample difference (default: 0.002)
  --gui-mean-tolerance=F    Mean per-channel pixel difference (default: 2.0)
  --gui-pixel-threshold=N   Per-channel diff that counts as changed (default: 8)
  --gui-max-fraction=F      Max fraction of changed pixels (default: 0.01)
  --gui-settle-ms=N         Time to pump editors before capture (default: 600)
  --no-gui                  Skip editor size/content comparison
  --no-audio                Skip audio rendering comparison
  --include-instruments     Include instruments that expose stereo audio I/O
  --ignore-latency          Do not fail groups whose reported latency differs
  --keep-all-artifacts      Save screenshots/WAVs for passing groups too
  --max-groups=N            Stop after N candidate groups
  --group-timeout-seconds=N Kill and fail a group that runs longer than N (default: 180)
  --report=PATH             JSON report path
  --artifacts=PATH          Artifact directory
  -h, --help                Show this help
";

        fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
            let mut options = Self::default();
            let mut args = args.peekable();
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "-h" | "--help" => {
                        print!("{}", Self::HELP);
                        std::process::exit(0);
                    }
                    "--no-gui" => options.gui = false,
                    "--no-audio" => options.audio = false,
                    "--include-instruments" => options.include_instruments = true,
                    "--ignore-latency" => options.ignore_latency = true,
                    "--keep-all-artifacts" => options.keep_all_artifacts = true,
                    _ => {
                        let (key, value) = arg
                            .split_once('=')
                            .map_or((arg.as_str(), None), |(key, value)| (key, Some(value)));
                        let take_value = |args: &mut std::iter::Peekable<_>| {
                            value
                                .map(ToString::to_string)
                                .or_else(|| args.next())
                                .ok_or_else(|| format!("{key} requires a value"))
                        };
                        match key {
                            "--filter" => options.filter = Some(take_value(&mut args)?),
                            "--formats" => {
                                let list = take_value(&mut args)?;
                                let mut formats = BTreeSet::new();
                                for name in list.split(',').map(str::trim).filter(|s| !s.is_empty())
                                {
                                    formats.insert(parse_format(name).ok_or_else(|| {
                                        format!("unknown plugin format {name:?}")
                                    })?);
                                }
                                options.formats = Some(formats);
                            }
                            "--seed" => {
                                options.seed = parse_value(&take_value(&mut args)?)?;
                            }
                            "--rounds" => {
                                options.rounds =
                                    parse_value::<usize>(&take_value(&mut args)?)?.max(1);
                            }
                            "--seconds" => {
                                options.seconds =
                                    parse_value::<f64>(&take_value(&mut args)?)?.max(0.01);
                            }
                            "--sample-rate" => {
                                options.sample_rate =
                                    parse_value::<u32>(&take_value(&mut args)?)?.max(1);
                            }
                            "--block-size" => {
                                options.block_size =
                                    parse_value::<usize>(&take_value(&mut args)?)?.max(1);
                            }
                            "--audio-tolerance" => {
                                options.audio_tolerance =
                                    parse_value::<f64>(&take_value(&mut args)?)?;
                            }
                            "--gui-mean-tolerance" => {
                                options.gui_mean_tolerance =
                                    parse_value::<f64>(&take_value(&mut args)?)?;
                            }
                            "--gui-pixel-threshold" => {
                                options.gui_pixel_threshold =
                                    parse_value::<u8>(&take_value(&mut args)?)?;
                            }
                            "--gui-max-fraction" => {
                                options.gui_max_fraction =
                                    parse_value::<f64>(&take_value(&mut args)?)?;
                            }
                            "--gui-settle-ms" => {
                                options.gui_settle_ms =
                                    parse_value::<u64>(&take_value(&mut args)?)?;
                            }
                            "--max-groups" => {
                                options.max_groups =
                                    Some(parse_value::<usize>(&take_value(&mut args)?)?);
                            }
                            "--group-timeout-seconds" => {
                                options.group_timeout_seconds =
                                    parse_value::<u64>(&take_value(&mut args)?)?;
                            }
                            "--worker-group" => {
                                options.worker_group = Some(PathBuf::from(take_value(&mut args)?));
                            }
                            "--worker-report" => {
                                options.worker_report = Some(PathBuf::from(take_value(&mut args)?));
                            }
                            "--report" => {
                                options.report = PathBuf::from(take_value(&mut args)?);
                            }
                            "--artifacts" => {
                                options.artifacts = PathBuf::from(take_value(&mut args)?);
                            }
                            _ => return Err(format!("unknown argument {arg:?}")),
                        }
                    }
                }
            }
            Ok(options)
        }

        fn report_options(&self) -> ReportOptions {
            ReportOptions {
                sample_rate: self.sample_rate,
                block_size: self.block_size,
                seconds: self.seconds,
                rounds: self.rounds,
                seed: self.seed,
                audio_tolerance: self.audio_tolerance,
                gui_mean_tolerance: self.gui_mean_tolerance,
                gui_pixel_threshold: self.gui_pixel_threshold,
                gui_max_fraction: self.gui_max_fraction,
                include_instruments: self.include_instruments,
                ignore_latency: self.ignore_latency,
                formats: self
                    .formats
                    .as_ref()
                    .map(|formats| formats.iter().copied().collect()),
            }
        }
    }

    fn parse_value<T: std::str::FromStr>(value: &str) -> Result<T, String> {
        value
            .parse()
            .map_err(|_| format!("invalid value {value:?}"))
    }

    fn parse_format(value: &str) -> Option<PluginFormat> {
        match value.to_ascii_lowercase().as_str() {
            "clap" => Some(PluginFormat::Clap),
            "vst3" => Some(PluginFormat::Vst3),
            "vst2" | "vst" => Some(PluginFormat::Vst2),
            "lv2" => Some(PluginFormat::Lv2),
            "au" | "au2" => Some(PluginFormat::Au),
            "au3" => Some(PluginFormat::Au3),
            _ => None,
        }
    }

    fn now_utc() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|duration| i64::try_from(duration.as_secs()).ok())
            .unwrap_or(0)
    }

    fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> &str {
        if let Some(message) = payload.downcast_ref::<&'static str>() {
            message
        } else if let Some(message) = payload.downcast_ref::<String>() {
            message.as_str()
        } else {
            "<non-string panic payload>"
        }
    }

    fn normalize_name(value: &str) -> String {
        value
            .chars()
            .filter(|character| character.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    }

    fn normalize_param_name(value: &str) -> String {
        let base = value.split('(').next().unwrap_or(value);
        normalize_name(base)
    }

    fn safe_name(value: &str) -> String {
        let mut name: String = value
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        name.truncate(90);
        if name.is_empty() {
            name.push_str("plugin");
        }
        name
    }

    #[derive(Serialize, Deserialize)]
    struct Group {
        name: String,
        vendor: String,
        descriptors: Vec<PluginDescriptor>,
    }

    impl Group {
        fn label(&self) -> String {
            if self.vendor.is_empty() {
                self.name.clone()
            } else {
                format!("{} — {}", self.name, self.vendor)
            }
        }

        fn key(&self) -> String {
            format!(
                "{}::{}",
                normalize_name(&self.name),
                normalize_name(&self.vendor)
            )
        }
    }

    fn build_groups(descriptors: Vec<PluginDescriptor>, options: &Options) -> Vec<Group> {
        let mut by_name: BTreeMap<String, BTreeMap<String, Vec<PluginDescriptor>>> =
            BTreeMap::new();
        for descriptor in descriptors {
            if !descriptor.format.enabled() {
                continue;
            }
            if let Some(formats) = &options.formats
                && !formats.contains(&descriptor.format)
            {
                continue;
            }
            by_name
                .entry(normalize_name(&descriptor.name))
                .or_default()
                .entry(normalize_name(&descriptor.vendor))
                .or_default()
                .push(descriptor);
        }

        let mut groups = Vec::new();
        for (_, mut vendors) in by_name {
            if let Some(mut missing_vendor) = vendors.remove("") {
                if vendors.len() == 1 {
                    if let Some((_, descriptors)) = vendors.iter_mut().next() {
                        descriptors.append(&mut missing_vendor);
                    }
                } else {
                    vendors.insert(String::new(), missing_vendor);
                }
            }
            for (vendor, mut descriptors) in vendors {
                descriptors.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.uid.cmp(&b.uid)));
                let mut by_format: BTreeMap<PluginFormat, PluginDescriptor> = BTreeMap::new();
                for descriptor in descriptors {
                    by_format.entry(descriptor.format).or_insert(descriptor);
                }
                if by_format.len() < 2 {
                    continue;
                }
                let descriptors: Vec<_> = by_format.into_values().collect();
                let name = descriptors
                    .iter()
                    .map(|descriptor| descriptor.name.as_str())
                    .next()
                    .unwrap_or_default()
                    .to_string();
                let group = Group {
                    name,
                    vendor: descriptors
                        .iter()
                        .map(|descriptor| descriptor.vendor.as_str())
                        .find(|vendor| !vendor.is_empty())
                        .unwrap_or(vendor.as_str())
                        .to_string(),
                    descriptors,
                };
                if let Some(filter) = &options.filter {
                    let needle = filter.to_ascii_lowercase();
                    if !group.name.to_ascii_lowercase().contains(&needle)
                        && !group.vendor.to_ascii_lowercase().contains(&needle)
                    {
                        continue;
                    }
                }
                groups.push(group);
            }
        }
        groups.sort_by(|a, b| {
            a.name
                .cmp(&b.name)
                .then_with(|| a.vendor.cmp(&b.vendor))
                .then_with(|| a.key().cmp(&b.key()))
        });
        groups
    }

    struct LoadedFormat {
        descriptor: PluginDescriptor,
        instance: Box<dyn PluginInstance>,
        capabilities: PluginCapabilities,
        params: Vec<ParamSpec>,
        latency: u32,
        rounds: Vec<Vec<f32>>,
        errors: Vec<String>,
    }

    fn run_group(host: &mut PluginHost, group: &Group, options: &Options) -> GroupReport {
        let config = RenderConfig {
            sample_rate: options.sample_rate,
            max_block_size: options.block_size,
            channels: 2,
            tempo_bpm: 120.0,
        };
        let input = test_signal(options.sample_rate, options.seconds);
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut loaded = Vec::new();

        for descriptor in &group.descriptors {
            if descriptor.is_instrument && !options.include_instruments {
                warnings.push(format!(
                    "{} skipped: instruments are excluded (use --include-instruments)",
                    descriptor.format.prefix()
                ));
                continue;
            }
            match load_format(host, descriptor, config) {
                LoadOutcome::Loaded(format) => loaded.push(format),
                LoadOutcome::Skipped(reason) => {
                    warnings.push(format!("{} skipped: {reason}", descriptor.format.prefix()));
                }
                LoadOutcome::Failed(error) => {
                    errors.push(format!("{}: {error}", descriptor.format.prefix()));
                }
            }
        }

        let shared_params = shared_visible_params(&loaded);
        if shared_params.is_empty() {
            warnings.push(
                "no visible parameter names are shared across the installed formats; audio was compared with default state"
                    .into(),
            );
        } else {
            for format in &loaded {
                let names = visible_param_names(&format.params);
                let missing: Vec<_> = shared_params
                    .iter()
                    .filter(|name| !names.contains(*name))
                    .take(8)
                    .cloned()
                    .collect();
                if !missing.is_empty() {
                    warnings.push(format!(
                        "{} is missing shared parameter name(s) {:?}",
                        format.descriptor.format.prefix(),
                        missing
                    ));
                }
                let extra = names.difference(&shared_params).count();
                if extra > 0 {
                    warnings.push(format!(
                        "{} exposes {extra} visible parameter(s) not shared by every format",
                        format.descriptor.format.prefix()
                    ));
                }
            }
        }

        let mut audio_report = AudioReport::default();
        if options.audio {
            run_audio(
                &mut loaded,
                &input,
                config,
                &shared_params,
                options,
                group,
                &mut audio_report,
                &mut errors,
            );
        } else {
            audio_report.skipped = Some("disabled by --no-audio".into());
        }

        let latencies: BTreeSet<u32> = loaded.iter().map(|format| format.latency).collect();
        if !options.ignore_latency && latencies.len() > 1 {
            errors.push(format!(
                "reported latency differs across formats: {:?}",
                loaded
                    .iter()
                    .map(|format| (format.descriptor.format.prefix(), format.latency))
                    .collect::<Vec<_>>()
            ));
        }

        let mut gui_report = GuiReport::default();
        if options.gui {
            run_gui(
                &loaded,
                options,
                group,
                &mut gui_report,
                &mut errors,
                &mut warnings,
            );
        } else {
            gui_report.skipped = Some("disabled by --no-gui".into());
        }

        let fail = !errors.is_empty()
            || audio_report
                .comparisons
                .iter()
                .any(|comparison| !comparison.pass)
            || gui_report
                .comparisons
                .iter()
                .any(|comparison| !comparison.pass);
        let skipped = loaded.len() < 2 && errors.is_empty();
        let status = if fail {
            "fail"
        } else if skipped {
            "skip"
        } else {
            "pass"
        };

        let keep_artifacts = options.keep_all_artifacts || fail;
        let artifacts = if keep_artifacts {
            match write_group_artifacts(group, &input, &loaded, &gui_report, options) {
                Ok(paths) => Some(paths),
                Err(error) => {
                    errors.push(format!("could not write artifacts: {error}"));
                    None
                }
            }
        } else {
            None
        };
        let status = if errors.is_empty() { status } else { "fail" };

        GroupReport {
            name: group.name.clone(),
            vendor: group.vendor.clone(),
            status: status.into(),
            descriptors: loaded
                .iter()
                .map(|format| DescriptorReport {
                    format: format.descriptor.format,
                    name: format.descriptor.name.clone(),
                    vendor: format.descriptor.vendor.clone(),
                    uid: format.descriptor.uid.clone(),
                    version: format.descriptor.version.clone(),
                    path: format.descriptor.path.clone(),
                    capabilities: Some(format.capabilities),
                    parameter_count: format.params.len(),
                    latency_samples: format.latency,
                })
                .collect(),
            audio: audio_report,
            gui: gui_report,
            warnings,
            errors,
            artifacts,
        }
    }

    enum LoadOutcome {
        Loaded(LoadedFormat),
        Skipped(String),
        Failed(String),
    }

    fn load_format(
        host: &mut PluginHost,
        descriptor: &PluginDescriptor,
        config: RenderConfig,
    ) -> LoadOutcome {
        let mut instance = match host.load(descriptor) {
            Ok(instance) => instance,
            Err(error) => return LoadOutcome::Failed(format!("load failed: {error}")),
        };
        let capabilities = instance.capabilities();
        if capabilities.audio_inputs < 2 || capabilities.audio_outputs < 2 {
            return LoadOutcome::Skipped(format!(
                "not a stereo effect (inputs {}, outputs {})",
                capabilities.audio_inputs, capabilities.audio_outputs
            ));
        }
        if let Err(error) = instance.prepare(config) {
            return LoadOutcome::Failed(format!("prepare failed: {error}"));
        }
        let transport = TransportInfo::default();
        let context = ProcessContext {
            config,
            sample_position: 0,
            playing: true,
            tempo_bpm: config.tempo_bpm,
            transport: &transport,
        };
        let mut block = vec![0.0_f32; config.max_block_size * 2];
        let status = instance.process(&context, &mut block, &[]);
        if status.overloaded {
            return LoadOutcome::Failed("warm-up block reported overload".into());
        }
        let latency = instance.latency().samples;
        let params = instance.describe_parameters();
        LoadOutcome::Loaded(LoadedFormat {
            descriptor: descriptor.clone(),
            instance,
            capabilities,
            params,
            latency,
            rounds: Vec::new(),
            errors: Vec::new(),
        })
    }

    fn visible_param_names(params: &[ParamSpec]) -> BTreeSet<String> {
        params
            .iter()
            .filter(|param| param.flags.visible)
            .map(|param| normalize_param_name(&param.name))
            .collect()
    }

    fn shared_visible_params(formats: &[LoadedFormat]) -> BTreeSet<String> {
        let mut iter = formats.iter();
        let Some(first) = iter.next() else {
            return BTreeSet::new();
        };
        let mut shared = visible_param_names(&first.params);
        for format in iter {
            let names = visible_param_names(&format.params);
            shared = shared.intersection(&names).cloned().collect();
        }
        shared
    }

    fn param_map(params: &[ParamSpec]) -> BTreeMap<String, &ParamSpec> {
        let mut map = BTreeMap::new();
        for param in params.iter().filter(|param| param.flags.visible) {
            map.entry(normalize_param_name(&param.name))
                .or_insert(param);
        }
        map
    }

    #[allow(clippy::too_many_arguments)]
    fn run_audio(
        loaded: &mut [LoadedFormat],
        input: &[f32],
        config: RenderConfig,
        shared_params: &BTreeSet<String>,
        options: &Options,
        group: &Group,
        report: &mut AudioReport,
        errors: &mut Vec<String>,
    ) {
        if loaded.len() < 2 {
            report.skipped = Some("fewer than two usable audio formats".into());
            return;
        }
        let mut values = Vec::with_capacity(options.rounds);
        for round in 0..options.rounds {
            let seed = options.seed
                ^ stable_hash(&group.key())
                ^ (u64::try_from(round)
                    .unwrap_or(u64::MAX)
                    .wrapping_mul(0x9e37_79b9_7f4a_7c15));
            values.push(random_param_values(seed, shared_params));
        }
        for (round_index, round_values) in values.iter().enumerate() {
            for format in loaded.iter_mut() {
                let mut round_errors = Vec::new();
                let output = render_round(format, config, input, round_values, &mut round_errors);
                if !round_errors.is_empty() {
                    format.errors.extend(
                        round_errors
                            .into_iter()
                            .map(|error| format!("round {}: {error}", round_index + 1)),
                    );
                }
                format.rounds.push(output);
            }
        }

        let reference_index = 0;
        let reference = &loaded[reference_index];
        for round in 0..options.rounds {
            let reference_output = reference.rounds.get(round).map_or(&[][..], Vec::as_slice);
            for format in &loaded[reference_index + 1..] {
                let output = format.rounds.get(round).map_or(&[][..], Vec::as_slice);
                let comparison = compare_audio(
                    reference,
                    reference_output,
                    format,
                    output,
                    options.audio_tolerance,
                    round,
                    options,
                );
                if !comparison.pass {
                    errors.push(format!(
                        "{} round {} differs from {} (max {:.6}, rms {:.6})",
                        format.descriptor.format.prefix(),
                        round + 1,
                        reference.descriptor.format.prefix(),
                        comparison.max_abs_diff,
                        comparison.rms_diff
                    ));
                }
                report.comparisons.push(comparison);
            }
        }
        for format in loaded.iter() {
            let mut rms = Vec::with_capacity(format.rounds.len());
            for output in &format.rounds {
                rms.push(signal_rms(output));
            }
            report.formats.push(FormatAudioSummary {
                format: format.descriptor.format,
                latency_samples: format.latency,
                parameter_count: format.params.len(),
                rounds: format.rounds.len(),
                rms,
                errors: format.errors.clone(),
            });
        }
    }

    fn render_round(
        format: &mut LoadedFormat,
        config: RenderConfig,
        input: &[f32],
        values: &BTreeMap<String, f32>,
        errors: &mut Vec<String>,
    ) -> Vec<f32> {
        if let Err(error) = format.instance.prepare(config) {
            errors.push(format!("prepare failed: {error}"));
            return Vec::new();
        }
        let by_name = param_map(&format.params);
        for (name, norm) in values {
            let Some(spec) = by_name.get(name) else {
                continue;
            };
            let value = spec.value_from_norm(Norm01::clamped(*norm));
            if value.as_f64().is_some_and(|value| !value.is_finite()) {
                errors.push(format!(
                    "parameter {:?} produced a non-finite value",
                    spec.name
                ));
                continue;
            }
            format.instance.set_param(spec.id, &value);
        }
        render_clip(format, config, input, errors)
    }

    fn render_clip(
        format: &mut LoadedFormat,
        config: RenderConfig,
        input: &[f32],
        errors: &mut Vec<String>,
    ) -> Vec<f32> {
        let transport = TransportInfo::default();
        let mut output = Vec::with_capacity(input.len());
        let mut sample_position = 0_u64;
        for chunk in input.chunks(config.max_block_size * 2) {
            let mut block = chunk.to_vec();
            let frames = block.len() / 2;
            let context = ProcessContext {
                config,
                sample_position,
                playing: true,
                tempo_bpm: config.tempo_bpm,
                transport: &transport,
            };
            let status: ProcessStatus = format.instance.process(&context, &mut block, &[]);
            if status.overloaded {
                errors.push(format!(
                    "process block at sample {sample_position} overloaded"
                ));
                break;
            }
            output.extend_from_slice(&block);
            sample_position += u64::try_from(frames).unwrap_or(0);
        }
        output
    }

    fn random_param_values(seed: u64, names: &BTreeSet<String>) -> BTreeMap<String, f32> {
        let mut rng = SplitMix64::new(seed);
        names
            .iter()
            .map(|name| (name.clone(), rng.next_f32()))
            .collect()
    }

    fn stable_hash(value: &str) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for byte in value.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }

    struct SplitMix64 {
        state: u64,
    }

    impl SplitMix64 {
        fn new(seed: u64) -> Self {
            Self { state: seed }
        }

        fn next_u64(&mut self) -> u64 {
            self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = self.state;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        }

        fn next_f32(&mut self) -> f32 {
            let value = self.next_u64() >> 40;
            value as f32 / (1_u32 << 24) as f32
        }
    }

    fn test_signal(sample_rate: u32, seconds: f64) -> Vec<f32> {
        let frames = (f64::from(sample_rate) * seconds).round().max(1.0) as usize;
        let mut signal = Vec::with_capacity(frames * 2);
        let mut noise = SplitMix64::new(0x6e6f_6973_6500_0001);
        for frame in 0..frames {
            let time = frame as f64 / f64::from(sample_rate);
            let left = 0.22 * (std::f64::consts::TAU * 440.0 * time).sin()
                + 0.08 * (std::f64::consts::TAU * 1_234.0 * time).sin()
                + 0.03 * (f64::from(noise.next_f32()) * 2.0 - 1.0);
            let right = 0.2 * (std::f64::consts::TAU * 523.25 * time).sin()
                + 0.07 * (std::f64::consts::TAU * 1_760.0 * time).sin()
                + 0.03 * (f64::from(noise.next_f32()) * 2.0 - 1.0);
            signal.push(left as f32);
            signal.push(right as f32);
        }
        signal
    }

    fn align<'a>(
        reference: &'a [f32],
        format: &'a [f32],
        reference_latency: u32,
        latency: u32,
    ) -> (&'a [f32], &'a [f32]) {
        let reference_start = usize::try_from(reference_latency)
            .unwrap_or(usize::MAX)
            .min(reference.len());
        let format_start = usize::try_from(latency)
            .unwrap_or(usize::MAX)
            .min(format.len());
        let length = (reference.len() - reference_start).min(format.len() - format_start);
        (
            &reference[reference_start..reference_start + length],
            &format[format_start..format_start + length],
        )
    }

    fn compare_audio(
        reference: &LoadedFormat,
        reference_output: &[f32],
        format: &LoadedFormat,
        output: &[f32],
        tolerance: f64,
        round: usize,
        options: &Options,
    ) -> AudioComparison {
        let (aligned_reference, aligned_format) =
            align(reference_output, output, reference.latency, format.latency);
        let mut max_abs_diff = 0.0_f64;
        let mut squared_sum = 0.0_f64;
        let mut reference_squared_sum = 0.0_f64;
        let mut format_squared_sum = 0.0_f64;
        let mut compared = 0_usize;
        for (left, right) in aligned_reference.iter().zip(aligned_format) {
            let left = f64::from(*left);
            let right = f64::from(*right);
            let diff = (left - right).abs();
            max_abs_diff = max_abs_diff.max(diff);
            squared_sum += diff * diff;
            reference_squared_sum += left * left;
            format_squared_sum += right * right;
            compared += 1;
        }
        let rms_diff = if compared == 0 {
            0.0
        } else {
            (squared_sum / compared as f64).sqrt()
        };
        let reference_rms = if compared == 0 {
            0.0
        } else {
            (reference_squared_sum / compared as f64).sqrt()
        };
        let format_rms = if compared == 0 {
            0.0
        } else {
            (format_squared_sum / compared as f64).sqrt()
        };
        let latency_mismatch = reference.latency != format.latency;
        let pass = compared > 0
            && max_abs_diff <= tolerance
            && rms_diff <= tolerance
            && (!latency_mismatch || options.ignore_latency);
        AudioComparison {
            reference_format: reference.descriptor.format,
            format: format.descriptor.format,
            round: round + 1,
            reference_latency: reference.latency,
            latency: format.latency,
            latency_mismatch,
            compared_samples: compared,
            max_abs_diff,
            rms_diff,
            reference_rms,
            format_rms,
            pass,
        }
    }

    fn signal_rms(signal: &[f32]) -> f64 {
        if signal.is_empty() {
            return 0.0;
        }
        let sum: f64 = signal
            .iter()
            .map(|sample| {
                let sample = f64::from(*sample);
                sample * sample
            })
            .sum();
        (sum / signal.len() as f64).sqrt()
    }

    #[allow(clippy::too_many_arguments)]
    fn run_gui(
        loaded: &[LoadedFormat],
        options: &Options,
        group: &Group,
        report: &mut GuiReport,
        errors: &mut Vec<String>,
        warnings: &mut Vec<String>,
    ) {
        let supported: Vec<_> = loaded
            .iter()
            .filter(|format| format.capabilities.has_editor)
            .collect();
        if supported.len() < 2 {
            report.skipped = Some(format!(
                "fewer than two formats report an editor ({} of {})",
                supported.len(),
                loaded.len()
            ));
            return;
        }
        let mut captures = Vec::new();
        for format in &supported {
            match capture_editor(format, options) {
                Ok(capture) => captures.push(capture),
                Err(error) => errors.push(format!(
                    "{} editor capture failed: {error}",
                    format.descriptor.format.prefix()
                )),
            }
        }
        if captures.len() < 2 {
            report.skipped = Some("fewer than two editor captures succeeded".into());
            return;
        }
        if captures.len() < supported.len() {
            warnings.push("some formats with an advertised editor could not be captured".into());
        }
        let reference = &captures[0];
        for capture in &captures[1..] {
            let comparison = compare_gui(reference, capture, options);
            if !comparison.pass {
                errors.push(format!(
                    "{} GUI differs from {} (mean {:.3}, changed {:.3}%)",
                    capture.format.prefix(),
                    reference.format.prefix(),
                    comparison.mean_abs_diff,
                    comparison.changed_fraction * 100.0
                ));
            }
            report.comparisons.push(comparison);
        }
        for capture in &captures {
            report.formats.push(GuiFormatSummary {
                format: capture.format,
                width: capture.width,
                height: capture.height,
                logical_size: capture.logical_size,
            });
        }
        report.group_key = Some(group.key());
    }

    struct GuiCapture {
        format: PluginFormat,
        width: u32,
        height: u32,
        logical_size: (u32, u32),
        rgb: Vec<u8>,
        png: Vec<u8>,
        stable: bool,
    }

    fn capture_editor(format: &LoadedFormat, options: &Options) -> Result<GuiCapture, String> {
        let mut window = NativePluginWindow::open(&format.descriptor, options.sample_rate, None)
            .map_err(|error| error.to_string())?;
        let mut stable = true;
        let settle_frames = (options.gui_settle_ms / 16).max(1);
        for _ in 0..settle_frames {
            if !window.is_open() {
                stable = false;
                break;
            }
            window.pump();
            pump_window_host();
            std::thread::sleep(Duration::from_millis(16));
        }
        let logical_size = window.size();
        let png = window.capture_png().map_err(|error| error.to_string())?;
        drop(window);
        let image = image::load_from_memory(&png)
            .map_err(|error| format!("decode PNG: {error}"))?
            .to_rgb8();
        let (width, height) = image.dimensions();
        if width == 0 || height == 0 {
            return Err("captured image is empty".into());
        }
        Ok(GuiCapture {
            format: format.descriptor.format,
            width,
            height,
            logical_size,
            rgb: image.into_raw(),
            png,
            stable,
        })
    }

    fn compare_gui(
        reference: &GuiCapture,
        capture: &GuiCapture,
        options: &Options,
    ) -> GuiComparison {
        let size_match = reference.logical_size == capture.logical_size;
        let dimensions_match =
            reference.width == capture.width && reference.height == capture.height;
        let mut mean_abs_diff = 0.0_f64;
        let mut changed_fraction = 0.0_f64;
        let mut max_channel_diff = 0_u8;
        let mut diff_png = None;
        if dimensions_match {
            let mut sum = 0_u64;
            let mut changed = 0_usize;
            let mut diff_image = image::RgbImage::new(reference.width, reference.height);
            for (pixel_index, (left, right)) in reference
                .rgb
                .chunks_exact(3)
                .zip(capture.rgb.chunks_exact(3))
                .enumerate()
            {
                let red = left[0].abs_diff(right[0]);
                let green = left[1].abs_diff(right[1]);
                let blue = left[2].abs_diff(right[2]);
                sum += u64::from(red) + u64::from(green) + u64::from(blue);
                max_channel_diff = max_channel_diff.max(red).max(green).max(blue);
                if red.max(green).max(blue) > options.gui_pixel_threshold {
                    changed += 1;
                }
                let x = u32::try_from(pixel_index % reference.width as usize).unwrap_or(0);
                let y = u32::try_from(pixel_index / reference.width as usize).unwrap_or(0);
                diff_image.put_pixel(
                    x,
                    y,
                    image::Rgb([
                        red.saturating_mul(8),
                        green.saturating_mul(8),
                        blue.saturating_mul(8),
                    ]),
                );
            }
            mean_abs_diff = sum as f64 / reference.rgb.len() as f64;
            changed_fraction =
                changed as f64 / (f64::from(reference.width) * f64::from(reference.height));
            diff_png = Some(encode_png(&diff_image));
        }
        let pass = size_match
            && dimensions_match
            && reference.stable
            && capture.stable
            && mean_abs_diff <= options.gui_mean_tolerance
            && changed_fraction <= options.gui_max_fraction;
        GuiComparison {
            reference_format: reference.format,
            format: capture.format,
            reference_size: reference.logical_size,
            size: capture.logical_size,
            size_match,
            dimensions_match,
            reference_pixels: (reference.width, reference.height),
            pixels: (capture.width, capture.height),
            mean_abs_diff,
            changed_fraction,
            max_channel_diff,
            pass,
            diff_png,
            reference_png: Some(reference.png.clone()),
            format_png: Some(capture.png.clone()),
        }
    }

    fn encode_png(image: &image::RgbImage) -> Vec<u8> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut bytes, image::ImageFormat::Png)
            .expect("encoding an in-memory PNG cannot fail");
        bytes.into_inner()
    }

    fn write_group_artifacts(
        group: &Group,
        input: &[f32],
        loaded: &[LoadedFormat],
        gui: &GuiReport,
        options: &Options,
    ) -> Result<ArtifactReport, String> {
        let dir = options.artifacts.join(safe_name(&group.key()));
        std::fs::create_dir_all(&dir)
            .map_err(|error| format!("create {}: {error}", dir.display()))?;
        let mut report = ArtifactReport {
            directory: dir.clone(),
            input_wav: None,
            audio_wavs: Vec::new(),
            screenshots: Vec::new(),
        };
        let input_path = dir.join("input.wav");
        write_wav(&input_path, options.sample_rate, input)?;
        report.input_wav = Some(input_path);

        for format in loaded {
            for (round, output) in format.rounds.iter().enumerate() {
                let path = dir.join(format!(
                    "{}_round{}.wav",
                    format.descriptor.format.prefix(),
                    round + 1
                ));
                write_wav(&path, options.sample_rate, output)?;
                report.audio_wavs.push(path);
            }
        }
        for comparison in &gui.comparisons {
            if let Some(png) = &comparison.reference_png {
                let path = dir.join(format!(
                    "{}_reference.png",
                    comparison.reference_format.prefix()
                ));
                std::fs::write(&path, png)
                    .map_err(|error| format!("write {}: {error}", path.display()))?;
                report.screenshots.push(path);
            }
            if let Some(png) = &comparison.format_png {
                let path = dir.join(format!("{}_candidate.png", comparison.format.prefix()));
                std::fs::write(&path, png)
                    .map_err(|error| format!("write {}: {error}", path.display()))?;
                report.screenshots.push(path);
            }
            if let Some(png) = &comparison.diff_png {
                let path = dir.join(format!(
                    "diff_{}_{}.png",
                    comparison.reference_format.prefix(),
                    comparison.format.prefix()
                ));
                std::fs::write(&path, png)
                    .map_err(|error| format!("write {}: {error}", path.display()))?;
                report.screenshots.push(path);
            }
        }
        Ok(report)
    }

    fn write_wav(path: &Path, sample_rate: u32, samples: &[f32]) -> Result<(), String> {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(path, spec)
            .map_err(|error| format!("create {}: {error}", path.display()))?;
        for sample in samples {
            writer
                .write_sample(*sample)
                .map_err(|error| format!("write {}: {error}", path.display()))?;
        }
        writer
            .finalize()
            .map_err(|error| format!("finalize {}: {error}", path.display()))
    }

    fn write_report(report: &Report, path: &Path) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(report)
            .map_err(|error| format!("serialize report: {error}"))?;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, bytes)
            .map_err(|error| format!("write {}: {error}", temporary.display()))?;
        std::fs::rename(&temporary, path)
            .map_err(|error| format!("replace {}: {error}", path.display()))
    }

    #[derive(Serialize)]
    struct Report {
        generated_utc: i64,
        options: ReportOptions,
        scanned_descriptors: usize,
        groups: Vec<serde_json::Value>,
    }

    #[derive(Serialize)]
    struct ReportOptions {
        sample_rate: u32,
        block_size: usize,
        seconds: f64,
        rounds: usize,
        seed: u64,
        audio_tolerance: f64,
        gui_mean_tolerance: f64,
        gui_pixel_threshold: u8,
        gui_max_fraction: f64,
        include_instruments: bool,
        ignore_latency: bool,
        formats: Option<Vec<PluginFormat>>,
    }

    #[derive(Serialize)]
    struct GroupReport {
        name: String,
        vendor: String,
        status: String,
        descriptors: Vec<DescriptorReport>,
        audio: AudioReport,
        gui: GuiReport,
        warnings: Vec<String>,
        errors: Vec<String>,
        artifacts: Option<ArtifactReport>,
    }

    impl GroupReport {
        fn failed(group: &Group, error: String) -> Self {
            Self {
                name: group.name.clone(),
                vendor: group.vendor.clone(),
                status: "fail".into(),
                descriptors: Vec::new(),
                audio: AudioReport::default(),
                gui: GuiReport::default(),
                warnings: Vec::new(),
                errors: vec![error],
                artifacts: None,
            }
        }
    }

    #[derive(Serialize)]
    struct DescriptorReport {
        format: PluginFormat,
        name: String,
        vendor: String,
        uid: String,
        version: String,
        path: PathBuf,
        capabilities: Option<PluginCapabilities>,
        parameter_count: usize,
        latency_samples: u32,
    }

    #[derive(Serialize, Default)]
    struct AudioReport {
        skipped: Option<String>,
        formats: Vec<FormatAudioSummary>,
        comparisons: Vec<AudioComparison>,
    }

    #[derive(Serialize)]
    struct FormatAudioSummary {
        format: PluginFormat,
        latency_samples: u32,
        parameter_count: usize,
        rounds: usize,
        rms: Vec<f64>,
        errors: Vec<String>,
    }

    #[derive(Serialize)]
    struct AudioComparison {
        reference_format: PluginFormat,
        format: PluginFormat,
        round: usize,
        reference_latency: u32,
        latency: u32,
        latency_mismatch: bool,
        compared_samples: usize,
        max_abs_diff: f64,
        rms_diff: f64,
        reference_rms: f64,
        format_rms: f64,
        pass: bool,
    }

    #[derive(Serialize, Default)]
    struct GuiReport {
        skipped: Option<String>,
        group_key: Option<String>,
        formats: Vec<GuiFormatSummary>,
        comparisons: Vec<GuiComparison>,
    }

    #[derive(Serialize)]
    struct GuiFormatSummary {
        format: PluginFormat,
        width: u32,
        height: u32,
        logical_size: (u32, u32),
    }

    #[derive(Serialize)]
    struct GuiComparison {
        reference_format: PluginFormat,
        format: PluginFormat,
        reference_size: (u32, u32),
        size: (u32, u32),
        size_match: bool,
        dimensions_match: bool,
        reference_pixels: (u32, u32),
        pixels: (u32, u32),
        mean_abs_diff: f64,
        changed_fraction: f64,
        max_channel_diff: u8,
        pass: bool,
        #[serde(skip)]
        diff_png: Option<Vec<u8>>,
        #[serde(skip)]
        reference_png: Option<Vec<u8>>,
        #[serde(skip)]
        format_png: Option<Vec<u8>>,
    }

    #[derive(Serialize)]
    struct ArtifactReport {
        directory: PathBuf,
        input_wav: Option<PathBuf>,
        audio_wavs: Vec<PathBuf>,
        screenshots: Vec<PathBuf>,
    }
}
