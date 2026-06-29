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

use crate::activity::schema::{ActivityColumns, NumericSeries};
use crate::media::telemetry_math::{bearing_degrees, finite_f64, round_f64};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct MetricDescriptor {
    /// Final metric values aligned with raw sample order.
    pub series: NumericSeries,
    /// Provenance summary used by parser coverage diagnostics.
    pub source: MetricSource,
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

/// Builds frontend-compatible coverage JSON from finalized descriptors.
///
/// Coverage is calculated after direct/derived combination so it reports what
/// widgets can actually consume, not just what the source file happened to
/// include.
pub fn build_metric_coverage(
    metric_series_map: &BTreeMap<String, MetricDescriptor>,
) -> serde_json::Value {
    let mut coverage = serde_json::Map::new();
    for (metric, descriptor) in metric_series_map {
        let available_count = descriptor
            .series
            .iter()
            .filter(|value| value.is_some())
            .count();
        coverage.insert(
            metric.clone(),
            serde_json::json!({
                "source": descriptor.source,
                "availableCount": available_count,
                "totalSamples": descriptor.series.len(),
            }),
        );
    }
    serde_json::Value::Object(coverage)
}

/// Smooths elevation locally before gradient derivation.
///
/// Gradient amplifies meter-level altitude noise, so the standard derivation
/// averages neighboring elevation samples before computing slope over distance.
fn smooth_elevation_series(elevation_series: &NumericSeries, radius: usize) -> NumericSeries {
    elevation_series
        .iter()
        .enumerate()
        .map(|(index, value)| {
            value.and_then(finite_f64)?;
            let start = index.saturating_sub(radius);
            let end = (index + radius).min(elevation_series.len().saturating_sub(1));
            let mut total = 0.0;
            let mut count = 0.0;
            for neighbor in elevation_series.iter().take(end + 1).skip(start) {
                if let Some(neighbor_value) = neighbor.and_then(finite_f64) {
                    total += neighbor_value;
                    count += 1.0;
                }
            }
            (count > 0.0).then(|| round_f64(total / count, 3)).flatten()
        })
        .collect()
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
        series: combined,
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
    let derived_speed = derive_numeric_rate_series(distance_series, elapsed_series);
    let derived_heading = derive_heading_series(course_series, distance_series, 2.0);
    let derived_gradient = derive_gradient_series(&direct["elevation"], distance_series);
    let derived_vertical_speed = derive_numeric_rate_series(&direct["elevation"], elapsed_series);
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
    insert_metric!("altitude", &direct["elevation"]);
    insert_metric!("cadence", &null_series);
    insert_metric!("core_temperature", &null_series);
    map.insert(
        "distance".to_string(),
        MetricDescriptor {
            series: direct["distance"].clone(),
            source: MetricSource::Direct,
        },
    );
    insert_metric!("elevation", &null_series);
    insert_metric!("g_force", &null_series);
    insert_metric!("gear_position", &null_series);
    map.insert(
        "gradient".to_string(),
        combine_series(&derived_gradient, &direct["gradient"], true),
    );
    insert_metric!("ground_contact_time", &null_series);
    insert_metric!("heading", &derived_heading);
    insert_metric!("heartrate", &null_series);
    insert_metric!("left_right_balance", &null_series);
    map.insert(
        "pace".to_string(),
        combine_series(&direct["pace"], &derived_pace, false),
    );
    insert_metric!("power", &null_series);
    map.insert(
        "speed".to_string(),
        combine_series(&direct["speed"], &derived_speed, false),
    );
    insert_metric!("stride_length", &null_series);
    insert_metric!("stroke_rate", &null_series);
    insert_metric!("temperature", &null_series);
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
    collect!("altitude", altitude);
    collect!("cadence", cadence);
    collect!("core_temperature", core_temperature);
    direct.insert("distance", distance_series.clone());
    direct.insert("elevation", elevation_base_series.clone());
    collect!("g_force", g_force);
    collect!("gear_position", gear_position);
    collect!("gradient", gradient);
    collect!("ground_contact_time", ground_contact_time);
    collect!("heading", heading);
    collect!("heartrate", heartrate);
    collect!("left_right_balance", left_right_balance);
    collect!("pace", pace);
    collect!("power", power);
    collect!("speed", speed);
    collect!("stride_length", stride_length);
    collect!("stroke_rate", stroke_rate);
    collect!("temperature", temperature);
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
