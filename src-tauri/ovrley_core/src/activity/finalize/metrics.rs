//! Metric derivation and source-coverage bookkeeping.
//!
//! Raw extraction samples can contain direct sensor values, partial series, or
//! only the ingredients needed to derive a metric. This module builds one
//! descriptor per metric by combining direct values with shared fallback
//! derivations, then records whether each series is direct, derived, mixed, or
//! missing for debug/UI coverage reporting.
//!
//! Phase 0 intentionally uses the standard gradient derivation for every format;
//! the legacy GPX path is not ported.

use crate::activity::elevation::preferred_elevation_series;
use crate::activity::schema::{ActivityColumns, GearSeries, NumericSeries};
use crate::media::telemetry_math::{bearing_degrees, finite_f64, haversine_distance, round_f64};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct MetricDescriptor {
    /// Final metric values aligned with raw sample order.
    pub series: MetricSeries,
    /// Provenance summary used by parser coverage diagnostics.
    pub source: MetricSource,
}

#[derive(Clone, Debug)]
pub enum MetricSeries {
    Numeric(NumericSeries),
    Gear(GearSeries),
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricSource {
    /// Values came from direct source fields.
    Direct,
    /// Values came from backend derivation.
    Derived,
    /// The series combines direct source values and derived fallback values.
    Mixed,
    /// No usable direct or derived values exist.
    Missing,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricCoverage {
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

    fn from_descriptor(descriptor: &MetricDescriptor) -> Self {
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
                    has_usable_values: nonzero_count > 0,
                }
            }
            MetricSeries::Gear(series) => Self::from_series(series, descriptor.source),
        }
    }

    pub fn is_available(&self) -> bool {
        self.has_usable_values
    }
}

/// Builds frontend-compatible coverage JSON from finalized descriptors.
///
/// Coverage is calculated after direct/derived combination so it reports what
/// widgets can actually consume, not just what the source file happened to
/// include.
pub fn build_metric_coverage(
    metric_series_map: &BTreeMap<String, MetricDescriptor>,
) -> BTreeMap<String, MetricCoverage> {
    let mut coverage = BTreeMap::new();
    for (metric, descriptor) in metric_series_map {
        coverage.insert(metric.clone(), MetricCoverage::from_descriptor(descriptor));
    }
    coverage
}

/// Smooths elevation locally before gradient derivation.
///
/// Gradient amplifies meter-level altitude noise, so the standard derivation
/// averages neighboring elevation samples before computing slope over distance.
fn smooth_elevation_series(elevation_series: &[Option<f64>], radius: usize) -> NumericSeries {
    let mut smoothed = Vec::with_capacity(elevation_series.len());
    for index in 0..elevation_series.len() {
        if elevation_series[index].and_then(finite_f64).is_none() {
            smoothed.push(None);
            continue;
        }

        let start = index.saturating_sub(radius);
        let end = (index + radius).min(elevation_series.len().saturating_sub(1));
        let mut total = 0.0;
        let mut count = 0.0;
        for neighbor in &elevation_series[start..=end] {
            if let Some(neighbor_value) = neighbor.and_then(finite_f64) {
                total += neighbor_value;
                count += 1.0;
            }
        }
        smoothed.push(if count > 0.0 {
            round_f64(total / count, 3)
        } else {
            None
        });
    }
    smoothed
}

