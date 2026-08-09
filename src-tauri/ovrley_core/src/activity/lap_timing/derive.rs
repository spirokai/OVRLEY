//! Assembly of canonical lap-timing series during activity finalization.
//!
//! This module coordinates boundary resolution, sample-aligned live lap times,
//! completed-lap duration metadata, and distance-based deltas. It deliberately
//! leaves the final observed lap incomplete until a subsequent boundary closes
//! it, matching the canonical parsed-activity contract.

use super::boundaries::resolve_laps;
use super::delta::compute_delta_to_best;
use super::LapTimingResult;
use crate::activity::schema::{LapMarkers, NumericSeries};

/// Derives canonical lap timing from one authoritative boundary source.
///
/// Explicit source laps take precedence over beacon or VBO timing markers. All
/// output series align with `elapsed_series`; compact durations include only
/// laps closed by a subsequent boundary.
pub(crate) fn derive_lap_timing(
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
        .min_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal))
        .copied();
    let lap_durations_best_so_far_seconds = compute_best_so_far(&lap_durations_seconds);
    let delta_to_best_lap_seconds = compute_delta_to_best(
        distance_series,
        &resolved.lap_number,
        &lap_time_seconds,
        &lap_durations_seconds,
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

/// Produces the canonical no-lap result while preserving sample alignment.
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

/// Computes sample-aligned live lap times and durations of completed laps.
///
/// Consecutive lap starts close completed laps. End-of-file never synthesizes a
/// closing boundary for the final observed lap.
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
    let lap_durations_seconds = lap_start_elapsed
        .windows(2)
        .map(|boundaries| boundaries[1] - boundaries[0])
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
    use super::{compute_best_so_far, derive_lap_timing};
    use crate::activity::schema::{LapMarkers, TimingMarker, TimingMarkerKind};

    /// Verifies an empty activity produces fully empty aligned lap series.
    #[test]
    fn empty_input_returns_empty_result() {
        let result = derive_lap_timing(&[], &[], &[], &[], &LapMarkers::None);
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

        let result = derive_lap_timing(&elapsed, &distance, &course, &source, &LapMarkers::None);

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

        let result = derive_lap_timing(&elapsed, &distance, &course, &source, &LapMarkers::None);

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

        let result = derive_lap_timing(
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

        let result = derive_lap_timing(&elapsed, &distance, &course, &source, &LapMarkers::None);

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
