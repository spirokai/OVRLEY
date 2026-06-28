//! Idle-gap filling and base activity timeline construction.
//!
//! Device files often skip samples while stationary, but overlay widgets expect
//! a continuous elapsed timeline. This module inserts zero-valued idle samples
//! only for stationary gaps, then builds the shared distance, elapsed, and
//! progress series used by metric derivation and rendering.
//!
//! The algorithms mirror the frontend migration source closely so Phase 0 can
//! move ownership without changing importer behavior.

use crate::activity::schema::RawSample;
use crate::media::telemetry_math::{finite_f64, haversine_distance, round_f64};
use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct GapDebug {
    /// Individual stationary gaps where synthetic idle samples were inserted.
    pub detected_gaps: Vec<DetectedGap>,
    /// Total number of synthetic samples added to the raw stream.
    pub inserted_sample_count: usize,
    /// Cadence inferred from short positive elapsed/timestamp deltas.
    pub recording_interval_seconds: f64,
    /// Gap size required before insertion is considered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap_threshold_seconds: Option<f64>,
    /// Maximum movement allowed for a gap to count as stationary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stationary_distance_threshold_m: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DetectedGap {
    /// Raw sample index before the detected gap.
    pub start_index: usize,
    /// Raw sample index after the detected gap.
    pub end_index: usize,
    /// Elapsed time at the gap start after explicit/timestamp fallback.
    pub start_elapsed_seconds: Option<f64>,
    /// Elapsed time at the gap end after explicit/timestamp fallback.
    pub end_elapsed_seconds: Option<f64>,
    /// Gap duration that triggered insertion.
    pub gap_seconds: Option<f64>,
    /// Number of synthetic samples inserted between start and end.
    pub inserted_samples: usize,
    /// Source timestamp at the gap start, normalized for debug output.
    pub start_timestamp: Option<String>,
    /// Source timestamp at the gap end, normalized for debug output.
    pub end_timestamp: Option<String>,
}

/// Builds the debug shape for formats that explicitly skip idle filling.
///
/// SRT and future pre-treated formats still need metadata parity with filled
/// paths, so the caller receives a real debug object with zero insertions rather
/// than special-casing `None`.
pub fn skipped_gap_debug() -> GapDebug {
    GapDebug {
        detected_gaps: Vec::new(),
        inserted_sample_count: 0,
        recording_interval_seconds: 1.0,
        gap_threshold_seconds: None,
        stationary_distance_threshold_m: None,
    }
}

/// Converts an optional RFC 3339 timestamp to epoch milliseconds.
///
/// Gap detection compares timestamps numerically and ignores malformed values
/// instead of failing the import, matching the tolerant frontend parser.
fn timestamp_ms(value: &Option<String>) -> Option<i64> {
    value
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc).timestamp_millis())
}

/// Normalizes a timestamp for diagnostic payloads.
///
/// Debug output should be stable regardless of the input offset spelling, so
/// accepted timestamps are re-emitted as UTC with millisecond precision.
fn safe_timestamp(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| {
            value
                .with_timezone(&Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        })
}

/// Finds the median using a sorted copy.
///
/// Cadence estimation needs a robust center value, and copying keeps callers
/// free to reuse their original delta ordering for diagnostics if needed.
fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.total_cmp(right));
    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        Some((sorted[middle - 1] + sorted[middle]) / 2.0)
    } else {
        Some(sorted[middle])
    }
}

/// Uses the lower half of deltas to avoid long pauses inflating cadence.
///
/// Activity files commonly include real movement gaps; biasing toward shorter
/// intervals estimates recording cadence rather than stop duration.
fn lower_half_median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.total_cmp(right));
    let cutoff = sorted.len().div_ceil(2).max(1);
    median(&sorted[..cutoff])
}

