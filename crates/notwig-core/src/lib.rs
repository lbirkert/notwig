#![forbid(unsafe_code)]

pub mod apply;
pub mod curve;
pub mod domain;
pub mod dsp;
pub mod edit;
pub mod ids;
pub mod protocol;
pub mod sequencer;
pub mod time;
pub mod tracks;

pub use apply::{ApplyResult, apply};
pub use curve::{Curve, CurvePoint, MAX_CURVATURE, MIN_CURVATURE, shape_ratio};
pub use domain::{
    AssetEntry, AssetIndex, AssetKind, AudioAssetBuffer, DeviceChain, DeviceInstance, DeviceKind,
    Meta, ParamValue, ProjectDoc, SCHEMA_VERSION, SoloContext, StateBlobRef, TransportDoc,
};
pub use ids::{
    AssetId, ClipId, DeviceChainId, DeviceId, Entity, Id, NoteId, ParamId, SendId, SliceId, TrackId,
};
pub use protocol::{
    AffectedSet, Command, CommandPayload, DocChange, EngineMsg, PlanHandle, PlaybackSchedule,
    Query, QueryResult, RtEvent, StateDelta,
};
pub use sequencer::{
    ArrangerState, AudioClipData, ClipEntity, ClipKind, ClipPayload, ExprValue, FadeShape, Lane,
    MidiClipData, NoteEntity,
};
pub use time::{
    Color, Db, GainLinear, Hz, Norm01, Pan, RangeMus, Rational, SampleRate, SchemaVersion,
    Semitones, TempoMap, TimeAudio, TimeMus,
};
pub use tracks::{
    AudioInputRef, Connection, MixerState, MonitorMode, RoutingGraph, Send as TrackSend,
    SignalType, Track, TrackForest, TrackIO, TrackKind, TrackOutput,
};

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum CoreError {
    #[error("invalid command: {reason}")]
    InvalidCommand { reason: String },
    #[error("not found: {id}")]
    NotFound { id: String },
    #[error("duplicate id: {id}")]
    DuplicateId { id: String },
    #[error("validation failed: {reason}")]
    ValidationFailed { reason: String },
    #[error("conflict: {reason}")]
    Conflict { reason: String },
    #[error("unsupported: {reason}")]
    Unsupported { reason: String },
}
