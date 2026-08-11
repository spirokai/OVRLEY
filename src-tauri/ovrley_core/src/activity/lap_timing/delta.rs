//! Distance-aligned delta calculation against the best completed lap.
//!
//! Each active lap is converted to lap-relative distance/time points. A sample
//! can receive a delta only after at least one earlier lap has completed and the
//! best prior lap contains enough distance coverage for interpolation.

use crate::activity::schema::NumericSeries;
use crate::interpolation::interpolate_points;

/// Computes each sample's live time delta against the best previously completed lap.
///
/// Samples outside a lap, on the first lap, or without usable distance/time
/// values remain `None`. After passing reference distance coverage, delta keeps
/// accumulating against the completed reference lap duration.
pub(super) fn compute_delta_to_best(
    distance_series: &[Option<f64>],
    lap_number: &[i64],
    lap_time_series: &[Option<f64>],
    lap_durations: &[f64],
) -> NumericSeries {
    let sample_count = lap_number.len();
    if sample_count == 0 || lap_durations.is_empty() {
        return vec![None; sample_count];
    }

    let lap_start_distances = compute_lap_start_distances(distance_series, lap_number);
    let reference_points_by_lap = build_reference_lap_points(
        distance_series,
        lap_time_series,
        lap_number,
        &lap_start_distances,
        lap_durations.len(),
    );
    let reference_lap_indices = best_lap_indices(lap_durations);

    let mut delta = vec![None; sample_count];

    for index in 0..sample_count {
        let lap = lap_number[index];
        if lap < 0 {
            continue;
        }
        let lap_index = lap as usize;
        if lap_index == 0 || lap_index > lap_durations.len() {
            continue;
        }
        let reference_lap_index = reference_lap_indices[lap_index - 1];
        if reference_lap_index >= lap_start_distances.len()
            || lap_index >= lap_start_distances.len()
        {
            continue;
        }

        let current_lap_time = match lap_time_series[index] {
            Some(time) => time,
            None => continue,
        };
        let current_distance = match distance_series[index] {
            Some(distance) if distance.is_finite() => distance,
            _ => continue,
        };
        let current_lap_distance = current_distance - lap_start_distances[lap_index];
        if current_lap_distance < 0.0 {
            continue;
        }

        let reference_points = &reference_points_by_lap[reference_lap_index];
        if reference_points.len() < 2 {
            continue;
        }
        let reference_lap_max_distance = reference_points[reference_points.len() - 1].0;
        if reference_lap_max_distance <= 0.0 {
            continue;
        }

        let reference_lap_time = if current_lap_distance > reference_lap_max_distance {
            lap_durations[reference_lap_index]
        } else if let Some(reference_lap_time) =
            interpolate_points(reference_points, current_lap_distance)
        {
            reference_lap_time
        } else {
            continue;
        };
        delta[index] = Some(current_lap_time - reference_lap_time);
    }

    delta
}

/// Captures cumulative activity distance at the first sample of each canonical lap.
///
/// Missing distance at a lap's opening sample retains the zero placeholder used
/// by the existing delta contract.
fn compute_lap_start_distances(distance_series: &[Option<f64>], lap_number: &[i64]) -> Vec<f64> {
    let max_lap = lap_number.iter().copied().max().unwrap_or(-1);
    if max_lap < 0 {
        return Vec::new();
    }
    let count = (max_lap + 1) as usize;
    let mut start_distances = vec![0.0; count];
    let mut current_lap = -1;
    for (index, &lap) in lap_number.iter().enumerate() {
        if lap != current_lap && lap >= 0 {
            let lap_index = lap as usize;
            if lap_index < count {
                if let Some(Some(distance)) = distance_series.get(index) {
                    start_distances[lap_index] = *distance;
                }
            }
            current_lap = lap;
        }
    }
    start_distances
}

/// Selects the fastest completed lap available at each completion point.
///
/// Entry `n` identifies the best lap among completed laps `0..=n`; equal
/// durations preserve the earliest matching lap.
fn best_lap_indices(durations: &[f64]) -> Vec<usize> {
    let mut best_index = 0;
    durations
        .iter()
        .enumerate()
        .map(|(index, duration)| {
            if *duration < durations[best_index] {
                best_index = index;
            }
            best_index
        })
        .collect()
}

/// Builds lap-relative distance/time interpolation points for completed laps.
///
/// The incomplete final lap is excluded because it cannot be selected as a
/// reference until a subsequent boundary closes it.
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
        let Ok(lap_index) = usize::try_from(*lap) else {
            continue;
        };
        if lap_index >= completed_lap_count {
            continue;
        }
        if let (Some(distance), Some(lap_time)) = (distance, lap_time) {
            points_by_lap[lap_index].push((distance - lap_start_distances[lap_index], *lap_time));
        }
    }
    points_by_lap
}

#[cfg(test)]
mod tests {
    use super::{best_lap_indices, compute_delta_to_best};

    /// Verifies reference selection tracks the earliest fastest completed lap.
    #[test]
    fn best_lap_selection_uses_prefix_minimum_indices() {
        assert_eq!(best_lap_indices(&[10.0, 5.0, 7.0, 4.0]), vec![0, 1, 1, 3]);
        assert_eq!(best_lap_indices(&[5.0, 5.0]), vec![0, 0]);
    }

    /// Verifies an overdue lap keeps accumulating delta after passing reference distance coverage.
    #[test]
    fn delta_continues_after_reference_lap_distance() {
        let delta = compute_delta_to_best(
            &[
                Some(0.0),
                Some(50.0),
                Some(100.0),
                Some(120.0),
                Some(230.0),
                Some(250.0),
            ],
            &[0, 0, 0, 1, 1, 1],
            &[
                Some(0.0),
                Some(5.0),
                Some(9.0),
                Some(0.0),
                Some(12.0),
                Some(20.0),
            ],
            &[10.0],
        );

        assert_eq!(delta[4], Some(2.0));
        assert_eq!(delta[5], Some(10.0));
    }
}
