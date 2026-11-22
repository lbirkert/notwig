use std::collections::BTreeSet;
use std::fmt::Debug;

use crate::time::{RangeMus, TimeMus};

pub mod algebra;
pub mod arranger;

pub use algebra::{
    Edge, ObjectMove, copy, cut, delete, duplicate, materialize, move_selection, paste,
    relocate_objects, split, trim_container, trim_object, trim_selection,
};
pub use arranger::{ArrangerModel, ClipContent, join_clips};

#[must_use]
pub fn epsilon() -> TimeMus {
    TimeMus::new(1, 1_000_000)
}

#[must_use]
pub fn min_clip_span() -> TimeMus {
    TimeMus::new(1, 16)
}

pub trait EditKey: Copy + Ord + std::hash::Hash + Debug + 'static {}
impl<T: Copy + Ord + std::hash::Hash + Debug + 'static> EditKey for T {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EditorKind {
    Arranger,
    AudioClip,
    Midi,
    Automation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ObjectKind {
    ClipMidi,
    ClipAudio,
    Slice,
    Note,
    AutomationPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectExtent {
    Span,
    Point,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaneBounds {
    pub start: TimeMus,
    pub end: Option<TimeMus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectView<L, O> {
    pub id: O,
    pub lane: L,
    pub span: RangeMus,
    pub kind: ObjectKind,
    pub extent: ObjectExtent,
}

impl<L, O> ObjectView<L, O> {
    #[must_use]
    pub fn is_point(&self) -> bool {
        self.extent == ObjectExtent::Point
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrimLimits {
    pub min_start: TimeMus,
    pub max_start: TimeMus,
    pub min_end: TimeMus,
    pub max_end: Option<TimeMus>,
}

pub trait EditModel {
    type Lane: EditKey;
    type Object: EditKey;
    type Content: Clone + PartialEq + Debug;

    fn editor(&self) -> EditorKind;

    fn container(&self) -> Option<RangeMus> {
        None
    }

    fn container_trim_limits(&self) -> Option<TrimLimits> {
        let container = self.container()?;
        let eps = epsilon();
        Some(TrimLimits {
            min_start: container.start,
            max_start: container.end - eps,
            min_end: container.start + eps,
            max_end: Some(container.end),
        })
    }

    fn lanes(&self) -> Vec<Self::Lane>;

    fn lane_bounds(&self, lane: Self::Lane) -> LaneBounds;

    fn objects(&self, lane: Self::Lane) -> Vec<ObjectView<Self::Lane, Self::Object>>;

    fn object(&self, object: Self::Object) -> Option<ObjectView<Self::Lane, Self::Object>>;

    fn trim_limits(&self, object: Self::Object) -> TrimLimits;

    fn extract(&self, object: Self::Object, window: RangeMus) -> Result<Self::Content, EditError>;

    fn accepts(&self, lane: Self::Lane, kind: ObjectKind) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    InvalidSelection(String),
    NotFound(String),
    IncompatibleLane(ObjectKind),
    InvalidSpan,
    InvalidExtraction(String),
    NoContainer,
    PointNotTrimmable,
    InvalidPasteTarget,
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSelection(reason) => write!(f, "invalid selection: {reason}"),
            Self::NotFound(id) => write!(f, "object not found: {id}"),
            Self::IncompatibleLane(kind) => write!(f, "incompatible lane for {kind:?}"),
            Self::InvalidSpan => write!(f, "invalid span"),
            Self::InvalidExtraction(reason) => write!(f, "invalid extraction: {reason}"),
            Self::NoContainer => write!(f, "editor has no container"),
            Self::PointNotTrimmable => write!(f, "point objects cannot be trimmed"),
            Self::InvalidPasteTarget => write!(f, "invalid paste target"),
        }
    }
}

impl std::error::Error for EditError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMode {
    Objects,
    Time,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeSelection<L: Ord> {
    pub span: RangeMus,
    pub lanes: BTreeSet<L>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection<L: Ord, O: Ord> {
    pub objects: BTreeSet<O>,
    pub time: Option<TimeSelection<L>>,
}

impl<L: Ord, O: Ord> Default for Selection<L, O> {
    fn default() -> Self {
        Self {
            objects: BTreeSet::new(),
            time: None,
        }
    }
}

impl<L: EditKey, O: EditKey> Selection<L, O> {
    #[must_use]
    pub fn objects(objects: impl IntoIterator<Item = O>) -> Self {
        Self {
            objects: objects.into_iter().collect(),
            time: None,
        }
    }

    #[must_use]
    pub fn time(span: RangeMus, lanes: impl IntoIterator<Item = L>) -> Self {
        Self {
            objects: BTreeSet::new(),
            time: Some(TimeSelection {
                span,
                lanes: lanes.into_iter().collect(),
            }),
        }
    }

    #[must_use]
    pub fn mode(&self) -> Option<SelectionMode> {
        if !self.objects.is_empty() {
            Some(SelectionMode::Objects)
        } else if self.time.is_some() {
            Some(SelectionMode::Time)
        } else {
            None
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty() && self.time.is_none()
    }

    pub fn clear(&mut self) {
        self.objects.clear();
        self.time = None;
    }

    pub fn select_objects(&mut self, objects: impl IntoIterator<Item = O>, additive: bool) {
        if additive {
            for object in objects {
                if !self.objects.remove(&object) {
                    self.objects.insert(object);
                }
            }
        } else {
            self.objects = objects.into_iter().collect();
            self.time = None;
        }
    }

    pub fn select_time(&mut self, span: RangeMus, lanes: impl IntoIterator<Item = L>) {
        self.objects.clear();
        self.time = Some(TimeSelection {
            span,
            lanes: lanes.into_iter().collect(),
        });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Materialized<L: Ord, O, C> {
    pub mode: SelectionMode,
    pub span: RangeMus,
    pub lanes: BTreeSet<L>,
    pub members: Vec<ObjectView<L, O>>,
    pub fragments: Vec<Fragment<L, C>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fragment<L, C> {
    pub lane: L,
    pub offset: TimeMus,
    pub length: TimeMus,
    pub kind: ObjectKind,
    pub content: C,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Extract<L, C> {
    pub domain: EditorKind,
    pub mode: SelectionMode,
    pub length: TimeMus,
    pub lanes: Vec<L>,
    pub fragments: Vec<Extracted<L, C>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Extracted<L, C> {
    pub lane: L,
    pub offset: TimeMus,
    pub length: TimeMus,
    pub kind: ObjectKind,
    pub content: C,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EditOp<L, O, C> {
    SetContainer {
        span: RangeMus,
    },
    Erase {
        lane: L,
        span: RangeMus,
    },
    Remove {
        object: O,
    },
    Split {
        object: O,
        at: TimeMus,
    },
    Move {
        object: O,
        lane: L,
        span: RangeMus,
    },
    Trim {
        object: O,
        span: RangeMus,
    },
    Place {
        lane: L,
        span: RangeMus,
        kind: ObjectKind,
        content: C,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectRef<O> {
    Existing(O),
    Created(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NextSelection<L: Ord, O: Ord> {
    Unchanged,
    None,
    Objects(Vec<ObjectRef<O>>),
    Time(TimeSelection<L>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan<L: Ord, O: Ord, C> {
    pub ops: Vec<EditOp<L, O, C>>,
    pub next: NextSelection<L, O>,
}

impl<L: Ord, O: Ord, C> Plan<L, O, C> {
    #[must_use]
    pub fn empty(next: NextSelection<L, O>) -> Self {
        Self {
            ops: Vec::new(),
            next,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectInLane<L, O> {
    pub lane: L,
    pub object: O,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionInfo<L: Ord, O: Ord> {
    pub objects: Vec<ObjectInLane<L, O>>,
    pub time: Option<TimeSelection<L>>,
}

impl<L: Ord, O: Ord> Default for SelectionInfo<L, O> {
    fn default() -> Self {
        Self {
            objects: Vec::new(),
            time: None,
        }
    }
}

impl<L: Ord, O: Ord> SelectionInfo<L, O> {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty() && self.time.is_none()
    }
}

pub type EditMaterialized<M> =
    Materialized<<M as EditModel>::Lane, <M as EditModel>::Object, <M as EditModel>::Content>;
pub type EditPlan<M> =
    Plan<<M as EditModel>::Lane, <M as EditModel>::Object, <M as EditModel>::Content>;
pub type EditExtract<M> = Extract<<M as EditModel>::Lane, <M as EditModel>::Content>;
