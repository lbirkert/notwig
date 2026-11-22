use notwig_core::dsp::devices::EngineCaps;
use notwig_core::dsp::{DeviceRegistry, compile, stock_registry};
use notwig_core::ids::{Device, DeviceChain, Track};
use notwig_core::protocol::{Command, CommandPayload};
use notwig_core::tracks::TrackKind;
use notwig_core::*;
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn doc() -> ProjectDoc {
    let meta = Meta {
        name: "DSP".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    ProjectDoc::new_template(meta, id::<Track>(1), id::<DeviceChain>(100)).unwrap()
}

fn caps() -> EngineCaps {
    EngineCaps {
        sample_rate: 48_000,
        max_block_size: 256,
        channels: 2,
    }
}

#[test]
fn compile_resolves_stock_chain_and_orders_master_last() {
    let doc = doc();
    let track = id::<Track>(2);
    let chain = id::<DeviceChain>(102);
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: track,
            chain_id: chain,
            kind: TrackKind::Audio,
            name: "Audio".into(),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc;
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::InsertDevice {
            id: id::<Device>(300),
            type_id: "notwig.tool".into(),
            version: None,
            kind: DeviceKind::AudioFx,
            target_chain: chain,
            index: None,
            child_chain_ids: vec![],
            name: None,
        }),
    )
    .unwrap()
    .doc;

    let plan = compile(&doc, &stock_registry(), caps()).unwrap();
    assert_eq!(plan.order.last(), Some(&doc.master_track().unwrap()));
    assert_eq!(plan.order.first(), Some(&track));
    assert!(plan.devices[&id::<Device>(300)].resolved);
    assert_eq!(plan.config.sample_rate, 48_000);
}

#[test]
fn unknown_plugins_remain_unresolved_but_compilable() {
    let doc = doc();
    let track = id::<Track>(2);
    let chain = id::<DeviceChain>(102);
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: track,
            chain_id: chain,
            kind: TrackKind::Instrument,
            name: "Inst".into(),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc;
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::InsertDevice {
            id: id::<Device>(301),
            type_id: "vst3.some-uid".into(),
            version: None,
            kind: DeviceKind::Instrument,
            target_chain: chain,
            index: None,
            child_chain_ids: vec![],
            name: None,
        }),
    )
    .unwrap()
    .doc;

    let plan = compile(&doc, &stock_registry(), caps()).unwrap();
    assert!(!plan.devices[&id::<Device>(301)].resolved);
}

fn latent_registry() -> DeviceRegistry {
    let mut registry = stock_registry();
    for (type_id, latency) in [("test.latent16", 16), ("test.latent32", 32)] {
        let mut spec = notwig_core::dsp::stock::tool_spec();
        spec.type_id = type_id.into();
        spec.latency_samples = latency;
        registry.register(spec);
    }
    registry
}

fn apply_ok(doc: &ProjectDoc, payload: CommandPayload) -> ProjectDoc {
    apply(doc, &Command::new(payload)).unwrap().doc
}

fn add_audio_track(doc: &ProjectDoc, n: u128) -> (ProjectDoc, TrackId, DeviceChainId) {
    let track = id::<Track>(n);
    let chain = id::<DeviceChain>(n + 100);
    let doc = apply_ok(
        doc,
        CommandPayload::CreateTrack {
            id: track,
            chain_id: chain,
            kind: TrackKind::Audio,
            name: format!("Track {n}"),
            parent: None,
            index: None,
            color: None,
        },
    );
    (doc, track, chain)
}

fn insert_device(
    doc: &ProjectDoc,
    n: u128,
    type_id: &str,
    target_chain: DeviceChainId,
    child_chain_ids: Vec<DeviceChainId>,
) -> ProjectDoc {
    apply_ok(
        doc,
        CommandPayload::InsertDevice {
            id: id::<Device>(n),
            type_id: type_id.into(),
            version: None,
            kind: notwig_core::DeviceKind::AudioFx,
            target_chain,
            index: None,
            child_chain_ids,
            name: None,
        },
    )
}

#[test]
fn bypassed_device_contributes_no_chain_latency() {
    let (doc, track, chain) = add_audio_track(&doc(), 2);
    let doc = insert_device(&doc, 300, "test.latent32", chain, vec![]);

    let plan = compile(&doc, &latent_registry(), caps()).unwrap();
    assert_eq!(plan.devices[&id::<Device>(300)].latency_samples, 32);
    assert_eq!(plan.tracks[&track].latency_samples, 32);
    assert_eq!(plan.total_latency, 32);

    let doc = apply_ok(
        &doc,
        CommandPayload::SetDeviceBypass {
            device_id: id::<Device>(300),
            bypass: true,
        },
    );
    let plan = compile(&doc, &latent_registry(), caps()).unwrap();
    assert_eq!(plan.devices[&id::<Device>(300)].latency_samples, 0);
    assert_eq!(plan.tracks[&track].latency_samples, 0);
    assert_eq!(plan.total_latency, 0);
}

