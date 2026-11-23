use notwig_core::DeviceInstance;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PluginFormat {
    Vst3,
    Vst2,
    Au3,
    Au,
    Clap,
    Lv2,
}

impl PluginFormat {
    fn detect(type_id: &str) -> Option<Self> {
        if type_id.starts_with("vst3.") {
            Some(Self::Vst3)
        } else if type_id.starts_with("vst2.") {
            Some(Self::Vst2)
        } else if type_id.starts_with("au3.") {
            Some(Self::Au3)
        } else if type_id.starts_with("au.") {
            Some(Self::Au)
        } else if type_id.starts_with("clap.") {
            Some(Self::Clap)
        } else if type_id.starts_with("lv2.") {
            Some(Self::Lv2)
        } else {
            None
        }
    }

    const fn tag(self) -> &'static str {
        match self {
            Self::Vst3 => "VST3",
            Self::Vst2 => "VST",
            Self::Au3 => "AU3",
            Self::Au => "AU",
            Self::Clap => "CLAP",
            Self::Lv2 => "LV2",
        }
    }
}

pub(super) fn is_plugin_type_id(type_id: &str) -> bool {
    PluginFormat::detect(type_id).is_some()
}

pub(super) fn is_plugin_device(device: &DeviceInstance) -> bool {
    is_plugin_type_id(&device.type_id)
}

pub(super) fn format_tag(type_id: &str) -> Option<&'static str> {
    PluginFormat::detect(type_id).map(PluginFormat::tag)
}
