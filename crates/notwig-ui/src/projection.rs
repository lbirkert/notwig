use notwig_core::{
    ClipEntity, ClipKind, ClipPayload, DeviceInstance, Id, ProjectDoc, RangeMus, RtEvent, TrackId,
    TrackKind,
};

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ProjectionError {
    #[error("track {0} does not exist")]
    MissingTrack(TrackId),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClipView {
    pub id: Id<notwig_core::ids::Clip>,
    pub name: String,
    pub range: RangeMus,
    pub kind: ClipKind,
    pub muted: bool,
    pub note_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArrangerRow {
    pub track: TrackId,
    pub name: String,
    pub kind: TrackKind,
    pub depth: usize,
    pub parent: Option<TrackId>,
    pub clips: Vec<ClipView>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ArrangerView {
    pub rows: Vec<ArrangerRow>,
    pub loop_range: Option<RangeMus>,
    pub tempo_bpm: f64,
    pub time_sig: (u8, u8),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MixerStrip {
    pub track: TrackId,
    pub name: String,
    pub volume_db: notwig_core::Db,
    pub pan: notwig_core::Pan,
    pub mute: bool,
    pub solo: bool,
    pub peak_l: f32,
    pub peak_r: f32,
    pub rms_l: f32,
    pub rms_r: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MixerView {
    pub strips: Vec<MixerStrip>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceSlot {
    pub id: Id<notwig_core::ids::Device>,
    pub type_id: String,
    pub name: String,
    pub bypass: bool,
    pub kind: notwig_core::DeviceKind,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SceneSummary {
    pub tracks: usize,
    pub clips: usize,
    pub devices: usize,
    pub assets: usize,
}

#[must_use]
pub fn project_arranger(doc: &ProjectDoc, track_id: Option<TrackId>) -> ArrangerView {
    let mut view = ArrangerView {
        loop_range: doc.transport.loop_range,
        tempo_bpm: doc.meta.tempo_bpm,
        time_sig: (doc.meta.time_sig_num, doc.meta.time_sig_den),
        rows: Vec::new(),
    };
    if let Some(id) = track_id {
        if let Some(row) = build_row(doc, id, 0) {
            view.rows.push(row);
        }
        return view;
    }
    for id in doc.track_order.depth_first() {
        if let Some(row) = build_row(doc, id, 0) {
            view.rows.push(row);
        }
    }
    view
}

fn build_row(doc: &ProjectDoc, id: TrackId, depth: usize) -> Option<ArrangerRow> {
    let track = doc.tracks.get(&id)?;
    let clips = doc
        .arranger
        .lanes
        .get(&id)
        .into_iter()
        .flat_map(|lane| lane.clips.iter())
        .filter_map(|clip_id| doc.arranger.clips.get(clip_id))
        .map(clip_view)
        .collect();
    Some(ArrangerRow {
        track: id,
        name: track.name.clone(),
        kind: track.kind,
        depth,
        parent: track.parent,
        clips,
    })
}

fn clip_view(clip: &ClipEntity) -> ClipView {
    let note_count = match &clip.payload {
        ClipPayload::Midi(midi) => midi.notes.len(),
        ClipPayload::Audio(_) => 0,
    };
    ClipView {
        id: clip.id,
        name: clip.name.clone().unwrap_or_else(|| "Clip".into()),
        range: clip.range,
        kind: clip.kind,
        muted: clip.muted,
        note_count,
    }
}

#[must_use]
pub fn project_mixer(doc: &ProjectDoc, meters: &[RtEvent]) -> MixerView {
    let mut meter_map = std::collections::BTreeMap::new();
    for event in meters {
        if let RtEvent::Meters {
            track_id,
            peak_l,
            peak_r,
            rms_l,
            rms_r,
        } = event
        {
            meter_map.insert(*track_id, (*peak_l, *peak_r, *rms_l, *rms_r));
        }
    }
    let strips = doc
        .track_order
        .depth_first()
        .into_iter()
        .filter_map(|id| doc.tracks.get(&id))
        .map(|track| {
            let (peak_l, peak_r, rms_l, rms_r) =
                meter_map.get(&track.id).copied().unwrap_or_default();
            MixerStrip {
                track: track.id,
                name: track.name.clone(),
                volume_db: track.mixer.volume_db,
                pan: track.mixer.pan,
                mute: track.mixer.mute,
                solo: track.mixer.solo,
                peak_l,
                peak_r,
                rms_l,
                rms_r,
            }
        })
        .collect();
    MixerView { strips }
}

#[must_use]
pub fn project_devices(
    doc: &ProjectDoc,
    track: TrackId,
) -> Result<Vec<DeviceSlot>, ProjectionError> {
    let track = doc
        .tracks
        .get(&track)
        .ok_or(ProjectionError::MissingTrack(track))?;
    let chain = doc
        .chains
        .get(&track.main_chain)
        .map(|chain| chain.devices.clone())
        .unwrap_or_default();
    Ok(chain
        .into_iter()
        .filter_map(|id| doc.devices.get(&id))
        .map(device_slot)
        .collect())
}

fn device_slot(device: &DeviceInstance) -> DeviceSlot {
    DeviceSlot {
        id: device.id,
        type_id: device.type_id.clone(),
        name: device
            .name
            .clone()
            .unwrap_or_else(|| device.type_id.clone()),
        bypass: device.bypass,
        kind: device.kind,
    }
}

#[must_use]
pub fn scene_summary(doc: &ProjectDoc) -> SceneSummary {
    SceneSummary {
        tracks: doc.tracks.len(),
        clips: doc.arranger.clips.len(),
        devices: doc.devices.len(),
        assets: doc.assets.assets.len(),
    }
}
