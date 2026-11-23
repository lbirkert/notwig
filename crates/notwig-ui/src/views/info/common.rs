use notwig_core::dsp::DeviceRegistry;
use notwig_core::{DeviceChainId, ProjectDoc};

use crate::theme;

pub(super) fn section_header(ui: &mut egui::Ui, label: &str) {
    ui.label(theme::heading(ui.ctx(), label));
}

/// Total processing latency of a device chain, including nested child chains.
///
/// Inactive devices are skipped because they are not instantiated. Bypassed
/// devices keep their planned latency so toggling bypass does not move the
/// latency-compensation baseline.
pub(super) fn chain_latency(
    doc: &ProjectDoc,
    registry: &DeviceRegistry,
    chain_id: DeviceChainId,
) -> u32 {
    let Some(chain) = doc.chains.get(&chain_id) else {
        return 0;
    };
    let mut total = 0u32;
    for id in &chain.devices {
        let Some(device) = doc.devices.get(id) else {
            continue;
        };
        if !device.active {
            continue;
        }
        if let Some(spec) = registry.resolve(&device.type_id, device.version) {
            total = total.saturating_add(spec.latency_samples);
        }
        for child in &device.child_chains {
            total = total.saturating_add(chain_latency(doc, registry, *child));
        }
    }
    total
}

pub(super) fn latency_label(samples: u32, sample_rate: u32) -> String {
    if sample_rate == 0 {
        return format!("{samples} samples");
    }
    let ms = f64::from(samples) * 1000.0 / f64::from(sample_rate);
    if ms >= 10.0 {
        format!("{ms:.0} ms")
    } else if ms > 0.0 {
        format!("{ms:.2} ms")
    } else {
        "0 ms".to_owned()
    }
}
