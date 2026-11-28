//! In-memory `IParameterChanges` implementation for host parameter updates.
//!
//! Setting a parameter on a separate VST3 edit controller is not enough for
//! the audio processor to observe it. Hosts that want the change applied to
//! DSP must also supply an `IParameterChanges` object in `ProcessData` at the
//! next process call. This module provides the minimal queue objects for that
//! path: one `IParamValueQueue` per changed parameter, each with one point at
//! sample offset zero.

use std::cell::RefCell;

use vst3::Class;
use vst3::Steinberg::Vst::{
    IParamValueQueue, IParamValueQueueTrait, IParameterChanges, IParameterChangesTrait, ParamID,
    ParamValue,
};
use vst3::Steinberg::{kResultFalse, kResultOk};

pub(crate) struct ParamValueQueue {
    id: ParamID,
    points: RefCell<Vec<(i32, ParamValue)>>,
}

impl ParamValueQueue {
    fn new(id: ParamID) -> Self {
        Self {
            id,
            points: RefCell::new(Vec::with_capacity(1)),
        }
    }

    fn push_point(&self, sample_offset: i32, value: ParamValue) {
        self.points.borrow_mut().push((sample_offset, value));
    }
}

impl Class for ParamValueQueue {
    type Interfaces = (IParamValueQueue,);
}

impl IParamValueQueueTrait for ParamValueQueue {
    unsafe fn getParameterId(&self) -> ParamID {
        self.id
    }

    unsafe fn getPointCount(&self) -> i32 {
        i32::try_from(self.points.borrow().len()).unwrap_or(i32::MAX)
    }

    unsafe fn getPoint(&self, index: i32, sample_offset: *mut i32, value: *mut ParamValue) -> i32 {
        if index < 0 || sample_offset.is_null() || value.is_null() {
            return kResultFalse;
        }
        let points = self.points.borrow();
        let Some((offset, point_value)) = points.get(index as usize) else {
            return kResultFalse;
        };
        unsafe {
            *sample_offset = *offset;
            *value = *point_value;
        }
        kResultOk
    }

    unsafe fn addPoint(&self, sample_offset: i32, value: ParamValue, index: *mut i32) -> i32 {
        let mut points = self.points.borrow_mut();
        let point_index = i32::try_from(points.len()).unwrap_or(i32::MAX);
        points.push((sample_offset, value));
        if !index.is_null() {
            unsafe { *index = point_index };
        }
        kResultOk
    }
}

pub(crate) struct ParameterChanges {
    queues: RefCell<Vec<vst3::ComWrapper<ParamValueQueue>>>,
}

impl ParameterChanges {
    pub(crate) fn from_pending(pending: &[(ParamID, ParamValue)]) -> Self {
        let queues = pending
            .iter()
            .map(|(id, value)| {
                let queue = vst3::ComWrapper::new(ParamValueQueue::new(*id));
                queue.push_point(0, *value);
                queue
            })
            .collect();
        Self {
            queues: RefCell::new(queues),
        }
    }
}

impl Class for ParameterChanges {
    type Interfaces = (IParameterChanges,);
}

impl IParameterChangesTrait for ParameterChanges {
    unsafe fn getParameterCount(&self) -> i32 {
        i32::try_from(self.queues.borrow().len()).unwrap_or(i32::MAX)
    }

    unsafe fn getParameterData(&self, index: i32) -> *mut IParamValueQueue {
        if index < 0 {
            return std::ptr::null_mut();
        }
        self.queues
            .borrow()
            .get(index as usize)
            .and_then(|queue| queue.as_com_ref::<IParamValueQueue>())
            .map_or(std::ptr::null_mut(), |queue| queue.as_ptr())
    }

    unsafe fn addParameterData(
        &self,
        id: *const ParamID,
        index: *mut i32,
    ) -> *mut IParamValueQueue {
        if id.is_null() {
            return std::ptr::null_mut();
        }
        let id = unsafe { *id };
        let queue = vst3::ComWrapper::new(ParamValueQueue::new(id));
        let pointer = queue
            .as_com_ref::<IParamValueQueue>()
            .map_or(std::ptr::null_mut(), |queue| queue.as_ptr());
        let mut queues = self.queues.borrow_mut();
        let queue_index = i32::try_from(queues.len()).unwrap_or(i32::MAX);
        queues.push(queue);
        if !index.is_null() {
            unsafe { *index = queue_index };
        }
        pointer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vst3::ComRef;
    use vst3::Steinberg::Vst::{IParamValueQueueTrait, IParameterChangesTrait};

    #[test]
    fn parameter_changes_expose_one_point_per_parameter() {
        let changes =
            vst3::ComWrapper::new(ParameterChanges::from_pending(&[(7, 0.25), (9, 0.75)]));
        unsafe {
            assert_eq!(changes.getParameterCount(), 2);

            let first = changes.getParameterData(0);
            assert!(!first.is_null());
            let first = ComRef::<IParamValueQueue>::from_raw(first).expect("first queue");
            assert_eq!(first.getParameterId(), 7);
            assert_eq!(first.getPointCount(), 1);
            let mut offset = -1;
            let mut value: ParamValue = 0.0;
            assert_eq!(
                first.getPoint(0, &raw mut offset, &raw mut value),
                kResultOk
            );
            assert_eq!(offset, 0);
            assert!((value - 0.25).abs() < f64::EPSILON);

            let second = changes.getParameterData(1);
            assert!(!second.is_null());
            let second = ComRef::<IParamValueQueue>::from_raw(second).expect("second queue");
            assert_eq!(second.getParameterId(), 9);

            assert!(changes.getParameterData(2).is_null());
        }
    }

    #[test]
    fn add_parameter_data_returns_a_live_queue() {
        let changes = vst3::ComWrapper::new(ParameterChanges::from_pending(&[]));
        let id: ParamID = 42;
        unsafe {
            let mut index = -1;
            let queue = changes.addParameterData(&raw const id, &raw mut index);
            assert!(!queue.is_null());
            let queue = ComRef::<IParamValueQueue>::from_raw(queue).expect("added queue");
            assert_eq!(index, 0);
            assert_eq!(queue.getParameterId(), 42);
            assert_eq!(changes.getParameterCount(), 1);
        }
    }
}
