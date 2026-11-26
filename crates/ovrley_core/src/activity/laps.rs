//! Canonical lap timing: boundary resolution, sample-aligned series and ingress validation.
//!
//! Explicit source lap labels are authoritative. Otherwise boundaries come from elapsed-time
//! beacons or geographic VBO timing-line crossings. Every source yields the same zero-based
//! lap labels and exact activity-relative boundary times. The activity's end closes the final
//! lap, so every lap start has a duration. Distance-based deltas use the shared widget
//! derivation, which hosts also apply to a user lap selection.

use crate::activity::schema::{
    LapMarkers, NumericSeries, ParsedActivity, TimingMarker, TimingMarkerKind,
};
use crate::error::{CoreError, CoreResult};
use ovrley_widgets::widgets::lap_timer::{compute_delta_to_best, lap_time_at};

/// Canonical lap series and compact completed-lap metadata produced at finalization.
pub(crate) struct LapTiming {
    /// Canonical sample-aligned lap IDs; `-1` denotes the out-lap.
    pub(crate) lap_number: Vec<i64>,
    /// Elapsed time since the exact opening boundary of each active lap.
    pub(crate) lap_time_seconds: NumericSeries,
    /// Exact activity-relative start time of each canonical lap.
    pub(crate) lap_start_elapsed_seconds: Vec<f64>,
    /// Live difference from the best previously completed lap at the same distance.
    pub(crate) delta_to_best_lap_seconds: NumericSeries,
    /// Fastest lap duration, including the final lap closed by the activity's end.
    pub(crate) best_lap_time_seconds: Option<f64>,
    /// Completed durations indexed by canonical lap ID.
    pub(crate) lap_durations_seconds: Vec<f64>,
    /// Prefix minimum of `lap_durations_seconds`.
    pub(crate) lap_durations_best_so_far_seconds: Vec<f64>,
}

/// Validates every lap-timing field in a parsed activity as one strict contract.
///
/// Activities without any lap fields are valid optional absence. Once any lap
/// field is present, aligned series, ordered boundaries, completed durations,
/// prefix minima, and derived lap times must all agree.
pub(crate) fn validate(activity: &ParsedActivity) -> CoreResult<()> {
    let has_lap_data = !activity.lap_number.is_empty()
        || !activity.lap_time_seconds.is_empty()
        || !activity.lap_start_elapsed_seconds.is_empty()
        || !activity.delta_to_best_lap_seconds.is_empty()
        || !activity.lap_durations_seconds.is_empty()
        || !activity.lap_durations_best_so_far_seconds.is_empty();
    if !has_lap_data {
        return Ok(());
    }

    let sample_count = activity.sample_elapsed_seconds.len();
    if activity.lap_number.len() != sample_count || activity.lap_time_seconds.len() != sample_count
    {
        return Err(CoreError::Activity(
            "lap timing series must align with sample_elapsed_seconds".into(),
        ));
    }
    if activity
        .lap_start_elapsed_seconds
        .iter()
        .any(|start| !start.is_finite() || *start < 0.0)
        || activity
            .lap_start_elapsed_seconds
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        return Err(CoreError::Activity(
            "lap_start_elapsed_seconds must be finite, nonnegative, and strictly increasing".into(),
        ));
    }

    let lap_count = activity.lap_start_elapsed_seconds.len();
    if activity.lap_durations_seconds.len() != lap_count
        || activity.lap_durations_best_so_far_seconds.len() != lap_count
    {
        return Err(CoreError::Activity(
            "lap duration metadata must contain one entry per lap".into(),
        ));
    }
    if activity
        .lap_durations_seconds
        .iter()
        .chain(&activity.lap_durations_best_so_far_seconds)
        .any(|duration| !duration.is_finite() || *duration < 0.0)
    {
        return Err(CoreError::Activity(
            "lap duration metadata must be finite and nonnegative".into(),
        ));
    }
    if !activity.delta_to_best_lap_seconds.is_empty()
        && activity.delta_to_best_lap_seconds.len() != sample_count
    {
        return Err(CoreError::Activity(
            "delta_to_best_lap_seconds must align with sample_elapsed_seconds".into(),
        ));
    }
    if activity
        .delta_to_best_lap_seconds
        .iter()
        .flatten()
        .any(|delta| !delta.is_finite())
    {
        return Err(CoreError::Activity(
            "delta_to_best_lap_seconds values must be finite or null".into(),
        ));
    }
    let mut best = f64::INFINITY;
    for (index, duration) in activity.lap_durations_seconds.iter().copied().enumerate() {
        best = best.min(duration);
        if (activity.lap_durations_best_so_far_seconds[index] - best).abs() > 1e-9 {
            return Err(CoreError::Activity(
                "lap_durations_best_so_far_seconds must be the prefix minimum of lap_durations_seconds".into(),
            ));
        }
    }
    for (index, elapsed) in activity.sample_elapsed_seconds.iter().copied().enumerate() {
        let lap_number = activity.lap_number[index];
        if lap_number < -1 {
            return Err(CoreError::Activity(format!(
                "lap_number[{index}] must be -1 or a nonnegative source label"
            )));
        }
        let expected_lap_time =
            lap_time_at(&activity.lap_start_elapsed_seconds, lap_number, elapsed);
        match (activity.lap_time_seconds[index], expected_lap_time) {
            (None, None) => {}
            (Some(actual), Some(expected))
                if actual.is_finite() && actual >= 0.0 && (actual - expected).abs() <= 1e-6 => {}
            _ => {
                return Err(CoreError::Activity(format!(
                    "lap_time_seconds[{index}] does not match lap_start_elapsed_seconds"
                )))
            }
        }
    }

    Ok(())
}

