use notwig_core::ids::{
    Asset, Device, DeviceChain as DeviceChainMarker, Send as SendMarker, Track,
};
use notwig_core::protocol::{Command, CommandPayload};
use notwig_core::tracks::{AudioInputRef, MonitorMode, Send as TrackSend, TrackKind, TrackOutput};
use notwig_core::*;
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn meta() -> Meta {
    Meta {
        name: "Apply".into(),
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

fn apply_checked(doc: &ProjectDoc, payload: CommandPayload) -> ProjectDoc {
    let result = apply(doc, &Command::new(payload)).expect("command should succeed");
    result
        .doc
        .validate()
        .expect("a successful apply must return a valid document");
    result.doc
}

fn asset_entry(asset: AssetId, uri: &str) -> AssetEntry {
    AssetEntry {
        id: asset,
        kind: AssetKind::Audio,
        uri: uri.into(),
        original_name: None,
        hash: None,
        format: "wav".into(),
        sample_rate: Some(48_000),
        channels: Some(2),
        duration_samples: Some(48_000),
        original_sample_rate: Some(48_000),
        original_channels: Some(2),
    }
}

fn seed() -> (ProjectDoc, TrackId, TrackId, DeviceId) {
    let doc = new_doc();
    let audio = id::<Track>(2);
    let bus = id::<Track>(3);
    let mut doc = apply_checked(
        &doc,
        CommandPayload::CreateTrack {
            id: audio,
            chain_id: id::<DeviceChainMarker>(102),
            kind: TrackKind::Audio,
            name: "Audio".into(),
            parent: None,
            index: None,
            color: None,
        },
    );
    doc = apply_checked(
        &doc,
        CommandPayload::CreateTrack {
            id: bus,
            chain_id: id::<DeviceChainMarker>(103),
            kind: TrackKind::BusFx,
            name: "Bus".into(),
            parent: None,
            index: None,
            color: None,
        },
    );
    let device = id::<Device>(300);
    doc = apply_checked(
        &doc,
        CommandPayload::InsertDevice {
            id: device,
            type_id: "notwig.tool".into(),
            version: None,
            kind: DeviceKind::AudioFx,
            target_chain: id::<DeviceChainMarker>(102),
            index: None,
            child_chain_ids: vec![],
            name: None,
        },
    );
    (doc, audio, bus, device)
}

#[test]
fn every_successful_apply_returns_a_valid_document() {
    let (mut doc, audio, bus, device) = seed();

    doc = apply_checked(
        &doc,
        CommandPayload::SetTrackColor {
            id: audio,
            color: Some(Color {
                r: 10,
                g: 20,
                b: 30,
                a: 255,
            }),
        },
    );
    doc = apply_checked(
        &doc,
        CommandPayload::SetTrackComment {
            id: audio,
            comment: "drums".into(),
        },
    );
    doc = apply_checked(
        &doc,
        CommandPayload::SetTrackActive {
            id: audio,
            active: false,
        },
    );
    doc = apply_checked(
        &doc,
        CommandPayload::SetTrackRecordArm {
            id: audio,
            armed: true,
        },
    );
    doc = apply_checked(
        &doc,
        CommandPayload::SetTrackInput {
            id: audio,
            input: Some(AudioInputRef {
                driver: "CoreAudio".into(),
                device: "Interface".into(),
                channel: 1,
                channels: 1,
                right_channel: None,
            }),
        },
    );
    doc = apply_checked(
        &doc,
        CommandPayload::SetTrackMonitorMode {
            id: audio,
            mode: MonitorMode::LowLatency,
        },
    );
    doc = apply_checked(
        &doc,
        CommandPayload::SetTrackMixer {
            id: audio,
            volume_db: Some(Db(-3.0)),
            pan: Some(Pan(-0.25)),
            mute: Some(true),
            solo: Some(true),
        },
    );
    doc = apply_checked(
        &doc,
        CommandPayload::SetTrackRoute {
            id: audio,
            audio_out: TrackOutput::Track(bus),
        },
    );

    let send = TrackSend {
        id: id::<SendMarker>(600),
        target: bus,
        amount_db: Db(-6.0),
        pre_fader: false,
        enabled: true,
    };
    doc = apply_checked(
        &doc,
        CommandPayload::SetSend {
            track_id: audio,
            send: send.clone(),
        },
    );
    let updated = TrackSend {
        amount_db: Db(-12.0),
        ..send
    };
    doc = apply_checked(
        &doc,
        CommandPayload::SetSend {
            track_id: audio,
            send: updated,
        },
    );
    assert_eq!(doc.tracks[&audio].sends.len(), 1);
    assert_eq!(doc.tracks[&audio].sends[0].amount_db, Db(-12.0));
    doc = apply_checked(
        &doc,
        CommandPayload::RemoveSend {
            track_id: audio,
            send_id: id::<SendMarker>(600),
        },
    );

    doc = apply_checked(
        &doc,
        CommandPayload::SetDeviceBypass {
            device_id: device,
            bypass: true,
        },
    );
    doc = apply_checked(
        &doc,
        CommandPayload::SetParam {
            device_id: device,
            param_id: id::<notwig_core::ids::Param>(400),
            value: ParamValue::F64(0.5),
            ramp: None,
        },
    );

    let asset = id::<Asset>(500);
    doc = apply_checked(
        &doc,
        CommandPayload::RegisterAsset {
            asset_id: asset,
            entry: asset_entry(asset, "assets/a.wav"),
        },
    );
    doc = apply_checked(
        &doc,
        CommandPayload::ReplaceAsset {
            asset_id: asset,
            entry: asset_entry(asset, "assets/b.wav"),
        },
    );
    doc = apply_checked(
        &doc,
        CommandPayload::RelinkAsset {
            asset_id: asset,
            uri: "assets/c.wav".into(),
        },
    );

    doc = apply_checked(&doc, CommandPayload::SetRecording { recording: true });
    doc = apply_checked(&doc, CommandPayload::Pause);
    apply_checked(&doc, CommandPayload::Stop);
}

#[test]
fn apply_rejects_a_command_that_would_leave_an_invalid_document() {
    let (doc, audio, _, _) = seed();
    let missing = id::<Track>(999);
    let before = doc.clone();
    let result = apply(
        &doc,
        &Command::new(CommandPayload::SetTrackRoute {
            id: audio,
            audio_out: TrackOutput::Track(missing),
        }),
    );
    assert!(matches!(result, Err(CoreError::ValidationFailed { .. })));
    assert_eq!(doc, before, "a failed command must not mutate its input");
}

#[test]
fn apply_rejects_a_send_to_a_track_that_cannot_receive_one() {
    let (doc, audio, _, _) = seed();
    let instrument = id::<Track>(4);
    let doc = apply_checked(
        &doc,
        CommandPayload::CreateTrack {
            id: instrument,
            chain_id: id::<DeviceChainMarker>(104),
            kind: TrackKind::Instrument,
            name: "Instrument".into(),
            parent: None,
            index: None,
            color: None,
        },
    );
    let result = apply(
        &doc,
        &Command::new(CommandPayload::SetSend {
            track_id: audio,
            send: TrackSend {
                id: id::<SendMarker>(601),
                target: instrument,
                amount_db: Db(0.0),
                pre_fader: false,
                enabled: true,
            },
        }),
    );
    assert!(matches!(result, Err(CoreError::ValidationFailed { .. })));
}
