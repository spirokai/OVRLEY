//! Canonical lap-state derivation for finalized activity samples.
//!
//! Owns: resolving explicit, beacon, and VBO crossing boundaries; aligned lap
//! numbers and live times; completed-lap metadata; and distance-based deltas.
//! The final observed lap is intentionally incomplete until another boundary
//! closes it.

use crate::activity::schema::{LapMarkers, NumericSeries, TimingMarker, TimingMarkerKind};
use crate::interpolation::interpolate_points;

/// Sample-aligned lap IDs plus the exact elapsed time of each lap boundary.
struct ResolvedLaps {
    lap_number: Vec<i64>,
    lap_start_elapsed: Vec<f64>,
}

#[derive(Clone, Copy)]
/// Planar point used only for local line-segment intersection calculations.
struct Point {
    x: f64,
    y: f64,
}

#[derive(Clone, Copy)]
/// Finite line segment; VBO timing lines and consecutive route samples use the same shape.
struct Segment {
    start: Point,
    end: Point,
}

/// Lap series and compact completed-lap metadata emitted by activity finalization.
pub struct LapTimingResult {
    /// Canonical sample-aligned lap IDs; `-1` denotes the out-lap.
    pub lap_number: Vec<i64>,
    /// Elapsed time since the exact opening boundary of each active lap.
    pub lap_time_seconds: NumericSeries,
    /// Exact activity-relative start time of each canonical lap.
    pub lap_start_elapsed_seconds: Vec<f64>,
    /// Live difference from the best previously completed lap at the same distance.
    pub delta_to_best_lap_seconds: NumericSeries,
    /// Fastest duration among laps closed by a subsequent boundary.
    pub best_lap_time_seconds: Option<f64>,
    /// Completed durations indexed by canonical lap ID.
    pub lap_durations_seconds: Vec<f64>,
    /// Prefix minimum of `lap_durations_seconds`.
    pub lap_durations_best_so_far_seconds: Vec<f64>,
}

/// Derives canonical lap timing from one authoritative boundary source.
///
/// Explicit source laps take precedence over beacon or VBO timing markers. All
/// output series align with `elapsed_series`; compact durations include only
/// laps closed by a subsequent boundary.
pub fn derive_lap_timing(
    elapsed_series: &[f64],
    distance_series: &[Option<f64>],
    course: &[(Option<f64>, Option<f64>)],
    lap_number_source: &[Option<i64>],
    lap_markers: &LapMarkers,
) -> LapTimingResult {
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
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .copied();
    let lap_durations_best_so_far_seconds = compute_best_so_far(&lap_durations_seconds);
    let delta_to_best_lap_seconds = compute_delta_to_best(
        distance_series,
        &resolved.lap_number,
        &lap_time_seconds,
        &lap_durations_seconds,
        &lap_durations_best_so_far_seconds,
    );

    LapTimingResult {
        lap_number: resolved.lap_number,
        lap_time_seconds,
        lap_start_elapsed_seconds: resolved.lap_start_elapsed,
        delta_to_best_lap_seconds,
        best_lap_time_seconds,
        lap_durations_seconds,
        lap_durations_best_so_far_seconds,
    }
}

fn empty_result(sample_count: usize) -> LapTimingResult {
    LapTimingResult {
        lap_number: vec![-1; sample_count],
        lap_time_seconds: vec![None; sample_count],
        lap_start_elapsed_seconds: Vec::new(),
        delta_to_best_lap_seconds: vec![None; sample_count],
        best_lap_time_seconds: None,
        lap_durations_seconds: Vec::new(),
        lap_durations_best_so_far_seconds: Vec::new(),
    }
}

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

