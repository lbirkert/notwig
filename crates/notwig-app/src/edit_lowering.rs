use notwig_core::edit::{
    ArrangerModel, ClipContent, EditModel, EditOp, NextSelection, ObjectInLane, ObjectRef, Plan,
    SelectionInfo, TimeSelection,
};
use notwig_core::ids::{ClipId, NoteId, SliceId, TrackId};
use notwig_core::protocol::CommandPayload;
use notwig_core::sequencer::ClipPayload;
use uuid::Uuid;

pub struct Lowered {
    pub commands: Vec<CommandPayload>,
    pub selection: Option<SelectionInfo<TrackId, ClipId>>,
}

pub fn lower_arranger(
    model: ArrangerModel<'_>,
    plan: Plan<TrackId, ClipId, ClipContent>,
) -> Result<Lowered, String> {
    let mut commands = Vec::new();
    let mut placed: Vec<Option<(ClipId, TrackId)>> = vec![None; plan.ops.len()];

    for (index, op) in plan.ops.iter().enumerate() {
        match op {
            EditOp::SetContainer { .. } => {
                return Err("the arranger has no container".into());
            }
            EditOp::Erase { lane, span } => {
                commands.push(CommandPayload::ClearRange {
                    track_id: *lane,
                    range: *span,
                });
            }
            EditOp::Remove { object } => {
                commands.push(CommandPayload::DeleteClip { clip_id: *object });
            }
            EditOp::Split { object, at } => {
                let Some(clip) = model.clip(*object) else {
                    return Err(format!("missing clip {object}"));
                };
                let mut new_note_ids = Vec::new();
                if let ClipPayload::Midi(midi) = &clip.payload {
                    let split_offset = *at - clip.range.start;
                    let split_content = midi.content_start + split_offset;
                    for note in &midi.notes {
                        if note.start < split_content && note.start + note.length > split_content {
                            new_note_ids.push(NoteId::from_uuid(Uuid::new_v4()));
                        }
                    }
                }
                commands.push(CommandPayload::SplitClip {
                    clip_id: *object,
                    at: *at,
                    new_clip_id: ClipId::from_uuid(Uuid::new_v4()),
                    new_note_ids,
                });
            }
            EditOp::Move { object, lane, span } => {
                commands.push(CommandPayload::MoveClip {
                    clip_id: *object,
                    track_id: Some(*lane),
                    range: *span,
                });
            }
            EditOp::Trim { object, span } => {
                commands.push(CommandPayload::EditClip {
                    clip_id: *object,
                    range: Some(*span),
                    name: None,
                    muted: None,
                    content_start: None,
                    content_length: None,
                    audio: None,
                });
            }
            EditOp::Place {
                lane,
                span,
                content,
                ..
            } => {
                let id = ClipId::from_uuid(Uuid::new_v4());
                placed[index] = Some((id, *lane));
                let mut payload = content.payload.clone();
                match &mut payload {
                    ClipPayload::Midi(midi) => {
                        for note in &mut midi.notes {
                            note.id = NoteId::from_uuid(Uuid::new_v4());
                        }
                    }
                    ClipPayload::Audio(audio) => {
                        for slice in &mut audio.slices {
                            slice.id = SliceId::from_uuid(Uuid::new_v4());
                        }
                    }
                }
                commands.push(CommandPayload::CreateClip {
                    id,
                    track_id: *lane,
                    kind: content.kind,
                    range: *span,
                    payload,
                });
                if content.name.is_some() || content.muted {
                    commands.push(CommandPayload::EditClip {
                        clip_id: id,
                        range: None,
                        name: Some(content.name.clone()),
                        muted: Some(content.muted),
                        content_start: None,
                        content_length: None,
                        audio: None,
                    });
                }
                if content.color.is_some() {
                    commands.push(CommandPayload::SetClipColor {
                        clip_id: id,
                        color: content.color,
                    });
                }
            }
        }
    }

    let selection = resolve_selection(model, plan.next, &placed)?;
    Ok(Lowered {
        commands,
        selection,
    })
}

fn resolve_selection(
    model: ArrangerModel<'_>,
    next: NextSelection<TrackId, ClipId>,
    placed: &[Option<(ClipId, TrackId)>],
) -> Result<Option<SelectionInfo<TrackId, ClipId>>, String> {
    match next {
        NextSelection::Unchanged => Ok(None),
        NextSelection::None => Ok(Some(SelectionInfo::default())),
        NextSelection::Objects(refs) => {
            let mut objects = Vec::with_capacity(refs.len());
            for object in refs {
                match object {
                    ObjectRef::Existing(id) => {
                        let view = model
                            .object(id)
                            .ok_or_else(|| format!("missing object {id}"))?;
                        objects.push(ObjectInLane {
                            lane: view.lane,
                            object: id,
                        });
                    }
                    ObjectRef::Created(index) => {
                        let (id, lane) = placed
                            .get(index)
                            .and_then(|entry| *entry)
                            .ok_or_else(|| "created object has no place op".to_string())?;
                        objects.push(ObjectInLane { lane, object: id });
                    }
                }
            }
            Ok(Some(SelectionInfo {
                objects,
                time: None,
            }))
        }
        NextSelection::Time(time) => Ok(Some(SelectionInfo {
            objects: Vec::new(),
            time: Some(TimeSelection {
                span: time.span,
                lanes: time.lanes,
            }),
        })),
    }
}
