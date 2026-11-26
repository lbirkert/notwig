//! Reusable time/value curves for automation and clip modulation.
//!
//! A curve is an ordered set of points. Each point owns the segment from its
//! own time to the next point. Curvature bends that segment without moving the
//! endpoints. Values are unitless; callers map them to dB, pan, or a parameter
//! range.

use serde::{Deserialize, Serialize};

use crate::{CoreError, TimeMus};

/// Lowest accepted segment curvature.
pub const MIN_CURVATURE: f64 = -2.0;
/// Highest accepted segment curvature.
pub const MAX_CURVATURE: f64 = 2.0;

/// One curve point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CurvePoint {
    /// Time of the point.
    pub time: TimeMus,
    /// Point value.
    pub value: f64,
    /// Bend of the segment that starts at this point.
    ///
    /// `0` is linear, positive values ease in, and negative values ease out.
    #[serde(default)]
    pub curvature: f64,
}

impl CurvePoint {
    /// Creates a linear point.
    #[must_use]
    pub fn new(time: TimeMus, value: f64) -> Self {
        Self {
            time,
            value,
            curvature: 0.0,
        }
    }

    /// Returns the point with `curvature`.
    #[must_use]
    pub fn with_curvature(mut self, curvature: f64) -> Self {
        self.curvature = curvature;
        self
    }
}

/// An ordered set of curve points. Multiple points may share one time to
/// represent a vertical step.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Curve {
    points: Vec<CurvePoint>,
}

impl Curve {
    /// Creates an empty curve.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a curve from points, using a stable sort by time.
    ///
    /// Multiple points may share a time; their original order is preserved
    /// within that time so a vertical step has a well-defined direction.
    /// Non-finite values and curvature are rejected.
    pub fn from_points(points: impl IntoIterator<Item = CurvePoint>) -> Result<Self, CoreError> {
        let mut points: Vec<CurvePoint> = points.into_iter().collect();
        points.sort_by_key(|point| point.time);
        for point in &points {
            if !point.value.is_finite() {
                return Err(CoreError::ValidationFailed {
                    reason: "curve point value must be finite".into(),
                });
            }
            if !point.curvature.is_finite() {
                return Err(CoreError::ValidationFailed {
                    reason: "curve point curvature must be finite".into(),
                });
            }
        }
        Ok(Self { points })
    }

    /// Returns the ordered points.
    #[must_use]
    pub fn points(&self) -> &[CurvePoint] {
        &self.points
    }

    /// Returns whether the curve has no points.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Returns the number of points.
    #[must_use]
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Returns a point by time.
    ///
    /// When several points share `time`, one of them is returned.
    #[must_use]
    pub fn point(&self, time: TimeMus) -> Option<&CurvePoint> {
        self.points
            .binary_search_by_key(&time, |point| point.time)
            .ok()
            .map(|index| &self.points[index])
    }

    /// Returns all points at `time`, in curve order.
    #[must_use]
    pub fn points_at(&self, time: TimeMus) -> &[CurvePoint] {
        let start = self.points.partition_point(|point| point.time < time);
        let end = self.points.partition_point(|point| point.time <= time);
        &self.points[start..end]
    }

    /// Validates ordering and finite values.
    pub fn validate(&self) -> Result<(), CoreError> {
        for pair in self.points.windows(2) {
            if pair[0].time > pair[1].time {
                return Err(CoreError::ValidationFailed {
                    reason: "curve points must be ordered by time".into(),
                });
            }
        }
        for point in &self.points {
            if !point.value.is_finite() {
                return Err(CoreError::ValidationFailed {
                    reason: "curve point value must be finite".into(),
                });
            }
            if !point.curvature.is_finite() {
                return Err(CoreError::ValidationFailed {
                    reason: "curve point curvature must be finite".into(),
                });
            }
        }
        Ok(())
    }

    /// Evaluates the curve at `time`.
    ///
    /// An empty curve returns `fallback`. Times before the first point and
    /// after the last point hold the nearest endpoint value. When several
    /// points share a time, the segment approaches the first value and the
    /// exact time evaluates to the last value, forming a vertical step.
    #[must_use]
    pub fn value_at(&self, time: TimeMus, fallback: f64) -> f64 {
        let Some(first) = self.points.first() else {
            return fallback;
        };
        if time < first.time {
            return first.value;
        }
        let last = self.points.last().unwrap_or(first);
        if time >= last.time {
            return last.value;
        }
        let index = self.points.partition_point(|point| point.time <= time);
        let Some(left) = index
            .checked_sub(1)
            .and_then(|index| self.points.get(index))
        else {
            return first.value;
        };
        let Some(right) = self.points.get(index) else {
            return last.value;
        };
        let span = right.time.as_f64() - left.time.as_f64();
        let ratio = if span > f64::EPSILON {
            (time.as_f64() - left.time.as_f64()) / span
        } else {
            0.0
        };
        let shaped = shape_ratio(ratio, left.curvature);
        left.value + (right.value - left.value) * shaped
    }
}

/// Maps a linear segment ratio through `curvature`.
///
/// Positive curvature eases in (slow start), negative curvature eases out
/// (fast start), and `0` is linear. Negative curvature is the exact horizontal
/// and vertical mirror of the same positive magnitude. The endpoints are
/// preserved and the result stays in `[0, 1]` for any curvature in
/// [`MIN_CURVATURE`]..=[`MAX_CURVATURE`].
#[must_use]
pub fn shape_ratio(ratio: f64, curvature: f64) -> f64 {
    let t = ratio.clamp(0.0, 1.0);
    let curve = curvature.clamp(MIN_CURVATURE, MAX_CURVATURE);
    if curve >= 0.0 {
        t.powf(2.0_f64.powf(curve))
    } else {
        1.0 - (1.0 - t).powf(2.0_f64.powf(-curve))
    }
    .clamp(0.0, 1.0)
}
