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

#[allow(unreachable_patterns)]
pub(crate) fn load_plugin(
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
        use notwig_rack::core::scanner::PluginScanner;
        let scanner = notwig_rack::vst3::Vst3Scanner::new();
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
        use notwig_rack::core::scanner::PluginScanner;
        let scanner = notwig_rack::vst3::Vst3Scanner::new();
        let plugin = scanner
            .load(&info_from_descriptor(descriptor))
            .map_err(|err| PluginError::Load(err.to_string()))?;
        Ok(Box::new(RackPluginInstance::new(
            Box::new(plugin),
            descriptor.clone(),
        )))
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
        use notwig_rack::core::scanner::PluginScanner;
        let scanner = notwig_rack::vst2::Vst2Scanner::new();
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
            .map(|info| descriptor_from_info(PluginFormat::Vst2, info))
            .collect())
    }

    fn load(&self, descriptor: &PluginDescriptor) -> Result<Box<dyn PluginInstance>, PluginError> {
        use notwig_rack::core::scanner::PluginScanner;
        let scanner = notwig_rack::vst2::Vst2Scanner::new();
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
        use notwig_rack::core::scanner::PluginScanner;
        let scanner = notwig_rack::au::AuScanner::new();
        let found = scanner
            .scan()
            .map_err(|err| PluginError::Scan(err.to_string()))?;
        Ok(found
            .into_iter()
            .map(|info| descriptor_from_info(PluginFormat::Au, info))
            .collect())
    }

    fn load(&self, descriptor: &PluginDescriptor) -> Result<Box<dyn PluginInstance>, PluginError> {
        use notwig_rack::core::scanner::PluginScanner;
        let scanner = notwig_rack::au::AuScanner::new();
        let plugin = scanner
            .load(&info_from_descriptor(descriptor))
            .map_err(|err| PluginError::Load(err.to_string()))?;
        Ok(Box::new(RackPluginInstance::new(
            Box::new(plugin),
            descriptor.clone(),
        )))
    }
}
