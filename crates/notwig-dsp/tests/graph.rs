use notwig_core::ids::{Device, DeviceChain, Track};
use notwig_core::protocol::{Command, CommandPayload};
use notwig_core::tracks::TrackKind;
use notwig_core::*;
use notwig_dsp::devices::EngineCaps;
use notwig_dsp::{compile, stock_registry};
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
