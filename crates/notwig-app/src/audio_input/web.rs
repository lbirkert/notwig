use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use notwig_core::TrackId;
use notwig_engine::{InputBus, MAX_INPUT_TRACKS};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{
    AudioContext, AudioContextOptions, AudioProcessingEvent, GainNode, MediaDeviceInfo,
    MediaDeviceKind, MediaStream, MediaStreamAudioSourceNode, MediaStreamConstraints,
    MediaStreamTrack, MediaTrackConstraints, ScriptProcessorNode,
};

use super::InputRequest;

const WEB_INPUT_DRIVER: &str = "WebAudio";
const WEB_INPUT_DEVICE: &str = "default";
const DEFAULT_WEB_INPUT_LABEL: &str = "Default Microphone";

thread_local! {
    static INPUT_OPTIONS: RefCell<Vec<notwig_ui::AudioInputView>> = const { RefCell::new(Vec::new()) };
    static INPUT_REFRESHING: Cell<bool> = const { Cell::new(false) };
}

type DeviceTargets = BTreeMap<(String, String), Vec<(u16, u16, u16)>>;
type StreamSignature = Vec<((String, String), Vec<(u16, u16, u16)>)>;

struct WebInput {
    stream: MediaStream,
    context: AudioContext,
    _source: MediaStreamAudioSourceNode,
    _processor: ScriptProcessorNode,
    _sink: GainNode,
    _callback: Closure<dyn FnMut(AudioProcessingEvent)>,
}

impl Drop for WebInput {
    fn drop(&mut self) {
        for track in self.stream.get_audio_tracks().iter() {
            if let Ok(track) = track.dyn_into::<MediaStreamTrack>() {
                track.stop();
            }
        }
        let _ = self.context.close();
    }
}

#[derive(Default)]
struct WebState {
    streams: Vec<WebInput>,
    signature: Option<StreamSignature>,
    generation: u64,
}

pub(crate) struct InputHub {
    bus: Arc<InputBus>,
    sample_rate: u32,
    state: Rc<RefCell<WebState>>,
    error: Rc<RefCell<Option<String>>>,
}

impl InputHub {
    pub(crate) fn new(sample_rate: u32) -> Self {
        Self {
            bus: Arc::new(InputBus::new(sample_rate)),
            sample_rate,
            state: Rc::new(RefCell::new(WebState::default())),
            error: Rc::new(RefCell::new(None)),
        }
    }

    pub(crate) fn bus(&self) -> Arc<InputBus> {
        Arc::clone(&self.bus)
    }

    pub(crate) fn configure(
        &self,
        requests: &[InputRequest],
    ) -> Result<Vec<(TrackId, u16)>, String> {
        if requests.len() > MAX_INPUT_TRACKS {
            return Err(format!(
                "at most {MAX_INPUT_TRACKS} tracks can be armed for recording"
            ));
        }
        let mut by_device: DeviceTargets = BTreeMap::new();
        let mut routes = Vec::with_capacity(requests.len());
        for (index, request) in requests.iter().enumerate() {
            let slot = u16::try_from(index).map_err(|_| "too many recording tracks".to_string())?;
            let input = &request.input;
            routes.push((request.track_id, slot));
            by_device
                .entry((input.driver.clone(), input.device.clone()))
                .or_default()
                .push((slot, input.channel, input.channels.max(1)));
        }

        let signature: StreamSignature = by_device
            .iter()
            .map(|(key, targets)| (key.clone(), targets.clone()))
            .collect();
        let generation = {
            let mut state = self.state.borrow_mut();
            if state.signature.as_ref() == Some(&signature) && !state.streams.is_empty() {
                return Ok(routes);
            }
            state.generation = state.generation.wrapping_add(1);
            state.streams.clear();
            state.signature = Some(signature);
            state.generation
        };
        *self.error.borrow_mut() = None;

        for ((driver, device), targets) in by_device {
            let state = Rc::clone(&self.state);
            let bus = Arc::clone(&self.bus);
            let error = Rc::clone(&self.error);
            let sample_rate = self.sample_rate;
            spawn_local(async move {
                let result = open_stream(&driver, &device, &targets, bus, sample_rate).await;
                if state.borrow().generation == generation {
                    match result {
                        Ok(stream) => state.borrow_mut().streams.push(stream),
                        Err(message) => *error.borrow_mut() = Some(message),
                    }
                }
                refresh_input_options();
            });
        }
        Ok(routes)
    }

    pub(crate) fn take_error(&self) -> Option<String> {
        self.error.borrow_mut().take()
    }
}

