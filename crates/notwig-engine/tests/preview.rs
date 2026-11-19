use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use notwig_core::ids::{Device as DeviceMarker, DeviceChain, Note as NoteMarker, Track};
use notwig_core::protocol::{Command, CommandPayload, PlaybackSchedule};
use notwig_core::tracks::TrackKind;
use notwig_core::{
    DeviceKind, Id, Meta, ParamId, ParamValue, ProjectDoc, SampleRate, TimeMus, TrackId, apply,
};
use notwig_dsp::devices::{ProcessContext, ProcessStatus};
use notwig_dsp::{CompiledPlan, DeviceProcessor, EngineCaps, NoteEvent, NoteKind, PlanDevice};
use notwig_engine::{DeviceFactory, Engine, EngineConfig, EngineControl, PreviewNote};
use uuid::Uuid;

type Captured = Arc<Mutex<Vec<(u64, NoteEvent)>>>;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn base_doc() -> ProjectDoc {
    let meta = Meta {
        name: "Preview".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    ProjectDoc::new_template(meta, id::<Track>(1), id::<DeviceChain>(100)).unwrap()
}

fn add_instrument_track(doc: &mut ProjectDoc) -> (TrackId, notwig_core::DeviceChainId) {
    let track = id::<Track>(2);
    let chain = id::<DeviceChain>(1002);
    let result = apply(
        doc,
        &Command::new(CommandPayload::CreateTrack {
            id: track,
            chain_id: chain,
            kind: TrackKind::Instrument,
            name: "Instrument".into(),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap();
    *doc = result.doc;
    let result = apply(
        doc,
        &Command::new(CommandPayload::InsertDevice {
            id: id::<DeviceMarker>(300),
            type_id: "test.record".into(),
            version: None,
            kind: DeviceKind::Instrument,
            target_chain: chain,
            index: None,
            child_chain_ids: vec![],
            name: None,
        }),
    )
    .unwrap();
    *doc = result.doc;
    (track, chain)
}

struct RecordingInstrument {
    events: Captured,
    elapsed_frames: u64,
}

impl DeviceProcessor for RecordingInstrument {
    fn prepare(&mut self, _config: notwig_dsp::RenderConfig) -> Result<(), notwig_dsp::GraphError> {
        Ok(())
    }

    fn reset(&mut self) {
        self.elapsed_frames = 0;
    }

    fn set_param(&mut self, _param_id: ParamId, _value: &ParamValue) {}

    fn process(
        &mut self,
        _ctx: &ProcessContext<'_>,
        audio: &mut [f32],
        events: &[NoteEvent],
    ) -> ProcessStatus {
        let frames = (audio.len() / 2) as u64;
        let mut captured = self.events.lock().unwrap();
        for event in events {
            captured.push((self.elapsed_frames + u64::from(event.sample_offset), *event));
        }
        self.elapsed_frames += frames;
        ProcessStatus::default()
    }
}

struct RecordingFactory {
    events: Captured,
}

impl DeviceFactory for RecordingFactory {
    fn create(
        &self,
        _plan: &CompiledPlan,
        device: &PlanDevice,
    ) -> Option<Box<dyn DeviceProcessor>> {
        if device.type_id == "test.record" {
            return Some(Box::new(RecordingInstrument {
                events: Arc::clone(&self.events),
                elapsed_frames: 0,
            }));
        }
        notwig_dsp::instantiate_stock_params(&device.type_id, &device.spec, &device.param_values)
    }
}

fn setup() -> (Engine, EngineControl, Captured, TrackId) {
    let mut doc = base_doc();
    let (track, _) = add_instrument_track(&mut doc);
    let config = EngineConfig::new(48_000, 256);
    let caps = EngineCaps {
        sample_rate: config.sample_rate,
        max_block_size: config.max_block_size,
        channels: 2,
    };
    let plan = Arc::new(notwig_dsp::compile(&doc, &notwig_dsp::stock_registry(), caps).unwrap());
    let schedule = Arc::new(PlaybackSchedule::from_doc(&doc));
    let events = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn DeviceFactory> = Arc::new(RecordingFactory {
        events: Arc::clone(&events),
    });
    let prepared = Engine::prepare_runtime(
        config,
        Arc::clone(&factory),
        plan,
        schedule,
        Arc::new(BTreeMap::new()),
    )
    .unwrap();
    let mut engine = Engine::with_factory(config, factory);
    engine.set_control_sync_frames(1);
    let mut producer = EngineControl::new();
    engine.attach_control(producer.handle());
    producer.data_mut().set_prepared_runtime(prepared);
    producer.flush();
    (engine, producer, events, track)
}

#[test]
fn preview_note_plays_and_releases_while_transport_is_stopped() {
    let (mut engine, mut producer, events, track) = setup();
    let mut output = vec![0.0_f32; 256 * 2];
    // Install the prepared runtime first.
    engine.process(&mut output, 256).unwrap();

    let note_id = id::<NoteMarker>(77);
    producer.data_mut().push_preview_note(PreviewNote {
        track_id: track,
        note_id,
        pitch: 64,
        velocity: 0.5,
        length_mus: Some(TimeMus::new(32, 100)),
    });
    producer.flush();

    for _ in 0..39 {
        engine.process(&mut output, 256).unwrap();
    }

    let events = events.lock().unwrap();
    let on = events
        .iter()
        .find(|(_, event)| event.kind == NoteKind::On && event.note_id == note_id)
        .expect("preview note on should reach the track");
    let off = events
        .iter()
        .find(|(_, event)| event.kind == NoteKind::Off && event.note_id == note_id)
        .expect("preview note off should reach the track");
    assert_eq!(on.1.pitch, 64);
    assert_eq!(on.1.velocity, 0.5);
    let duration = off.0 - on.0;
    assert!(
        (7_000..=8_000).contains(&duration),
        "preview duration was {duration} frames"
    );
}

#[test]
fn all_notes_off_drops_pending_previews() {
    let (mut engine, mut producer, events, track) = setup();
    let note_id = id::<NoteMarker>(78);

    let mut output = vec![0.0_f32; 256 * 2];
    // Install the prepared runtime first.
    engine.process(&mut output, 256).unwrap();

    producer.data_mut().push_preview_note(PreviewNote {
        track_id: track,
        note_id,
        pitch: 60,
        velocity: 1.0,
        length_mus: None,
    });
    producer.data_mut().request_all_notes_off();
    producer.flush();
    engine.process(&mut output, 256).unwrap();

    let events = events.lock().unwrap();
    assert!(
        events.iter().all(|(_, event)| event.kind != NoteKind::On),
        "all-notes-off should clear the pending preview before it sounds"
    );
}

#[test]
fn held_preview_note_releases_on_explicit_note_off() {
    let (mut engine, mut producer, events, track) = setup();
    let note_id = id::<NoteMarker>(79);
    let mut output = vec![0.0_f32; 256 * 2];
    engine.process(&mut output, 256).unwrap();

    producer.data_mut().push_preview_note(PreviewNote {
        track_id: track,
        note_id,
        pitch: 67,
        velocity: 0.7,
        length_mus: None,
    });
    producer.flush();
    for _ in 0..4 {
        engine.process(&mut output, 256).unwrap();
    }
    {
        let events = events.lock().unwrap();
        assert!(
            events
                .iter()
                .any(|(_, event)| event.kind == NoteKind::On && event.note_id == note_id)
        );
        assert!(
            events
                .iter()
                .all(|(_, event)| event.kind != NoteKind::Off || event.note_id != note_id)
        );
    }

    producer.data_mut().push_preview_off(track, note_id);
    producer.flush();
    engine.process(&mut output, 256).unwrap();
    let events = events.lock().unwrap();
    assert!(
        events
            .iter()
            .any(|(_, event)| event.kind == NoteKind::Off && event.note_id == note_id)
    );
}
