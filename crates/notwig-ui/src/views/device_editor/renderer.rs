use notwig_core::dsp::{DeviceSpec, DeviceUiSpec};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeviceRendererKind {
    Generic,
    Eq,
    Tool,
    Chain,
    Extension(String),
}

pub(crate) fn device_renderer(spec: Option<&DeviceSpec>) -> DeviceRendererKind {
    let Some(DeviceUiSpec::Custom { renderer, .. }) = spec.map(|spec| &spec.ui) else {
        return DeviceRendererKind::Generic;
    };
    match renderer.as_str() {
        "notwig.eq" => DeviceRendererKind::Eq,
        "notwig.tool" => DeviceRendererKind::Tool,
        "notwig.chain" => DeviceRendererKind::Chain,
        other if other.starts_with("ext:") => {
            DeviceRendererKind::Extension(other.trim_start_matches("ext:").to_owned())
        }
        _ => DeviceRendererKind::Generic,
    }
}

pub(crate) fn card_width(spec: Option<&DeviceSpec>) -> f32 {
    let preferred = match spec.map(|spec| &spec.ui) {
        Some(DeviceUiSpec::Custom {
            preferred_width, ..
        }) => *preferred_width,
        _ => None,
    };
    preferred.unwrap_or(300.0)
}
