use notwig_core::ids::{
    Clip as ClipMarker, DeviceChain as DeviceChainMarker, Note as NoteMarker, Track as TrackMarker,
};
use notwig_core::protocol::CommandPayload;
use notwig_core::{ClipPayload, Db, Id, TrackId, TrackKind};
use uuid::Uuid;

use super::Workspace;

impl Workspace {
    pub fn create_track(&mut self, kind: TrackKind) -> Result<TrackId, String> {
        self.create_track_at(kind, None)
    }

    pub fn create_track_at(
        &mut self,
        kind: TrackKind,
        index: Option<usize>,
    ) -> Result<TrackId, String> {
        self.create_track_with_parent(kind, None, index)
    }

    fn create_track_with_parent(
        &mut self,
        kind: TrackKind,
        parent: Option<TrackId>,
        index: Option<usize>,
    ) -> Result<TrackId, String> {
        let track = Id::<TrackMarker>::from_uuid(Uuid::new_v4());
        let chain = Id::<DeviceChainMarker>::from_uuid(Uuid::new_v4());
        let name = match kind {
            TrackKind::Group => format!("Group {}", self.doc.tracks.len()),
            _ => format!("{} {}", kind.name(), self.doc.tracks.len()),
        };
        self.apply(CommandPayload::CreateTrack {
            id: track,
            chain_id: chain,
            kind,
            name,
            parent,
            index,
            color: None,
        })?;
        Ok(track)
    }

    pub fn group_tracks(&mut self, tracks: &[TrackId]) -> Result<TrackId, String> {
        let selected: Vec<TrackId> = self
            .doc
            .track_order
            .depth_first()
            .into_iter()
            .filter(|id| tracks.contains(id))
            .filter(|id| {
                self.doc
                    .tracks
                    .get(id)
                    .is_some_and(|track| track.kind != TrackKind::Master)
            })
            .collect();
        if selected.is_empty() {
            return Err("select at least one track to group".into());
        }
        let first = selected[0];
        let first_track = self
            .doc
            .tracks
            .get(&first)
            .ok_or_else(|| "track not found".to_string())?;
        let parent = first_track.parent;
        let siblings = match parent {
            Some(parent) => self
                .doc
                .track_order
                .children
                .get(&parent)
                .cloned()
                .unwrap_or_default(),
            None => self.doc.track_order.roots.clone(),
        };
        let index = siblings
            .iter()
            .position(|id| *id == first)
            .unwrap_or(siblings.len());
        let group = self.create_track_with_parent(TrackKind::Group, parent, Some(index))?;
        for track in selected {
            let next = self
                .doc
                .track_order
                .children
                .get(&group)
                .map_or(0, Vec::len);
            self.apply(CommandPayload::MoveTrack {
                id: track,
                parent: Some(group),
                index: next,
            })?;
        }
        Ok(group)
    }

    pub fn duplicate_track(&mut self, track_id: TrackId) -> Result<TrackId, String> {
        let track = self
            .doc
            .tracks
            .get(&track_id)
            .cloned()
            .ok_or_else(|| "track not found".to_string())?;
        if track.kind == TrackKind::Master {
            return Err("Master track cannot be duplicated".into());
        }
        let parent = track.parent;
        let siblings = match parent {
            Some(parent) => self
                .doc
                .track_order
                .children
                .get(&parent)
                .cloned()
                .unwrap_or_default(),
            None => self.doc.track_order.roots.clone(),
        };
        let index = siblings
            .iter()
            .position(|id| *id == track_id)
            .map_or(siblings.len(), |index| index + 1);
        let new_track = self.create_track_with_parent(track.kind, parent, Some(index))?;
        self.apply(CommandPayload::RenameTrack {
            id: new_track,
            name: format!("{} copy", track.name),
        })?;
        self.apply(CommandPayload::SetTrackMixer {
            id: new_track,
            volume_db: Some(track.mixer.volume_db),
            pan: Some(track.mixer.pan),
            mute: Some(track.mixer.mute),
            solo: Some(track.mixer.solo),
        })?;
        if !track.active {
            self.apply(CommandPayload::SetTrackActive {
                id: new_track,
                active: false,
            })?;
        }
        if track.io.audio_out != notwig_core::tracks::TrackOutput::Default {
            self.apply(CommandPayload::SetTrackRoute {
                id: new_track,
                audio_out: track.io.audio_out,
            })?;
        }
        let new_chain = self
            .doc
            .tracks
            .get(&new_track)
            .ok_or_else(|| "duplicated track missing".to_string())?
            .main_chain;
        self.duplicate_chain_devices(track.main_chain, new_chain)?;

        let clips: Vec<notwig_core::ClipEntity> = self
            .doc
            .arranger
            .lanes
            .get(&track_id)
            .map(|lane| lane.clips.clone())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|clip_id| self.doc.arranger.clips.get(&clip_id).cloned())
            .collect();
        for clip in clips {
            let mut payload = clip.payload.clone();
            if let ClipPayload::Midi(midi) = &mut payload {
                for note in &mut midi.notes {
                    note.id = Id::<NoteMarker>::from_uuid(Uuid::new_v4());
                }
            }
            self.apply(CommandPayload::CreateClip {
                id: Id::<ClipMarker>::from_uuid(Uuid::new_v4()),
                track_id: new_track,
                kind: clip.kind,
                range: clip.range,
                payload,
            })?;
        }
        Ok(new_track)
    }

    pub fn set_mixer(
        &mut self,
        track_id: TrackId,
        volume_db: Option<Db>,
        pan: Option<notwig_core::Pan>,
        mute: Option<bool>,
        solo: Option<bool>,
    ) -> Result<(), String> {
        self.apply(CommandPayload::SetTrackMixer {
            id: track_id,
            volume_db,
            pan,
            mute,
            solo,
        })
    }
}
