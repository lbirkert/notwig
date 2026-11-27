//! Shared curve selection operations for clip editors.
//!
//! Built-in clip editors and extension-rendered editors use these operations so
//! copy, cut, paste, duplicate, delete, and select-all behave the same way.

use std::collections::BTreeSet;

use notwig_core::{Curve, CurvePoint, RangeMus, TimeMus};

pub use notwig_ext_api::AudioSubEditorSelectionActions as CurveEditActions;

/// Result of applying one frame of curve edit gestures.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CurveEditResult {
    /// The curve or selection changed.
    pub changed: bool,
    /// Points to publish to the shared curve clipboard.
    pub copied: Option<Vec<CurvePoint>>,
}

/// Applies shared selection gestures to `curve`.
///
/// Selection indices point into `curve`, and `time_selection` is clip-local.
/// `paste_at` is already mapped into the same clip-local timeline.
pub fn apply_curve_edit_actions(
    curve: &mut Curve,
    selected: &mut BTreeSet<usize>,
    time_selection: &mut Option<RangeMus>,
    actions: &CurveEditActions,
    clipboard: &[CurvePoint],
) -> CurveEditResult {
    let mut result = CurveEditResult::default();
    if actions.select_all {
        *selected = (0..curve.len()).collect();
        *time_selection = None;
    }
    let points = selected_points(curve, selected);
    if (actions.copy || actions.cut) && !points.is_empty() {
        result.copied = Some(points.clone());
        if actions.cut {
            *curve = remove_selected(curve, selected);
            selected.clear();
            *time_selection = None;
            result.changed = true;
        }
    }
    if actions.delete && !selected.is_empty() {
        *curve = remove_selected(curve, selected);
        selected.clear();
        *time_selection = None;
        result.changed = true;
    }
    if actions.duplicate && !points.is_empty() {
        let start = points
            .iter()
            .map(|point| point.time)
            .min()
            .unwrap_or_else(TimeMus::zero);
        let end = points
            .iter()
            .map(|point| point.time)
            .max()
            .unwrap_or_else(TimeMus::zero);
        let mut offset = time_selection.map_or(end - start, RangeMus::length);
        if offset <= TimeMus::zero() {
            offset = TimeMus::new(1, 1);
        }
        let copies: Vec<CurvePoint> = points
            .iter()
            .map(|point| {
                let mut copy = *point;
                copy.time = copy.time + offset;
                copy
            })
            .collect();
        if let Some(selection) = append_points(curve, copies) {
            *selected = selection;
            *time_selection = time_selection
                .and_then(|range| RangeMus::new(range.start + offset, range.end + offset).ok());
            result.changed = true;
        }
    }
    if actions.paste && !clipboard.is_empty() {
        let origin = clipboard
            .iter()
            .map(|point| point.time)
            .min()
            .unwrap_or_else(TimeMus::zero);
        let offset = actions.paste_at - origin;
        let copies: Vec<CurvePoint> = clipboard
            .iter()
            .map(|point| {
                let mut copy = *point;
                copy.time = copy.time + offset;
                copy
            })
            .collect();
        if let Some(selection) = append_points(curve, copies) {
            *selected = selection;
            *time_selection = None;
            result.changed = true;
        }
    }
    result
}

fn selected_points(curve: &Curve, selection: &BTreeSet<usize>) -> Vec<CurvePoint> {
    selection
        .iter()
        .filter_map(|index| curve.points().get(*index))
        .copied()
        .collect()
}

fn remove_selected(curve: &Curve, selection: &BTreeSet<usize>) -> Curve {
    Curve::from_points(
        curve
            .points()
            .iter()
            .enumerate()
            .filter(|(index, _)| !selection.contains(index))
            .map(|(_, point)| *point),
    )
    .unwrap_or_else(|_| curve.clone())
}

fn append_points(curve: &mut Curve, copies: Vec<CurvePoint>) -> Option<BTreeSet<usize>> {
    let mut tagged: Vec<(bool, CurvePoint)> = curve
        .points()
        .iter()
        .copied()
        .map(|point| (false, point))
        .collect();
    for point in copies {
        tagged.push((true, point));
    }
    tagged.sort_by_key(|(_, point)| point.time);
    let selection = tagged
        .iter()
        .enumerate()
        .filter(|(_, (added, _))| *added)
        .map(|(index, _)| index)
        .collect();
    *curve = Curve::from_points(tagged.into_iter().map(|(_, point)| point)).ok()?;
    Some(selection)
}