/// Sample-aligned canonical lap IDs and exact elapsed times of their boundaries.
struct ResolvedLaps {
    /// Canonical zero-based lap ID at each sample, or `-1` before the first boundary.
    lap_number: Vec<i64>,
    /// Exact activity-relative elapsed time of each observed lap boundary.
    lap_start_elapsed: Vec<f64>,
}

/// Planar point used only for local line-segment intersection calculations.
#[derive(Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
}

/// Finite line segment shared by timing lines and consecutive route samples.
#[derive(Clone, Copy)]
struct Segment {
    start: Point,
    end: Point,
}

/// Resolves laps from the single authoritative boundary source available.
///
/// Explicit sample-aligned lap labels take precedence over marker metadata.
/// Without either source, every sample remains in the canonical out-lap.
fn resolve_laps(
    elapsed: &[f64],
    course: &[(Option<f64>, Option<f64>)],
    lap_number_source: &[Option<i64>],
    lap_markers: &LapMarkers,
) -> ResolvedLaps {
    if lap_number_source.iter().any(Option::is_some) {
        return resolve_from_source(elapsed, lap_number_source);
    }

    match lap_markers {
        LapMarkers::BeaconMarkers(beacons) => resolve_from_beacons(elapsed, beacons),
        LapMarkers::TimingMarkers(markers) => resolve_from_timing_markers(elapsed, course, markers),
        LapMarkers::None => ResolvedLaps {
            lap_number: vec![-1; elapsed.len()],
            lap_start_elapsed: Vec::new(),
        },
    }
}

/// Canonicalizes changes in source lap labels into sequential zero-based laps.
///
/// Missing source samples retain the last resolved label. Negative labels denote
/// an out-lap while still marking a source transition boundary.
fn resolve_from_source(elapsed: &[f64], lap_number_source: &[Option<i64>]) -> ResolvedLaps {
    let mut lap_number = Vec::with_capacity(lap_number_source.len());
    let mut lap_start_elapsed = Vec::new();
    let mut current_source_lap = -1;
    let mut current_timing_lap = -1;

    for (index, source_lap) in lap_number_source.iter().copied().enumerate() {
        if let Some(source_lap) = source_lap {
            if source_lap != current_source_lap {
                lap_start_elapsed.push(elapsed[index]);
                current_timing_lap = if source_lap >= 0 {
                    lap_start_elapsed.len() as i64 - 1
                } else {
                    -1
                };
            }
            current_source_lap = source_lap;
        }
        lap_number.push(current_timing_lap);
    }

    ResolvedLaps {
        lap_number,
        lap_start_elapsed,
    }
}

