use notwig_core::ids::{Clip, Device, DeviceChain as DeviceChainMarker, Param, Track};
use notwig_core::protocol::{Command, CommandPayload, EngineMsg};
use notwig_core::sequencer::{ClipKind, ClipPayload, MidiClipData};
use notwig_core::tracks::TrackKind;
use notwig_core::*;
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn meta() -> Meta {
    Meta {
        name: "Schedule".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    }
}

fn new_doc() -> ProjectDoc {
    ProjectDoc::new_template(meta(), id::<Track>(1), id::<DeviceChainMarker>(100)).unwrap()
}

fn add_instrument_track(doc: &ProjectDoc, n: u128) -> (ProjectDoc, TrackId, DeviceChainId) {
    let track = id::<Track>(n);
    let chain = id::<DeviceChainMarker>(n + 1000);
    let result = apply(
        doc,
        &Command::new(CommandPayload::CreateTrack {
            id: track,
            chain_id: chain,
            kind: TrackKind::Instrument,
            name: format!("Track {n}"),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap();
    (result.doc, track, chain)
}

fn schedule_updates(result: &ApplyResult) -> usize {
    result
        .engine_msgs
        .iter()
        .filter(|msg| matches!(msg, EngineMsg::UpdateSchedule { .. }))
        .count()
}

#[test]
fn clip_command_emits_exactly_one_schedule_update() {
    let doc = new_doc();
    let (doc, track, _) = add_instrument_track(&doc, 2);

    let result = apply(
        &doc,
        &Command::new(CommandPayload::CreateClip {
            id: id::<Clip>(20),
            track_id: track,
            kind: ClipKind::Midi,
            range: RangeMus::new(TimeMus::zero(), TimeMus::new(4, 1)).unwrap(),
            payload: ClipPayload::Midi(MidiClipData {
                length: TimeMus::new(4, 1),
                content_start: TimeMus::zero(),
                notes: Vec::new(),
            }),
        }),
    )
    .unwrap();

    assert_eq!(schedule_updates(&result), 1);
}

#[test]
fn set_param_emits_no_schedule_update() {
    let doc = new_doc();
    let (doc, _, chain) = add_instrument_track(&doc, 2);
    let device = id::<Device>(300);
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::InsertDevice {
            id: device,
            type_id: "notwig.tool".into(),
            version: None,
            kind: DeviceKind::AudioFx,
            target_chain: chain,
            index: None,
            child_chain_ids: Vec::new(),
            name: None,
        }),
    )
    .unwrap()
    .doc;

    let result = apply(
        &doc,
        &Command::new(CommandPayload::SetParam {
            device_id: device,
            param_id: id::<Param>(400),
            value: ParamValue::F64(0.5),
            ramp: None,
        }),
    )
    .unwrap();

    assert_eq!(schedule_updates(&result), 0);
    assert!(
        result
            .engine_msgs
            .iter()
            .any(|msg| matches!(msg, EngineMsg::SetParam { .. }))
    );
}
