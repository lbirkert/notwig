use notwig_core::ProjectDoc;

const MAX_UNDO: usize = 200;

#[derive(Debug, Default)]
pub struct History {
    undo: Vec<ProjectDoc>,
    redo: Vec<ProjectDoc>,
}

impl History {
    pub fn record(&mut self, before: ProjectDoc) {
        self.undo.push(before);
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, current: &ProjectDoc) -> Option<ProjectDoc> {
        let previous = self.undo.pop()?;
        self.redo.push(current.clone());
        Some(previous)
    }

    pub fn redo(&mut self, current: &ProjectDoc) -> Option<ProjectDoc> {
        let next = self.redo.pop()?;
        self.undo.push(current.clone());
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notwig_core::ids::{DeviceChain, Track};
    use notwig_core::{Id, Meta, SampleRate};
    use uuid::Uuid;

    fn doc() -> ProjectDoc {
        let meta = Meta {
            name: "History".into(),
            created_utc: 0,
            modified_utc: 0,
            sample_rate: SampleRate::DEFAULT,
            tempo_bpm: 120.0,
            time_sig_num: 4,
            time_sig_den: 4,
        };
        ProjectDoc::new_template(
            meta,
            Id::<Track>::from_uuid(Uuid::from_u128(1)),
            Id::<DeviceChain>::from_uuid(Uuid::from_u128(2)),
        )
        .unwrap()
    }

    #[test]
    fn undo_and_redo_roundtrip() {
        let mut history = History::default();
        let mut current = doc();
        let before = current.clone();
        current.revision = 1;
        history.record(before.clone());
        let previous = history.undo(&current).unwrap();
        assert_eq!(previous.revision, before.revision);
        let next = history.redo(&previous).unwrap();
        assert_eq!(next.revision, 1);
    }

    #[test]
    fn recording_clears_redo() {
        let mut history = History::default();
        let current = doc();
        history.record(current.clone());
        let previous = history.undo(&current).unwrap();
        history.record(previous);
        assert_eq!(history.redo.len(), 0);
    }
}