/// Estimates source recording cadence from elapsed and timestamp deltas.
///
/// Only short positive deltas are considered so idle pauses and clock glitches
/// do not make synthetic sample spacing too coarse.
pub fn estimate_recording_interval_seconds(raw_samples: &[RawSample]) -> f64 {
    let mut deltas = Vec::new();
    let mut previous_elapsed = None;
    let mut previous_timestamp_ms = None;

    for sample in raw_samples {
        if let Some(elapsed) = sample.elapsed_seconds.and_then(finite_f64) {
            if let Some(previous) = previous_elapsed {
                let delta = elapsed - previous;
                if delta > 0.0 && delta <= 10.0 {
                    deltas.push(delta);
                }
            }
            previous_elapsed = Some(elapsed);
        }

        if let Some(current) = timestamp_ms(&sample.timestamp) {
            if let Some(previous) = previous_timestamp_ms {
                let delta = (current - previous) as f64 / 1000.0;
                if delta > 0.0 && delta <= 10.0 {
                    deltas.push(delta);
                }
            }
            previous_timestamp_ms = Some(current);
        }
    }

    lower_half_median(&deltas).unwrap_or(1.0)
}

/// Resolves a sample's elapsed seconds with timestamp fallback.
///
/// FIT/SRT can provide explicit elapsed time, while GPX commonly only has wall
/// clock timestamps; using a shared fallback lets gap filling work for both
/// shapes without format-specific branches.
fn elapsed_seconds_for_sample(
    sample: &RawSample,
    fallback_origin_timestamp_ms: Option<i64>,
) -> Option<f64> {
    if let Some(explicit) = sample.elapsed_seconds.and_then(finite_f64) {
        return Some(explicit);
    }
    let time_ms = timestamp_ms(&sample.timestamp)?;
    let origin = fallback_origin_timestamp_ms?;
    Some((time_ms - origin) as f64 / 1000.0)
}

/// Computes movement across two samples for stationary-gap detection.
///
/// Direct cumulative distance is preferred when present because it reflects the
/// parser's source-of-truth units; otherwise GPS haversine distance provides a
/// conservative fallback.
fn distance_meters_for_pair(previous: &RawSample, current: &RawSample) -> f64 {
    if let (Some(previous_distance), Some(current_distance)) = (
        previous.distance.and_then(finite_f64),
        current.distance.and_then(finite_f64),
    ) {
        return (current_distance - previous_distance).max(0.0);
    }

    match (
        previous.latitude.and_then(finite_f64),
        previous.longitude.and_then(finite_f64),
        current.latitude.and_then(finite_f64),
        current.longitude.and_then(finite_f64),
    ) {
        (Some(lat1), Some(lon1), Some(lat2), Some(lon2)) => {
            haversine_distance(lat1, lon1, lat2, lon2)
        }
        _ => 0.0,
    }
}

