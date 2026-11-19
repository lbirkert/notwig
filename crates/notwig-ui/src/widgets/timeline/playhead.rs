use notwig_core::TimeMus;

use crate::runtime::RuntimeView;

pub(crate) fn playhead_for_strip(runtime: &RuntimeView) -> TimeMus {
    if runtime.playing {
        runtime.playhead
    } else {
        TimeMus::new(-1_000_000_000, 1)
    }
}