/// Derives percent grade over a roughly 5 meter distance window.
///
/// A distance-window baseline is less sensitive to sample cadence than adjacent
/// differences. The result is clamped to realistic display bounds and carries
/// the last valid value through sparse elevation/distance holes.
pub fn derive_gradient_series(
    elevation_series: &NumericSeries,
    distance_series: &NumericSeries,
) -> NumericSeries {
    let smoothed_elevation = smooth_elevation_series(elevation_series, 2);
    let mut gradient_series = Vec::with_capacity(distance_series.len());
    let mut last_gradient = 0.0;

    for index in 0..distance_series.len() {
        let Some(current_distance) = distance_series[index].and_then(finite_f64) else {
            gradient_series.push(None);
            continue;
        };

        let mut left_index = index;
        while left_index > 0
            && distance_series[left_index].is_some_and(|distance| current_distance - distance < 5.0)
        {
            left_index -= 1;
        }

        let mut right_index = index;
        while right_index < distance_series.len() - 1
            && distance_series[right_index]
                .is_some_and(|distance| distance - current_distance < 5.0)
        {
            right_index += 1;
        }

        let left_distance = distance_series[left_index].and_then(finite_f64);
        let right_distance = distance_series[right_index].and_then(finite_f64);
        let left_elevation = smoothed_elevation[left_index].and_then(finite_f64);
        let right_elevation = smoothed_elevation[right_index].and_then(finite_f64);

        let (
            Some(left_distance),
            Some(right_distance),
            Some(left_elevation),
            Some(right_elevation),
        ) = (
            left_distance,
            right_distance,
            left_elevation,
            right_elevation,
        )
        else {
            gradient_series.push(round_f64(last_gradient, 3));
            continue;
        };

        let horizontal_distance = right_distance - left_distance;
        if horizontal_distance < 1.0 {
            gradient_series.push(round_f64(last_gradient, 3));
            continue;
        }

        let next_gradient =
            (((right_elevation - left_elevation) / horizontal_distance) * 100.0).clamp(-30.0, 30.0);
        last_gradient = next_gradient;
        gradient_series.push(round_f64(next_gradient, 3));
    }

    gradient_series
}

/// Derives heading from course points using a distance-based baseline.
///
/// The centered lookback/lookahead path reduces GPS jitter around the current
/// point; the fallback lookback keeps headings available near the tail where no
/// future sample exists.
pub fn derive_heading_series(
    course_series: &[(Option<f64>, Option<f64>)],
    distance_series: &NumericSeries,
    min_distance_meters: f64,
) -> NumericSeries {
    let mut derived = Vec::with_capacity(course_series.len());
    let mut last_heading = None;
    let half_baseline_meters = min_distance_meters / 2.0;

    for index in 0..course_series.len() {
        let current_distance = distance_series[index].and_then(finite_f64);
        let mut heading = None;

        if let Some(current_distance) = current_distance {
            let mut centered_lookback_index = index as isize - 1;
            while centered_lookback_index >= 0
                && distance_series[centered_lookback_index as usize]
                    .is_some_and(|distance| current_distance - distance < half_baseline_meters)
            {
                centered_lookback_index -= 1;
            }

            let mut lookahead_index = index + 1;
            while lookahead_index < course_series.len()
                && distance_series[lookahead_index]
                    .is_some_and(|distance| distance - current_distance < half_baseline_meters)
            {
                lookahead_index += 1;
            }

            let mut fallback_lookback_index = index as isize - 1;
            while fallback_lookback_index >= 0
                && distance_series[fallback_lookback_index as usize]
                    .is_some_and(|distance| current_distance - distance < min_distance_meters)
            {
                fallback_lookback_index -= 1;
            }

            let has_centered = centered_lookback_index >= 0
                && distance_series[centered_lookback_index as usize].is_some();
            let has_lookahead =
                lookahead_index < course_series.len() && distance_series[lookahead_index].is_some();
            let has_fallback = fallback_lookback_index >= 0
                && distance_series[fallback_lookback_index as usize].is_some();

            if has_centered && has_lookahead {
                heading = bearing_between(
                    course_series[centered_lookback_index as usize],
                    course_series[lookahead_index],
                );
            } else if has_fallback {
                heading = bearing_between(
                    course_series[fallback_lookback_index as usize],
                    course_series[index],
                );
            }
        }

        if let Some(value) = heading.and_then(|value| round_f64(value, 3)) {
            last_heading = Some(value);
        }
        derived.push(last_heading);
    }

    derived
}

