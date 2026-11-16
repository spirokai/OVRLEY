//! Port of `lib/activity`: the activity trim (`activity-trim.js`). The activity context and
//! coverage (`activity-context.js`) join with the video model. The summary lives with the media
//! slice (`ActivitySummary`).

use crate::lib::utils::clamp;

/// `MIN_ACTIVITY_TRIM_SECONDS`: the shortest trimmed activity.
pub const MIN_ACTIVITY_TRIM_SECONDS: f64 = 1.0;

/// The trimmed part of the external activity, in activity seconds (which are timeline seconds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActivityTrim {
    pub start_second: f64,
    pub end_second: f64,
}

/// `resolveActivityWindow`: the rendered activity window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActivityWindow {
    pub start_second: f64,
    pub end_second: f64,
    pub is_trimmed: bool,
}

/// Which edge of the trim moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrimEdge {
    Start,
    End,
}

/// `resolveActivityWindow`.
pub fn resolve_activity_window(duration_seconds: f64, trim: Option<ActivityTrim>) -> ActivityWindow {
    match trim {
        None => ActivityWindow { start_second: 0.0, end_second: duration_seconds, is_trimmed: false },
        Some(trim) => ActivityWindow { start_second: trim.start_second, end_second: trim.end_second, is_trimmed: true },
    }
}

/// `trimActivityEdge`: moves one edge, keeping it inside the activity and the edges
/// [`MIN_ACTIVITY_TRIM_SECONDS`] apart. A trim covering the whole activity is `None`.
pub fn trim_activity_edge(trim: Option<ActivityTrim>, duration_seconds: f64, edge: TrimEdge, second: f64) -> Option<ActivityTrim> {
    let window = resolve_activity_window(duration_seconds, trim);
    let next = match edge {
        TrimEdge::Start => ActivityTrim {
            start_second: clamp(second, 0.0, (window.end_second - MIN_ACTIVITY_TRIM_SECONDS).max(0.0)),
            end_second: window.end_second,
        },
        TrimEdge::End => ActivityTrim {
            start_second: window.start_second,
            end_second: clamp(second, duration_seconds.min(window.start_second + MIN_ACTIVITY_TRIM_SECONDS), duration_seconds),
        },
    };
    if next.start_second <= 0.0 && next.end_second >= duration_seconds {
        None
    } else {
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trim(start_second: f64, end_second: f64) -> Option<ActivityTrim> {
        Some(ActivityTrim { start_second, end_second })
    }

    #[test]
    fn the_window_is_the_whole_activity_without_a_trim() {
        assert_eq!(resolve_activity_window(60.0, None), ActivityWindow { start_second: 0.0, end_second: 60.0, is_trimmed: false });
        assert_eq!(resolve_activity_window(60.0, trim(5.0, 40.0)), ActivityWindow { start_second: 5.0, end_second: 40.0, is_trimmed: true });
    }

    #[test]
    fn an_edge_moves_inside_the_activity() {
        assert_eq!(trim_activity_edge(None, 60.0, TrimEdge::Start, 10.0), trim(10.0, 60.0));
        assert_eq!(trim_activity_edge(None, 60.0, TrimEdge::End, 45.0), trim(0.0, 45.0));
        assert_eq!(trim_activity_edge(trim(10.0, 60.0), 60.0, TrimEdge::Start, -5.0), None, "back to the whole activity");
        assert_eq!(trim_activity_edge(trim(0.0, 45.0), 60.0, TrimEdge::End, 90.0), None);
    }

    #[test]
    fn edges_stay_one_second_apart() {
        assert_eq!(trim_activity_edge(trim(10.0, 20.0), 60.0, TrimEdge::Start, 25.0), trim(19.0, 20.0));
        assert_eq!(trim_activity_edge(trim(10.0, 20.0), 60.0, TrimEdge::End, 5.0), trim(10.0, 11.0));
    }

    #[test]
    fn a_short_activity_keeps_its_edges_inside() {
        assert_eq!(trim_activity_edge(None, 0.5, TrimEdge::Start, 0.4), None, "no room for a trim");
        assert_eq!(trim_activity_edge(trim(0.2, 0.5), 0.5, TrimEdge::End, 0.1), trim(0.2, 0.5));
    }

    #[test]
    fn the_end_stays_after_the_start() {
        assert_eq!(trim_activity_edge(trim(30.0, 60.0), 60.0, TrimEdge::End, 10.0), trim(30.0, 31.0));
    }
}
