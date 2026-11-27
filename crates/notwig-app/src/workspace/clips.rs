use crate::edit_lowering::lower_arranger;
use notwig_core::edit::{self, ArrangerModel, ClipContent, SelectionInfo};
use notwig_core::ids::Clip as ClipMarker;
use notwig_core::protocol::CommandPayload;
use notwig_core::sequencer::MidiClipData;
use notwig_core::{AssetId, ClipId, ClipPayload, Db, Id, TrackId};
use notwig_ui::Clipboard;
use uuid::Uuid;

use super::Workspace;

impl Workspace {
    pub fn create_midi_clip(
        &mut self,
        track_id: TrackId,
        range: notwig_core::RangeMus,
    ) -> Result<ClipId, String> {
        let id = Id::<ClipMarker>::from_uuid(Uuid::new_v4());
        self.apply(CommandPayload::CreateClip {
            id,
            track_id,
            kind: notwig_core::ClipKind::Midi,
            range,
            payload: ClipPayload::Midi(MidiClipData {
                length: range.length(),
                content_start: notwig_core::TimeMus::zero(),
                notes: Vec::new(),
            }),
        })?;
        Ok(id)
    }

    pub fn create_note(
        &mut self,
        clip_id: ClipId,
        note: notwig_core::NoteEntity,
    ) -> Result<(), String> {
        self.apply(CommandPayload::CreateNote { clip_id, note })
    }

    pub fn edit_notes(
        &mut self,
        clip_id: ClipId,
        notes: Vec<notwig_core::NoteEntity>,
        removed: Vec<notwig_core::NoteId>,
    ) -> Result<(), String> {
        let mut payloads = Vec::new();
        if !removed.is_empty() {
            payloads.push(CommandPayload::DeleteNotes {
                clip_id,
                note_ids: removed,
            });
        }
        if !notes.is_empty() {
            payloads.push(CommandPayload::SetNotes { clip_id, notes });
        }
        self.apply_batch(payloads)
    }

    pub fn create_audio_clip(
        &mut self,
        track_id: TrackId,
        range: notwig_core::RangeMus,
    ) -> Result<ClipId, String> {
        let id = Id::<ClipMarker>::from_uuid(Uuid::new_v4());
        self.apply(CommandPayload::CreateClip {
            id,
            track_id,
            kind: notwig_core::ClipKind::Audio,
            range,
            payload: ClipPayload::Audio(notwig_core::AudioClipData {
                asset_id: AssetId::nil(),
                source_start_sample: 0,
                source_end_sample: 1,
                gain_db: Db(0.0),
                gain_curve: None,
                pan_curve: None,
                fade_in_samples: 0,
                fade_out_samples: 0,
                fade_shape: notwig_core::sequencer::FadeShape::Linear,
                slices: Vec::new(),
                slices_enabled: true,
            }),
        })?;
        Ok(id)
    }

    pub fn split_clip(&mut self, clip_id: ClipId, at: notwig_core::TimeMus) -> Result<(), String> {
        let Some(clip) = self.doc.arranger.clips.get(&clip_id) else {
            return Err("clip not found".into());
        };
        if at <= clip.range.start || at >= clip.range.end {
            return Err("split position must lie inside the clip".into());
        }
        let track_id = clip.track_id;
        let range = notwig_core::RangeMus::new(at, at).map_err(|err| err.to_string())?;
        let selection = edit::Selection::time(range, [track_id]);
        let plan = {
            let model = ArrangerModel::new(&self.doc);
            edit::split(&model, &selection).map_err(|err| err.to_string())?
        };
        self.lower_and_apply(plan)?;
        Ok(())
    }

    pub fn duplicate_clip(&mut self, clip_id: ClipId) -> Result<ClipId, String> {
        let selection = self.duplicate_selection(&[clip_id], None)?;
        selection
            .objects
            .first()
            .map(|entry| entry.object)
            .ok_or_else(|| "nothing duplicated".to_string())
    }

    pub fn join_clips(
        &mut self,
        clip_ids: &[ClipId],
    ) -> Result<SelectionInfo<TrackId, ClipId>, String> {
        let plan = {
            let model = ArrangerModel::new(&self.doc);
            edit::join_clips(&model, clip_ids).map_err(|err| err.to_string())?
        };
        self.lower_and_apply(plan).map(Option::unwrap_or_default)
    }

    pub fn insert_audio_clip(
        &mut self,
        track_id: TrackId,
        asset_id: AssetId,
    ) -> Result<(), String> {
        self.insert_audio_clip_at(track_id, asset_id, self.doc.transport.playhead)
    }