async fn open_stream(
    driver: &str,
    device: &str,
    targets: &[(u16, u16, u16)],
    bus: Arc<InputBus>,
    sample_rate: u32,
) -> Result<WebInput, String> {
    if driver != WEB_INPUT_DRIVER && !driver.is_empty() {
        return Err(format!(
            "the browser cannot open the {driver} input driver; use the Web Audio driver"
        ));
    }
    let window = web_sys::window().ok_or_else(|| "no browser window".to_string())?;
    let navigator = window.navigator();
    let media = navigator
        .media_devices()
        .map_err(|_| "microphone access is not available in this browser".to_string())?;
    let constraints = MediaStreamConstraints::new();
    if !device.is_empty() && device != WEB_INPUT_DEVICE {
        let audio = MediaTrackConstraints::new();
        audio.set_device_id_str(device);
        constraints.set_audio_media_track_constraints(&audio);
    } else {
        constraints.set_audio_bool(true);
    }
    let promise = media
        .get_user_media_with_constraints(&constraints)
        .map_err(|error| format!("microphone request failed: {error:?}"))?;
    let stream: MediaStream = JsFuture::from(promise)
        .await
        .map_err(|_| "microphone permission was denied".to_string())?
        .dyn_into()
        .map_err(|_| "browser returned an invalid microphone stream".to_string())?;

    let options = AudioContextOptions::new();
    options.set_sample_rate(sample_rate as f32);
    let context = AudioContext::new_with_context_options(&options)
        .map_err(|_| "could not create a Web Audio input context".to_string())?;
    let _ = context.resume();
    let source = context
        .create_media_stream_source(&stream)
        .map_err(|_| "could not open the microphone stream".to_string())?;
    let processor = context
        .create_script_processor_with_buffer_size_and_number_of_input_channels_and_number_of_output_channels(
            1024, 2, 2,
        )
        .map_err(|_| "could not create a Web Audio input processor".to_string())?;
    let sink = context
        .create_gain()
        .map_err(|_| "could not create a Web Audio input sink".to_string())?;
    sink.gain().set_value(0.0);

    let targets = targets.to_vec();
    let mut scratch_left = vec![0.0_f32; 0];
    let mut scratch_right = vec![0.0_f32; 0];
    let callback =
        Closure::<dyn FnMut(AudioProcessingEvent)>::new(move |event: AudioProcessingEvent| {
            let Ok(input) = event.input_buffer() else {
                return;
            };
            let frames = input.length() as usize;
            if frames == 0 {
                return;
            }
            if scratch_left.len() < frames {
                scratch_left.resize(frames, 0.0);
                scratch_right.resize(frames, 0.0);
            }
            let source_channels = input.number_of_channels().max(1);
            if input
                .copy_from_channel(&mut scratch_left[..frames], 0)
                .is_err()
            {
                return;
            }
            if source_channels > 1 {
                let _ = input.copy_from_channel(&mut scratch_right[..frames], 1);
            } else {
                scratch_right[..frames].copy_from_slice(&scratch_left[..frames]);
            }
            for frame in 0..frames {
                for (slot, channel, channels) in &targets {
                    let left_channel = usize::from(*channel).min((source_channels - 1) as usize);
                    let (left, right) = if left_channel == 0 {
                        (scratch_left[frame], scratch_right[frame])
                    } else {
                        (scratch_right[frame], scratch_right[frame])
                    };
                    if *channels >= 2 {
                        bus.push_stereo(*slot, left, right);
                    } else {
                        bus.push(*slot, left);
                    }
                }
            }
        });
    processor.set_onaudioprocess(Some(callback.as_ref().unchecked_ref()));
    let _ = source.connect_with_audio_node(&processor);
    let _ = processor.connect_with_audio_node(&sink);
    let _ = sink.connect_with_audio_node(&context.destination());

    Ok(WebInput {
        stream,
        context,
        _source: source,
        _processor: processor,
        _sink: sink,
        _callback: callback,
    })
}

pub(crate) fn available_input_options() -> Vec<notwig_ui::AudioInputView> {
    refresh_input_options();
    INPUT_OPTIONS.with(|options| {
        let mut options = options.borrow_mut();
        if options.is_empty() {
            options.push(default_input());
        }
        options.clone()
    })
}

fn default_input() -> notwig_ui::AudioInputView {
    notwig_ui::AudioInputView {
        driver: WEB_INPUT_DRIVER.into(),
        device: WEB_INPUT_DEVICE.into(),
        label: DEFAULT_WEB_INPUT_LABEL.into(),
        channels: 2,
    }
}

fn refresh_input_options() {
    if INPUT_REFRESHING.with(Cell::get) {
        return;
    }
    INPUT_REFRESHING.with(|refreshing| refreshing.set(true));
    spawn_local(async move {
        let result = enumerate_inputs().await;
        if let Ok(inputs) = result
            && !inputs.is_empty()
        {
            INPUT_OPTIONS.with(|options| {
                *options.borrow_mut() = inputs;
            });
        }
        INPUT_REFRESHING.with(|refreshing| refreshing.set(false));
    });
}

async fn enumerate_inputs() -> Result<Vec<notwig_ui::AudioInputView>, String> {
    let window = web_sys::window().ok_or_else(|| "no browser window".to_string())?;
    let media = window
        .navigator()
        .media_devices()
        .map_err(|_| "microphone access is not available in this browser".to_string())?;
    let promise = media
        .enumerate_devices()
        .map_err(|error| format!("could not list microphones: {error:?}"))?;
    let devices = JsFuture::from(promise)
        .await
        .map_err(|_| "could not list microphones".to_string())?;
    let devices: js_sys::Array = devices
        .dyn_into()
        .map_err(|_| "browser returned an invalid device list".to_string())?;
    let mut inputs = Vec::new();
    for index in 0..devices.length() {
        let Some(device) = devices.get(index).dyn_into::<MediaDeviceInfo>().ok() else {
            continue;
        };
        if device.kind() != MediaDeviceKind::Audioinput {
            continue;
        }
        let label = device.label();
        let label = if label.is_empty() {
            format!("Microphone {}", index + 1)
        } else {
            label
        };
        inputs.push(notwig_ui::AudioInputView {
            driver: WEB_INPUT_DRIVER.into(),
            device: device.device_id(),
            label,
            channels: 2,
        });
    }
    Ok(inputs)
}
