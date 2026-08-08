//! Canonical lap-state contract and queries.

use crate::activity::schema::ParsedActivity;
use crate::error::{CoreError, CoreResult};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LapState {
    pub lap_number: i64,
    pub lap_time_seconds: Option<f64>,
}

pub fn lap_state_at(lap_start_elapsed_seconds: &[f64], elapsed: f64) -> LapState {
    let started_lap_count = lap_start_elapsed_seconds.partition_point(|start| *start <= elapsed);
    if started_lap_count == 0 {
        return LapState {
            lap_number: -1,
            lap_time_seconds: None,
        };
    }

    let lap_index = started_lap_count - 1;
    LapState {
        lap_number: lap_index as i64,
        lap_time_seconds: Some(elapsed - lap_start_elapsed_seconds[lap_index]),
    }
}

pub fn lap_time_at(
    lap_start_elapsed_seconds: &[f64],
    lap_number: i64,
    elapsed: f64,
) -> Option<f64> {
    if lap_number < 0 {
        return None;
    }
    lap_start_elapsed_seconds
        .get(
            lap_start_elapsed_seconds
                .partition_point(|start| *start <= elapsed)
                .checked_sub(1)?,
        )
        .map(|start| elapsed - start)
}

pub fn validate_lap_timing_contract(activity: &ParsedActivity) -> CoreResult<()> {
    let has_lap_data = !activity.lap_number.is_empty()
        || !activity.lap_time_seconds.is_empty()
        || !activity.lap_start_elapsed_seconds.is_empty()
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

    let completed_lap_count = activity.lap_start_elapsed_seconds.len().saturating_sub(1);
    if activity.lap_durations_seconds.len() != completed_lap_count
        || activity.lap_durations_best_so_far_seconds.len() != completed_lap_count
    {
        return Err(CoreError::Activity(
            "lap duration metadata must contain one entry per completed lap".into(),
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

#[cfg(test)]
mod tests {
    use super::lap_state_at;

    #[test]
    fn lap_state_resets_at_exact_boundaries() {
        let starts = [0.8, 2.9];
        let targets = [0.0, 0.8, 1.0, 2.8, 2.9, 3.0];
        let states = targets
            .iter()
            .map(|target| lap_state_at(&starts, *target))
            .collect::<Vec<_>>();

        assert_eq!(states[0].lap_number, -1);
        assert_eq!(states[0].lap_time_seconds, None);
        assert_eq!(states[1].lap_number, 0);
        assert_eq!(states[1].lap_time_seconds, Some(0.0));
        assert!((states[3].lap_time_seconds.unwrap() - 2.0).abs() < 1e-9);
        assert_eq!(states[4].lap_number, 1);
        assert_eq!(states[4].lap_time_seconds, Some(0.0));
        assert!((states[5].lap_time_seconds.unwrap() - 0.1).abs() < 1e-9);
    }
}