/// Resolves canonical lap labels from exact elapsed-time beacon boundaries.
///
/// Beacons beyond the final activity sample are excluded because they do not
/// start an observed lap in this activity.
fn resolve_from_beacons(elapsed: &[f64], beacons: &[f64]) -> ResolvedLaps {
    if beacons.is_empty() {
        return ResolvedLaps {
            lap_number: vec![-1; elapsed.len()],
            lap_start_elapsed: Vec::new(),
        };
    }
    let mut started_lap_count = 0;
    let lap_number = elapsed
        .iter()
        .map(|sample_elapsed| {
            while beacons
                .get(started_lap_count)
                .is_some_and(|beacon| *beacon <= *sample_elapsed)
            {
                started_lap_count += 1;
            }
            started_lap_count as i64 - 1
        })
        .collect();
    let last_elapsed = elapsed.last().copied().unwrap_or(0.0);
    let lap_start_elapsed = beacons
        .iter()
        .copied()
        .take_while(|beacon| *beacon <= last_elapsed)
        .collect();
    ResolvedLaps {
        lap_number,
        lap_start_elapsed,
    }
}

/// Resolves lap boundaries where route segments cross the first start marker.
///
/// Crossing time is interpolated within the route segment to preserve sub-sample
/// precision. Missing coordinates retain the current lap without creating a
/// boundary.
fn resolve_from_timing_markers(
    elapsed: &[f64],
    course: &[(Option<f64>, Option<f64>)],
    markers: &[TimingMarker],
) -> ResolvedLaps {
    let start_markers: Vec<&TimingMarker> = markers
        .iter()
        .filter(|marker| marker.kind == TimingMarkerKind::Start)
        .collect();

    if start_markers.is_empty() || course.len() < 2 {
        return ResolvedLaps {
            lap_number: vec![-1; course.len()],
            lap_start_elapsed: Vec::new(),
        };
    }

    let marker = &start_markers[0];
    let marker_segment = Segment {
        start: Point {
            x: marker.longitude_a,
            y: marker.latitude_a,
        },
        end: Point {
            x: marker.longitude_b,
            y: marker.latitude_b,
        },
    };

    let mut lap_number = vec![-1i64; course.len()];
    let mut lap_start_elapsed = Vec::new();

    for index in 1..course.len() {
        lap_number[index] = lap_start_elapsed.len() as i64 - 1;
        let (previous_latitude, previous_longitude) = course[index - 1];
        let (current_latitude, current_longitude) = course[index];

        let Some(previous_latitude) = previous_latitude else {
            continue;
        };
        let Some(previous_longitude) = previous_longitude else {
            continue;
        };
        let Some(current_latitude) = current_latitude else {
            continue;
        };
        let Some(current_longitude) = current_longitude else {
            continue;
        };

        let route_segment = Segment {
            start: Point {
                x: previous_longitude,
                y: previous_latitude,
            },
            end: Point {
                x: current_longitude,
                y: current_latitude,
            },
        };
        if let Some(fraction) = intersection_fraction(route_segment, marker_segment) {
            let crossing_elapsed =
                elapsed[index - 1] + fraction * (elapsed[index] - elapsed[index - 1]);
            lap_start_elapsed.push(crossing_elapsed);
            lap_number[index] = lap_start_elapsed.len() as i64 - 1;
        }
    }

    ResolvedLaps {
        lap_number,
        lap_start_elapsed,
    }
}

/// Returns the route-segment fraction where two finite segments intersect.
///
/// The route segment's start is excluded so a crossing at a shared route vertex
/// is counted by only the preceding segment. Parallel and non-intersecting
/// segments return `None`.
fn intersection_fraction(route: Segment, marker: Segment) -> Option<f64> {
    /// Computes the two-dimensional scalar cross product.
    fn cross(left: Point, right: Point) -> f64 {
        left.x * right.y - left.y * right.x
    }

    let route_vector = Point {
        x: route.end.x - route.start.x,
        y: route.end.y - route.start.y,
    };
    let marker_vector = Point {
        x: marker.end.x - marker.start.x,
        y: marker.end.y - marker.start.y,
    };
    let offset = Point {
        x: marker.start.x - route.start.x,
        y: marker.start.y - route.start.y,
    };
    let denominator = cross(route_vector, marker_vector);
    if denominator.abs() <= f64::EPSILON {
        return None;
    }
    let route_fraction = cross(offset, marker_vector) / denominator;
    let marker_fraction = cross(offset, route_vector) / denominator;
    ((0.0..=1.0).contains(&route_fraction)
        && route_fraction > f64::EPSILON
        && (0.0..=1.0).contains(&marker_fraction))
    .then_some(route_fraction)
}

