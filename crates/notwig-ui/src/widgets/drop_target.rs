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
    pub analyzing: bool,
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

#[doc(hidden)]
pub mod test_api {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    pub fn resolve_drop_target(
        doc: &ProjectDoc,
        rows: &BTreeMap<TrackId, Rect>,
        pointer_y: f32,
    ) -> Option<DropAudioTarget> {
        super::resolve_drop_target(doc, rows, pointer_y)
    }
}
