use notwig_rack_core::bus::{BusLayout, ChannelConfig};

/// Cached metadata about one LV2 port.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PortInfo {
    pub(crate) index: u32,
    pub(crate) kind: PortKind,
    pub(crate) is_input: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum PortKind {
    Audio,
    Control { default: f32 },
    AtomSequence,
    Other,
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

pub(crate) unsafe fn classify_ports(
    world: *mut lilv_sys::LilvWorld,
    plugin: *const lilv_sys::LilvPlugin,
) -> (Vec<PortInfo>, usize, usize, Vec<usize>, Vec<f32>) {
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

    let mut ports = Vec::with_capacity(count as usize);
    let mut audio_in_count: usize = 0;
    let mut audio_out_count: usize = 0;
    let mut control_value_offset = Vec::with_capacity(count as usize);
    let mut control_values = Vec::new();

    let mut defaults: Vec<f32> = vec![0.0; count as usize];
    unsafe {
        // Pull every control port's declared default in one call.
        let defaults_ptr = defaults.as_mut_ptr();
        lilv_sys::lilv_plugin_get_port_ranges_float(
            plugin,
            std::ptr::null_mut(), // mins (don't care)
            std::ptr::null_mut(), // maxes (don't care)
            defaults_ptr,
        );
    }

    for idx in 0..count {
        let port = unsafe { lilv_sys::lilv_plugin_get_port_by_index(plugin, idx) };
        if port.is_null() {
            ports.push(PortInfo {
                index: idx,
                kind: PortKind::Other,
                is_input: false,
            });
            control_value_offset.push(usize::MAX);
            continue;
        }
        let is_input = unsafe { lilv_sys::lilv_port_is_a(plugin, port, input_uri) };
        let is_output = unsafe { lilv_sys::lilv_port_is_a(plugin, port, output_uri) };
        let kind = if unsafe { lilv_sys::lilv_port_is_a(plugin, port, audio_uri) } {
            if is_input {
                audio_in_count += 1;
            } else if is_output {
                audio_out_count += 1;
            }
            PortKind::Audio
        } else if unsafe { lilv_sys::lilv_port_is_a(plugin, port, control_uri) } {
            let default = if defaults[idx as usize].is_finite() {
                defaults[idx as usize]
            } else {
                0.0
            };
            PortKind::Control { default }
        } else if unsafe { lilv_sys::lilv_port_is_a(plugin, port, atom_uri) } {
            // We don't currently introspect the port's
            // `atom:supports` triples; assume any atom-port can
            // carry MIDI. Plugins with non-MIDI atom expectations
            // will see an unrecognised event type and ignore it,
            // which is the LV2 spec's required behaviour.
            PortKind::AtomSequence
        } else {
            PortKind::Other
        };
        if let PortKind::Control { default } = kind {
            control_value_offset.push(control_values.len());
            control_values.push(default);
        } else {
            control_value_offset.push(usize::MAX);
        }
        ports.push(PortInfo {
            index: idx,
            kind,
            is_input,
        });
    }

    unsafe {
        lilv_sys::lilv_node_free(audio_uri);
        lilv_sys::lilv_node_free(control_uri);
        lilv_sys::lilv_node_free(atom_uri);
        lilv_sys::lilv_node_free(input_uri);
        lilv_sys::lilv_node_free(output_uri);
    }

    (
        ports,
        audio_in_count,
        audio_out_count,
        control_value_offset,
        control_values,
    )
}