fn resolve_from_beacons(elapsed: &[f64], beacons: &[f64]) -> ResolvedLaps {
    if beacons.is_empty() {
        return ResolvedLaps {
            lap_number: vec![-1; elapsed.len()],
            lap_start_elapsed: Vec::new(),
        };
    }
    let lap_number: Vec<i64> = elapsed
        .iter()
        .map(|e| {
            let mut lap: i64 = -1;
            for (i, beacon) in beacons.iter().enumerate() {
                if *e >= *beacon {
                    lap = i as i64;
                } else {
                    break;
                }
            }
            lap
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

fn resolve_from_timing_markers(
    elapsed: &[f64],
    course: &[(Option<f64>, Option<f64>)],
    markers: &[TimingMarker],
) -> ResolvedLaps {
    let start_markers: Vec<&TimingMarker> = markers
        .iter()
        .filter(|m| m.kind == TimingMarkerKind::Start)
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

    for i in 1..course.len() {
        // Missing route coordinates retain the current lap but cannot create a crossing.
        lap_number[i] = lap_start_elapsed.len() as i64 - 1;
        let (prev_lat, prev_lon) = course[i - 1];
        let (curr_lat, curr_lon) = course[i];

        let Some(prev_lat) = prev_lat else { continue };
        let Some(prev_lon) = prev_lon else { continue };
        let Some(curr_lat) = curr_lat else { continue };
        let Some(curr_lon) = curr_lon else { continue };

        let route_segment = Segment {
            start: Point {
                x: prev_lon,
                y: prev_lat,
            },
            end: Point {
                x: curr_lon,
                y: curr_lat,
            },
        };
        if let Some(fraction) = intersection_fraction(route_segment, marker_segment) {
            // Preserve sub-sample crossing precision instead of quantizing the
            // lap start to the first route sample after the timing line.
            let crossing_elapsed = elapsed[i - 1] + fraction * (elapsed[i] - elapsed[i - 1]);
            lap_start_elapsed.push(crossing_elapsed);
            lap_number[i] = lap_start_elapsed.len() as i64 - 1;
        }
    }

    ResolvedLaps {
        lap_number,
        lap_start_elapsed,
    }
}

fn intersection_fraction(route: Segment, marker: Segment) -> Option<f64> {
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
    // Excluding the route segment's start gives a shared route vertex to the
    // preceding segment, preventing one physical crossing from being counted twice.
    ((0.0..=1.0).contains(&route_fraction)
        && route_fraction > f64::EPSILON
        && (0.0..=1.0).contains(&marker_fraction))
    .then_some(route_fraction)
}

fn compute_lap_times(
    elapsed: &[f64],
    lap_number: &[i64],
    lap_start_elapsed: &[f64],
) -> (NumericSeries, Vec<f64>) {
    let n = elapsed.len();
    if n == 0 {
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
    // Consecutive starts close completed laps; there is deliberately no EOF duration.
    let lap_durations_seconds = lap_start_elapsed
        .windows(2)
        .map(|boundaries| boundaries[1] - boundaries[0])
        .collect();

    (lap_time_seconds, lap_durations_seconds)
}

fn compute_best_so_far(durations: &[f64]) -> Vec<f64> {
    let mut best_so_far = Vec::with_capacity(durations.len());
    let mut current_best: Option<f64> = None;
    for &d in durations {
        if let Some(best) = current_best {
            if d < best {
                current_best = Some(d);
            }
        } else {
            current_best = Some(d);
        }
        best_so_far.push(current_best.unwrap_or(0.0));
    }
    best_so_far
}

fn compute_delta_to_best(
    distance_series: &[Option<f64>],
    lap_number: &[i64],
    lap_time_series: &[Option<f64>],
    lap_durations: &[f64],
    best_so_far: &[f64],
) -> NumericSeries {
    let n = lap_number.len();
    if n == 0 || lap_durations.is_empty() || best_so_far.is_empty() {
        return vec![None; n];
    }

    let lap_start_distances = compute_lap_start_distances(distance_series, lap_number);
    let reference_points_by_lap = build_reference_lap_points(
        distance_series,
        lap_time_series,
        lap_number,
        &lap_start_distances,
        lap_durations.len(),
    );

    let mut delta = vec![None; n];

    for i in 0..n {
        let lap = lap_number[i];
        if lap < 0 {
            continue;
        }
        let lap_idx = lap as usize;
        if lap_idx == 0 || lap_idx > best_so_far.len() {
            continue;
        }
        let reference_lap_idx = best_lap_index_for_lap(lap_idx, lap_durations, best_so_far);
        if reference_lap_idx >= lap_start_distances.len() || lap_idx >= lap_start_distances.len() {
            continue;
        }

        let current_lap_time = match lap_time_series[i] {
            Some(t) => t,
            None => continue,
        };

        let current_distance = match distance_series[i] {
            Some(d) if d.is_finite() => d,
            _ => continue,
        };

        let cur_lap_start = lap_start_distances[lap_idx];

        let current_lap_distance = current_distance - cur_lap_start;
        if current_lap_distance < 0.0 {
            continue;
        }

        let reference_points = &reference_points_by_lap[reference_lap_idx];
        if reference_points.len() < 2 {
            continue;
        }

        let ref_lap_max_distance = reference_points[reference_points.len() - 1].0;
        if ref_lap_max_distance <= 0.0 || current_lap_distance > ref_lap_max_distance {
            continue;
        }

        let reference_lap_time = interpolate_points(reference_points, current_lap_distance);

        if let Some(ref_time) = reference_lap_time {
            delta[i] = Some(current_lap_time - ref_time);
        }
    }

    delta
}

fn compute_lap_start_distances(distance_series: &[Option<f64>], lap_number: &[i64]) -> Vec<f64> {
    let max_lap = lap_number.iter().copied().max().unwrap_or(-1);
    if max_lap < 0 {
        return Vec::new();
    }
    let count = (max_lap + 1) as usize;
    let mut start_distances = vec![0.0; count];
    let mut current_lap: i64 = -1;
    for (i, &lap) in lap_number.iter().enumerate() {
        if lap != current_lap && lap >= 0 {
            let lap_idx = lap as usize;
            if lap_idx < count {
                if let Some(Some(d)) = distance_series.get(i) {
                    start_distances[lap_idx] = *d;
                }
            }
            current_lap = lap;
        }
    }
    start_distances
}

fn best_lap_index_for_lap(lap_idx: usize, durations: &[f64], best_so_far: &[f64]) -> usize {
    let target_best = best_so_far[lap_idx - 1];
    let mut candidate = lap_idx - 1;
    for i in (0..lap_idx).rev() {
        if (durations[i] - target_best).abs() < 1e-9 {
            candidate = i;
        }
    }
    candidate
}

fn build_reference_lap_points(
    distance_series: &[Option<f64>],
    lap_time_series: &[Option<f64>],
    lap_number: &[i64],
    lap_start_distances: &[f64],
    completed_lap_count: usize,
) -> Vec<Vec<(f64, f64)>> {
    assert!(completed_lap_count <= lap_start_distances.len());
    let mut points_by_lap = vec![Vec::new(); completed_lap_count];
    for ((distance, lap_time), lap) in distance_series.iter().zip(lap_time_series).zip(lap_number) {
        let Ok(lap_idx) = usize::try_from(*lap) else {
            continue;
        };
        if lap_idx >= completed_lap_count {
            continue;
        }
        if let (Some(distance), Some(lap_time)) = (distance, lap_time) {
            points_by_lap[lap_idx].push((distance - lap_start_distances[lap_idx], *lap_time));
        }
    }
    points_by_lap
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_returns_empty_result() {
        let result = derive_lap_timing(&[], &[], &[], &[], &LapMarkers::None);
        assert!(result.lap_number.is_empty());
        assert!(result.lap_time_seconds.is_empty());
        assert!(result.delta_to_best_lap_seconds.is_empty());
        assert_eq!(result.best_lap_time_seconds, None);
    }

    #[test]
    fn no_lap_data_produces_minus_ones() {
        let elapsed = vec![0.0, 1.0, 2.0, 3.0];
        let distance: Vec<Option<f64>> = vec![Some(0.0), Some(10.0), Some(20.0), Some(30.0)];
        let course = vec![];
        let source: Vec<Option<i64>> = vec![None, None, None, None];

        let result = derive_lap_timing(&elapsed, &distance, &course, &source, &LapMarkers::None);

        assert_eq!(result.lap_number, vec![-1, -1, -1, -1]);
        assert_eq!(result.lap_time_seconds, vec![None; 4]);
        assert_eq!(result.delta_to_best_lap_seconds, vec![None; 4]);
        assert_eq!(result.best_lap_time_seconds, None);
        assert!(result.lap_durations_seconds.is_empty());
    }

    #[test]
    fn explicit_lap_number_source() {
        let elapsed = vec![0.0, 1.0, 2.0, 3.0, 4.0];
        let distance: Vec<Option<f64>> =
            vec![Some(0.0), Some(10.0), Some(20.0), Some(30.0), Some(40.0)];
        let course = vec![(None, None); 5];
        let source = vec![Some(-1), Some(-1), Some(1), Some(1), Some(2)];

        let result = derive_lap_timing(&elapsed, &distance, &course, &source, &LapMarkers::None);

        assert_eq!(result.lap_number, vec![-1, -1, 0, 0, 1]);
        assert_eq!(result.lap_time_seconds[0], None);
        assert!(result.lap_time_seconds[2].unwrap() >= 0.0);
        assert!(result.lap_durations_seconds.len() >= 1);
    }

    #[test]
    fn beacon_markers_resolve_lap_numbers() {
        let elapsed = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        let distance: Vec<Option<f64>> = (0..6).map(|i| Some(i as f64 * 10.0)).collect();
        let course = vec![(None, None); 6];
        let source: Vec<Option<i64>> = vec![];
        let beacons = vec![2.0, 4.5];

        let result = derive_lap_timing(
            &elapsed,
            &distance,
            &course,
            &source,
            &LapMarkers::BeaconMarkers(beacons),
        );

        assert_eq!(result.lap_number, vec![-1, -1, 0, 0, 0, 1]);
    }

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

    #[test]
    fn best_so_far_computation() {
        assert_eq!(
            compute_best_so_far(&[10.0, 5.0, 7.0, 4.0]),
            vec![10.0, 5.0, 5.0, 4.0]
        );
    }

    #[test]
    fn delta_to_best_null_before_first_lap_completion() {
        let elapsed = vec![0.0, 0.5, 1.0];
        let distance: Vec<Option<f64>> = vec![Some(0.0), Some(5.0), Some(10.0)];
        let course = vec![(None, None); 3];
        let source = vec![Some(0), Some(0), Some(0)];

        let result = derive_lap_timing(&elapsed, &distance, &course, &source, &LapMarkers::None);

        assert!(result.delta_to_best_lap_seconds.iter().all(|v| v.is_none()));
    }

    #[test]
    fn timing_marker_crossing_detection() {
        let course = vec![
            (Some(0.0), Some(0.0)),
            (Some(2.0), Some(2.0)),
            (Some(4.0), Some(4.0)),
        ];
        let source: Vec<Option<i64>> = vec![];
        let markers = vec![TimingMarker {
            kind: TimingMarkerKind::Start,
            latitude_a: 1.0,
            longitude_a: 0.0,
            latitude_b: 1.0,
            longitude_b: 3.0,
        }];

        let result = derive_lap_timing(
            &[0.0, 1.0, 2.0],
            &[Some(0.0), Some(10.0), Some(20.0)],
            &course,
            &source,
            &LapMarkers::TimingMarkers(markers),
        );

        assert_eq!(result.lap_number.len(), 3);
    }
}
