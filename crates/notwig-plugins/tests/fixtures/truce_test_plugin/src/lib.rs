//! Test-only truce plugin used by Notwig's manual plugin integration suite.
//!
//! Keep the parameters and processing deliberately simple: the integration
//! tests assert exact gain/invert/mute/bypass behavior, parameter defaults,
//! state round-trips, and advertised latency.

use std::sync::Arc;

use truce::prelude::*;
use truce_gui::IntoLayoutEditor;
use truce_gui_types::layout::{GridLayout, dropdown, knob, toggle, widgets};

use NotwigTestParamsParamId as P;

/// Non-zero on purpose so the host-level tests can distinguish formats whose
/// adapter forwards plugin latency from formats that still report zero.
pub const TEST_LATENCY_SAMPLES: u32 = 7;

#[derive(ParamEnum)]
pub enum Mode {
    Clean,
    Invert,
    Mute,
}

#[derive(Params)]
pub struct NotwigTestParams {
    #[param(name = "Gain", range = "linear(-60, 6)", unit = "dB", default = 0.0)]
    pub gain: FloatParam,

    #[param(name = "Bypass", default = false)]
    pub bypass: BoolParam,

    #[param(name = "Mode", default = 0)]
    pub mode: EnumParam<Mode>,
}

#[derive(Default)]
pub struct NotwigTestPlugin;

impl PluginLogic for NotwigTestPlugin {
    type Params = NotwigTestParams;
    type DspState = Self;

    fn process(
        _state: &mut Self::DspState,
        params: &Self::Params,
        buffer: &mut AudioBuffer,
        _events: &EventList,
        _context: &mut ProcessContext,
    ) -> ProcessStatus {
        let bypassed = params.bypass.value();
        let gain = db_to_linear(params.gain.read());
        let polarity = match params.mode.value() {
            Mode::Clean => 1.0,
            Mode::Invert => -1.0,
            Mode::Mute => 0.0,
        };
        for sample in 0..buffer.num_samples() {
            for channel in 0..buffer.channels() {
                let (input, output) = buffer.io(channel);
                output[sample] = if bypassed {
                    input[sample]
                } else {
                    input[sample] * gain * polarity
                };
            }
        }
        ProcessStatus::Normal
    }

    fn latency(_state: &Self::DspState) -> u32 {
        TEST_LATENCY_SAMPLES
    }

    fn editor(params: Arc<Self::Params>) -> Box<dyn Editor> {
        GridLayout::build(vec![widgets(vec![
            knob(P::Gain, "Gain"),
            toggle(P::Bypass, "Bypass"),
            dropdown(P::Mode, "Mode"),
        ])])
        .with_cols(3)
        .with_cell_size(64.0)
        .with_title("NOTWIG TEST")
        .into_editor(&params)
    }
}

truce::plugin! {
    logic: NotwigTestPlugin,
    params: NotwigTestParams,
}

truce::enable_rt_paranoid!();