/// Creates a synthetic zero-output sample inside an idle gap.
///
/// The sample clones contextual fields from the preceding real sample, then
/// clears derived/discrete motion-sensitive fields so widgets show stationary
/// state instead of stretching the previous effort value across the pause.
fn zero_filled_idle_sample(
    sample: &RawSample,
    elapsed_seconds: f64,
    timestamp_ms_value: Option<i64>,
) -> RawSample {
    let mut synthetic = sample.clone();
    synthetic.elapsed_seconds = round_f64(elapsed_seconds, 3);
    synthetic.timestamp = timestamp_ms_value
        .and_then(DateTime::<Utc>::from_timestamp_millis)
        .map(|value| value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
    synthetic.speed = Some(0.0);
    synthetic.cadence = Some(0.0);
    synthetic.power = Some(0.0);
    synthetic.stroke_rate = Some(0.0);
    synthetic.vertical_speed = Some(0.0);
    synthetic.g_force = Some(0.0);
    synthetic.gradient = Some(0.0);
    synthetic.pace = None;
    synthetic.torque = None;
    synthetic.stride_length = None;
    synthetic.ground_contact_time = None;
    synthetic.vertical_oscillation = None;
    synthetic.synthetic_idle = true;
    synthetic
}

/// Inserts synthetic samples for stationary recording gaps.
///
/// A gap is filled only when elapsed time exceeds the cadence-based threshold
/// and physical movement stays below the stationary threshold. This preserves
/// real travel gaps while preventing stopped periods from collapsing to one
/// long interpolation jump.
pub fn insert_idle_gap_samples(raw_samples: &[RawSample]) -> (Vec<RawSample>, GapDebug) {
    if raw_samples.len() < 2 {
        return (
            raw_samples.to_vec(),
            GapDebug {
                detected_gaps: Vec::new(),
                inserted_sample_count: 0,
                recording_interval_seconds: 1.0,
                gap_threshold_seconds: None,
                stationary_distance_threshold_m: None,
            },
        );
    }

    let origin_timestamp_ms = raw_samples
        .iter()
        .find_map(|sample| timestamp_ms(&sample.timestamp));
    let recording_interval_seconds = estimate_recording_interval_seconds(raw_samples).max(0.2);
    let gap_threshold_seconds = 3.0_f64.max(recording_interval_seconds * 3.0);
    let stationary_distance_threshold_m = 5.0_f64.max(recording_interval_seconds * 2.5);
    let mut detected_gaps = Vec::new();
    let mut filled_samples = vec![raw_samples[0].clone()];
    let mut inserted_sample_count = 0;

    for index in 1..raw_samples.len() {
        let previous = &raw_samples[index - 1];
        let current = &raw_samples[index];
        let previous_elapsed = elapsed_seconds_for_sample(previous, origin_timestamp_ms);
        let current_elapsed = elapsed_seconds_for_sample(current, origin_timestamp_ms);
        let elapsed_delta = match (previous_elapsed, current_elapsed) {
            (Some(previous), Some(current)) => Some(current - previous),
            _ => None,
        };

        let mut inserted_for_gap = 0;
        if elapsed_delta.is_some_and(|delta| delta > gap_threshold_seconds) {
            let distance_delta = distance_meters_for_pair(previous, current);
            if distance_delta <= stationary_distance_threshold_m {
                let previous_timestamp_ms = timestamp_ms(&previous.timestamp);
                let current_timestamp_ms = timestamp_ms(&current.timestamp);
                let max_insertion_count =
                    (elapsed_delta.unwrap() / recording_interval_seconds).floor() as usize - 1;

                for insert_index in 1..=max_insertion_count {
                    let synthetic_elapsed = previous_elapsed.unwrap()
                        + recording_interval_seconds * insert_index as f64;
                    if synthetic_elapsed >= current_elapsed.unwrap() - 1e-6 {
                        break;
                    }

                    let synthetic_timestamp_ms = match (previous_timestamp_ms, current_timestamp_ms)
                    {
                        (Some(previous_ms), Some(current_ms)) if current_ms > previous_ms => Some(
                            current_ms.min(
                                previous_ms
                                    + (recording_interval_seconds * 1000.0 * insert_index as f64)
                                        .round() as i64,
                            ),
                        ),
                        _ => None,
                    };

                    filled_samples.push(zero_filled_idle_sample(
                        previous,
                        synthetic_elapsed,
                        synthetic_timestamp_ms,
                    ));
                    inserted_for_gap += 1;
                }
            }
        }

        if inserted_for_gap > 0 {
            detected_gaps.push(DetectedGap {
                start_index: index - 1,
                end_index: index,
                start_elapsed_seconds: previous_elapsed.and_then(|value| round_f64(value, 3)),
                end_elapsed_seconds: current_elapsed.and_then(|value| round_f64(value, 3)),
                gap_seconds: elapsed_delta.and_then(|value| round_f64(value, 3)),
                inserted_samples: inserted_for_gap,
                start_timestamp: safe_timestamp(&previous.timestamp),
                end_timestamp: safe_timestamp(&current.timestamp),
            });
            inserted_sample_count += inserted_for_gap;
        }

        filled_samples.push(current.clone());
    }

    (
        filled_samples,
        GapDebug {
            detected_gaps,
            inserted_sample_count,
            recording_interval_seconds: round_f64(recording_interval_seconds, 3).unwrap_or(1.0),
            gap_threshold_seconds: round_f64(gap_threshold_seconds, 3),
            stationary_distance_threshold_m: round_f64(stationary_distance_threshold_m, 3),
        },
    )
}

/// Builds cumulative distance aligned to course samples.
///
/// Direct distance values remain authoritative when present and monotonic; GPS
/// segment distance fills the gaps so every sample has a usable progress base.
pub fn build_distance_series(
    course_points: &[(Option<f64>, Option<f64>)],
    direct_distance_series: &[Option<f64>],
) -> Vec<Option<f64>> {
    let mut distance_series = Vec::with_capacity(course_points.len());
    let mut total_distance_meters: f64 = 0.0;

    for index in 0..course_points.len() {
        if let Some(direct_distance) = direct_distance_series
            .get(index)
            .copied()
            .flatten()
            .and_then(finite_f64)
        {
            total_distance_meters = total_distance_meters.max(direct_distance);
            distance_series.push(round_f64(total_distance_meters, 3));
            continue;
        }

        if index > 0 {
            if let (Some(lat1), Some(lon1), Some(lat2), Some(lon2)) = (
                course_points[index - 1].0,
                course_points[index - 1].1,
                course_points[index].0,
                course_points[index].1,
            ) {
                total_distance_meters += haversine_distance(lat1, lon1, lat2, lon2);
            }
        }
        distance_series.push(round_f64(total_distance_meters, 3));
    }

    distance_series
}

/// Builds a monotonic elapsed-time series for every sample.
///
/// Explicit elapsed values win, timestamp deltas fill holes, and tiny increments
/// break duplicate timestamps so interpolation always has a forward-moving axis.
pub fn build_elapsed_series(raw_samples: &[RawSample], time_series: &[Option<String>]) -> Vec<f64> {
    let explicit_elapsed: Vec<Option<f64>> = raw_samples
        .iter()
        .map(|sample| sample.elapsed_seconds.and_then(finite_f64))
        .collect();
    let has_explicit_elapsed = explicit_elapsed.iter().any(Option::is_some);
    let valid_timestamps: Vec<Option<DateTime<Utc>>> = time_series
        .iter()
        .map(|value| {
            value
                .as_deref()
                .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
                .map(|value| value.with_timezone(&Utc))
        })
        .collect();
    let origin = valid_timestamps.iter().flatten().next().copied();

    if has_explicit_elapsed {
        let mut elapsed_series = Vec::with_capacity(raw_samples.len());
        let mut last_value: f64 = 0.0;
        for index in 0..explicit_elapsed.len() {
            if let Some(current) = explicit_elapsed[index] {
                last_value = last_value.max(current);
                elapsed_series.push(round_f64(last_value, 3).unwrap_or(0.0));
                continue;
            }
            if let (Some(origin), Some(timestamp)) = (origin, valid_timestamps[index]) {
                let computed = ((timestamp - origin).num_milliseconds() as f64 / 1000.0).max(0.0);
                last_value = last_value.max(computed);
                elapsed_series.push(round_f64(last_value, 3).unwrap_or(0.0));
                continue;
            }
            if index == 0 {
                elapsed_series.push(0.0);
            } else {
                last_value = elapsed_series[index - 1];
                elapsed_series.push(round_f64(last_value, 3).unwrap_or(0.0));
            }
        }
        return elapsed_series;
    }

    let Some(origin) = origin else {
        return raw_samples
            .iter()
            .enumerate()
            .map(|(index, _)| round_f64(index as f64, 3).unwrap_or(index as f64))
            .collect();
    };

    let mut last_value: f64 = 0.0;
    valid_timestamps
        .iter()
        .enumerate()
        .map(|(index, timestamp)| {
            let Some(timestamp) = timestamp else {
                return round_f64(last_value, 3).unwrap_or(last_value);
            };
            let next_value = ((*timestamp - origin).num_milliseconds() as f64 / 1000.0).max(0.0);
            if next_value <= last_value && index > 0 {
                last_value += 0.001;
                return round_f64(last_value, 3).unwrap_or(last_value);
            }
            last_value = next_value;
            round_f64(next_value, 3).unwrap_or(next_value)
        })
        .collect()
}

/// Normalizes cumulative distance into full-activity progress.
///
/// Route and elevation widgets consume progress as `0..1`; zero-distance
/// activities return zeros so callers never divide by an unusable total.
pub fn build_progress_series(distance_series: &[Option<f64>]) -> Vec<f64> {
    let total_distance_meters = distance_series.last().copied().flatten().unwrap_or(0.0);
    if !total_distance_meters.is_finite() || total_distance_meters <= 0.0 {
        return distance_series.iter().map(|_| 0.0).collect();
    }
    distance_series
        .iter()
        .map(|value| round_f64(value.unwrap_or(0.0) / total_distance_meters, 6).unwrap_or(0.0))
        .collect()
}
