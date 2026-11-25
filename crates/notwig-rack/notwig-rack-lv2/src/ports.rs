use std::ffi::CStr;

use notwig_rack_core::bus::{BusLayout, ChannelConfig};
use notwig_rack_core::info::{ParameterFlags, ParameterInfo};

/// Cached metadata about one LV2 port.
#[derive(Debug, Clone)]
pub(crate) struct PortInfo {
    pub(crate) index: u32,
    pub(crate) kind: PortKind,
    pub(crate) is_input: bool,
    pub(crate) symbol: String,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum PortKind {
    Audio,
    Control { default: f32 },
    AtomSequence,
    Other,
}

/// Port classification plus the derived host-facing parameter list.
pub(crate) struct ClassifiedPorts {
    pub(crate) ports: Vec<PortInfo>,
    pub(crate) audio_in_count: usize,
    pub(crate) audio_out_count: usize,
    pub(crate) control_value_offset: Vec<usize>,
    pub(crate) control_values: Vec<f32>,
    pub(crate) parameters: Vec<ParameterInfo>,
    pub(crate) latency_offset: Option<usize>,
}

pub(crate) fn audio_layout(in_count: usize, out_count: usize) -> BusLayout {
    let mut layout = BusLayout::new();
    if in_count > 0 {
        layout.inputs.push(notwig_rack_core::bus::Bus::main(
            "Input",
            channel_config(in_count),
        ));
    }
    if out_count > 0 {
        layout.outputs.push(notwig_rack_core::bus::Bus::main(
            "Output",
            channel_config(out_count),
        ));
    }
    layout
}

fn channel_config(n: usize) -> ChannelConfig {
    match n {
        1 => ChannelConfig::Mono,
        2 => ChannelConfig::Stereo,
        6 => ChannelConfig::Surround5_1,
        8 => ChannelConfig::Surround7_1,
        n => ChannelConfig::Discrete(u32::try_from(n).unwrap_or(0)),
    }
}

unsafe fn node_string(node: *const lilv_sys::LilvNode) -> String {
    if node.is_null() {
        return String::new();
    }
    let ptr = unsafe { lilv_sys::lilv_node_as_string(node) };
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

unsafe fn uri_fragment(world: *mut lilv_sys::LilvWorld, uri: &[u8]) -> *mut lilv_sys::LilvNode {
    unsafe { lilv_sys::lilv_new_uri(world, uri.as_ptr().cast()) }
}

unsafe fn unit_label(
    world: *mut lilv_sys::LilvWorld,
    plugin: *const lilv_sys::LilvPlugin,
    port: *const lilv_sys::LilvPort,
    unit_uri: *const lilv_sys::LilvNode,
) -> String {
    let node = unsafe { lilv_sys::lilv_port_get(plugin, port, unit_uri) };
    if node.is_null() {
        return String::new();
    }
    let raw = unsafe { lilv_sys::lilv_node_as_uri(node) };
    let label = if raw.is_null() {
        String::new()
    } else {
        let uri = unsafe { CStr::from_ptr(raw) }.to_string_lossy();
        uri.rsplit('#')
            .next()
            .map(|fragment| match fragment {
                "db" => "dB".to_string(),
                "hz" => "Hz".to_string(),
                "pc" => "%".to_string(),
                "st" => "st".to_string(),
                "deg" => "°".to_string(),
                other => other.to_string(),
            })
            .unwrap_or_default()
    };
    let _ = world;
    unsafe { lilv_sys::lilv_node_free(node) };
    label
}

#[allow(
    clippy::too_many_lines,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
pub(crate) unsafe fn classify_ports(
    world: *mut lilv_sys::LilvWorld,
    plugin: *const lilv_sys::LilvPlugin,
) -> ClassifiedPorts {
    let count = unsafe { lilv_sys::lilv_plugin_get_num_ports(plugin) };
    let audio_uri =
        unsafe { lilv_sys::lilv_new_uri(world, lilv_sys::LILV_URI_AUDIO_PORT.as_ptr().cast()) };
    let control_uri =
        unsafe { lilv_sys::lilv_new_uri(world, lilv_sys::LILV_URI_CONTROL_PORT.as_ptr().cast()) };
    let atom_uri =
        unsafe { lilv_sys::lilv_new_uri(world, lilv_sys::LILV_URI_ATOM_PORT.as_ptr().cast()) };
    let input_uri =
        unsafe { lilv_sys::lilv_new_uri(world, lilv_sys::LILV_URI_INPUT_PORT.as_ptr().cast()) };
    let output_uri =
        unsafe { lilv_sys::lilv_new_uri(world, lilv_sys::LILV_URI_OUTPUT_PORT.as_ptr().cast()) };
    let unit_uri = unsafe { uri_fragment(world, b"http://lv2plug.in/ns/extensions/units#unit\0") };
    let integer_uri = unsafe { uri_fragment(world, b"http://lv2plug.in/ns/lv2core#integer\0") };
    let enumeration_uri =
        unsafe { uri_fragment(world, b"http://lv2plug.in/ns/lv2core#enumeration\0") };
    let toggled_uri = unsafe { uri_fragment(world, b"http://lv2plug.in/ns/lv2core#toggled\0") };
    let not_on_gui_uri =
        unsafe { uri_fragment(world, b"http://lv2plug.in/ns/ext/port-props#notOnGUI\0") };
    let reports_latency_uri =
        unsafe { uri_fragment(world, b"http://lv2plug.in/ns/lv2core#reportsLatency\0") };

    let mut ports = Vec::with_capacity(count as usize);
    let mut audio_in_count: usize = 0;
    let mut audio_out_count: usize = 0;
    let mut control_value_offset = Vec::with_capacity(count as usize);
    let mut control_values = Vec::new();
    let mut parameters = Vec::new();
    let mut latency_offset = None;

    let mut defaults: Vec<f32> = vec![0.0; count as usize];
    let mut mins: Vec<f32> = vec![0.0; count as usize];
    let mut maxes: Vec<f32> = vec![0.0; count as usize];
    unsafe {
        lilv_sys::lilv_plugin_get_port_ranges_float(
            plugin,
            mins.as_mut_ptr(),
            maxes.as_mut_ptr(),
            defaults.as_mut_ptr(),
        );
    }

    for idx in 0..count {
        let port = unsafe { lilv_sys::lilv_plugin_get_port_by_index(plugin, idx) };
        if port.is_null() {
            ports.push(PortInfo {
                index: idx,
                kind: PortKind::Other,
                is_input: false,
                symbol: String::new(),
            });
            control_value_offset.push(usize::MAX);
            continue;
        }
        let symbol_node = unsafe { lilv_sys::lilv_port_get_symbol(plugin, port) };
        let symbol = unsafe { node_string(symbol_node) };
        let name_node = unsafe { lilv_sys::lilv_port_get_name(plugin, port) };
        let mut name = unsafe { node_string(name_node) };
        if !name_node.is_null() {
            unsafe { lilv_sys::lilv_node_free(name_node) };
        }
        if name.is_empty() {
            name.clone_from(&symbol);
        }
        let is_input = unsafe { lilv_sys::lilv_port_is_a(plugin, port, input_uri) };
        let is_output = unsafe { lilv_sys::lilv_port_is_a(plugin, port, output_uri) };
        let is_control = unsafe { lilv_sys::lilv_port_is_a(plugin, port, control_uri) };
        let reports_latency = is_control
            && !is_input
            && unsafe { lilv_sys::lilv_port_has_property(plugin, port, reports_latency_uri) };
        let kind = if unsafe { lilv_sys::lilv_port_is_a(plugin, port, audio_uri) } {
            if is_input {
                audio_in_count += 1;
            } else if is_output {
                audio_out_count += 1;
            }
            PortKind::Audio
        } else if is_control {
            let default = if defaults[idx as usize].is_finite() {
                defaults[idx as usize]
            } else {
                0.0
            };
            PortKind::Control { default }
        } else if unsafe { lilv_sys::lilv_port_is_a(plugin, port, atom_uri) } {
            PortKind::AtomSequence
        } else {
            PortKind::Other
        };

        let mut offset = usize::MAX;
        if let PortKind::Control { default } = kind {
            offset = control_values.len();
            control_values.push(default);
            if reports_latency {
                latency_offset = Some(offset);
            }
        }
        control_value_offset.push(offset);

        if is_control && is_input {
            let min = if mins[idx as usize].is_finite() {
                mins[idx as usize]
            } else {
                0.0
            };
            let max = if maxes[idx as usize].is_finite() {
                maxes[idx as usize]
            } else {
                0.0
            };
            let default = if defaults[idx as usize].is_finite() {
                defaults[idx as usize]
            } else {
                min
            };
            let mut flags = ParameterFlags::AUTOMATABLE;
            if unsafe { lilv_sys::lilv_port_has_property(plugin, port, not_on_gui_uri) } {
                flags |= ParameterFlags::HIDDEN;
            }
            let toggled = unsafe { lilv_sys::lilv_port_has_property(plugin, port, toggled_uri) };
            let integer = unsafe { lilv_sys::lilv_port_has_property(plugin, port, integer_uri) };
            let enumerated =
                unsafe { lilv_sys::lilv_port_has_property(plugin, port, enumeration_uri) };
            let scale_count = {
                let points = unsafe { lilv_sys::lilv_port_get_scale_points(plugin, port) };
                if points.is_null() {
                    0
                } else {
                    let n = unsafe { lilv_sys::lilv_scale_points_size(points) };
                    unsafe { lilv_sys::lilv_scale_points_free(points) };
                    n
                }
            };
            if enumerated || scale_count > 0 {
                flags |= ParameterFlags::ENUMERATED;
            }
            let step_count = if toggled
                || (integer && min.abs() < f32::EPSILON && (max - 1.0).abs() < f32::EPSILON)
            {
                1
            } else if enumerated || scale_count > 0 {
                scale_count.saturating_sub(1)
            } else if integer {
                (max - min).round().max(0.0) as u32
            } else {
                0
            };
            parameters.push(ParameterInfo {
                id: idx,
                name: name.clone(),
                short_name: symbol.clone(),
                unit: unsafe { unit_label(world, plugin, port, unit_uri) },
                min: f64::from(min),
                max: f64::from(max),
                default: f64::from(default),
                step_count,
                flags,
            });
        }

        ports.push(PortInfo {
            index: idx,
            kind,
            is_input,
            symbol,
        });
    }

    unsafe {
        for node in [
            audio_uri,
            control_uri,
            atom_uri,
            input_uri,
            output_uri,
            unit_uri,
            integer_uri,
            enumeration_uri,
            toggled_uri,
            not_on_gui_uri,
            reports_latency_uri,
        ] {
            if !node.is_null() {
                lilv_sys::lilv_node_free(node);
            }
        }
    }

    ClassifiedPorts {
        ports,
        audio_in_count,
        audio_out_count,
        control_value_offset,
        control_values,
        parameters,
        latency_offset,
    }
}
