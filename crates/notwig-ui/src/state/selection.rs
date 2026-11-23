use std::collections::BTreeSet;

use notwig_core::{ClipId, DeviceId, RangeMus, TrackId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionGroup<T: Ord> {
    items: BTreeSet<T>,
    primary: Option<T>,
}

impl<T: Ord + Copy> Default for SelectionGroup<T> {
    fn default() -> Self {
        Self {
            items: BTreeSet::new(),
            primary: None,
        }
    }
}

impl<T: Ord + Copy> SelectionGroup<T> {
    pub fn clear(&mut self) {
        self.items.clear();
        self.primary = None;
    }

    pub fn select_only(&mut self, item: T) {
        self.items.clear();
        self.items.insert(item);
        self.primary = Some(item);
    }

    pub fn toggle(&mut self, item: T) {
        if !self.items.remove(&item) {
            self.items.insert(item);
            self.primary = Some(item);
        } else if self.primary == Some(item) {
            self.primary = self.items.iter().next().copied();
        }
    }

    pub fn insert(&mut self, item: T) {
        self.items.insert(item);
        self.primary = Some(item);
    }

    pub fn contains(&self, item: &T) -> bool {
        self.items.contains(item)
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.items.iter()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn primary(&self) -> Option<T> {
        self.primary
    }

    pub fn set_primary(&mut self, item: T) {
        if self.items.contains(&item) {
            self.primary = Some(item);
        }
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&T) -> bool) {
        self.items.retain(|item| keep(item));
        if self
            .primary
            .is_some_and(|primary| !self.items.contains(&primary))
        {
            self.primary = self.items.iter().next().copied();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeSelection {
    pub range: RangeMus,
    pub tracks: SelectionGroup<TrackId>,
}

impl TimeSelection {
    pub fn contains_track(&self, track: TrackId) -> bool {
        self.tracks.contains(&track)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    pub tracks: SelectionGroup<TrackId>,
    pub clips: SelectionGroup<ClipId>,
    pub devices: SelectionGroup<DeviceId>,
    pub time: Option<TimeSelection>,
}

impl Selection {
    pub fn clear(&mut self) {
        self.tracks.clear();
        self.clips.clear();
        self.devices.clear();
        self.time = None;
    }

    pub fn select_time(&mut self, range: RangeMus, tracks: &[TrackId]) {
        self.clear();
        let mut group = SelectionGroup::default();
        for track in tracks {
            group.insert(*track);
            self.tracks.insert(*track);
        }
        self.time = Some(TimeSelection {
            range,
            tracks: group,
        });
    }

    pub fn select_track(&mut self, id: TrackId, additive: bool) {
        if additive {
            self.tracks.toggle(id);
        } else {
            self.clear();
            self.tracks.select_only(id);
        }
    }

    pub fn add_track(&mut self, id: TrackId) {
        self.clips.clear();
        self.devices.clear();
        self.time = None;
        self.tracks.insert(id);
    }

    pub fn select_track_range(&mut self, anchor: TrackId, target: TrackId, order: &[TrackId]) {
        let anchor_index = order.iter().position(|track| *track == anchor);
        let target_index = order.iter().position(|track| *track == target);
        let (Some(anchor_index), Some(target_index)) = (anchor_index, target_index) else {
            self.clear();
            self.tracks.select_only(target);
            return;
        };
        let (first, last) = if anchor_index <= target_index {
            (anchor_index, target_index)
        } else {
            (target_index, anchor_index)
        };
        self.clear();
        for track in &order[first..=last] {
            self.tracks.insert(*track);
        }
        self.tracks.set_primary(target);
    }

    pub fn select_clip(&mut self, id: ClipId, track: TrackId, additive: bool) {
        if additive {
            self.clips.toggle(id);
            self.tracks.set_primary(track);
        } else {
            self.clear();
            self.clips.select_only(id);
            self.tracks.select_only(track);
        }
    }

    pub fn select_clips(&mut self, clips: &[(ClipId, TrackId)]) {
        self.clear();
        for (clip, track) in clips {
            self.clips.insert(*clip);
            self.tracks.insert(*track);
        }
    }

    pub fn select_device(&mut self, id: DeviceId, track: TrackId, additive: bool) {
        if additive {
            self.devices.toggle(id);
            self.tracks.set_primary(track);
        } else {
            self.clear();
            self.devices.select_only(id);
            self.tracks.select_only(track);
        }
    }

    pub fn select_only_track(&mut self, id: TrackId) {
        self.select_track(id, false);
    }

    pub fn toggle_track(&mut self, id: TrackId) {
        self.select_track(id, true);
    }

    pub fn primary_track(&self) -> Option<TrackId> {
        self.tracks.primary()
    }

    pub fn primary_device(&self) -> Option<DeviceId> {
        self.devices.primary()
    }

    pub fn primary_clip(&self) -> Option<ClipId> {
        self.clips.primary()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
            && self.clips.is_empty()
            && self.devices.is_empty()
            && self.time.is_none()
    }

    #[must_use]
    pub fn to_edit_selection(&self) -> notwig_core::edit::Selection<TrackId, ClipId> {
        notwig_core::edit::Selection {
            objects: self.clips.iter().copied().collect(),
            time: self
                .time
                .as_ref()
                .map(|time| notwig_core::edit::TimeSelection {
                    span: time.range,
                    lanes: time.tracks.iter().copied().collect(),
                }),
        }
    }

    pub fn apply_info(&mut self, info: &notwig_core::edit::SelectionInfo<TrackId, ClipId>) {
        if let Some(time) = &info.time {
            let lanes: Vec<TrackId> = time.lanes.iter().copied().collect();
            self.select_time(time.span, &lanes);
        } else if !info.objects.is_empty() {
            let clips: Vec<(ClipId, TrackId)> = info
                .objects
                .iter()
                .map(|entry| (entry.object, entry.lane))
                .collect();
            self.select_clips(&clips);
        } else {
            self.clips.clear();
            self.time = None;
        }
    }
}
