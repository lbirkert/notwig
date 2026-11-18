use notwig_core::ProjectDoc;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SceneSummary {
    pub tracks: usize,
    pub clips: usize,
    pub devices: usize,
    pub assets: usize,
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
