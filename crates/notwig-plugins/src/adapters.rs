use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use crate::host::HostAdapter;
use crate::rack::RackPluginInstance;
use crate::types::{PluginDescriptor, PluginError, PluginFormat, PluginInstance};

fn descriptor_from_info(
    format: PluginFormat,
    info: notwig_rack::core::PluginInfo,
) -> PluginDescriptor {
    let metadata = std::fs::metadata(&info.path).ok();
    let binary_size = metadata.as_ref().map_or(0, std::fs::Metadata::len);
    let modified_utc = metadata
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| {
            i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
        });
    PluginDescriptor {
        type_id: PluginDescriptor::type_id_for(format, &info.unique_id),
        format,
        path: info.path,
        name: info.name,
        vendor: info.vendor,
        version: info.version.to_string(),
        uid: info.unique_id,
        is_instrument: matches!(info.category, notwig_rack::core::PluginCategory::Instrument),
        binary_size,
        modified_utc,
    }
}

pub(crate) fn info_from_descriptor(descriptor: &PluginDescriptor) -> notwig_rack::core::PluginInfo {
    notwig_rack::core::PluginInfo {
        name: descriptor.name.clone(),
        vendor: descriptor.vendor.clone(),
        version: descriptor.version.parse().unwrap_or(0),
        category: if descriptor.is_instrument {
            notwig_rack::core::PluginCategory::Instrument
        } else {
            notwig_rack::core::PluginCategory::Effect
        },
        path: descriptor.path.clone(),
        unique_id: descriptor.uid.clone(),
        format: descriptor.format.prefix(),
        has_editor: true,
        accepts_midi: descriptor.is_instrument,
    }
}

#[cfg(target_os = "macos")]
#[allow(unreachable_patterns)]
pub(crate) fn load_plugin(
    descriptor: &PluginDescriptor,
) -> Result<Box<dyn notwig_rack::core::Plugin<f32>>, PluginError> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        load_plugin_inner(descriptor)
    })) {
        Ok(result) => result,
        Err(payload) => Err(PluginError::Load(format!(
            "plugin load panicked: {}",
            panic_message(&payload)
        ))),
    }
}

#[cfg(target_os = "macos")]
#[allow(unreachable_patterns)]
fn load_plugin_inner(
    descriptor: &PluginDescriptor,
) -> Result<Box<dyn notwig_rack::core::Plugin<f32>>, PluginError> {
    match descriptor.format {
        #[cfg(feature = "vst3")]
        PluginFormat::Vst3 => {
            use notwig_rack::core::scanner::PluginScanner;
            let scanner = notwig_rack::vst3::Vst3Scanner::new();
            let plugin = scanner
                .load(&info_from_descriptor(descriptor))
                .map_err(|err| PluginError::Load(err.to_string()))?;
            Ok(Box::new(plugin))
        }
        #[cfg(feature = "vst2")]
        PluginFormat::Vst2 => {
            use notwig_rack::core::scanner::PluginScanner;
            let scanner = notwig_rack::vst2::Vst2Scanner::new();
            let plugin = scanner
                .load(&info_from_descriptor(descriptor))
                .map_err(|err| PluginError::Load(err.to_string()))?;
            Ok(Box::new(plugin))
        }
        #[cfg(all(feature = "au", target_vendor = "apple"))]
        PluginFormat::Au => {
            use notwig_rack::core::scanner::PluginScanner;
            let scanner = notwig_rack::au::AuScanner::new();
            let plugin = scanner
                .load(&info_from_descriptor(descriptor))
                .map_err(|err| PluginError::Load(err.to_string()))?;
            Ok(Box::new(plugin))
        }
        #[cfg(feature = "clap")]
        PluginFormat::Clap => {
            use notwig_rack::core::scanner::PluginScanner;
            let scanner = notwig_rack::clap::ClapScanner::new();
            let plugin = scanner
                .load(&info_from_descriptor(descriptor))
                .map_err(|err| PluginError::Load(err.to_string()))?;
            Ok(Box::new(plugin))
        }
        #[cfg(feature = "lv2")]
        PluginFormat::Lv2 => {
            use notwig_rack::core::scanner::PluginScanner;
            let scanner = notwig_rack::lv2::Lv2Scanner::new();
            let plugin = scanner
                .load(&info_from_descriptor(descriptor))
                .map_err(|err| PluginError::Load(err.to_string()))?;
            Ok(Box::new(plugin))
        }
        #[cfg(all(feature = "au3", target_vendor = "apple"))]
        PluginFormat::Au3 => {
            use notwig_rack::core::scanner::PluginScanner;
            let scanner = notwig_rack::au3::Au3Scanner::new();
            let plugin = scanner
                .load(&info_from_descriptor(descriptor))
                .map_err(|err| PluginError::Load(err.to_string()))?;
            Ok(Box::new(plugin))
        }
        _ => Err(PluginError::BackendUnavailable(descriptor.format)),
    }
}

