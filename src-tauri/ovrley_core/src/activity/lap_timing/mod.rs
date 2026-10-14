//! Native canonical lap-timing derivation and activity ingress validation.
//!
//! Finalization calls
//! [`derive_lap_timing`] to construct canonical sample-aligned series, activity
//! ingestion calls [`validate_lap_timing_contract`] at the parsed-activity
//! boundary. Trimming and densification call [`lap_time_at`] to construct their
//! aligned timer series using the shared widget sampler's boundary arithmetic.
//!
//! Boundary-source resolution and distance-based delta calculation remain
//! private implementation details so callers consume one canonical contract.

mod boundaries;
mod delta;
mod derive;

use crate::activity::schema::{NumericSeries, ParsedActivity};
use crate::error::{CoreError, CoreResult};
use ovrley_widgets::widgets::lap_timer::sampling::lap_time_at;

pub(crate) use derive::derive_lap_timing;

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