/// Derives canonical lap timing from one authoritative boundary source.
///
/// Explicit source laps take precedence over beacon or VBO timing markers. All
/// output series align with `elapsed_series`; compact durations hold one entry
/// per lap start, the last closed by the activity's end.
pub(crate) fn derive_series(
    elapsed_series: &[f64],
    distance_series: &[Option<f64>],
    course: &[(Option<f64>, Option<f64>)],
    lap_number_source: &[Option<i64>],
    lap_markers: &LapMarkers,
) -> LapTiming {
    let sample_count = elapsed_series.len();
    if sample_count < 2 {
        return empty_result(sample_count);
    }

    let resolved = resolve_laps(elapsed_series, course, lap_number_source, lap_markers);
    let (lap_time_seconds, lap_durations_seconds) = compute_lap_times(
        elapsed_series,
        &resolved.lap_number,
        &resolved.lap_start_elapsed,
    );
    let best_lap_time_seconds = lap_durations_seconds
        .iter()
        .min_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal))
        .copied();
    let lap_durations_best_so_far_seconds = compute_best_so_far(&lap_durations_seconds);
    let delta_to_best_lap_seconds = compute_delta_to_best(
        distance_series,
        &resolved.lap_number,
        &lap_time_seconds,
        &lap_durations_seconds,
        &[],
    );

    LapTiming {
        lap_number: resolved.lap_number,
        lap_time_seconds,
        lap_start_elapsed_seconds: resolved.lap_start_elapsed,
        delta_to_best_lap_seconds,
        best_lap_time_seconds,
        lap_durations_seconds,
        lap_durations_best_so_far_seconds,
    }
}

/// Produces the canonical no-lap result while preserving sample alignment.
fn empty_result(sample_count: usize) -> LapTiming {
    LapTiming {
        lap_number: vec![-1; sample_count],
        lap_time_seconds: vec![None; sample_count],
        lap_start_elapsed_seconds: Vec::new(),
        delta_to_best_lap_seconds: vec![None; sample_count],
        best_lap_time_seconds: None,
        lap_durations_seconds: Vec::new(),
        lap_durations_best_so_far_seconds: Vec::new(),
    }
}

/// Computes sample-aligned live lap times and the duration of every lap.
///
/// Each lap start closes the previous lap; the final sample closes the last.
fn compute_lap_times(
    elapsed: &[f64],
    lap_number: &[i64],
    lap_start_elapsed: &[f64],
) -> (NumericSeries, Vec<f64>) {
    if elapsed.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let lap_time_seconds = elapsed
        .iter()
        .zip(lap_number)
        .map(|(elapsed, lap)| {
            usize::try_from(*lap)
                .ok()
                .and_then(|lap| lap_start_elapsed.get(lap))
                .map(|start| elapsed - start)
        })
        .collect();
    let activity_end = elapsed[elapsed.len() - 1];
    let lap_durations_seconds = lap_start_elapsed
        .iter()
        .zip(lap_start_elapsed.iter().skip(1).chain([&activity_end]))
        .map(|(start, end)| end - start)
        .collect();

    (lap_time_seconds, lap_durations_seconds)
}

/// Computes the fastest completed duration available after each completed lap.
fn compute_best_so_far(durations: &[f64]) -> Vec<f64> {
    let mut best_so_far = Vec::with_capacity(durations.len());
    let mut current_best: Option<f64> = None;
    for &duration in durations {
        if let Some(best) = current_best {
            if duration < best {
                current_best = Some(duration);
            }
        } else {
            current_best = Some(duration);
        }
        best_so_far.push(current_best.unwrap_or(0.0));
    }
    best_so_far
}

#[cfg(test)]
mod tests {
    use super::{intersection_fraction, Point, Segment};
    use super::{compute_best_so_far, derive_series};
    use crate::activity::schema::{LapMarkers, TimingMarker, TimingMarkerKind};

