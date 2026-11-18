use notwig_core::ids::{DeviceChain, Track};
use notwig_core::{Id, Meta, ProjectDoc, SampleRate};
use notwig_ui::scene_summary;
use uuid::Uuid;

#[test]
fn summary_counts_document_children() {
    let meta = Meta {
        name: "Summary".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    let doc = ProjectDoc::new_template(
        meta,
        Id::<Track>::from_uuid(Uuid::from_u128(1)),
        Id::<DeviceChain>::from_uuid(Uuid::from_u128(100)),
    )
    .unwrap();
    let summary = scene_summary(&doc);
    assert_eq!(summary.tracks, 1);
    assert_eq!(summary.clips, 0);
    assert_eq!(summary.devices, 0);
    assert_eq!(summary.assets, 0);
}
