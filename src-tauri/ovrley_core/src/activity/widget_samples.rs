//! Native column access for shared widget samples. Formatting and frame rules belong to widgets.
use super::schema::{DenseActivityReport, ParsedActivity};
use ovrley_widgets::{
    model::types::MetricKind,
    widgets::{
        elevation::sampling::{
            elevation_profile_series, preferred_elevation_series, ElevationActivity,
        },
        g_force::validation::GForceAxis,
        lap_timer::format::{LapHistory, LapSample},
        route::sampling::RouteActivity,
        value::format::MetricSample,
    },
};

impl ParsedActivity {
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