#[test]
fn nested_chain_latency_sums_children_and_respects_bypass() {
    let (doc, track, chain) = add_audio_track(&doc(), 2);
    let child_chain = id::<DeviceChain>(400);
    let doc = insert_device(
        &doc,
        300,
        notwig_core::dsp::stock::CHAIN_TYPE_ID,
        chain,
        vec![child_chain],
    );
    let doc = insert_device(&doc, 301, "test.latent32", child_chain, vec![]);
    let doc = insert_device(&doc, 302, "test.latent16", chain, vec![]);

    let plan = compile(&doc, &latent_registry(), caps()).unwrap();
    assert_eq!(plan.tracks[&track].latency_samples, 48);
    assert_eq!(plan.total_latency, 48);

    let doc = apply_ok(
        &doc,
        CommandPayload::SetDeviceBypass {
            device_id: id::<Device>(301),
            bypass: true,
        },
    );
    let plan = compile(&doc, &latent_registry(), caps()).unwrap();
    assert_eq!(plan.tracks[&track].latency_samples, 16);
    assert_eq!(plan.total_latency, 16);

    let doc = apply_ok(
        &doc,
        CommandPayload::SetDeviceBypass {
            device_id: id::<Device>(300),
            bypass: true,
        },
    );
    let plan = compile(&doc, &latent_registry(), caps()).unwrap();
    assert_eq!(plan.tracks[&track].latency_samples, 16);
    assert_eq!(plan.total_latency, 16);
}

#[test]
fn inactive_track_contributes_no_chain_latency() {
    let (doc, track, chain) = add_audio_track(&doc(), 2);
    let doc = insert_device(&doc, 300, "test.latent32", chain, vec![]);
    let doc = apply_ok(
        &doc,
        CommandPayload::SetTrackActive {
            id: track,
            active: false,
        },
    );

    let plan = compile(&doc, &latent_registry(), caps()).unwrap();
    assert_eq!(plan.tracks[&track].latency_samples, 0);
    assert_eq!(plan.total_latency, 0);
    assert_eq!(plan.tracks[&track].latency_to_master, 0);
}

fn add_track_kind(
    doc: &ProjectDoc,
    n: u128,
    kind: TrackKind,
    parent: Option<TrackId>,
) -> (ProjectDoc, TrackId) {
    let track = id::<Track>(n);
    let chain = id::<DeviceChain>(n + 200);
    let doc = apply_ok(
        doc,
        CommandPayload::CreateTrack {
            id: track,
            chain_id: chain,
            kind,
            name: format!("Track {n}"),
            parent,
            index: None,
            color: None,
        },
    );
    (doc, track)
}

fn set_solo(doc: &ProjectDoc, track: TrackId, solo: bool) -> ProjectDoc {
    apply_ok(
        doc,
        CommandPayload::SetTrackMixer {
            id: track,
            volume_db: None,
            pan: None,
            mute: None,
            solo: Some(solo),
        },
    )
}

fn assert_solo_agreement(doc: &ProjectDoc, expected: &[(TrackId, bool)]) {
    let plan = compile(doc, &stock_registry(), caps()).unwrap();
    let solo = SoloContext::new(doc);
    for id in doc.tracks.keys() {
        let core = doc.effective_solo(*id);
        assert_eq!(
            solo.is_effective(doc, *id),
            core,
            "SoloContext disagreed with ProjectDoc for {id}"
        );
        assert_eq!(
            plan.tracks[id].solo_effective, core,
            "plan disagreed with core solo evaluation for {id}"
        );
    }
    for (id, effective) in expected {
        assert_eq!(
            doc.effective_solo(*id),
            *effective,
            "unexpected effective solo for {id}"
        );
    }
    let master = doc.master_track().unwrap();
    assert!(doc.effective_solo(master));
    assert!(plan.tracks[&master].solo_effective);
}

#[test]
fn solo_effective_agrees_across_core_and_plan() {
    let (doc, group) = add_track_kind(&doc(), 20, TrackKind::Group, None);
    let (doc, child) = add_track_kind(&doc, 21, TrackKind::Instrument, Some(group));
    let (doc, other) = add_track_kind(&doc, 22, TrackKind::Audio, None);

    assert_solo_agreement(&doc, &[(group, true), (child, true), (other, true)]);

    let soloed_other = set_solo(&doc, other, true);
    assert_solo_agreement(
        &soloed_other,
        &[(group, false), (child, false), (other, true)],
    );

    let soloed_group = set_solo(&doc, group, true);
    assert_solo_agreement(
        &soloed_group,
        &[(group, true), (child, true), (other, false)],
    );

    let soloed_child = set_solo(&doc, child, true);
    assert_solo_agreement(
        &soloed_child,
        &[(group, true), (child, true), (other, false)],
    );

    let soloed_master = set_solo(&doc, doc.master_track().unwrap(), true);
    assert_solo_agreement(
        &soloed_master,
        &[(group, false), (child, false), (other, false)],
    );
}
