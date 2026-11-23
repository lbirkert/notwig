//! Parameter conversion and host transport context translation.

use notwig_rack_core::info::{ParameterFlags, ParameterInfo};
use notwig_rack_core::transport::TransportInfo;

use vst3::Steinberg::Vst::{
    ParameterInfo as Vst3ParameterInfo, ParameterInfo_::ParameterFlags_,
    ProcessContext as Vst3ProcessContext, ProcessContext_::StatesAndFlags_,
};

pub(crate) fn empty_parameter_info() -> Vst3ParameterInfo {
    Vst3ParameterInfo {
        id: 0,
        title: [0; 128],
        shortTitle: [0; 128],
        units: [0; 128],
        stepCount: 0,
        defaultNormalizedValue: 0.0,
        unitId: 0,
        flags: 0,
    }
}

pub(crate) fn vst3_param_info_to_rack(info: &Vst3ParameterInfo) -> ParameterInfo {
    let name = string128_to_string(&info.title);
    let short_name = string128_to_string(&info.shortTitle);
    let unit = string128_to_string(&info.units);
    let mut flags = ParameterFlags::empty();
    if info.flags & ParameterFlags_::kIsBypass != 0 {
        flags |= ParameterFlags::BYPASS;
    }
    if info.flags & ParameterFlags_::kCanAutomate != 0 {
        flags |= ParameterFlags::AUTOMATABLE;
    }
    if info.flags & ParameterFlags_::kIsHidden != 0 {
        flags |= ParameterFlags::HIDDEN;
    }
    if info.flags & ParameterFlags_::kIsReadOnly != 0 {
        flags |= ParameterFlags::READ_ONLY;
    }
    if info.flags & ParameterFlags_::kIsList != 0 {
        flags |= ParameterFlags::ENUMERATED;
    }
    ParameterInfo {
        id: info.id,
        name,
        short_name,
        unit,
        min: 0.0,
        max: 1.0,
        default: info.defaultNormalizedValue,
        step_count: u32::try_from(info.stepCount).unwrap_or(0),
        flags,
    }
}

/// VST3 `String128` is `[char16; 128]` (UTF-16). Walk the slice
/// until the first NUL and decode to UTF-8.
fn string128_to_string(buf: &[u16; 128]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// Translate a host [`TransportInfo`] snapshot into Steinberg's
/// `Vst::ProcessContext`. Only the fields the host reported get
/// their `k…Valid` flag set; everything else stays zero so the
/// plugin treats it as absent.
// `StatesAndFlags` is `c_int` or `c_uint` depending on platform, so
// the `as u32` casts are only redundant on some targets.
#[allow(clippy::unnecessary_cast)]
pub(crate) fn build_vst3_context(t: &TransportInfo, sample_rate: f64) -> Vst3ProcessContext {
    // SAFETY: ProcessContext is a repr(C) aggregate of integers,
    // floats, and small POD sub-structs (Chord, FrameRate). An
    // all-zero value is the valid "no flags set" state; we then
    // fill in the fields the host actually reported.
    let mut ctx: Vst3ProcessContext = unsafe { std::mem::zeroed() };
    ctx.sampleRate = sample_rate;

    let mut state: u32 = 0;
    if let Some(tempo) = t.tempo_bpm {
        ctx.tempo = tempo;
        state |= StatesAndFlags_::kTempoValid as u32;
    }
    if let Some((num, den)) = t.time_signature {
        ctx.timeSigNumerator = i32::try_from(num).unwrap_or(4);
        ctx.timeSigDenominator = i32::try_from(den).unwrap_or(4);
        state |= StatesAndFlags_::kTimeSigValid as u32;
    }
    if let Some(beats) = t.song_position_beats {
        // projectTimeMusic is in quarter notes (TQuarterNotes = f64).
        ctx.projectTimeMusic = beats;
        state |= StatesAndFlags_::kProjectTimeMusicValid as u32;
    }
    if let Some(samples) = t.song_position_samples {
        ctx.projectTimeSamples = samples;
    }
    if let Some(bar) = t.bar_start_beats {
        ctx.barPositionMusic = bar;
        state |= StatesAndFlags_::kBarPositionValid as u32;
    }
    if t.playing {
        state |= StatesAndFlags_::kPlaying as u32;
    }
    if t.recording {
        state |= StatesAndFlags_::kRecording as u32;
    }
    if t.loop_active {
        state |= StatesAndFlags_::kCycleActive as u32;
    }
    ctx.state = state;
    ctx
}
