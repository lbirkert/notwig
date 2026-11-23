use serde::{Deserialize, Serialize};

use crate::ids::{DeviceId, TrackId};
use crate::time::TimeMus;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RtEvent {
    TransportTick {
        playhead_samples: u64,
        playhead_mus: TimeMus,
        playing: bool,
    },
    Meters {
        track_id: TrackId,
        peak_l: f32,
        peak_r: f32,
        rms_l: f32,
        rms_r: f32,
    },
    Status {
        xruns: u64,
        cpu_load: Option<f32>,
    },
    LatencyChanged {
        device_id: Option<DeviceId>,
        samples: u32,
    },
    Error {
        scope: String,
        code: String,
        message: String,
    },
}
