use egui::Rect;
use notwig_core::{ProjectDoc, TrackId};

use crate::shell::{NotwigUi, track_depth};

use super::super::{TRACK_INDENT, TrackDrop};

impl NotwigUi {
    pub(super) fn track_drop_target(
        &self,
        doc: &ProjectDoc,
        dragged: TrackId,
        pointer: egui::Pos2,
    ) -> Option<TrackDrop> {
        let descendants = doc.track_order.descendants(dragged);
        let valid_parent = |parent: Option<TrackId>| {
            parent.is_none_or(|parent| parent != dragged && !descendants.contains(&parent))
        };
        let master = doc.master_track();
        let mut rows: Vec<(TrackId, Rect)> = doc
            .track_order
            .depth_first()
            .into_iter()
            .filter(|id| Some(*id) != master)
            .filter_map(|id| self.row_rect(id).map(|row| (id, row)))
            .collect();
        rows.sort_by(|left, right| left.1.top().total_cmp(&right.1.top()));
        let first_below = rows.iter().position(|(_, row)| pointer.y < row.center().y);
        let (above, below, y) = if let Some(index) = first_below {
            let below = rows[index];
            (
                index.checked_sub(1).map(|previous| rows[previous]),
                Some(below),
                below.1.top(),
            )
        } else {
            let above = *rows.last()?;
            (Some(above), None, above.1.bottom())
        };
        let current_parent = doc.tracks.get(&dragged)?.parent;
        let current_index = Self::sibling_index(doc, current_parent, dragged);
        let mut candidates: Vec<(Option<TrackId>, usize)> = Vec::new();
        let mut add_candidate =
            |parent: Option<TrackId>, reference: Option<TrackId>, after: bool| {
                if !valid_parent(parent) {
                    return;
                }
                if let Some(index) = Self::insertion_index(doc, parent, reference, after, dragged) {
                    let candidate = (parent, index);
                    if !candidates.contains(&candidate) {
                        candidates.push(candidate);
                    }
                }
            };
        if let Some((below_id, _)) = below {
            add_candidate(doc.tracks.get(&below_id)?.parent, Some(below_id), false);
        }
        if let Some((above_id, _)) = above {
            let ends_here = below.is_none_or(|(below_id, _)| {
                !doc.track_order.descendants(above_id).contains(&below_id)
            });
            if ends_here {
                let mut current = above_id;
                loop {
                    let parent = doc.tracks.get(&current)?.parent;
                    add_candidate(parent, Some(current), true);
                    let Some(parent_id) = parent else {
                        break;
                    };
                    if Self::siblings(doc, Some(parent_id)).last() != Some(&current) {
                        break;
                    }
                    current = parent_id;
                }
            }
            let above_is_empty_group = doc
                .tracks
                .get(&above_id)
                .is_some_and(|track| track.kind.can_have_children())
                && doc.track_order.children_of(above_id).is_empty();
            if above_is_empty_group && valid_parent(Some(above_id)) {
                add_candidate(Some(above_id), None, true);
            }
        }
        let base_x = rows.first()?.1.left() + 4.0;
        let mut fallback: Option<(Option<TrackId>, usize, usize)> = None;
        let mut chosen: Option<(Option<TrackId>, usize, usize)> = None;
        for (parent, index) in candidates {
            let depth = parent.map_or(0, |parent| track_depth(doc, parent) + 1);
            let candidate = (parent, index, depth);
            if fallback.is_none_or(|(_, _, best)| depth < best) {
                fallback = Some(candidate);
            }
            if base_x + depth as f32 * TRACK_INDENT <= pointer.x + 0.5
                && chosen.is_none_or(|(_, _, best)| depth > best)
            {
                chosen = Some(candidate);
            }
        }
        let (parent, index, depth) = chosen.or(fallback)?;
        if parent == current_parent && index == current_index {
            return None;
        }
        Some(TrackDrop {
            parent,
            index,
            y,
            depth,
        })
    }

    fn siblings(doc: &ProjectDoc, parent: Option<TrackId>) -> &[TrackId] {
        match parent {
            Some(parent) => doc.track_order.children_of(parent),
            None => &doc.track_order.roots,
        }
    }

    fn insertion_index(
        doc: &ProjectDoc,
        parent: Option<TrackId>,
        reference: Option<TrackId>,
        after: bool,
        dragged: TrackId,
    ) -> Option<usize> {
        let siblings = Self::siblings(doc, parent);
        let dragged_index = siblings.iter().position(|id| *id == dragged);
        match reference {
            Some(reference) => {
                if reference == dragged {
                    dragged_index
                } else {
                    let mut index = siblings.iter().position(|id| *id == reference)?;
                    if dragged_index.is_some_and(|dragged| dragged < index) {
                        index -= 1;
                    }
                    Some(if after { index + 1 } else { index })
                }
            }
            None => Some(siblings.len() - usize::from(dragged_index.is_some())),
        }
    }

    fn sibling_index(doc: &ProjectDoc, parent: Option<TrackId>, id: TrackId) -> usize {
        let siblings = Self::siblings(doc, parent);
        siblings
            .iter()
            .position(|sibling| *sibling == id)
            .unwrap_or(siblings.len())
    }
}
