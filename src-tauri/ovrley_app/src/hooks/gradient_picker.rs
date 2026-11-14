//! Pointer and keyboard rules of `GradientPicker` (port of `app/src/hooks/useGradientPicker.js`).

use crate::lib::gradient_stops::{
    move_gradient_stop, remove_gradient_stop, to_gradient_offset, GradientStop, MAX_GRADIENT_STOPS,
    MIN_GRADIENT_STOPS,
};

/// Pointer travel, in logical pixels, before pressing a stop becomes a drag.
pub const DRAG_THRESHOLD_PX: f64 = 3.0;
pub const KEYBOARD_STEP: f64 = 0.01;
pub const KEYBOARD_LARGE_STEP: f64 = 0.1;

pub fn can_add_stop(stop_count: usize) -> bool {
    stop_count < MAX_GRADIENT_STOPS
}

pub fn can_remove_stop(stop_count: usize) -> bool {
    stop_count > MIN_GRADIENT_STOPS
}

/// The offset under a pointer `x` pixels from the track's left edge.
pub fn offset_at(x: f64, track_width: f64) -> f64 {
    to_gradient_offset(x / track_width)
}

/// Whether a press that started at `start_x` has moved far enough to drag.
pub fn exceeds_drag_threshold(start_x: f64, x: f64) -> bool {
    (x - start_x).abs() >= DRAG_THRESHOLD_PX
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopKey {
    Delete,
    ArrowLeft,
    ArrowRight,
    Other,
}

/// What a key press on a stop node does.
#[derive(Clone, Debug, PartialEq)]
pub enum StopKeyResult {
    /// The key is not handled by the stop.
    Ignored,
    /// The key is handled, but the stops stay as they are (Delete with only two stops).
    Handled,
    /// The key is handled and these stops are applied (changed and committed).
    Apply(Vec<GradientStop>),
}

/// Delete and Backspace remove the stop (never below the minimum); ← and → move it by 0.01,
/// or 0.1 with Shift. A move applies even when clamping leaves the offset unchanged.
pub fn stop_key(stops: &[GradientStop], index: usize, key: StopKey, shift: bool) -> StopKeyResult {
    match key {
        StopKey::Delete if can_remove_stop(stops.len()) => {
            StopKeyResult::Apply(remove_gradient_stop(stops, index))
        }
        StopKey::Delete => StopKeyResult::Handled,
        StopKey::ArrowLeft | StopKey::ArrowRight => {
            let magnitude = if shift {
                KEYBOARD_LARGE_STEP
            } else {
                KEYBOARD_STEP
            };
            let step = if key == StopKey::ArrowLeft {
                -magnitude
            } else {
                magnitude
            };
            StopKeyResult::Apply(move_gradient_stop(
                stops,
                index,
                to_gradient_offset(stops[index].offset + step),
            ))
        }
        StopKey::Other => StopKeyResult::Ignored,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(offset: f64, color: &str) -> GradientStop {
        GradientStop {
            offset,
            color: color.into(),
        }
    }

    #[test]
    fn limits_adding_and_removing() {
        assert!(can_add_stop(4));
        assert!(!can_add_stop(5));
        assert!(can_remove_stop(3));
        assert!(!can_remove_stop(2));
    }

    #[test]
    fn maps_pointer_x_to_an_offset() {
        assert_eq!(offset_at(50.0, 200.0), 0.25);
        assert_eq!(offset_at(-10.0, 200.0), 0.0);
        assert_eq!(offset_at(250.0, 200.0), 1.0);
    }

    #[test]
    fn starts_dragging_after_three_pixels() {
        assert!(!exceeds_drag_threshold(10.0, 12.9));
        assert!(exceeds_drag_threshold(10.0, 13.0));
        assert!(exceeds_drag_threshold(10.0, 7.0));
    }

    #[test]
    fn delete_removes_only_above_the_minimum() {
        let three = vec![
            stop(0.0, "#000000"),
            stop(1.0, "#ffffff"),
            stop(0.5, "#808080"),
        ];
        assert_eq!(
            stop_key(&three, 2, StopKey::Delete, false),
            StopKeyResult::Apply(three[..2].to_vec())
        );
        assert_eq!(
            stop_key(&three[..2], 0, StopKey::Delete, false),
            StopKeyResult::Handled
        );
    }

    #[test]
    fn arrows_move_by_small_and_large_steps() {
        let stops = vec![stop(0.5, "#000000"), stop(1.0, "#ffffff")];
        assert_eq!(
            stop_key(&stops, 0, StopKey::ArrowRight, false),
            StopKeyResult::Apply(vec![stop(0.51, "#000000"), stops[1].clone()])
        );
        assert_eq!(
            stop_key(&stops, 0, StopKey::ArrowLeft, true),
            StopKeyResult::Apply(vec![stop(0.4, "#000000"), stops[1].clone()])
        );
        assert_eq!(
            stop_key(&stops, 1, StopKey::ArrowRight, false),
            StopKeyResult::Apply(stops.clone())
        );
        assert_eq!(
            stop_key(&stops, 0, StopKey::Other, false),
            StopKeyResult::Ignored
        );
    }
}