#[cfg(any(feature = "vst3", feature = "vst2", feature = "clap"))]
fn scan_with<S>(
    scanner: &S,
    format: PluginFormat,
    paths: &[PathBuf],
) -> Result<Vec<PluginDescriptor>, PluginError>
where
    S: notwig_rack::core::scanner::PluginScanner,
{
    let mut found = Vec::new();
    if paths.is_empty() {
        found.extend(
            scanner
                .scan()
                .map_err(|err| PluginError::Scan(err.to_string()))?,
        );
    } else {
        for path in paths {
            if let Ok(mut plugins) = scanner.scan_path(path) {
                found.append(&mut plugins);
            }
        }
    }
    Ok(found
        .into_iter()
        .map(|info| descriptor_from_info(format, info))
        .collect())
}

/// Formats that discover through a registry rather than a filesystem path
/// (Audio Unit, LV2) always walk the full registry; the host's scan paths do
/// not apply.
#[cfg(any(
    feature = "lv2",
    all(feature = "au", target_vendor = "apple"),
    all(feature = "au3", target_vendor = "apple")
))]
fn scan_registry<S>(scanner: &S, format: PluginFormat) -> Result<Vec<PluginDescriptor>, PluginError>
where
    S: notwig_rack::core::scanner::PluginScanner,
{
    let found = scanner
        .scan()
        .map_err(|err| PluginError::Scan(err.to_string()))?;
    Ok(found
        .into_iter()
        .map(|info| descriptor_from_info(format, info))
        .collect())
}

fn load_with<S>(
    scanner: &S,
    descriptor: &PluginDescriptor,
) -> Result<Box<dyn PluginInstance>, PluginError>
where
    S: notwig_rack::core::scanner::PluginScanner,
    S::Plugin: notwig_rack::core::Plugin<f32> + 'static,
{
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let plugin = scanner
            .load(&info_from_descriptor(descriptor))
            .map_err(|err| PluginError::Load(err.to_string()))?;
        Ok::<_, PluginError>(Box::new(RackPluginInstance::new(
            Box::new(plugin),
            descriptor.clone(),
        )) as Box<dyn PluginInstance>)
    }));
    match result {
        Ok(result) => result,
        Err(payload) => Err(PluginError::Load(format!(
            "plugin load panicked: {}",
            panic_message(&payload)
        ))),
    }
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

#[cfg(feature = "vst3")]
#[derive(Debug, Default)]
pub(crate) struct Vst3Adapter;

#[cfg(feature = "vst3")]
impl HostAdapter for Vst3Adapter {
    fn format(&self) -> PluginFormat {
        PluginFormat::Vst3
    }

    fn scan(&self, paths: &[PathBuf]) -> Result<Vec<PluginDescriptor>, PluginError> {
        scan_with(&notwig_rack::vst3::Vst3Scanner::new(), self.format(), paths)
    }

