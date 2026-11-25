//! Editing linear gradient stops, and the gradient picker's pointer and keyboard rules.
//!
//! A gradient is a list of stops whose `offset` is in [0, 1] and whose `color` is a normalized
//! `#rrggbb` string. List order is insertion order; drawing sorts by offset, so a dragged stop
//! keeps its index.

use crate::controls::color::{hex_to_rgb, rgb_to_hex, Rgba};

pub const MIN_GRADIENT_STOPS: usize = 2;
pub const MAX_GRADIENT_STOPS: usize = 5;

const OFFSET_PRECISION: f64 = 1000.0;

#[derive(Clone, Debug, PartialEq)]
pub struct GradientStop {
    pub offset: f64,
    pub color: String,
}

/// One piece of the track fill: a two-colour gradient between `start` and `end` (offsets).
/// Slint gradients have a fixed stop count, so the track is drawn as these segments.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackSegment {
    pub start: f64,
    pub end: f64,
    pub start_color: String,
    pub end_color: String,
}

/// Clamps an offset into [0, 1] and rounds it to 0.001.
pub fn to_gradient_offset(offset: f64) -> f64 {
    (offset.clamp(0.0, 1.0) * OFFSET_PRECISION).round() / OFFSET_PRECISION
}

/// A copy of the stops ordered by offset; equal offsets keep their order.
pub fn sort_gradient_stops(stops: &[GradientStop]) -> Vec<GradientStop> {
    let mut sorted = stops.to_vec();
    sorted.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    sorted
}

/// The track as solid ends plus one gradient segment per pair of neighbouring stops, matching
/// `linear-gradient(90deg, c0 o0%, c1 o1%, …)` over the sorted stops.
pub fn track_segments(stops: &[GradientStop]) -> Vec<TrackSegment> {
    let sorted = sort_gradient_stops(stops);
    let first = &sorted[0];
    let last = &sorted[sorted.len() - 1];
    let solid = |start: f64, end: f64, color: &str| TrackSegment {
        start,
        end,
        start_color: color.into(),
        end_color: color.into(),
    };
    let mut segments = vec![solid(0.0, first.offset, &first.color)];
    segments.extend(sorted.windows(2).map(|pair| TrackSegment {
        start: pair[0].offset,
        end: pair[1].offset,
        start_color: pair[0].color.clone(),
        end_color: pair[1].color.clone(),
    }));
    segments.push(solid(last.offset, 1.0, &last.color));
    segments.retain(|segment| segment.end > segment.start);
    segments
}

/// The interpolated colour at an offset, clamped to the end stops outside their range.
pub fn sample_gradient_color(stops: &[GradientStop], offset: f64) -> String {
    let sorted = sort_gradient_stops(stops);
    let first = &sorted[0];
    let last = &sorted[sorted.len() - 1];
    if offset <= first.offset {
        return first.color.clone();
    }
    if offset >= last.offset {
        return last.color.clone();
    }
    let upper_index = sorted
        .iter()
        .position(|stop| stop.offset >= offset)
        .expect("offset lies inside the stop range");
    let lower = &sorted[upper_index - 1];
    let upper = &sorted[upper_index];
    let t = (offset - lower.offset) / (upper.offset - lower.offset);
    let (lower, upper) = (hex_to_rgb(&lower.color, 1.0), hex_to_rgb(&upper.color, 1.0));
    let mix = |from: f64, to: f64| from + (to - from) * t;
    rgb_to_hex(Rgba { r: mix(lower.r, upper.r), g: mix(lower.g, upper.g), b: mix(lower.b, upper.b), a: 1.0 })
}

/// Appends a stop at the offset, coloured to match the gradient there.
pub fn add_gradient_stop(stops: &[GradientStop], offset: f64) -> Vec<GradientStop> {
    let mut next = stops.to_vec();
    next.push(GradientStop {
        offset,
        color: sample_gradient_color(stops, offset),
    });
    next
}

pub fn move_gradient_stop(stops: &[GradientStop], index: usize, offset: f64) -> Vec<GradientStop> {
    let mut next = stops.to_vec();
    next[index].offset = offset;
    next
}

pub fn set_gradient_stop_color(
    stops: &[GradientStop],
    index: usize,
    color: &str,
) -> Vec<GradientStop> {
    let mut next = stops.to_vec();
    next[index].color = color.into();
    next
}

pub fn remove_gradient_stop(stops: &[GradientStop], index: usize) -> Vec<GradientStop> {
    let mut next = stops.to_vec();
    next.remove(index);
    next
}

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

    fn stops() -> Vec<GradientStop> {
        vec![stop(1.0, "#ffffff"), stop(0.0, "#000000")]
    }

    #[test]
    fn builds_track_segments_ordered_by_offset() {
        // Replaces the CSS case `linear-gradient(90deg, #000000 0%, #ffffff 100%)`.
        assert_eq!(
            track_segments(&stops()),
            vec![TrackSegment {
                start: 0.0,
                end: 1.0,
                start_color: "#000000".into(),
                end_color: "#ffffff".into()
            }]
        );
        assert_eq!(
            track_segments(&[stop(0.75, "#ff0000"), stop(0.25, "#0000ff")]),
            vec![
                TrackSegment {
                    start: 0.0,
                    end: 0.25,
                    start_color: "#0000ff".into(),
                    end_color: "#0000ff".into()
                },
                TrackSegment {
                    start: 0.25,
                    end: 0.75,
                    start_color: "#0000ff".into(),
                    end_color: "#ff0000".into()
                },
                TrackSegment {
                    start: 0.75,
                    end: 1.0,
                    start_color: "#ff0000".into(),
                    end_color: "#ff0000".into()
                },
            ]
        );
    }

    #[test]
    fn clamps_and_rounds_offsets() {
        assert_eq!(to_gradient_offset(-0.2), 0.0);
        assert_eq!(to_gradient_offset(1.4), 1.0);
        assert_eq!(to_gradient_offset(0.123456), 0.123);
    }

    #[test]
    fn samples_interpolated_colors_and_clamps_outside_the_stop_range() {
        assert_eq!(sample_gradient_color(&stops(), 0.5), "#808080");
        assert_eq!(
            sample_gradient_color(&[stop(0.2, "#ff0000"), stop(1.0, "#ffffff")], 0.1),
            "#ff0000"
        );
    }

    #[test]
    fn adds_a_stop_colored_to_match_the_gradient_at_its_offset() {
        let mut expected = stops();
        expected.push(stop(0.25, "#404040"));
        assert_eq!(add_gradient_stop(&stops(), 0.25), expected);
    }

    #[test]
    fn moves_recolors_and_removes_stops_by_index_without_reordering() {
        let s = stops();
        assert_eq!(
            move_gradient_stop(&s, 1, 0.4),
            vec![s[0].clone(), stop(0.4, "#000000")]
        );
        assert_eq!(
            set_gradient_stop_color(&s, 0, "#ff0000"),
            vec![stop(1.0, "#ff0000"), s[1].clone()]
        );
        assert_eq!(remove_gradient_stop(&s, 0), vec![s[1].clone()]);
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
