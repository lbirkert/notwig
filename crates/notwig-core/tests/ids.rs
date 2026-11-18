use notwig_core::ids::{Asset, Id, Track};
use notwig_core::test_id;
use uuid::Uuid;

#[test]
fn ids_roundtrip_canonical_text() {
    let id = Id::<Track>::from_uuid(Uuid::from_u128(42));
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, format!("\"{id}\""));
    let back: Id<Track> = serde_json::from_str(&json).unwrap();
    assert_eq!(id, back);
}

#[test]
fn typed_ids_are_distinct_types() {
    let track = test_id!(Track, 1);
    let asset = test_id!(Asset, 1);
    assert_eq!(track.as_uuid(), asset.as_uuid());
}
