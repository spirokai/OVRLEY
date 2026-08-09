//! Canonical lap-timing derivation, validation, and time-based queries.
//!
//! This module is the single owner of the lap-timing domain. Finalization calls
//! [`derive_lap_timing`] to construct canonical sample-aligned series, activity
//! ingestion calls [`validate_lap_timing_contract`] at the parsed-activity
//! boundary. Trimming and densification call [`lap_time_at`] to construct their
//! aligned timer series, while rendering calls [`lap_state_from_aligned`] so it
//! consumes those canonical series instead of deriving them again.
//!
//! Boundary-source resolution and distance-based delta calculation remain
//! private implementation details so callers consume one canonical contract.

mod boundaries;
mod delta;
mod derive;

use crate::activity::schema::{NumericSeries, ParsedActivity};
use crate::error::{CoreError, CoreResult};

pub(crate) use derive::derive_lap_timing;

/// Canonical lap state at one activity-relative elapsed time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LapState {
    /// Canonical zero-based lap number, or `-1` while outside an active lap.
    pub(crate) lap_number: i64,
    /// Elapsed seconds since the active lap boundary, if a lap is active.
    pub(crate) lap_time_seconds: Option<f64>,
    /// Number of laps closed by a subsequent boundary at this elapsed time.
    pub(crate) completed_lap_count: usize,
}

/// Canonical lap series and compact completed-lap metadata produced at finalization.
pub(crate) struct LapTimingResult {
    /// Canonical sample-aligned lap IDs; `-1` denotes the out-lap.
    pub(crate) lap_number: Vec<i64>,
    /// Elapsed time since the exact opening boundary of each active lap.
    pub(crate) lap_time_seconds: NumericSeries,
    /// Exact activity-relative start time of each canonical lap.
    pub(crate) lap_start_elapsed_seconds: Vec<f64>,
    /// Live difference from the best previously completed lap at the same distance.
    pub(crate) delta_to_best_lap_seconds: NumericSeries,
    /// Fastest duration among laps closed by a subsequent boundary.
    pub(crate) best_lap_time_seconds: Option<f64>,
    /// Completed durations indexed by canonical lap ID.
    pub(crate) lap_durations_seconds: Vec<f64>,
    /// Prefix minimum of `lap_durations_seconds`.
    pub(crate) lap_durations_best_so_far_seconds: Vec<f64>,
}

/// Computes live time for one canonical lap at an activity-relative elapsed time.
///
/// The aligned lap number selects its exact opening boundary. Negative labels
/// represent an out-lap and therefore have no live lap time.
pub(crate) fn lap_time_at(
    lap_start_elapsed_seconds: &[f64],
    lap_number: i64,
    elapsed: f64,
) -> Option<f64> {
    usize::try_from(lap_number)
        .ok()
        .and_then(|lap_index| lap_start_elapsed_seconds.get(lap_index))
        .map(|lap_start| elapsed - lap_start)
}

/// Combines canonical aligned frame state with boundary-derived lap history.
///
/// The aligned lap number and time remain authoritative for the active frame.
/// Exact boundaries only determine how many laps have been closed by `elapsed`,
/// including during an out-lap after completed laps.
pub(crate) fn lap_state_from_aligned(
    lap_start_elapsed_seconds: &[f64],
    lap_number: i64,
    lap_time_seconds: Option<f64>,
    elapsed: f64,
) -> LapState {
    let started_lap_count = lap_start_elapsed_seconds.partition_point(|start| *start <= elapsed);
    LapState {
        lap_number,
        lap_time_seconds,
        completed_lap_count: started_lap_count.saturating_sub(1),
    }
}

/// Validates every lap-timing field in a parsed activity as one strict contract.
///
/// Activities without any lap fields are valid optional absence. Once any lap
/// field is present, aligned series, ordered boundaries, completed durations,
/// prefix minima, and derived lap times must all agree.
pub(crate) fn validate_lap_timing_contract(activity: &ParsedActivity) -> CoreResult<()> {
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

#[cfg(test)]
mod tests {
    use super::{lap_state_from_aligned, lap_time_at};

    /// Verifies active lap time resets at each exact canonical boundary.
    #[test]
    fn lap_state_resets_at_exact_boundaries() {
        let starts = [0.8, 2.9];
        let targets = [0.0, 0.8, 1.0, 2.8, 2.9, 3.0];
        let lap_numbers = [-1, 0, 0, 0, 1, 1];
        let lap_times = targets
            .iter()
            .zip(lap_numbers)
            .map(|(target, lap_number)| lap_time_at(&starts, lap_number, *target))
            .collect::<Vec<_>>();

        assert_eq!(lap_times[0], None);
        assert_eq!(lap_times[1], Some(0.0));
        assert!((lap_times[3].unwrap() - 2.0).abs() < 1e-9);
        assert_eq!(lap_times[4], Some(0.0));
        assert!((lap_times[5].unwrap() - 0.1).abs() < 1e-9);
    }

    /// Verifies an inactive state retains the count of previously completed laps.
    #[test]
    fn inactive_state_keeps_completed_lap_history() {
        let state = lap_state_from_aligned(&[0.8, 2.9, 5.0], -1, None, 6.0);

        assert_eq!(state.lap_number, -1);
        assert_eq!(state.lap_time_seconds, None);
        assert_eq!(state.completed_lap_count, 2);
    }
}