    pub fn insert_audio_clip_at(
        &mut self,
        track_id: TrackId,
        asset_id: AssetId,
        start: notwig_core::TimeMus,
    ) -> Result<(), String> {
        let entry = self
            .doc
            .assets
            .assets
            .get(&asset_id)
            .ok_or_else(|| "asset not found".to_string())?
            .clone();
        let duration = entry
            .duration_samples
            .ok_or_else(|| "asset has no duration".to_string())?;
        let length = self.doc.tempo_map().samples_to_mus(duration);
        let range =
            notwig_core::RangeMus::new(start, start + length).map_err(|err| err.to_string())?;
        let id = Id::<ClipMarker>::from_uuid(Uuid::new_v4());
        let payloads = vec![CommandPayload::CreateClip {
            id,
            track_id,
            kind: notwig_core::ClipKind::Audio,
            range,
            payload: ClipPayload::Audio(notwig_core::AudioClipData {
                asset_id,
                source_start_sample: 0,
                source_end_sample: duration,
                gain_db: Db(0.0),
                gain_curve: None,
                pan_curve: None,
                fade_in_samples: 0,
                fade_out_samples: 0,
                fade_shape: notwig_core::FadeShape::Linear,
                slices: Vec::new(),
                slices_enabled: false,
            }),
        }];
        self.apply_batch(payloads)
    }

    pub fn relocate_clips(
        &mut self,
        clips: &[(ClipId, TrackId, notwig_core::RangeMus)],
    ) -> Result<(), String> {
        let mut ordered = clips.to_vec();
        let moving_right = ordered
            .first()
            .and_then(|(id, _, range)| {
                self.doc
                    .arranger
                    .clips
                    .get(id)
                    .map(|clip| range.start > clip.range.start)
            })
            .unwrap_or(false);
        ordered.sort_by(|a, b| {
            if moving_right {
                b.2.start.cmp(&a.2.start)
            } else {
                a.2.start.cmp(&b.2.start)
            }
        });
        let moves: Vec<edit::ObjectMove<TrackId, ClipId>> = ordered
            .iter()
            .map(|(object, lane, span)| edit::ObjectMove {
                object: *object,
                lane: *lane,
                span: *span,
            })
            .collect();
        let plan = {
            let model = ArrangerModel::new(&self.doc);
            edit::relocate_objects(&model, &moves).map_err(|err| err.to_string())?
        };
        self.lower_and_apply(plan).map(|_| ())
    }

    pub fn set_clip_range(
        &mut self,
        clip_id: ClipId,
        range: notwig_core::RangeMus,
    ) -> Result<(), String> {
        self.apply(CommandPayload::SetClipRange { clip_id, range })
    }

    pub fn trim_clip(
        &mut self,
        clip_id: ClipId,
        range: notwig_core::RangeMus,
    ) -> Result<(), String> {
        let Some(clip) = self.doc.arranger.clips.get(&clip_id).cloned() else {
            return Err("clip not found".into());
        };
        let edge = if range.start != clip.range.start && range.end == clip.range.end {
            Some((edit::Edge::Start, range.start))
        } else if range.end != clip.range.end && range.start == clip.range.start {
            Some((edit::Edge::End, range.end))
        } else {
            None
        };
        let plan = {
            let model = ArrangerModel::new(&self.doc);
            match edge {
                Some((edge, to)) => Some(
                    edit::trim_object(&model, clip_id, edge, to).map_err(|err| err.to_string())?,
                ),
                None => None,
            }
        };
        if let Some(plan) = plan {
            self.lower_and_apply(plan).map(|_| ())
        } else {
            self.apply(CommandPayload::EditClip {
                clip_id,
                range: Some(range),
                name: None,
                muted: None,
                content_start: None,
                content_length: None,
                audio: None,
            })
        }
    }

    fn edit_selection(
        clip_ids: &[ClipId],
        time: Option<(notwig_core::RangeMus, Vec<TrackId>)>,
    ) -> edit::Selection<TrackId, ClipId> {
        if let Some((range, tracks)) = time {
            edit::Selection::time(range, tracks)
        } else {
            edit::Selection::objects(clip_ids.iter().copied())
        }
    }

    fn lower_and_apply(
        &mut self,
        plan: edit::Plan<TrackId, ClipId, ClipContent>,
    ) -> Result<Option<SelectionInfo<TrackId, ClipId>>, String> {
        let lowered = { lower_arranger(ArrangerModel::new(&self.doc), plan)? };
        if !lowered.commands.is_empty() {
            self.apply_batch(lowered.commands)?;
        }
        Ok(lowered.selection)
    }

    pub fn copy_selection(
        &mut self,
        clip_ids: &[ClipId],
        time: Option<(notwig_core::RangeMus, Vec<TrackId>)>,
    ) -> usize {
        let selection = Self::edit_selection(clip_ids, time);
        let result = {
            let model = ArrangerModel::new(&self.doc);
            edit::copy(&model, &selection)
        };
        match result {
            Ok(extract) => {
                let count = extract.fragments.len();
                self.set_clipboard(Clipboard::Arranger(extract));
                self.status = Some("Copied selection.".into());
                self.error = None;
                count
            }
            Err(err) => {
                self.error = Some(err.to_string());
                0
            }
        }
    }

    #[doc(hidden)]
    pub fn copy_clips(&mut self, clip_ids: &[ClipId]) -> usize {
        self.copy_selection(clip_ids, None)
    }

