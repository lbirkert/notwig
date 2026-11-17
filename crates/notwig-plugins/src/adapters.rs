use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use crate::host::HostAdapter;
use crate::rack::RackPluginInstance;
use crate::types::{PluginDescriptor, PluginError, PluginFormat, PluginInstance};

fn descriptor_from_info(
    format: PluginFormat,
    info: truce_rack::core::PluginInfo,
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
        type_id: format!("{}.{}", format.prefix(), info.unique_id),
        format,
        path: info.path,
        name: info.name,
        vendor: info.vendor,
        version: info.version.to_string(),
        uid: info.unique_id,
        is_instrument: matches!(info.category, truce_rack::core::PluginCategory::Instrument),
        binary_size,
        modified_utc,
    }
}

pub(crate) fn info_from_descriptor(descriptor: &PluginDescriptor) -> truce_rack::core::PluginInfo {
    truce_rack::core::PluginInfo {
        name: descriptor.name.clone(),
        vendor: descriptor.vendor.clone(),
        version: descriptor.version.parse().unwrap_or(0),
        category: if descriptor.is_instrument {
            truce_rack::core::PluginCategory::Instrument
        } else {
            truce_rack::core::PluginCategory::Effect
        },
        path: descriptor.path.clone(),
        unique_id: descriptor.uid.clone(),
        format: match descriptor.format {
            PluginFormat::Vst3 => "vst3",
            PluginFormat::Au => "au",
            PluginFormat::Vst2 => "vst2",
        },
        has_editor: true,
        accepts_midi: descriptor.is_instrument,
    }
}

pub(crate) fn load_plugin(
    descriptor: &PluginDescriptor,
) -> Result<Box<dyn truce_rack::core::Plugin<f32>>, PluginError> {
    match descriptor.format {
        #[cfg(feature = "vst3")]
        PluginFormat::Vst3 => {
            use truce_rack::core::scanner::PluginScanner;
            let scanner = truce_rack::vst3::Vst3Scanner::new();
            let plugin = scanner
                .load(&info_from_descriptor(descriptor))
                .map_err(|err| PluginError::Load(err.to_string()))?;
            Ok(Box::new(plugin))
        }
        #[cfg(all(feature = "au", target_vendor = "apple"))]
        PluginFormat::Au => {
            use truce_rack::core::scanner::PluginScanner;
            let scanner = truce_rack::au::AuScanner::new();
            let plugin = scanner
                .load(&info_from_descriptor(descriptor))
                .map_err(|err| PluginError::Load(err.to_string()))?;
            Ok(Box::new(plugin))
        }
        _ => Err(PluginError::BackendUnavailable(descriptor.format)),
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
        use truce_rack::core::scanner::PluginScanner;
        let scanner = truce_rack::vst3::Vst3Scanner::new();
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
            .map(|info| descriptor_from_info(PluginFormat::Vst3, info))
            .collect())
    }

    fn load(&self, descriptor: &PluginDescriptor) -> Result<Box<dyn PluginInstance>, PluginError> {
        use truce_rack::core::scanner::PluginScanner;
        let scanner = truce_rack::vst3::Vst3Scanner::new();
        let plugin = scanner
            .load(&info_from_descriptor(descriptor))
            .map_err(|err| PluginError::Load(err.to_string()))?;
        Ok(Box::new(RackPluginInstance::new(
            Box::new(plugin),
            descriptor.clone(),
        )))
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
        use truce_rack::core::scanner::PluginScanner;
        let scanner = truce_rack::au::AuScanner::new();
        let found = scanner
            .scan()
            .map_err(|err| PluginError::Scan(err.to_string()))?;
        Ok(found
            .into_iter()
            .map(|info| descriptor_from_info(PluginFormat::Au, info))
            .collect())
    }

    fn load(&self, descriptor: &PluginDescriptor) -> Result<Box<dyn PluginInstance>, PluginError> {
        use truce_rack::core::scanner::PluginScanner;
        let scanner = truce_rack::au::AuScanner::new();
        let plugin = scanner
            .load(&info_from_descriptor(descriptor))
            .map_err(|err| PluginError::Load(err.to_string()))?;
        Ok(Box::new(RackPluginInstance::new(
            Box::new(plugin),
            descriptor.clone(),
        )))
    }
}
