use std::collections::BTreeSet;

use egui::Rect;
use notwig_core::{ProjectDoc, TrackId};

use super::NotwigUi;

impl NotwigUi {
    pub(crate) fn zoom_to_fit(&mut self, doc: &ProjectDoc) {
        let end = doc
            .arranger
            .clips
            .values()
            .map(|clip| clip.range.end.as_f64())
            .fold(8.0_f64, f64::max)
            + 1.0;
        self.state.viewport.start = notwig_core::TimeMus::zero();
        self.state.viewport.pixels_per_beat = ((self.timeline_width - 4.0) / end as f32).clamp(
            crate::state::Viewport::MIN_PPB,
            crate::state::Viewport::MAX_PPB,
        );
    }

    pub(crate) fn track_at(&self, y: f32) -> Option<TrackId> {
        self.track_rows
            .iter()
            .chain(self.last_track_rows.iter())
            .find(|(_, rect)| y >= rect.top() && y < rect.bottom())
            .map(|(id, _)| *id)
    }

    pub(crate) fn row_rect(&self, track: TrackId) -> Option<Rect> {
        self.track_rows
            .get(&track)
            .or_else(|| self.last_track_rows.get(&track))
            .copied()
    }
}

pub(crate) fn track_depth(doc: &ProjectDoc, id: TrackId) -> usize {
    let mut depth = 0usize;
    let mut seen = BTreeSet::new();
    let mut current = doc.tracks.get(&id).and_then(|track| track.parent);
    while let Some(parent) = current {
        if !seen.insert(parent) {
            break;
        }
        depth += 1;
        current = doc.tracks.get(&parent).and_then(|track| track.parent);
    }
    depth
}
