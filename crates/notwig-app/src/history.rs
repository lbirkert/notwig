use notwig_core::ProjectDoc;

const MAX_UNDO: usize = 200;

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub doc: ProjectDoc,
    pub structural: bool,
}

#[derive(Debug, Default)]
pub struct History {
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
}

impl History {
    pub fn record(&mut self, before: ProjectDoc, structural: bool) {
        self.undo.push(HistoryEntry {
            doc: before,
            structural,
        });
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, current: &ProjectDoc) -> Option<HistoryEntry> {
        let previous = self.undo.pop()?;
        self.redo.push(HistoryEntry {
            doc: current.clone(),
            structural: previous.structural,
        });
        Some(previous)
    }

    pub fn redo(&mut self, current: &ProjectDoc) -> Option<HistoryEntry> {
        let next = self.redo.pop()?;
        self.undo.push(HistoryEntry {
            doc: current.clone(),
            structural: next.structural,
        });
        Some(next)
    }

    #[doc(hidden)]
    #[must_use]
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }
}
