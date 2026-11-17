use std::collections::BTreeMap;

use egui::Rect;
use std::path::PathBuf;

use notwig_core::{ProjectDoc, TimeMus, TrackId, TrackKind};

use crate::intent::DropAudioTarget;

#[derive(Debug, Clone, PartialEq)]
pub struct DragPreview {
    pub path: PathBuf,
    pub target: DropAudioTarget,
    pub at: TimeMus,
    pub duration: TimeMus,
}

pub(crate) fn resolve_drop_target(
    doc: &ProjectDoc,
    rows: &BTreeMap<TrackId, Rect>,
    pointer_y: f32,
) -> Option<DropAudioTarget> {
    let order: Vec<TrackId> = doc.track_order.depth_first();
    for (index, track_id) in order.iter().enumerate() {
        let Some(rect) = rows.get(track_id) else {
            continue;
        };
        if (pointer_y - rect.top()).abs() <= 7.0 {
            return Some(DropAudioTarget::NewTrack { index });
        }
        if (pointer_y - rect.bottom()).abs() <= 7.0 {
            return Some(DropAudioTarget::NewTrack { index: index + 1 });
        }
        if pointer_y > rect.top() && pointer_y < rect.bottom() {
            return Some(
                if doc
                    .tracks
                    .get(track_id)
                    .is_some_and(|track| track.kind == TrackKind::Audio)
                {
                    DropAudioTarget::Track(*track_id)
                } else {
                    DropAudioTarget::NewTrack { index: index + 1 }
                },
            );
        }
    }
    let last_bottom = rows
        .values()
        .map(egui::Rect::bottom)
        .fold(f32::NEG_INFINITY, f32::max);
    (pointer_y > last_bottom).then_some(DropAudioTarget::NewTrack { index: order.len() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Rect, pos2};
    use notwig_core::ids::{DeviceChain, Track};
    use notwig_core::protocol::{Command, CommandPayload};
    use notwig_core::{Id, Meta, ProjectDoc, SampleRate, apply};
    use uuid::Uuid;

    fn id<T>(n: u128) -> Id<T> {
        Id::from_uuid(Uuid::from_u128(n))
    }

    fn doc() -> ProjectDoc {
        let meta = Meta {
            name: "Drop".into(),
            created_utc: 0,
            modified_utc: 0,
            sample_rate: SampleRate::DEFAULT,
            tempo_bpm: 120.0,
            time_sig_num: 4,
            time_sig_den: 4,
        };
        let doc = ProjectDoc::new_template(meta, id::<Track>(1), id::<DeviceChain>(100)).unwrap();
        let mut doc = apply(
            &doc,
            &Command::new(CommandPayload::CreateTrack {
                id: id::<Track>(2),
                chain_id: id::<DeviceChain>(102),
                kind: TrackKind::Audio,
                name: "Audio".into(),
                parent: None,
                index: None,
                color: None,
            }),
        )
        .unwrap()
        .doc;
        doc = apply(
            &doc,
            &Command::new(CommandPayload::CreateTrack {
                id: id::<Track>(3),
                chain_id: id::<DeviceChain>(103),
                kind: TrackKind::Instrument,
                name: "Instrument".into(),
                parent: None,
                index: None,
                color: None,
            }),
        )
        .unwrap()
        .doc;
        doc
    }

    fn rows() -> BTreeMap<TrackId, Rect> {
        BTreeMap::from([
            (
                id::<Track>(1),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(800.0, 48.0)),
            ),
            (
                id::<Track>(2),
                Rect::from_min_max(pos2(0.0, 48.0), pos2(800.0, 96.0)),
            ),
            (
                id::<Track>(3),
                Rect::from_min_max(pos2(0.0, 96.0), pos2(800.0, 144.0)),
            ),
        ])
    }

    #[test]
    fn audio_lane_targets_existing_track() {
        let doc = doc();
        assert_eq!(
            resolve_drop_target(&doc, &rows(), 72.0),
            Some(DropAudioTarget::Track(id::<Track>(2)))
        );
    }

    #[test]
    fn boundary_creates_track_at_boundary() {
        let doc = doc();
        assert_eq!(
            resolve_drop_target(&doc, &rows(), 96.0),
            Some(DropAudioTarget::NewTrack { index: 2 })
        );
        assert_eq!(
            resolve_drop_target(&doc, &rows(), 49.0),
            Some(DropAudioTarget::NewTrack { index: 1 })
        );
    }

    #[test]
    fn non_audio_lane_creates_track_after_it() {
        let doc = doc();
        assert_eq!(
            resolve_drop_target(&doc, &rows(), 120.0),
            Some(DropAudioTarget::NewTrack { index: 3 })
        );
    }

    #[test]
    fn below_rows_creates_track_at_end() {
        let doc = doc();
        assert_eq!(
            resolve_drop_target(&doc, &rows(), 170.0),
            Some(DropAudioTarget::NewTrack { index: 3 })
        );
    }
}