    fn load(&self, descriptor: &PluginDescriptor) -> Result<Box<dyn PluginInstance>, PluginError> {
        load_with(&notwig_rack::vst3::Vst3Scanner::new(), descriptor)
    }
}

#[cfg(feature = "vst2")]
#[derive(Debug, Default)]
pub(crate) struct Vst2Adapter;

#[cfg(feature = "vst2")]
impl HostAdapter for Vst2Adapter {
    fn format(&self) -> PluginFormat {
        PluginFormat::Vst2
    }

    fn scan(&self, paths: &[PathBuf]) -> Result<Vec<PluginDescriptor>, PluginError> {
        scan_with(&notwig_rack::vst2::Vst2Scanner::new(), self.format(), paths)
    }

    fn load(&self, descriptor: &PluginDescriptor) -> Result<Box<dyn PluginInstance>, PluginError> {
        load_with(&notwig_rack::vst2::Vst2Scanner::new(), descriptor)
    }
}

#[cfg(all(feature = "au", target_vendor = "apple"))]
#[derive(Debug, Default)]
pub(crate) struct AuAdapter;

#[cfg(all(feature = "au", target_vendor = "apple"))]
impl HostAdapter for AuAdapter {
    fn format(&self) -> PluginFormat {
        PluginFormat::Au
    }

    fn scan(&self, _paths: &[PathBuf]) -> Result<Vec<PluginDescriptor>, PluginError> {
        scan_registry(&notwig_rack::au::AuScanner::new(), self.format())
    }

    fn load(&self, descriptor: &PluginDescriptor) -> Result<Box<dyn PluginInstance>, PluginError> {
        load_with(&notwig_rack::au::AuScanner::new(), descriptor)
    }
}

#[cfg(feature = "clap")]
#[derive(Debug, Default)]
pub(crate) struct ClapAdapter;

#[cfg(feature = "clap")]
impl HostAdapter for ClapAdapter {
    fn format(&self) -> PluginFormat {
        PluginFormat::Clap
    }

    fn scan(&self, paths: &[PathBuf]) -> Result<Vec<PluginDescriptor>, PluginError> {
        scan_with(&notwig_rack::clap::ClapScanner::new(), self.format(), paths)
    }

    fn load(&self, descriptor: &PluginDescriptor) -> Result<Box<dyn PluginInstance>, PluginError> {
        load_with(&notwig_rack::clap::ClapScanner::new(), descriptor)
    }
}

#[cfg(feature = "lv2")]
#[derive(Debug, Default)]
pub(crate) struct Lv2Adapter;

#[cfg(feature = "lv2")]
impl HostAdapter for Lv2Adapter {
    fn format(&self) -> PluginFormat {
        PluginFormat::Lv2
    }

    fn scan(&self, _paths: &[PathBuf]) -> Result<Vec<PluginDescriptor>, PluginError> {
        scan_registry(&notwig_rack::lv2::Lv2Scanner::new(), self.format())
    }

    fn load(&self, descriptor: &PluginDescriptor) -> Result<Box<dyn PluginInstance>, PluginError> {
        load_with(&notwig_rack::lv2::Lv2Scanner::new(), descriptor)
    }
}

#[cfg(all(feature = "au3", target_vendor = "apple"))]
#[derive(Debug, Default)]
pub(crate) struct Au3Adapter;

#[cfg(all(feature = "au3", target_vendor = "apple"))]
impl HostAdapter for Au3Adapter {
    fn format(&self) -> PluginFormat {
        PluginFormat::Au3
    }

    fn scan(&self, _paths: &[PathBuf]) -> Result<Vec<PluginDescriptor>, PluginError> {
        scan_registry(&notwig_rack::au3::Au3Scanner::new(), self.format())
    }

    fn load(&self, descriptor: &PluginDescriptor) -> Result<Box<dyn PluginInstance>, PluginError> {
        load_with(&notwig_rack::au3::Au3Scanner::new(), descriptor)
    }
}
