//! Original-density lap numbers use hold; timer resets use exact boundaries; deltas preserve gaps.
use super::{
    delta::compute_delta_to_best,
    format::{LapHistory, LapSample},
};
use crate::{
    error::{WidgetError, WidgetResult},
    interpolation::{interpolate_numeric_series_value, MissingSamplePolicy},
};
use serde::Deserialize;

/// The canonical held lap selects its exact opening boundary; inactive laps have no timer.
pub fn lap_time_at(starts: &[f64], lap_number: i64, elapsed: f64) -> Option<f64> {
    usize::try_from(lap_number)
        .ok()
        .and_then(|index| starts.get(index))
        .map(|start| elapsed - start)
}

#[derive(Default, Deserialize)]
pub(crate) struct LapTiming {
    #[serde(default)]
    lap_number: Vec<i64>,
    #[serde(default)]
    lap_time_seconds: Vec<Option<f64>>,
    #[serde(default)]
    lap_start_elapsed_seconds: Vec<f64>,
    #[serde(default)]
    delta_to_best_lap_seconds: Vec<Option<f64>>,
    #[serde(default)]
    lap_durations_seconds: Vec<f64>,
    /// User-disabled canonical lap IDs, strictly increasing; set by [`LapTiming::select`].
    #[serde(skip)]
    disabled_laps: Vec<usize>,
    /// Delta against the best enabled completed lap; the finalized series when none is disabled.
    #[serde(skip)]
    selected_delta_seconds: Vec<Option<f64>>,
}

impl LapTiming {
    pub fn validate(&self, times: &[f64]) -> WidgetResult<()> {
        if [
            self.lap_number.len(),
            self.lap_time_seconds.len(),
            self.delta_to_best_lap_seconds.len(),
        ]
        .into_iter()
        .any(|len| len != 0 && len != times.len())
        {
            return Err(WidgetError::Config(
                "lap timing columns must align with timestamps".into(),
            ));
        }
        if self
            .lap_start_elapsed_seconds
            .iter()
            .any(|start| !start.is_finite())
            || self
                .lap_start_elapsed_seconds
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || self.lap_number.iter().any(|number| {
                *number < -1
                    || (*number >= 0 && *number as usize >= self.lap_start_elapsed_seconds.len())
            })
        {
            return Err(WidgetError::Config(
                "lap timing requires ordered boundaries and canonical lap numbers".into(),
            ));
        }
        if self.lap_durations_seconds.len() != self.lap_start_elapsed_seconds.len() {
            return Err(WidgetError::Config(
                "lap duration metadata must contain one entry per lap".into(),
            ));
        }
        if self
            .lap_durations_seconds
            .iter()
            .any(|time| !time.is_finite() || *time < 0.0)
        {
            return Err(WidgetError::Config(
                "lap duration metadata must be finite and nonnegative".into(),
            ));
        }
        Ok(())
    }

    /// Applies the user's disabled laps: they leave the history and stop serving as delta references.
    ///
    /// `disabled` must hold strictly increasing IDs of laps this activity observed.
    /// `distance` is the finalized distance column, empty when the activity has none.
    pub fn select(&mut self, disabled: Vec<usize>, distance: &[Option<f64>]) -> WidgetResult<()> {
        if disabled.windows(2).any(|pair| pair[0] >= pair[1])
            || disabled
                .last()
                .is_some_and(|lap| *lap >= self.lap_start_elapsed_seconds.len())
        {
            return Err(WidgetError::Config(
                "disabled laps must be strictly increasing IDs of observed laps".into(),
            ));
        }
        self.selected_delta_seconds = if disabled.is_empty() {
            self.delta_to_best_lap_seconds.clone()
        } else {
            if self.lap_time_seconds.len() != self.lap_number.len() {
                return Err(WidgetError::Config(
                    "lap selection requires aligned lap numbers and lap times".into(),
                ));
            }
            compute_delta_to_best(
                distance,
                &self.lap_number,
                &self.lap_time_seconds,
                &self.lap_durations_seconds,
                &disabled,
            )
        };
        self.disabled_laps = disabled;
        Ok(())
    }

    pub fn history(&self, end: f64) -> LapHistory<'_> {
        LapHistory {
            starts: &self.lap_start_elapsed_seconds,
            durations: &self.lap_durations_seconds,
            disabled: &self.disabled_laps,
            end,
        }
    }

    pub fn at(&self, times: &[f64], second: f64) -> LapSample {
        // Empty external telemetry and times before its first held sample are inactive laps.
        let number = times
            .partition_point(|time| *time <= second + 1e-9)
            .checked_sub(1)
            .and_then(|index| self.lap_number.get(index))
            .copied()
            .unwrap_or(-1);
        LapSample::from_aligned(
            &self.lap_start_elapsed_seconds,
            number,
            lap_time_at(&self.lap_start_elapsed_seconds, number, second),
            second,
            interpolate_numeric_series_value(
                times,
                &self.selected_delta_seconds,
                second,
                MissingSamplePolicy::Preserve,
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timers_reset_at_exact_boundaries_and_inactive_laps_retain_completed_history() {
        let starts = [0.8, 2.9];
        let times: Vec<_> = [0.0, 0.8, 1.0, 2.8, 2.9, 3.0]
            .into_iter()
            .zip([-1, 0, 0, 0, 1, 1])
            .map(|(second, number)| lap_time_at(&starts, number, second))
            .collect();
        assert_eq!(times[0], None);
        assert_eq!(times[1], Some(0.0));
        assert!((times[3].unwrap() - 2.0).abs() < 1e-9);
        assert_eq!(times[4], Some(0.0));
        assert!((times[5].unwrap() - 0.1).abs() < 1e-9);
        let inactive = LapSample::from_aligned(&[0.8, 2.9, 5.0], -1, None, 6.0, None);
        assert_eq!(inactive.lap_number, -1);
        assert_eq!(inactive.lap_time_seconds, None);
        assert_eq!(inactive.completed_lap_count, 2);
    }
}