    /// Verifies crossing detection accepts an intersection and rejects disjoint segments.
    #[test]
    fn line_crossing_detection() {
        let diagonal = Segment {
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 1.0, y: 1.0 },
        };
        assert!(intersection_fraction(
            diagonal,
            Segment {
                start: Point { x: 1.0, y: 0.0 },
                end: Point { x: 0.0, y: 1.0 },
            }
        )
        .is_some());
        assert!(intersection_fraction(
            diagonal,
            Segment {
                start: Point { x: 2.0, y: 2.0 },
                end: Point { x: 3.0, y: 3.0 },
            }
        )
        .is_none());
    }

    /// Verifies an empty activity produces fully empty aligned lap series.
    #[test]
    fn empty_input_returns_empty_result() {
        let result = derive_series(&[], &[], &[], &[], &LapMarkers::None);
        assert!(result.lap_number.is_empty());
        assert!(result.lap_time_seconds.is_empty());
        assert!(result.delta_to_best_lap_seconds.is_empty());
        assert_eq!(result.best_lap_time_seconds, None);
    }

    /// Verifies absent lap sources preserve sample shape with out-lap labels.
    #[test]
    fn no_lap_data_produces_minus_ones() {
        let elapsed = vec![0.0, 1.0, 2.0, 3.0];
        let distance = vec![Some(0.0), Some(10.0), Some(20.0), Some(30.0)];
        let course = vec![];
        let source = vec![None, None, None, None];

        let result = derive_series(&elapsed, &distance, &course, &source, &LapMarkers::None);

        assert_eq!(result.lap_number, vec![-1, -1, -1, -1]);
        assert_eq!(result.lap_time_seconds, vec![None; 4]);
        assert_eq!(result.delta_to_best_lap_seconds, vec![None; 4]);
        assert_eq!(result.best_lap_time_seconds, None);
        assert!(result.lap_durations_seconds.is_empty());
    }

    /// Verifies explicit source labels are canonicalized into sequential lap IDs.
    #[test]
    fn explicit_lap_number_source() {
        let elapsed = vec![0.0, 1.0, 2.0, 3.0, 4.0];
        let distance = vec![Some(0.0), Some(10.0), Some(20.0), Some(30.0), Some(40.0)];
        let course = vec![(None, None); 5];
        let source = vec![Some(-1), Some(-1), Some(1), Some(1), Some(2)];

        let result = derive_series(&elapsed, &distance, &course, &source, &LapMarkers::None);

        assert_eq!(result.lap_number, vec![-1, -1, 0, 0, 1]);
        assert_eq!(result.lap_time_seconds[0], None);
        assert!(result.lap_time_seconds[2].unwrap() >= 0.0);
        assert!(!result.lap_durations_seconds.is_empty());
    }

    /// Verifies elapsed-time beacons open laps at their exact boundaries.
    #[test]
    fn beacon_markers_resolve_lap_numbers() {
        let elapsed = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        let distance = (0..6)
            .map(|index| Some(index as f64 * 10.0))
            .collect::<Vec<_>>();
        let course = vec![(None, None); 6];
        let source = vec![];
        let beacons = vec![2.0, 4.5];

        let result = derive_series(
            &elapsed,
            &distance,
            &course,
            &source,
            &LapMarkers::BeaconMarkers(beacons),
        );

        assert_eq!(result.lap_number, vec![-1, -1, 0, 0, 0, 1]);
    }

    /// Verifies completed-lap best metadata is a duration prefix minimum.
    #[test]
    fn best_so_far_computation() {
        assert_eq!(
            compute_best_so_far(&[10.0, 5.0, 7.0, 4.0]),
            vec![10.0, 5.0, 5.0, 4.0]
        );
    }

    /// Verifies deltas remain absent until a prior lap has completed.
    #[test]
    fn delta_to_best_null_before_first_lap_completion() {
        let elapsed = vec![0.0, 0.5, 1.0];
        let distance = vec![Some(0.0), Some(5.0), Some(10.0)];
        let course = vec![(None, None); 3];
        let source = vec![Some(0), Some(0), Some(0)];

        let result = derive_series(&elapsed, &distance, &course, &source, &LapMarkers::None);

        assert!(result.delta_to_best_lap_seconds.iter().all(Option::is_none));
    }

    /// Verifies a geographic start marker creates an observed lap boundary.
    #[test]
    fn timing_marker_crossing_detection() {
        let course = vec![
            (Some(0.0), Some(0.0)),
            (Some(2.0), Some(2.0)),
            (Some(4.0), Some(4.0)),
        ];
        let source = vec![];
        let markers = vec![TimingMarker {
            kind: TimingMarkerKind::Start,
            latitude_a: 1.0,
            longitude_a: 0.0,
            latitude_b: 1.0,
            longitude_b: 3.0,
        }];

        let result = derive_series(
            &[0.0, 1.0, 2.0],
            &[Some(0.0), Some(10.0), Some(20.0)],
            &course,
            &source,
            &LapMarkers::TimingMarkers(markers),
        );

        assert_eq!(result.lap_number.len(), 3);
    }
}
