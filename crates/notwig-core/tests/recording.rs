use notwig_core::apply::create_track;
use notwig_core::ids::{DeviceChain, Track};
use notwig_core::protocol::{Command, CommandPayload};
use notwig_core::*;
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn meta() -> Meta {
    Meta {
        name: "Recording".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    }
}

fn new_doc() -> ProjectDoc {
    ProjectDoc::new_template(meta(), id::<Track>(1), id::<DeviceChain>(100)).unwrap()
}

fn apply_payload(doc: &mut ProjectDoc, payload: CommandPayload) -> Result<(), CoreError> {
    *doc = apply(doc, &Command::new(payload))?.doc;
    Ok(())
}

#[test]
fn audio_tracks_arm_and_select_a_single_input_channel() {
    let mut doc = new_doc();
    let track = id::<Track>(2);
    let chain = id::<DeviceChain>(102);
    create_track(
        &mut doc,
        track,
        chain,
        tracks::TrackKind::Audio,
        "Input".into(),
        None,
        None,
        None,
    )
    .unwrap();
    let input = AudioInputRef {
        driver: "CoreAudio".into(),
        device: "Interface".into(),
        channel: 1,
        channels: 1,
        right_channel: None,
    };
    apply_payload(
        &mut doc,
        CommandPayload::SetTrackInput {
            id: track,
            input: Some(input.clone()),
        },
    )
    .unwrap();
    apply_payload(
        &mut doc,
        CommandPayload::SetTrackRecordArm {
            id: track,
            armed: true,
        },
    )
    .unwrap();

    let stored = doc.tracks.get(&track).unwrap();
    assert_eq!(stored.io.audio_input, Some(input));
    assert!(stored.record_armed);
}

#[test]
fn only_audio_tracks_can_arm_or_select_input() {
    let mut doc = new_doc();
    let instrument = id::<Track>(2);
    create_track(
        &mut doc,
        instrument,
        id::<DeviceChain>(102),
        tracks::TrackKind::Instrument,
        "Instrument".into(),
        None,
        None,
        None,
    )
    .unwrap();
    let arm = apply(
        &doc,
        &Command::new(CommandPayload::SetTrackRecordArm {
            id: instrument,
            armed: true,
        }),
    );
    assert!(arm.is_err());
    let input = apply(
        &doc,
        &Command::new(CommandPayload::SetTrackInput {
            id: instrument,
            input: Some(AudioInputRef {
                driver: "CoreAudio".into(),
                device: "Interface".into(),
                channel: 0,
                channels: 1,
                right_channel: None,
            }),
        }),
    );
    assert!(input.is_err());
}

#[test]
fn recording_state_is_not_part_of_transport_validation() {
    let mut doc = new_doc();
    apply_payload(&mut doc, CommandPayload::SetRecording { recording: true }).unwrap();
    assert!(doc.transport.recording);
    apply_payload(&mut doc, CommandPayload::Stop).unwrap();
    assert!(!doc.transport.recording);
    assert!(!doc.transport.playing);
}
