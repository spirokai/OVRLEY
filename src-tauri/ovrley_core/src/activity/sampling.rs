//! Native activity column access, source bounds, and shared widget sampling.

use super::schema::{DenseActivityReport, DenseSeriesReport, ParsedActivity, TrimmedActivity};
use ovrley_widgets::model::telemetry::{numeric_metrics, ActivitySeries, NumericActivity};
use ovrley_widgets::{
    model::types::MetricKind,
    widgets::{
        diving_profile::sampling::DivingActivity,
        elevation::sampling::{
            elevation_profile_series, preferred_elevation_series, ElevationActivity,
        },
        g_force::validation::GForceAxis,
        lap_timer::format::{LapHistory, LapSample},
        route::sampling::RouteActivity,
        value::format::MetricSample,
    },
};

use std::collections::HashMap;

macro_rules! implement_numeric_activity {
    ($kind:ident numeric { $($field:ident: $variant:ident),* } other { $($other:ident: $unused:ident),* }) => {
        impl NumericActivity for $kind {
            fn numeric_series(&self, series: ActivitySeries) -> &[Option<f64>] {
                match series {
                    $(ActivitySeries::$variant => &self.$field,)*
                    _ => panic!("manifest numeric sources must be numeric activity columns"),
                }
            }
        }

    };
}

ovrley_widgets::activity_series!(implement_numeric_activity, ParsedActivity);
ovrley_widgets::activity_series!(implement_numeric_activity, TrimmedActivity);
ovrley_widgets::activity_series!(implement_numeric_activity, DenseSeriesReport);

/// Returns observed finite bounds; missing external telemetry has no range.
pub fn calculate_metric_range(values: &[Option<f64>]) -> Option<(f64, f64)> {
    values
        .iter()
        .copied()
        .flatten()
        .filter(|value| value.is_finite())
        .fold(None, |range, value| {
            Some(match range {
                None => (value, value),
                Some((min, max)) => (min.min(value), max.max(value)),
            })
        })
}

/// Calculates each numeric metric's bounds in canonical units from the full activity.
pub fn calculate_metric_ranges(activity: &ParsedActivity) -> HashMap<MetricKind, (f64, f64)> {
    let mut ranges = HashMap::new();
    for &metric in numeric_metrics() {
        let values = activity
            .numeric_series_for(metric)
            .expect("numeric metrics declare numeric sources");
        if let Some(range) = calculate_metric_range(values) {
            ranges.insert(metric, range);
        }
    }
    ranges
}

impl ParsedActivity {
    pub fn diving_activity(&self) -> DivingActivity {
        DivingActivity::new(
            &self.sample_elapsed_seconds,
            &self.depth,
            &self.first_stop_depth,
            self.trim_end_seconds,
        )
    }

    pub fn route_activity(&self) -> RouteActivity {
        RouteActivity::new(
            &self.sample_elapsed_seconds,
            &self.sample_course_points,
            &self.sample_distance_progress,
            self.trim_end_seconds,
        )
    }

    pub fn elevation_activity(&self) -> ElevationActivity {
        ElevationActivity::new(
            &self.sample_elapsed_seconds,
            elevation_profile_series(&self.sample_elevations, &self.elevation),
            preferred_elevation_series(&self.barometric_altitude, &self.elevation),
            &self.sample_distance_progress,
            self.trim_end_seconds,
        )
    }

    pub fn lap_history(&self) -> LapHistory<'_> {
        LapHistory {
            starts: &self.lap_start_elapsed_seconds,
            durations: &self.lap_durations_seconds,
            best_so_far: &self.lap_durations_best_so_far_seconds,
            end: self.trim_end_seconds,
        }
    }

    pub fn g_force_axis(&self, axis: GForceAxis) -> &[Option<f64>] {
        match axis {
            GForceAxis::X => &self.g_force_x,
            GForceAxis::Y => &self.g_force_y,
            GForceAxis::Z => &self.g_force_z,
        }
    }
}

impl DenseActivityReport {
    pub fn metric_sample(
        &self,
        metric: MetricKind,
        index: usize,
        altitude_offset_m: f64,
    ) -> MetricSample<'_> {
        let series = self.series.numeric_series_for(metric);
        MetricSample {
            numeric: series
                .and_then(|series| series.get(index).copied().flatten())
                .map(|value| value + altitude_offset_m),
            available: series.is_some_and(|series| !series.is_empty()),
            gear: self
                .series
                .gear_position
                .get(index)
                .and_then(Option::as_deref),
            latitude: self.series.course_lat.get(index).copied().flatten(),
            longitude: self.series.course_lon.get(index).copied().flatten(),
            fraction_he: self.series.fraction_he.get(index).copied().flatten(),
            full_activity_distance: self.full_activity_distance,
            full_activity_total_ascent: self.full_activity_total_ascent,
        }
    }

    pub fn g_force_axis(&self, axis: GForceAxis) -> &[Option<f64>] {
        match axis {
            GForceAxis::X => &self.series.g_force_x,
            GForceAxis::Y => &self.series.g_force_y,
            GForceAxis::Z => &self.series.g_force_z,
        }
    }

    pub fn lap_history(&self) -> LapHistory<'_> {
        LapHistory {
            starts: &self.series.lap_start_elapsed_seconds,
            durations: &self.series.lap_durations_seconds,
            best_so_far: &self.series.lap_durations_best_so_far_seconds,
            end: self.frame_elapsed_seconds.last().copied().unwrap_or(0.0),
        }
    }

    pub fn lap_sample(&self, index: usize) -> LapSample {
        LapSample::from_aligned(
            &self.series.lap_start_elapsed_seconds,
            self.series
                .lap_number
                .get(index)
                .copied()
                .flatten()
                .unwrap_or(-1),
            self.series.lap_time_seconds.get(index).copied().flatten(),
            self.frame_elapsed_seconds[index],
            self.series
                .delta_to_best_lap_seconds
                .get(index)
                .copied()
                .flatten(),
        )
    }
}