    #[doc(hidden)]
    pub fn copy_time_range(&mut self, range: notwig_core::RangeMus, tracks: &[TrackId]) -> usize {
        self.copy_selection(&[], Some((range, tracks.to_vec())))
    }

    pub fn cut_selection(
        &mut self,
        clip_ids: &[ClipId],
        time: Option<(notwig_core::RangeMus, Vec<TrackId>)>,
    ) -> usize {
        let selection = Self::edit_selection(clip_ids, time);
        let pair = {
            let model = ArrangerModel::new(&self.doc);
            edit::cut(&model, &selection)
        };
        let (extract, plan) = match pair {
            Ok(pair) => pair,
            Err(err) => {
                self.error = Some(err.to_string());
                return 0;
            }
        };
        let count = extract.fragments.len();
        if let Err(err) = self.lower_and_apply(plan) {
            self.error = Some(err);
            return 0;
        }
        self.set_clipboard(Clipboard::Arranger(extract));
        self.status = Some("Cut selection.".into());
        count
    }

    pub fn paste_clips(
        &mut self,
        at: notwig_core::TimeMus,
    ) -> Result<SelectionInfo<TrackId, ClipId>, String> {
        let clipboard = self.clipboard();
        let extract = match &clipboard {
            Clipboard::Arranger(extract) => extract.clone(),
            Clipboard::Empty => return Ok(SelectionInfo::default()),
            Clipboard::AudioClip(_) | Clipboard::Curve(_) => {
                return Err("Clipboard belongs to another editor.".into());
            }
            Clipboard::Devices => {
                return Err("Clipboard contains devices.".into());
            }
        };
        let plan = {
            let model = ArrangerModel::new(&self.doc);
            edit::paste(&model, &extract, at).map_err(|err| err.to_string())?
        };
        let selection = self.lower_and_apply(plan)?.unwrap_or_default();
        self.status = Some("Pasted selection.".into());
        Ok(selection)
    }

    /// Returns the active clipboard snapshot.
    #[must_use]
    pub fn clipboard(&self) -> Clipboard {
        self.extensions
            .get_extension(notwig_ext_api::EP_NOTWIG_CLIPBOARD)
            .get()
    }

    /// Replaces the clipboard payload through the active provider.
    pub fn set_clipboard(&mut self, clipboard: Clipboard) {
        self.extensions
            .get_extension(notwig_ext_api::EP_NOTWIG_CLIPBOARD)
            .set(clipboard);
    }

    /// Replaces the active clipboard provider.
    pub fn set_clipboard_provider(&mut self, provider: Box<dyn notwig_ext_api::ClipboardProvider>) {
        self.extensions
            .replace(
                notwig_ext_api::EP_NOTWIG_CLIPBOARD,
                "external.clipboard",
                std::sync::Arc::from(provider),
            )
            .expect("clipboard provider registration");
    }

    pub fn delete_selection(
        &mut self,
        clip_ids: &[ClipId],
        time: Option<(notwig_core::RangeMus, Vec<TrackId>)>,
    ) -> Result<SelectionInfo<TrackId, ClipId>, String> {
        let selection = Self::edit_selection(clip_ids, time);
        let plan = {
            let model = ArrangerModel::new(&self.doc);
            edit::delete(&model, &selection).map_err(|err| err.to_string())?
        };
        self.lower_and_apply(plan).map(Option::unwrap_or_default)
    }

    #[doc(hidden)]
    pub fn delete_time_range(
        &mut self,
        range: notwig_core::RangeMus,
        tracks: &[TrackId],
    ) -> Result<(), String> {
        self.delete_selection(&[], Some((range, tracks.to_vec())))?;
        self.status = Some("Deleted time selection.".into());
        Ok(())
    }

    pub fn duplicate_selection(
        &mut self,
        clip_ids: &[ClipId],
        time: Option<(notwig_core::RangeMus, Vec<TrackId>)>,
    ) -> Result<SelectionInfo<TrackId, ClipId>, String> {
        let selection = Self::edit_selection(clip_ids, time);
        let plan = {
            let model = ArrangerModel::new(&self.doc);
            edit::duplicate(&model, &selection).map_err(|err| err.to_string())?
        };
        self.lower_and_apply(plan).map(Option::unwrap_or_default)
    }

    #[doc(hidden)]
    pub fn duplicate_time_range(
        &mut self,
        range: notwig_core::RangeMus,
        tracks: &[TrackId],
    ) -> Result<(), String> {
        self.duplicate_selection(&[], Some((range, tracks.to_vec())))?;
        Ok(())
    }

    pub fn move_time_selection(
        &mut self,
        range: notwig_core::RangeMus,
        tracks: &[TrackId],
        delta: notwig_core::TimeMus,
    ) -> Result<SelectionInfo<TrackId, ClipId>, String> {
        let selection = edit::Selection::time(range, tracks.iter().copied());
        let plan = {
            let model = ArrangerModel::new(&self.doc);
            edit::move_selection(&model, &selection, delta).map_err(|err| err.to_string())?
        };
        self.lower_and_apply(plan).map(Option::unwrap_or_default)
    }
}
