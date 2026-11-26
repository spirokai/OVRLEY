//! Metric derivation and source-coverage bookkeeping.
//!
//! Raw extraction samples can contain direct sensor values, partial series, or
//! only the ingredients needed to derive a metric. This module builds one
//! descriptor per metric by combining direct values with shared fallback
//! derivations, then records whether each series is direct, derived, mixed, or
//! missing for coverage reporting.

use super::finalize::{moving_average, smoothing_window_for_seconds, zero_phase_smooth};
use crate::activity::schema::{ActivityColumns, GearSeries, NumericSeries};
use crate::import::{bearing_degrees, finite_f64, haversine_distance, round_f64};
use ovrley_widgets::model::telemetry::ZERO_AVAILABLE_ACTIVITY_SERIES;
use ovrley_widgets::widgets::elevation::preferred_elevation_series;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(super) struct MetricDescriptor {
    /// Final metric values aligned with raw sample order.
    pub series: MetricSeries,
    /// Provenance summary used by parser coverage diagnostics.
    pub source: MetricSource,
}

#[derive(Clone, Debug)]
pub(super) enum MetricSeries {
    Numeric(NumericSeries),
    Gear(GearSeries),
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum MetricSource {
    /// Values came from direct source fields.
    Direct,
    /// Values came from derivation.
    Derived,
    /// The series combines direct source values and derived fallback values.
    Mixed,
    /// No usable direct or derived values exist.
    Missing,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MetricCoverage {
    source: MetricSource,
    available_count: usize,
    #[serde(skip_serializing_if = "is_zero")]
    nonzero_count: usize,
    total_samples: usize,
    #[serde(skip)]
    has_usable_values: bool,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

impl MetricCoverage {
    pub(super) fn from_direct_presence(available_count: usize, total_samples: usize) -> Self {
        Self {
            source: if available_count > 0 {
                MetricSource::Direct
            } else {
                MetricSource::Missing
            },
            available_count,
            nonzero_count: 0,
            total_samples,
            has_usable_values: available_count > 0,
        }
    }

    fn from_series<T>(series: &[Option<T>], source: MetricSource) -> Self {
        let available_count = series.iter().filter(|value| value.is_some()).count();
        Self {
            source,
            available_count,
            nonzero_count: available_count,
            total_samples: series.len(),
            has_usable_values: available_count > 0,
        }
    }

    fn from_descriptor(descriptor: &MetricDescriptor, zero_is_available: bool) -> Self {
        match &descriptor.series {
            MetricSeries::Numeric(series) => {
                let available_count = series.iter().filter(|v| v.is_some()).count();
                let nonzero_count = series
                    .iter()
                    .filter(|v| v.is_some_and(|x| x != 0.0))
                    .count();
                Self {
                    source: descriptor.source,
                    available_count,
                    nonzero_count,
                    total_samples: series.len(),
                    has_usable_values: if zero_is_available {
                        available_count > 0
                    } else {
                        nonzero_count > 0
                    },
                }
            }
            MetricSeries::Gear(series) => Self::from_series(series, descriptor.source),
        }
    }

    pub(super) fn is_available(&self) -> bool {
        self.has_usable_values
    }
}

/// Builds the coverage of every finalized metric.
///
/// Coverage is calculated after direct/derived combination so it reports what
/// widgets can actually consume, not just what the source file happened to
/// include.
pub(super) fn build_metric_coverage(
    metric_series_map: &BTreeMap<String, MetricDescriptor>,
) -> BTreeMap<String, MetricCoverage> {
    let mut coverage = BTreeMap::new();
    for (metric, descriptor) in metric_series_map {
        coverage.insert(
            metric.clone(),
            MetricCoverage::from_descriptor(
                descriptor,
                ZERO_AVAILABLE_ACTIVITY_SERIES.contains(&metric.as_str()),
            ),
        );
    }
    coverage
}

/// Speed below which a moving rider counts as stopped; heading and gradient hold.
const STOPPED_BELOW_KMH: f64 = 2.0;
/// Speed a stopped rider must exceed before heading and gradient update again.
/// The gap to the stop threshold keeps GPS speed noise from toggling the state.
const MOVING_ABOVE_KMH: f64 = 3.0;
/// How far in time a heading baseline may reach on either side of a sample.
const HEADING_BASELINE_WINDOW_SECONDS: f64 = 5.0;
/// Distance on each side of the rider that the gradient line is fitted over.
const GRADIENT_HALF_WINDOW_METERS: f64 = 15.0;
/// How far in time the gradient window may reach on either side of a sample.
/// When GPS distance stalls, the window would otherwise span minutes of
/// altitude change over a few metres of recorded distance.
const GRADIENT_HALF_WINDOW_SECONDS: f64 = 20.0;

/// Stop/start state with hysteresis. Samples without speed keep the previous state.
struct MotionState {
    moving: bool,
}

impl MotionState {
    fn new() -> Self {
        Self { moving: true }
    }

    fn update(&mut self, speed_mps: Option<f64>) -> bool {
        if let Some(speed_kmh) = speed_mps.and_then(finite_f64).map(|mps| mps * 3.6) {
            if self.moving && speed_kmh < STOPPED_BELOW_KMH {
                self.moving = false;
            } else if !self.moving && speed_kmh > MOVING_ABOVE_KMH {
                self.moving = true;
            }
        }
        self.moving
    }
}

/// Least-squares accumulator for elevation against distance offset.
struct SlopeFit {
    count: f64,
    sum_x: f64,
    sum_y: f64,
    sum_xx: f64,
    sum_xy: f64,
    min_x: f64,
    max_x: f64,
}

impl SlopeFit {
    fn new() -> Self {
        Self {
            count: 0.0,
            sum_x: 0.0,
            sum_y: 0.0,
            sum_xx: 0.0,
            sum_xy: 0.0,
            min_x: f64::INFINITY,
            max_x: f64::NEG_INFINITY,
        }
    }

    fn add(&mut self, x: f64, y: f64) {
        self.count += 1.0;
        self.sum_x += x;
        self.sum_y += y;
        self.sum_xx += x * x;
        self.sum_xy += x * y;
        self.min_x = self.min_x.min(x);
        self.max_x = self.max_x.max(x);
    }

    /// Percent grade once the points span at least one half window.
    fn percent_grade(&self) -> Option<f64> {
        if self.count < 3.0 || self.max_x - self.min_x < GRADIENT_HALF_WINDOW_METERS {
            return None;
        }
        let variance = self.count * self.sum_xx - self.sum_x * self.sum_x;
        let covariance = self.count * self.sum_xy - self.sum_x * self.sum_y;
        Some(covariance / variance * 100.0)
    }
}

/// Derives percent grade at each sample as the least-squares slope of
/// elevation against cumulative distance over a centred distance window.
///
/// Filtering in the distance domain keeps the result independent of sample
/// rate, and the centred window places the slope at the rider instead of
/// lagging behind like a trailing bike-computer window. The window is also
/// bounded in time, because stalled GPS distance would otherwise stretch it
/// over minutes. The last valid grade holds while the rider is stopped, when
/// the window cannot span enough distance, and across elevation holes.
/// Missing distance samples stay missing and break the window.
fn derive_gradient_series(
    elevation_series: &NumericSeries,
    distance_series: &NumericSeries,
    speed_series: &NumericSeries,
    elapsed_series: &[f64],
) -> NumericSeries {
    let mut gradient_series = Vec::with_capacity(distance_series.len());
    let mut last_gradient = 0.0;
    let mut motion = MotionState::new();

    for index in 0..distance_series.len() {
        let moving = motion.update(speed_series[index]);
        let Some(current_distance) = distance_series[index] else {
            gradient_series.push(None);
            continue;
        };

        if moving {
            let mut fit = SlopeFit::new();
            let mut add_window = |candidates: &mut dyn Iterator<Item = usize>| {
                for candidate in candidates {
                    let Some(distance) = distance_series[candidate] else {
                        break;
                    };
                    if (distance - current_distance).abs() > GRADIENT_HALF_WINDOW_METERS
                        || (elapsed_series[candidate] - elapsed_series[index]).abs()
                            > GRADIENT_HALF_WINDOW_SECONDS
                    {
                        break;
                    }
                    if let Some(elevation) = elevation_series[candidate] {
                        fit.add(distance - current_distance, elevation);
                    }
                }
            };
            add_window(&mut (index..=index));
            add_window(&mut (0..index).rev());
            add_window(&mut (index + 1..distance_series.len()));
            if let Some(gradient) = fit.percent_grade() {
                last_gradient = gradient;
            }
        }
        gradient_series.push(round_f64(last_gradient, 3));
    }

    gradient_series
}

/// A running engine never pulls manifold pressure this low in absolute terms,
/// so a boost log that reaches it is already gauge pressure.
const GAUGE_BOOST_MAX_MINIMUM_KPA: f64 = 10.0;
/// Atmospheric pressure assumed when the activity records no air pressure.
const STANDARD_ATMOSPHERE_KPA: f64 = 101.325;

/// Converts an absolute boost log to gauge pressure.
///
/// Exporters rarely say which one they record. Absolute manifold pressure stays
/// above roughly 20 kPa even at full vacuum, while any gauge log that includes
/// idle, cruise or engine-off samples reaches zero or below. A log whose minimum
/// stays above the threshold is therefore absolute; the recorded air pressure,
/// or the standard atmosphere without one, is subtracted per sample.
fn derive_gauge_boost_pressure_series(
    boost_kpa: &NumericSeries,
    air_pressure_bar: &NumericSeries,
) -> NumericSeries {
    let is_gauge = boost_kpa
        .iter()
        .flatten()
        .any(|pressure| *pressure <= GAUGE_BOOST_MAX_MINIMUM_KPA);
    if is_gauge {
        return boost_kpa.clone();
    }
    boost_kpa
        .iter()
        .zip(air_pressure_bar)
        .map(|(boost, air)| {
            let atmosphere_kpa = air.map_or(STANDARD_ATMOSPHERE_KPA, |bar| bar * 100.0);
            boost.and_then(|absolute| round_f64(absolute - atmosphere_kpa, 3))
        })
        .collect()
}

/// Derives heading from course points using a straight-line baseline.
///
/// A stopped receiver's position wanders inside a few metres, so path length
/// keeps growing while the rider stands still. The baseline therefore measures
/// straight-line displacement: the centered pair needs half the baseline on each
/// side, and the lookback fallback keeps headings available near the tail. When
/// no displacement is large enough within the time window, the last heading
/// holds. Speed gates updates with hysteresis so drift at a standstill never
/// turns the heading; samples without speed keep the previous moving state.
fn derive_heading_series(
    course_series: &[(Option<f64>, Option<f64>)],
    speed_series: &NumericSeries,
    elapsed_series: &[f64],
    min_displacement_meters: f64,
) -> NumericSeries {
    let mut derived = Vec::with_capacity(course_series.len());
    let mut last_heading = None;
    let mut motion = MotionState::new();
    let half_baseline_meters = min_displacement_meters / 2.0;

    for index in 0..course_series.len() {
        let moving = motion.update(speed_series[index]);

        let heading = present_gps_point(course_series[index])
            .filter(|_| moving)
            .and_then(|current| {
                let displaced = |radius: f64, candidates: &mut dyn Iterator<Item = usize>| {
                    baseline_point(
                        course_series,
                        elapsed_series,
                        index,
                        current,
                        radius,
                        candidates,
                    )
                };
                let centered = displaced(half_baseline_meters, &mut (0..index).rev()).zip(
                    displaced(half_baseline_meters, &mut (index + 1..course_series.len())),
                );
                match centered {
                    Some((left, right)) => bearing_degrees(left.0, left.1, right.0, right.1),
                    None => displaced(min_displacement_meters, &mut (0..index).rev())
                        .and_then(|left| bearing_degrees(left.0, left.1, current.0, current.1)),
                }
            });

        if let Some(value) = heading.and_then(|value| round_f64(value, 3)) {
            last_heading = Some(value);
        }
        derived.push(last_heading);
    }

    derived
}

/// Finds the nearest candidate at least `radius` metres in a straight line from
/// `current`, scanning only within the heading baseline time window.
fn baseline_point(
    course_series: &[(Option<f64>, Option<f64>)],
    elapsed_series: &[f64],
    index: usize,
    current: (f64, f64),
    radius: f64,
    candidates: &mut dyn Iterator<Item = usize>,
) -> Option<(f64, f64)> {
    let current_elapsed = finite_f64(elapsed_series[index])?;
    for candidate in candidates {
        let elapsed = finite_f64(elapsed_series[candidate])?;
        if (elapsed - current_elapsed).abs() > HEADING_BASELINE_WINDOW_SECONDS {
            return None;
        }
        let Some(point) = present_gps_point(course_series[candidate]) else {
            continue;
        };
        if haversine_distance(current.0, current.1, point.0, point.1) >= radius {
            return Some(point);
        }
    }
    None
}

/// Derives the surface distance from the first valid GPS coordinate.
///
/// The home coordinate is selected once from source order. Coordinate ranges
/// were validated by the finalization ingress before this derivation runs.
/// Samples with a missing coordinate remain missing; no synthetic position is
/// introduced for an absent GPS observation.
fn derive_distance_to_home_series(
    course_series: &[(Option<f64>, Option<f64>)],
) -> NumericSeries {
    let mut home = None;
    for point in course_series {
        if let Some(coordinate) = present_gps_point(*point) {
            home = Some(coordinate);
            break;
        }
    }
    let Some((home_latitude, home_longitude)) = home else {
        return vec![None; course_series.len()];
    };

    let mut distances = Vec::with_capacity(course_series.len());
    for point in course_series {
        let distance = match present_gps_point(*point) {
            Some((latitude, longitude)) => round_f64(
                haversine_distance(home_latitude, home_longitude, latitude, longitude),
                3,
            ),
            None => None,
        };
        distances.push(distance);
    }
    distances
}

/// Derives cumulative positive elevation gain from the preferred altitude source.
///
/// Barometric altitude is selected for the whole activity when at least one
/// valid barometric sample exists. Otherwise generic elevation is used. The
/// selected source is smoothed before adjacent valid samples are compared so
/// high-rate sensor noise does not become ascent. Missing samples stay missing
/// while the cumulative total is preserved for the next valid observation.
fn derive_total_ascent_series(
    barometric_altitude: &[Option<f64>],
    elevation: &[Option<f64>],
) -> NumericSeries {
    let source = preferred_elevation_series(barometric_altitude, elevation);
    // Ascent accumulates meter-level noise, so neighbours are averaged before comparison.
    let smoothed = moving_average(source, 5);
    let mut cumulative = 0.0;
    let mut previous_altitude: Option<f64> = None;

    let mut ascent = Vec::with_capacity(smoothed.len());
    for altitude in smoothed {
        let Some(altitude) = altitude.and_then(|altitude| round_f64(altitude, 3)) else {
            ascent.push(None);
            continue;
        };

        if let Some(previous_altitude) = previous_altitude {
            cumulative += (altitude - previous_altitude).max(0.0);
        }
        previous_altitude = Some(altitude);
        ascent.push(round_f64(cumulative, 3));
    }
    ascent
}

fn present_gps_point(point: (Option<f64>, Option<f64>)) -> Option<(f64, f64)> {
    match point {
        (Some(latitude), Some(longitude)) => Some((latitude, longitude)),
        _ => None,
    }
}

/// Derives a per-second rate from adjacent numeric samples.
///
/// Speed and vertical speed should follow the same sample cadence as the source
/// stream; carrying the last valid rate through holes matches the existing UI
/// behavior without inventing values before the first valid difference.
fn derive_numeric_rate_series(
    numerator_series: &NumericSeries,
    elapsed_series: &[f64],
) -> NumericSeries {
    let mut derived = Vec::with_capacity(numerator_series.len());
    let mut last_value = None;

    for index in 0..numerator_series.len() {
        if index == 0 {
            derived.push(None);
            continue;
        }

        let previous_value = numerator_series[index - 1].and_then(finite_f64);
        let current_value = numerator_series[index].and_then(finite_f64);
        let previous_elapsed = finite_f64(elapsed_series[index - 1]);
        let current_elapsed = finite_f64(elapsed_series[index]);

        let (
            Some(previous_value),
            Some(current_value),
            Some(previous_elapsed),
            Some(current_elapsed),
        ) = (
            previous_value,
            current_value,
            previous_elapsed,
            current_elapsed,
        )
        else {
            derived.push(last_value);
            continue;
        };

        let elapsed_delta = current_elapsed - previous_elapsed;
        if elapsed_delta <= 0.0 {
            derived.push(last_value);
            continue;
        }

        last_value = round_f64((current_value - previous_value) / elapsed_delta, 6);
        derived.push(last_value);
    }

    derived
}

/// Converts speed in meters per second to seconds per kilometer.
///
/// Pace is only meaningful for positive finite speed, so stopped or missing
/// samples remain null instead of reporting infinite values.
fn derive_pace_series(speed_series: &NumericSeries) -> NumericSeries {
    speed_series
        .iter()
        .map(|speed| {
            let speed = speed.and_then(finite_f64)?;
            (speed > 0.0)
                .then(|| round_f64(1000.0 / speed, 3))
                .flatten()
        })
        .collect()
}

/// Derives cumulative calories from power samples and their elapsed times.
///
/// Power is mechanical work per second, so each interval contributes the
/// trapezoidal average of its endpoint powers multiplied by the elapsed-time
/// delta. The estimate assumes 22% cycling efficiency: mechanical work is
/// converted from joules to metabolic kilocalories using 4,184 J per kcal and
/// that efficiency. Missing or non-increasing intervals do not invent work;
/// the last cumulative value is carried through once power has produced an
/// estimate.
fn derive_calories_from_power(
    power_series: &NumericSeries,
    elapsed_series: &[f64],
) -> NumericSeries {
    const CYCLING_EFFICIENCY: f64 = 0.22;

    let mut cumulative_joules = 0.0;
    let mut previous_sample: Option<(f64, f64)> = None;
    let mut has_estimate = false;
    let mut calories = Vec::with_capacity(power_series.len());

    for (index, power) in power_series.iter().enumerate() {
        let current_power = power.and_then(finite_f64).map(|value| value.max(0.0));
        let current_elapsed = elapsed_series.get(index).copied().and_then(finite_f64);

        if let (Some(power), Some(elapsed)) = (current_power, current_elapsed) {
            if let Some((previous_power, previous_elapsed)) = previous_sample {
                let elapsed_delta = elapsed - previous_elapsed;
                if elapsed_delta > 0.0 {
                    cumulative_joules += ((previous_power + power) / 2.0) * elapsed_delta;
                }
            }
            previous_sample = Some((power, elapsed));
            has_estimate = true;
        } else {
            previous_sample = None;
        }

        calories.push(if has_estimate {
            round_f64(cumulative_joules / (4_184.0 * CYCLING_EFFICIENCY), 3)
        } else {
            None
        });
    }

    calories
}

/// Derives crank torque from power and cadence.
///
/// The angular-velocity formula requires positive cadence; nulling invalid
/// samples prevents divide-by-zero artifacts from reaching widgets.
fn derive_torque_series(
    power_series: &NumericSeries,
    cadence_series: &NumericSeries,
) -> NumericSeries {
    power_series
        .iter()
        .zip(cadence_series)
        .map(|(power, cadence)| {
            let power = power.and_then(finite_f64)?;
            let cadence = cadence.and_then(finite_f64)?;
            if cadence <= 0.0 {
                return None;
            }
            let angular_velocity = (2.0 * std::f64::consts::PI * cadence) / 60.0;
            if !angular_velocity.is_finite() || angular_velocity <= 0.0 {
                return None;
            }
            round_f64(power / angular_velocity, 6)
        })
        .collect()
}

/// Derives theoretical lean angle from ground speed and heading rate.
///
/// Uses the steady-state coordinated-turn formula `atan(v · ω / g)` where `v`
/// is speed in m/s and `ω` is the heading change rate in rad/s. The rate is
/// measured against the newest heading at least one second older than each
/// sample to reduce GPS noise. Samples before the first computed lean take that
/// value; later samples without such a window stay `None`.
fn derive_lean_angle_series(
    speed_mps: &NumericSeries,
    heading_deg: &NumericSeries,
    elapsed_seconds: &[f64],
) -> NumericSeries {
    const G: f64 = 9.806_65;
    const RATE_WINDOW_SECONDS: f64 = 1.0;
    let mut lean: NumericSeries = (0..elapsed_seconds.len())
        .map(|index| {
            let speed = speed_mps[index]?;
            let heading = heading_deg[index]?;
            let current_time = elapsed_seconds[index];
            let (dt, previous) = (0..index).rev().find_map(|previous| {
                let dt = current_time - elapsed_seconds[previous];
                (dt >= RATE_WINDOW_SECONDS)
                    .then_some(heading_deg[previous])
                    .flatten()
                    .map(|previous| (dt, previous))
            })?;
            let delta = (heading - previous + 540.0).rem_euclid(360.0) - 180.0;
            let omega_rad_s = delta.to_radians() / dt;
            finite_f64((speed * omega_rad_s / G).atan().to_degrees())
        })
        .collect();
    // The rate window leaves the start empty; hold the first lean back over it. Later gaps stay missing.
    if let Some(first) = lean.iter().position(Option::is_some) {
        let value = lean[first];
        lean[..first].fill(value);
    }
    lean
}

/// Combines preferred and fallback series while preserving provenance.
///
/// Some metrics prefer direct sensor values, while gradient intentionally
/// prefers the standard derived path. The provenance logic follows that
/// preference so coverage explains why a widget sees its final values.
fn combine_series(
    primary: &NumericSeries,
    fallback: &NumericSeries,
    prefer_derived: bool,
) -> MetricDescriptor {
    let combined: NumericSeries = primary
        .iter()
        .enumerate()
        .map(|(index, value)| value.or_else(|| fallback.get(index).copied().flatten()))
        .collect();
    let primary_count = primary.iter().filter(|value| value.is_some()).count();
    let fallback_only_count = combined
        .iter()
        .enumerate()
        .filter(|(index, value)| {
            value.is_some() && primary.get(*index).copied().flatten().is_none()
        })
        .count();

    let source = match (primary_count > 0, fallback_only_count > 0, prefer_derived) {
        (true, true, _) => MetricSource::Mixed,
        (true, false, true) => MetricSource::Derived,
        (true, false, false) => MetricSource::Direct,
        (false, true, true) => MetricSource::Direct,
        (false, true, false) => MetricSource::Derived,
        _ => MetricSource::Missing,
    };

    MetricDescriptor {
        series: MetricSeries::Numeric(combined),
        source,
    }
}

/// Selects one complete source series without repairing holes in direct data.
fn select_series(primary: &NumericSeries, fallback: &NumericSeries) -> MetricDescriptor {
    if primary.iter().any(Option::is_some) {
        return MetricDescriptor {
            series: MetricSeries::Numeric(primary.clone()),
            source: MetricSource::Direct,
        };
    }
    let source = if fallback.iter().any(Option::is_some) {
        MetricSource::Derived
    } else {
        MetricSource::Missing
    };
    MetricDescriptor {
        series: MetricSeries::Numeric(fallback.clone()),
        source,
    }
}

/// Builds every finalized metric descriptor for a raw activity.
///
/// Direct source fields are collected once, shared derivations are computed from
/// the aligned base series, and each metric combines them by its own precedence.
pub(super) fn derive_activity_metric_series(
    course_series: &[(Option<f64>, Option<f64>)],
    distance_series: &NumericSeries,
    elevation_base_series: &NumericSeries,
    barometric_altitude_series: &NumericSeries,
    elapsed_series: &[f64],
    columns: &ActivityColumns,
) -> BTreeMap<String, MetricDescriptor> {
    let direct = direct_metrics(
        columns,
        distance_series,
        elevation_base_series,
        barometric_altitude_series,
    );
    let null_series: NumericSeries = columns.timestamp.iter().map(|_| None).collect();
    let preferred_altitude =
        preferred_elevation_series(&direct["barometric_altitude"], &direct["elevation"]).to_vec();
    let derived_speed = derive_numeric_rate_series(distance_series, elapsed_series);
    // Explicit extraction settings are applied once by the finalizer. Otherwise,
    // smooth distance-derived speed over one second before mixing in recorded
    // values. At 1 Hz or slower the window is one sample, preserving those rates.
    let derived_speed = if columns.options.smoothing.contains_key("speed") {
        derived_speed
    } else {
        let timestamps_ms: Vec<_> = elapsed_series
            .iter()
            .map(|second| second * 1000.0)
            .collect();
        let window = smoothing_window_for_seconds(&timestamps_ms, 1.0);
        zero_phase_smooth(&derived_speed, window)
    };
    let motion_speed: NumericSeries = direct["speed"]
        .iter()
        .zip(&derived_speed)
        .map(|(recorded, derived)| recorded.or(*derived))
        .collect();
    let derived_heading = derive_heading_series(course_series, &motion_speed, elapsed_series, 8.0);
    let derived_distance_to_home = derive_distance_to_home_series(course_series);
    let derived_total_ascent =
        derive_total_ascent_series(&direct["barometric_altitude"], &direct["elevation"]);
    let derived_gradient = derive_gradient_series(
        &preferred_altitude,
        distance_series,
        &motion_speed,
        elapsed_series,
    );
    let derived_vertical_speed = derive_numeric_rate_series(&preferred_altitude, elapsed_series);
    // Depth grows downwards, so its rate is negated to read positive while ascending.
    let derived_depth_vertical_speed: NumericSeries =
        derive_numeric_rate_series(&direct["depth"], elapsed_series)
            .into_iter()
            .map(|rate| rate.map(|rate| 0.0 - rate))
            .collect();
    let derived_pace = derive_pace_series(
        &direct["speed"]
            .iter()
            .enumerate()
            .map(|(index, value)| value.or(derived_speed[index]))
            .collect(),
    );
    let derived_torque = derive_torque_series(&direct["power"], &direct["cadence"]);

    let mut map = BTreeMap::new();
    for (name, values) in &direct {
        map.insert(
            (*name).to_owned(),
            combine_series(values, &null_series, false),
        );
    }
    map.insert(
        "boost_pressure".to_string(),
        combine_series(
            &derive_gauge_boost_pressure_series(&direct["boost_pressure"], &direct["air_pressure"]),
            &null_series,
            false,
        ),
    );
    let derived_calories = derive_calories_from_power(&direct["power"], elapsed_series);
    map.insert(
        "calories".to_string(),
        select_series(&direct["calories"], &derived_calories),
    );
    map.insert(
        "distance".to_string(),
        MetricDescriptor {
            series: MetricSeries::Numeric(direct["distance"].clone()),
            source: MetricSource::Direct,
        },
    );
    map.insert(
        "distance_to_home".to_string(),
        select_series(&direct["distance_to_home"], &derived_distance_to_home),
    );
    map.insert(
        "total_ascent".to_string(),
        MetricDescriptor {
            source: if derived_total_ascent.iter().any(Option::is_some) {
                MetricSource::Derived
            } else {
                MetricSource::Missing
            },
            series: MetricSeries::Numeric(derived_total_ascent),
        },
    );
    map.insert(
        "depth_vertical_speed".to_string(),
        MetricDescriptor {
            source: if derived_depth_vertical_speed.iter().any(Option::is_some) {
                MetricSource::Derived
            } else {
                MetricSource::Missing
            },
            series: MetricSeries::Numeric(derived_depth_vertical_speed),
        },
    );
    map.insert(
        "gear_position".to_string(),
        MetricDescriptor {
            series: MetricSeries::Gear(columns.gear_position.clone()),
            source: if columns.gear_position.iter().any(Option::is_some) {
                MetricSource::Direct
            } else {
                MetricSource::Missing
            },
        },
    );
    map.insert(
        "gradient".to_string(),
        combine_series(&derived_gradient, &direct["gradient"], true),
    );
    let heading = if columns.preserve_direct_metric_gaps.heading {
        select_series(&direct["heading"], &derived_heading)
    } else {
        combine_series(&direct["heading"], &derived_heading, false)
    };
    let speed = if columns.preserve_direct_metric_gaps.speed {
        select_series(&direct["speed"], &derived_speed)
    } else {
        combine_series(&direct["speed"], &derived_speed, false)
    };
    // Lean derives from the final speed and heading, so every source with GPS gets one.
    if let (MetricSeries::Numeric(speed), MetricSeries::Numeric(heading)) =
        (&speed.series, &heading.series)
    {
        let derived_lean = derive_lean_angle_series(speed, heading, elapsed_series);
        map.insert(
            "lean_angle".to_string(),
            combine_series(&direct["lean_angle"], &derived_lean, false),
        );
    }
    map.insert("heading".to_string(), heading);
    map.insert(
        "pace".to_string(),
        combine_series(&direct["pace"], &derived_pace, false),
    );
    map.insert("speed".to_string(), speed);
    map.insert(
        "torque".to_string(),
        combine_series(&direct["torque"], &derived_torque, false),
    );
    map.insert(
        "vertical_speed".to_string(),
        combine_series(&direct["vertical_speed"], &derived_vertical_speed, false),
    );
    map
}

/// Extracts direct metric vectors from normalized raw samples.
///
/// Keeping direct collection table-driven gives derived metrics a consistent
/// field naming surface and isolates the RawSample-to-metric mapping in one
/// place.
macro_rules! direct_activity_columns {
    (
        numeric { $($numeric:ident,)* } text { $($text:ident,)* } integer { $($integer:ident,)* }
        samples { numeric { $($sample:ident,)* } text { $($sample_text:ident,)* } integer { $($sample_integer:ident,)* } }
        sources { $($source:ident: $variant:ident,)* } absent { $($absent:ident,)* }
    ) => {
        fn direct_metrics(
            columns: &ActivityColumns,
            distance_series: &NumericSeries,
            elevation_base_series: &NumericSeries,
            barometric_altitude_series: &NumericSeries,
        ) -> BTreeMap<&'static str, NumericSeries> {
            let mut direct = BTreeMap::new();
            $(direct.insert(
                stringify!($source),
                columns.$source.iter().map(finite_sample).collect(),
            );)*
            direct.insert("distance", distance_series.clone());
            direct.insert("elevation", elevation_base_series.clone());
            direct.insert("barometric_altitude", barometric_altitude_series.clone());
            direct
        }
    };
}

fn finite_sample(value: &Option<f64>) -> Option<f64> {
    value.and_then(finite_f64)
}

ovrley_widgets::activity_input_columns!(direct_activity_columns);

#[cfg(test)]
mod tests {
    use super::{
        derive_distance_to_home_series, derive_gauge_boost_pressure_series, derive_gradient_series,
        derive_heading_series, derive_total_ascent_series,
    };

    #[test]
    fn gauge_boost_logs_pass_through() {
        // Idle vacuum clamped to zero, then boost.
        let boost = vec![Some(0.05), None, Some(26.14)];
        assert_eq!(
            derive_gauge_boost_pressure_series(&boost, &vec![None; 3]),
            boost
        );
        // Unclamped idle vacuum is negative.
        let boost = vec![Some(-65.0), Some(80.0)];
        assert_eq!(
            derive_gauge_boost_pressure_series(&boost, &vec![None; 2]),
            boost
        );
    }

    #[test]
    fn absolute_boost_logs_subtract_atmosphere() {
        // Engine off, idle vacuum, then boost, in absolute kilopascals.
        let boost = vec![Some(101.325), Some(30.0), Some(201.325)];
        assert_eq!(
            derive_gauge_boost_pressure_series(&boost, &vec![None; 3]),
            vec![Some(0.0), Some(-71.325), Some(100.0)]
        );
        // Recorded air pressure in bar replaces the standard atmosphere.
        assert_eq!(
            derive_gauge_boost_pressure_series(
                &vec![Some(90.0), Some(190.0)],
                &vec![Some(0.9), None]
            ),
            vec![Some(0.0), Some(88.675)]
        );
    }

    #[test]
    fn heading_windows_preserve_thresholds_plateaus_holes_and_endpoints() {
        let distances = vec![
            None,
            Some(0.0),
            Some(0.0),
            Some(1.0),
            Some(5.0),
            Some(5.0),
            Some(6.0),
            Some(10.0),
            None,
            Some(15.0),
            Some(16.0),
            Some(20.0),
            Some(20.0),
        ];
        let course = distances
            .iter()
            .enumerate()
            .map(|(index, distance)| match distance {
                Some(distance) if index < 8 => (Some(0.0), Some(distance / 100_000.0)),
                Some(distance) => (Some(distance / 100_000.0), Some(0.0001)),
                None => (None, None),
            })
            .collect::<Vec<_>>();

        // Speeds are m/s. The rider stops at index 9 (1.8 km/h) and stays stopped at
        // index 10 (2.52 km/h, between the thresholds) until index 11 (7.2 km/h).
        let speeds = vec![
            Some(5.0),
            Some(5.0),
            Some(5.0),
            Some(5.0),
            Some(5.0),
            None,
            Some(5.0),
            Some(5.0),
            None,
            Some(0.5),
            Some(0.7),
            Some(2.0),
            Some(5.0),
        ];
        let elapsed = (0..distances.len())
            .map(|second| second as f64)
            .collect::<Vec<_>>();
        assert_eq!(
            derive_heading_series(&course, &speeds, &elapsed, 2.0),
            vec![
                None,
                None,
                None,
                Some(90.0),
                Some(90.0),
                Some(90.0),
                Some(90.0),
                Some(14.931),
                Some(14.931),
                Some(14.931),
                Some(14.931),
                Some(0.0),
                Some(0.0),
            ]
        );
    }

    #[test]
    fn distance_windows_handle_long_stationary_stretches() {
        let sample_count = 100_000;
        let distances = vec![Some(10.0); sample_count];
        let elevations = vec![Some(100.0); sample_count];
        // A stationary receiver drifts about 1 m around its fix; without speed,
        // only the straight-line baseline keeps that drift from producing headings.
        let course = (0..sample_count)
            .map(|index| {
                let drift = [0.0, 1.0, -1.0, 0.5][index % 4] * 0.000_008;
                (Some(47.0 + drift), Some(8.0 - drift))
            })
            .collect::<Vec<_>>();
        let speeds = vec![None; sample_count];
        let elapsed = (0..sample_count)
            .map(|index| index as f64 / 10.0)
            .collect::<Vec<_>>();

        let gradients = derive_gradient_series(&elevations, &distances, &speeds, &elapsed);
        let headings = derive_heading_series(&course, &speeds, &elapsed, 5.0);
        assert_eq!(gradients.len(), sample_count);
        assert_eq!(headings.len(), sample_count);
        assert!(gradients.iter().all(|value| *value == Some(0.0)));
        assert!(headings.iter().all(Option::is_none));
    }

    fn seconds(count: usize) -> Vec<f64> {
        (0..count).map(|second| second as f64).collect()
    }

    #[test]
    fn gradient_fits_steady_slopes_without_clamping() {
        // 1 Hz at 5 m/s on a 60% lift line: steep grades are real and pass through.
        let distances: Vec<_> = (0..20).map(|index| Some(index as f64 * 5.0)).collect();
        let elevations: Vec<_> = (0..20).map(|index| Some(index as f64 * 3.0)).collect();
        let speeds = vec![Some(5.0); 20];

        let gradients = derive_gradient_series(&elevations, &distances, &speeds, &seconds(20));
        assert!(gradients.iter().all(|value| *value == Some(60.0)));
    }

    #[test]
    fn gradient_least_squares_averages_altitude_steps() {
        // Barometric altitude quantized to 0.2 m steps on an even 3% grade.
        let distances: Vec<_> = (0..40).map(|index| Some(index as f64 * 5.0)).collect();
        let elevations: Vec<_> = (0..40)
            .map(|index| Some((index as f64 * 0.15 / 0.2).floor() * 0.2))
            .collect();
        let speeds = vec![Some(5.0); 40];

        let gradients = derive_gradient_series(&elevations, &distances, &speeds, &seconds(40));
        for value in &gradients[3..37] {
            assert!((value.unwrap() - 3.0).abs() < 0.5, "{value:?}");
        }
    }

    #[test]
    fn gradient_holds_while_stopped_and_through_gps_stalls() {
        // Riding up 10% at 5 m/s, then stopped for 30 s while barometric altitude
        // drifts 2 m, then riding with GPS distance frozen for 60 s while the
        // barometer records a real 10 m drop, then riding flat.
        let mut distances = Vec::new();
        let mut elevations = Vec::new();
        let mut speeds = Vec::new();
        for index in 0..10 {
            distances.push(Some(index as f64 * 5.0));
            elevations.push(Some(index as f64 * 0.5));
            speeds.push(Some(5.0));
        }
        for index in 0..30 {
            distances.push(Some(45.0));
            elevations.push(Some(4.5 - index as f64 / 15.0));
            speeds.push(Some(0.0));
        }
        for index in 0..60 {
            distances.push(Some(45.0 + ((index + 1) / 20) as f64));
            elevations.push(Some(2.5 - index as f64 / 6.0));
            speeds.push(None);
        }
        let stalled_end = distances.len();
        for index in 0..20 {
            distances.push(Some(53.0 + index as f64 * 5.0));
            elevations.push(Some(-7.5));
            speeds.push(Some(5.0));
        }

        let gradients =
            derive_gradient_series(&elevations, &distances, &speeds, &seconds(distances.len()));
        // Stopped: the last moving grade holds instead of following altitude drift.
        assert!(gradients[11..40]
            .iter()
            .all(|value| *value == gradients[10]));
        // Stalled distance never turns 10 m over 1 m into a wall.
        assert!(gradients[..stalled_end]
            .iter()
            .all(|value| value.unwrap().abs() <= 12.0));
        assert_eq!(gradients.last().copied().flatten(), Some(0.0));
    }

    #[test]
    fn gradient_preserves_missing_distance_and_breaks_the_window() {
        let mut distances: Vec<_> = (0..20).map(|index| Some(index as f64 * 5.0)).collect();
        let elevations: Vec<_> = (0..20)
            .map(|index| {
                Some(if index < 10 {
                    0.0
                } else {
                    (index - 10) as f64 * 1.0
                })
            })
            .collect();
        distances[10] = None;
        let speeds = vec![Some(5.0); 20];

        let gradients = derive_gradient_series(&elevations, &distances, &speeds, &seconds(20));
        assert_eq!(gradients[10], None);
        // The flat side never leaks across the hole into the 20% side.
        assert_eq!(gradients[9], Some(0.0));
        assert_eq!(gradients[11], Some(20.0));
    }

    #[test]
    fn distance_to_home_uses_first_present_coordinate_and_preserves_gaps() {
        let series = derive_distance_to_home_series(&[
            (None, Some(0.0)),
            (Some(0.0), Some(0.0)),
            (Some(0.0), Some(0.0)),
            (Some(0.0), Some(1.0)),
            (Some(0.0), None),
        ]);

        assert_eq!(series[0], None);
        assert_eq!(series[1], Some(0.0));
        assert_eq!(series[2], Some(0.0));
        assert!((series[3].expect("valid coordinate has a distance") - 111_194.927).abs() < 0.001);
        assert_eq!(series[4], None);
    }

    #[test]
    fn total_ascent_prefers_barometric_altitude_and_smooths_noise() {
        let series = derive_total_ascent_series(
            &[
                Some(100.0),
                Some(100.2),
                Some(100.0),
                Some(101.0),
                Some(101.1),
            ],
            &[Some(0.0), Some(50.0), Some(100.0), Some(150.0), Some(200.0)],
        );

        assert_eq!(series.last().copied().flatten(), Some(0.633));
        assert!(series.windows(2).all(|pair| match (pair[0], pair[1]) {
            (Some(previous), Some(current)) => current >= previous,
            _ => true,
        }));
    }

    #[test]
    fn total_ascent_falls_back_to_elevation_when_barometric_is_absent() {
        let series = derive_total_ascent_series(
            &[None, None, None],
            &[Some(10.0), Some(12.0), Some(11.0), Some(15.0)],
        );

        assert_eq!(series, vec![Some(0.0), Some(1.0), Some(1.0), Some(1.667)]);
    }
}
