//! Original-density metric sampling. Columns and bridge/hold indices are prepared once.
use crate::{
    error::{WidgetError, WidgetResult},
    interpolation::{
        collect_valid_numeric_points, interpolate_heading_points, interpolate_numeric_series_value,
        interpolate_points, MissingSamplePolicy,
    },
    model::{
        standard_metrics::{standard_metric_interpolation, StandardMetricInterpolationKind},
        types::MetricKind,
    },
    widgets::value::{format::MetricSample, validation::ValidatedValueWidget},
};
use serde::Deserialize;
use std::collections::HashMap;
use crate::widgets::g_force::{
    format::{sorted_magnitudes, percentile_max_g, GForceSample},
    validation::ValidatedGForceWidget,
};

macro_rules! activity_columns {
    ($($field:ident: $kind:ident),* $(,)?) => {
        /// Canonical finalized columns. Missing external telemetry is an empty series.
        #[derive(Deserialize)]
        struct PreviewActivity {
            sample_elapsed_seconds: Vec<f64>,
            trim_end_seconds: f64,
            #[serde(default)] time: Vec<Option<String>>,
            #[serde(default)] sync_time: Option<String>,
            #[serde(default)] metadata: serde_json::Value,
            $(#[serde(default)] $field: Vec<Option<f64>> ,)*
            #[serde(default)] elevation: Vec<Option<f64>>,
            #[serde(default)] barometric_altitude: Vec<Option<f64>>,
            #[serde(default)] gradient: Vec<Option<f64>>,
            #[serde(default)] g_force_x: Vec<Option<f64>>,
            #[serde(default)] g_force_y: Vec<Option<f64>>,
            #[serde(default)] g_force_z: Vec<Option<f64>>,
            #[serde(default)] gear_position: Vec<Option<String>>,
            #[serde(default)] course: Vec<(Option<f64>, Option<f64>)>,
            #[serde(flatten)] lap_timing: crate::widgets::lap_timer::sampling::LapTiming,
        }
        impl PreviewActivity {
            fn take_numeric(&mut self) -> HashMap<MetricKind, Vec<Option<f64>>> {
                HashMap::from([$( (MetricKind::$kind, std::mem::take(&mut self.$field)), )*])
            }
        }
    }
}
activity_columns! {
    speed: Speed,
    distance: Distance,
    heartrate: Heartrate,
    cadence: Cadence,
    power: Power,
    engine_power: EnginePower,
    engine_load: EngineLoad,
    temperature: Temperature,
    calories: Calories,
    pace: Pace,
    g_force: GForce,
    air_pressure: AirPressure,
    ground_contact_time: GroundContactTime,
    left_right_balance: LeftRightBalance,
    stride_length: StrideLength,
    stroke_rate: StrokeRate,
    torque: Torque,
    vertical_speed: VerticalSpeed,
    vertical_ratio: VerticalRatio,
    vertical_oscillation: VerticalOscillation,
    core_temperature: CoreTemperature,
    heading: Heading,
    iso: Iso,
    aperture: Aperture,
    shutter_speed: ShutterSpeed,
    focal_length: FocalLength,
    ev: Ev,
    color_temperature: ColorTemperature,
    rpm: Rpm,
    throttle_position: ThrottlePosition,
    brake_position: BrakePosition,
    lean_angle: LeanAngle,
    distance_to_home: DistanceToHome,
    total_ascent: TotalAscent
}

struct NumericColumn {
    values: Vec<Option<f64>>,
    points: Vec<(f64, f64)>,
    range: Option<(f64, f64)>,
    policy: StandardMetricInterpolationKind,
}
impl NumericColumn {
    fn new(
        times: &[f64],
        values: Vec<Option<f64>>,
        policy: StandardMetricInterpolationKind,
    ) -> WidgetResult<Self> {
        if !values.is_empty() && values.len() != times.len() {
            return Err(WidgetError::Config(
                "finalized activity columns must align with timestamps".into(),
            ));
        }
        let points = collect_valid_numeric_points(times, &values);
        let range = points.iter().filter(|point| point.1.is_finite()).fold(None, |range, point| {
            Some(match range {
                None => (point.1, point.1),
                Some((min, max)) => (f64::min(min, point.1), f64::max(max, point.1)),
            })
        });
        Ok(Self {
            values,
            points,
            range,
            policy,
        })
    }
    fn contains(&self, second: f64) -> bool {
        self.points
            .first()
            .zip(self.points.last())
            .is_some_and(|(first, last)| second >= first.0 && second <= last.0)
    }
    fn at(&self, times: &[f64], second: f64) -> Option<f64> {
        match self.policy {
            StandardMetricInterpolationKind::Linear => interpolate_points(&self.points, second),
            StandardMetricInterpolationKind::Preserve => interpolate_numeric_series_value(
                times,
                &self.values,
                second,
                MissingSamplePolicy::Preserve,
            ),
            StandardMetricInterpolationKind::Hold => self
                .points
                .partition_point(|p| p.0 <= second + 1e-9)
                .checked_sub(1)
                .map(|i| self.points[i].1),
        }
    }
}

pub struct ActivitySampler {
    pub(crate) time: crate::widgets::time::sampling::TimeSampler,
    pub(crate) timezone: Option<chrono_tz::Tz>,
    times: Vec<f64>,
    end: f64,
    numeric: HashMap<MetricKind, NumericColumn>,
    gear: Vec<(f64, String)>,
    latitude: NumericColumn,
    longitude: NumericColumn,
    gradient: NumericColumn,
    g_force_axes: [NumericColumn; 3],
    g_force_magnitudes: [Vec<f64>; 3],
    full_activity_distance: Option<f64>,
    full_activity_total_ascent: Option<f64>,
    lap_timing: crate::widgets::lap_timer::sampling::LapTiming,
}
impl ActivitySampler {
    pub fn from_json(bytes: &[u8]) -> WidgetResult<Self> {
        let mut activity: PreviewActivity = serde_json::from_slice(bytes)
            .map_err(|e| WidgetError::Config(format!("preview activity: {e}")))?;
        let times = &activity.sample_elapsed_seconds;
        if !activity.trim_end_seconds.is_finite()
            || activity.trim_end_seconds < 0.0
            || times.iter().any(|t| !t.is_finite())
            || times.windows(2).any(|p| p[0] >= p[1])
        {
            return Err(WidgetError::Config(
                "finalized activity timestamps must be finite and increasing".into(),
            ));
        }
        for len in [
            activity.time.len(),
            activity.gear_position.len(),
            activity.course.len(),
            activity.elevation.len(),
            activity.barometric_altitude.len(),
        ] {
            if len != 0 && len != times.len() {
                return Err(WidgetError::Config(
                    "finalized activity columns must align with timestamps".into(),
                ));
            }
        }
        activity.lap_timing.validate(times)?;
        let altitude = if activity.barometric_altitude.iter().any(Option::is_some) {
            std::mem::take(&mut activity.barometric_altitude)
        } else {
            std::mem::take(&mut activity.elevation)
        };
        let mut columns = activity.take_numeric();
        let time = crate::widgets::time::sampling::TimeSampler::new(
            activity.sync_time.as_deref(),
            &activity.sample_elapsed_seconds,
            &activity.time,
        );
        let timezone = activity.metadata.get("timezone").filter(|v| !v.is_null())
            .map(|value| {
                value.as_str()
                    .ok_or_else(|| WidgetError::Config("activity timezone must be an IANA timezone string".into()))?
                    .parse().map_err(|e| WidgetError::Config(format!("activity timezone: {e}")))
            })
            .transpose()?;
        columns.insert(MetricKind::Altitude, altitude);
        let times = activity.sample_elapsed_seconds;
        // Gradient bridges internal gaps; samples outside observed coverage stay missing.
        let gradient = NumericColumn::new(&times, activity.gradient, StandardMetricInterpolationKind::Linear)?;
        let g_force_axes = [activity.g_force_x, activity.g_force_y, activity.g_force_z]
            .map(|values| NumericColumn::new(&times, values, StandardMetricInterpolationKind::Preserve));
        let [x, y, z] = g_force_axes;
        let g_force_axes = [x?, y?, z?];
        let g_force_magnitudes = [(0, 1), (0, 2), (1, 2)]
            .map(|(h, v)| sorted_magnitudes(&g_force_axes[h].values, &g_force_axes[v].values));
        let numeric: HashMap<_, _> = columns
            .into_iter()
            .map(|(kind, values)| {
                let policy = standard_metric_interpolation(kind)
                    .expect("metric column has a manifest policy");
                NumericColumn::new(&times, values, policy).map(|column| (kind, column))
            })
            .collect::<WidgetResult<_>>()?;
        let full_activity_distance = numeric[&MetricKind::Distance].points.last().map(|p| p.1);
        let full_activity_total_ascent =
            numeric[&MetricKind::TotalAscent].points.last().map(|p| p.1);
        let latitude = NumericColumn::new(
            &times,
            activity.course.iter().map(|p| p.0).collect(),
            StandardMetricInterpolationKind::Linear,
        )?;
        let longitude = NumericColumn::new(
            &times,
            activity.course.iter().map(|p| p.1).collect(),
            StandardMetricInterpolationKind::Linear,
        )?;
        let gear = times
            .iter()
            .copied()
            .zip(activity.gear_position)
            .filter_map(|(time, value)| value.map(|value| (time, value)))
            .collect();
        Ok(Self {
            time,
            timezone,
            times,
            end: activity.trim_end_seconds,
            numeric,
            gear,
            latitude,
            longitude,
            gradient,
            g_force_axes,
            g_force_magnitudes,
            full_activity_distance,
            full_activity_total_ascent,
            lap_timing: activity.lap_timing,
        })
    }

    pub fn gauge_range(&self, metric: MetricKind) -> (f64, f64) {
        crate::widgets::gauges::metric::metric_range(self.numeric[&metric].range)
    }

    pub fn gauge_altitude_offset(&self, target: Option<f64>) -> f64 {
        target.zip(self.numeric[&MetricKind::Altitude].points.first())
            .map_or(0.0, |(target, first)| target - first.1)
    }

    /// Gauges retain native bridge/hold/preserve policies and observed coverage.
    pub fn gauge_sample(&self, metric: MetricKind, second: f64) -> MetricSample<'_> {
        if second < 0.0 || second > self.end { return MetricSample::default(); }
        let column = &self.numeric[&metric];
        MetricSample {
            numeric: if metric == MetricKind::Heading {
                self.heading_at(second)
            } else if column.points.is_empty() || column.contains(second) {
                column.at(&self.times, second)
            } else { None },
            available: !column.values.is_empty(),
            full_activity_distance: self.full_activity_distance,
            full_activity_total_ascent: self.full_activity_total_ascent,
            ..MetricSample::default()
        }
    }

    pub fn lap_history(&self) -> crate::widgets::lap_timer::format::LapHistory<'_> {
        self.lap_timing.history(self.end)
    }

    pub fn lap_timer_at(&self, second: f64) -> crate::widgets::lap_timer::format::LapSample {
        if second < 0.0 || second > self.end {
            return crate::widgets::lap_timer::format::LapSample::default();
        }
        self.lap_timing.at(&self.times, second)
    }

    pub fn gradient_at(&self, second: f64) -> Option<f64> {
        if second < 0.0 || second > self.end || !self.gradient.contains(second) {
            return None;
        }
        self.gradient.at(&self.times, second)
    }

    pub fn lean_angle_at(&self, second: f64) -> Option<f64> {
        let column = &self.numeric[&MetricKind::LeanAngle];
        if second < 0.0 || second > self.end || !column.contains(second) {
            return None;
        }
        column.at(&self.times, second)
    }

    /// Heading uses the shortest arc while retaining gap bridging and edge clamping.
    pub fn heading_at(&self, second: f64) -> Option<f64> {
        if second < 0.0 || second > self.end {
            return None;
        }
        interpolate_heading_points(&self.numeric[&MetricKind::Heading].points, second)
    }

    pub fn g_force_at(&self, widget: &ValidatedGForceWidget, second: f64) -> GForceSample {
        let h = widget.axis_horizontal.index();
        let v = widget.axis_vertical.index();
        let pair = match (h.min(v), h.max(v)) {
            (0, 1) => 0,
            (0, 2) => 1,
            (1, 2) => 2,
            _ => unreachable!("validated G-force axes are different"),
        };
        let sample_axis = |axis: usize, invert: bool| {
            if second < 0.0 || second > self.end { return None; }
            self.g_force_axes[axis].at(&self.times, second).map(|value| if invert { -value } else { value })
        };
        GForceSample {
            horizontal: sample_axis(h, widget.invert_horizontal),
            vertical: sample_axis(v, widget.invert_vertical),
            max_g: percentile_max_g(&self.g_force_magnitudes[pair], widget.clip_percentile),
        }
    }

    pub fn sample(&self, widget: &ValidatedValueWidget, second: f64) -> MetricSample<'_> {
        if second < 0.0 || second > self.end {
            return MetricSample::default();
        }
        let mut sample = MetricSample {
            full_activity_distance: self.full_activity_distance,
            full_activity_total_ascent: self.full_activity_total_ascent,
            ..MetricSample::default()
        };
        match widget.metric {
            MetricKind::GearPosition => {
                sample.gear = self
                    .gear
                    .partition_point(|p| p.0 <= second + 1e-9)
                    .checked_sub(1)
                    .map(|i| self.gear[i].1.as_str());
            }
            MetricKind::GpsCoordinates => {
                sample.latitude = self.latitude.at(&self.times, second);
                sample.longitude = self.longitude.at(&self.times, second);
            }
            kind => {
                let column = &self.numeric[&kind];
                // Heading and coordinates retain native bridge/edge-clamping behavior.
                sample.available = if kind == MetricKind::Heading || column.points.is_empty() {
                    !column.values.is_empty()
                } else {
                    column.contains(second)
                };
                if sample.available {
                    sample.numeric = if kind == MetricKind::Heading {
                        self.heading_at(second)
                    } else {
                        column.at(&self.times, second)
                    };
                    if kind == MetricKind::Altitude {
                        let offset = widget
                            .starting_altitude_m
                            .zip(column.points.first())
                            .map_or(0.0, |(target, first)| target - first.1);
                        sample.numeric = sample.numeric.map(|v| v + offset);
                    }
                }
            }
        }
        sample
    }
}