/// Derives the surface distance from the first valid GPS coordinate.
///
/// The home coordinate is selected once from source order. Coordinate ranges
/// were validated by the finalization ingress before this derivation runs.
/// Samples with a missing coordinate remain missing; no synthetic position is
/// introduced for an absent GPS observation.
pub fn derive_distance_to_home_series(
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
pub fn derive_total_ascent_series(
    barometric_altitude: &[Option<f64>],
    elevation: &[Option<f64>],
) -> NumericSeries {
    let source = preferred_elevation_series(barometric_altitude, elevation);
    let smoothed = smooth_elevation_series(source, 2);
    let mut cumulative = 0.0;
    let mut previous_altitude: Option<f64> = None;

    let mut ascent = Vec::with_capacity(smoothed.len());
    for altitude in smoothed {
        let Some(altitude) = altitude.and_then(finite_f64) else {
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

/// Resolves a bearing between two optional course points.
///
/// Keeping the option handling here lets heading derivation stay focused on
/// baseline selection while invalid coordinates simply produce no update.
fn bearing_between(
    from: (Option<f64>, Option<f64>),
    to: (Option<f64>, Option<f64>),
) -> Option<f64> {
    bearing_degrees(from.0?, from.1?, to.0?, to.1?)
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
pub fn derive_calories_from_power(
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
/// the aligned base series, and each metric is combined according to the legacy
/// frontend precedence rules now owned by Rust.
pub fn derive_activity_metric_series(
    course_series: &[(Option<f64>, Option<f64>)],
    distance_series: &NumericSeries,
    elevation_base_series: &NumericSeries,
    elapsed_series: &[f64],
    columns: &ActivityColumns,
) -> BTreeMap<String, MetricDescriptor> {
    let direct = direct_metrics(columns, distance_series, elevation_base_series);
    let null_series: NumericSeries = columns.timestamp.iter().map(|_| None).collect();
    let preferred_altitude =
        preferred_elevation_series(&direct["barometric_altitude"], &direct["elevation"]).to_vec();
    let derived_speed = derive_numeric_rate_series(distance_series, elapsed_series);
    let derived_heading = derive_heading_series(course_series, distance_series, 2.0);
    let derived_distance_to_home = derive_distance_to_home_series(course_series);
    let derived_total_ascent =
        derive_total_ascent_series(&direct["barometric_altitude"], &direct["elevation"]);
    let derived_gradient = derive_gradient_series(&preferred_altitude, distance_series);
    let derived_vertical_speed = derive_numeric_rate_series(&preferred_altitude, elapsed_series);
    let derived_pace = derive_pace_series(
        &direct["speed"]
            .iter()
            .enumerate()
            .map(|(index, value)| value.or(derived_speed[index]))
            .collect(),
    );
    let derived_torque = derive_torque_series(&direct["power"], &direct["cadence"]);

    let mut map = BTreeMap::new();
    macro_rules! insert_metric {
        ($name:literal, $fallback:expr) => {
            map.insert(
                $name.to_string(),
                combine_series(&direct[$name], $fallback, false),
            );
        };
    }

    insert_metric!("air_pressure", &null_series);
    insert_metric!("barometric_altitude", &null_series);
    insert_metric!("cadence", &null_series);
    insert_metric!("core_temperature", &null_series);
    insert_metric!("engine_power", &null_series);
    insert_metric!("engine_load", &null_series);
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
    insert_metric!("elevation", &null_series);
    insert_metric!("brake_position", &null_series);
    insert_metric!("g_force", &null_series);
    insert_metric!("g_force_x", &null_series);
    insert_metric!("g_force_y", &null_series);
    insert_metric!("g_force_z", &null_series);
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
    insert_metric!("ground_contact_time", &null_series);
    map.insert(
        "heading".to_string(),
        if columns.preserve_direct_metric_gaps.heading {
            select_series(&direct["heading"], &derived_heading)
        } else {
            combine_series(&direct["heading"], &derived_heading, false)
        },
    );
    insert_metric!("heartrate", &null_series);
    insert_metric!("left_right_balance", &null_series);
    insert_metric!("lean_angle", &null_series);
    map.insert(
        "pace".to_string(),
        combine_series(&direct["pace"], &derived_pace, false),
    );
    insert_metric!("power", &null_series);
    insert_metric!("rpm", &null_series);
    map.insert(
        "speed".to_string(),
        if columns.preserve_direct_metric_gaps.speed {
            select_series(&direct["speed"], &derived_speed)
        } else {
            combine_series(&direct["speed"], &derived_speed, false)
        },
    );
    insert_metric!("stride_length", &null_series);
    insert_metric!("stroke_rate", &null_series);
    insert_metric!("temperature", &null_series);
    insert_metric!("throttle_position", &null_series);
    map.insert(
        "torque".to_string(),
        combine_series(&direct["torque"], &derived_torque, false),
    );
    insert_metric!("vertical_oscillation", &null_series);
    map.insert(
        "vertical_speed".to_string(),
        combine_series(&direct["vertical_speed"], &derived_vertical_speed, false),
    );
    insert_metric!("iso", &null_series);
    insert_metric!("aperture", &null_series);
    insert_metric!("shutter_speed", &null_series);
    insert_metric!("focal_length", &null_series);
    insert_metric!("ev", &null_series);
    insert_metric!("color_temperature", &null_series);
    map
}

/// Extracts direct metric vectors from normalized raw samples.
///
/// Keeping direct collection table-driven gives derived metrics a consistent
/// field naming surface and isolates the RawSample-to-metric mapping in one
/// place.
fn direct_metrics(
    columns: &ActivityColumns,
    distance_series: &NumericSeries,
    elevation_base_series: &NumericSeries,
) -> BTreeMap<&'static str, NumericSeries> {
    let mut direct = BTreeMap::new();
    macro_rules! collect {
        ($name:literal, $field:ident) => {
            direct.insert(
                $name,
                columns
                    .$field
                    .iter()
                    .map(|value| value.and_then(finite_f64))
                    .collect(),
            );
        };
    }
    collect!("air_pressure", air_pressure);
    collect!("barometric_altitude", barometric_altitude);
    collect!("cadence", cadence);
    collect!("calories", calories);
    collect!("core_temperature", core_temperature);
    direct.insert("distance", distance_series.clone());
    collect!("distance_to_home", distance_to_home);
    direct.insert("elevation", elevation_base_series.clone());
    collect!("brake_position", brake_position);
    collect!("g_force", g_force);
    collect!("g_force_x", g_force_x);
    collect!("g_force_y", g_force_y);
    collect!("g_force_z", g_force_z);
    collect!("gradient", gradient);
    collect!("ground_contact_time", ground_contact_time);
    collect!("heading", heading);
    collect!("heartrate", heartrate);
    collect!("left_right_balance", left_right_balance);
    collect!("lean_angle", lean_angle);
    collect!("pace", pace);
    collect!("power", power);
    collect!("engine_power", engine_power);
    collect!("engine_load", engine_load);
    collect!("rpm", rpm);
    collect!("speed", speed);
    collect!("stride_length", stride_length);
    collect!("stroke_rate", stroke_rate);
    collect!("temperature", temperature);
    collect!("throttle_position", throttle_position);
    collect!("torque", torque);
    collect!("vertical_oscillation", vertical_oscillation);
    collect!("vertical_speed", vertical_speed);
    collect!("iso", iso);
    collect!("aperture", aperture);
    collect!("shutter_speed", shutter_speed);
    collect!("focal_length", focal_length);
    collect!("ev", ev);
    collect!("color_temperature", color_temperature);
    direct
}

#[cfg(test)]
mod tests {
    use super::{derive_distance_to_home_series, derive_total_ascent_series};

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
