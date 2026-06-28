//! Raw-activity finalization into the canonical parsed activity contract.
//!
//! Format-specific extraction stays at the edge of the system and sends
//! normalized [`RawActivity`] samples here. This module owns the shared backend
//! path that was previously duplicated in frontend JavaScript: gap filling,
//! timeline construction, metric derivation, attribute bookkeeping, and final
//! assembly into [`ParsedActivity`].
//!
//! The implementation deliberately keeps MP4 telemetry pre-treatment out of
//! scope. MP4 extraction already performs camera-specific smoothing/culling
//! before this shared seam and will later emit the same raw contract.

pub mod gap;
pub mod metrics;
pub mod smoothing;

use crate::activity::finalize::gap::{
    build_distance_series, build_elapsed_series, build_progress_series, insert_idle_gap_samples,
    skipped_gap_debug,
};
use crate::activity::finalize::metrics::{
    build_metric_coverage, derive_activity_metric_series, MetricDescriptor,
};
use crate::activity::finalize::smoothing::{
    circular_ema, smoothing_window_for_seconds, zero_phase_smooth,
};
use crate::activity::schema::{ParsedActivity, RawActivity, RawSample};
use crate::error::{CoreError, CoreResult};
use crate::media::telemetry_math::{finite_f64, round_f64};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const CORE_ACTIVITY_ATTRIBUTES: &[&str] = &[
    "cadence",
    "course",
    "elevation",
    "gradient",
    "heartrate",
    "power",
    "speed",
    "time",
    "temperature",
];

const EXTENDED_ACTIVITY_ATTRIBUTES: &[&str] = &[
    "air_pressure",
    "altitude",
    "aperture",
    "color_temperature",
    "core_temperature",
    "distance",
    "ev",
    "focal_length",
    "g_force",
    "gear_position",
    "ground_contact_time",
    "heading",
    "iso",
    "left_right_balance",
    "pace",
    "shutter_speed",
    "stroke_rate",
    "stride_length",
    "torque",
    "vertical_oscillation",
    "vertical_speed",
];

#[derive(Clone, Debug, Serialize)]
pub struct FinalizeActivityResponse {
    /// Canonical activity payload consumed by render, trim, and interpolation.
    pub parsed_activity: ParsedActivity,
    /// Dev-only diagnostic payload; release builds return `None` to avoid
    /// unnecessary serialization and accidental debug-file persistence.
    pub debug_payload: Option<Value>,
}

struct FinalizedActivity {
    parsed_activity: ParsedActivity,
    gap_debug: gap::GapDebug,
}

/// Parses the backend raw contract before finalization.
///
/// This keeps Tauri command handlers thin: serde validation and activity-domain
/// error wording live beside the finalizer, while callers still pass plain JSON.
pub fn parse_raw_activity_json(input: &str) -> CoreResult<RawActivity> {
    serde_json::from_str(input)
        .map_err(|error| CoreError::Activity(format!("Invalid RawActivity payload: {error}")))
}

/// Runs the command-facing finalization path from raw JSON to response JSON.
///
/// The parsed activity is always returned, but debug details are gated by
/// `debug_assertions` so production builds do not pay for or expose parser
/// internals.
pub fn finalize_raw_activity_json(input: &str) -> CoreResult<FinalizeActivityResponse> {
    let raw_activity = parse_raw_activity_json(input)?;
    let finalized = finalize_with_debug(&raw_activity);
    let debug_payload = cfg!(debug_assertions).then(|| {
        json!({
            "generated_at": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            "file_name": raw_activity.file_name,
            "file_format": raw_activity.file_format,
            "idle_gap_fill": finalized.gap_debug,
            "parsed_activity": finalized.parsed_activity,
        })
    });

    Ok(FinalizeActivityResponse {
        parsed_activity: finalized.parsed_activity,
        debug_payload,
    })
}

/// Converts normalized extraction samples into the existing ParsedActivity shape.
///
/// The frontend and renderer already agree on `ParsedActivity`, so this function
/// preserves that schema while moving the construction rules into Rust. It
/// applies the shared phases in order: optional idle-gap insertion, aligned
/// time/course/distance series, derived metrics, metadata enrichment, and legacy
/// compatibility fields stored through `ParsedActivity::extra`.
pub fn finalize_parsed_activity(raw_activity: &RawActivity) -> ParsedActivity {
    finalize_with_debug(raw_activity).parsed_activity
}

/// Runs finalization while retaining intermediate diagnostics.
///
/// Public consumers only need `ParsedActivity`, but debug payload generation
/// must preserve gap-fill details from the normalization phase without
/// recomputing the activity or threading debug state through render-facing
/// schemas.
fn finalize_with_debug(raw_activity: &RawActivity) -> FinalizedActivity {
    let (normalized_raw_samples, gap_debug) = if raw_activity.options.skip_idle_gap_fill {
        (raw_activity.raw_samples.clone(), skipped_gap_debug())
    } else {
        insert_idle_gap_samples(&raw_activity.raw_samples)
    };

    let time_series = build_time_series(&normalized_raw_samples);
    let course_series = build_course_series(&normalized_raw_samples);
    let direct_distance_series: Vec<Option<f64>> = normalized_raw_samples
        .iter()
        .map(|sample| sample.distance.and_then(finite_f64))
        .collect();
    let distance_series = build_distance_series(&course_series, &direct_distance_series);
    let elapsed_series = build_elapsed_series(&normalized_raw_samples, &time_series);
    let elevation_base_series: Vec<Option<f64>> = normalized_raw_samples
        .iter()
        .map(|sample| sample.elevation.and_then(finite_f64))
        .collect();
    let mut metric_series_map = derive_activity_metric_series(
        &course_series,
        &distance_series,
        &elevation_base_series,
        &elapsed_series,
        &normalized_raw_samples,
    );
    apply_metric_smoothing(
        &mut metric_series_map,
        &elapsed_series,
        &raw_activity.options.smoothing,
    );

    let valid_attributes = build_valid_attributes(&metric_series_map, &course_series, &time_series);
    let extended_attributes = build_extended_attributes(&metric_series_map);
    let duration_seconds = elapsed_series.last().copied().unwrap_or(0.0);
    let total_distance_meters = distance_series.last().copied().flatten().unwrap_or(0.0);
    let start_time = time_series.iter().find_map(Clone::clone);
    let end_time = time_series.iter().rev().find_map(Clone::clone);
    let coverage = build_metric_coverage(&metric_series_map);
    let distance_progress_series = build_progress_series(&distance_series);

    let mut metadata = raw_activity.metadata.clone();
    if !metadata.is_object() {
        metadata = json!({});
    }
    if let Some(object) = metadata.as_object_mut() {
        object.insert(
            "duration_seconds".to_string(),
            json!(round_f64(duration_seconds, 3).unwrap_or(0.0)),
        );
        object.insert("start_time".to_string(), json!(start_time));
        object.insert("end_time".to_string(), json!(end_time));
        object.insert(
            "total_distance_m".to_string(),
            json!(round_f64(total_distance_meters, 3).unwrap_or(0.0)),
        );
        object.insert(
            "sample_count".to_string(),
            json!(normalized_raw_samples.len()),
        );
        object.insert(
            "original_sample_count".to_string(),
            json!(raw_activity.raw_samples.len()),
        );
        object.insert(
            "inserted_idle_sample_count".to_string(),
            json!(gap_debug.inserted_sample_count),
        );
    }

    let mut extra = BTreeMap::new();
    extra.insert("metric_units".to_string(), metric_units());
    extra.insert("coverage".to_string(), coverage);
    extra.insert("valid_attributes".to_string(), json!(valid_attributes));
    extra.insert(
        "extended_attributes".to_string(),
        json!(extended_attributes),
    );

    let parsed_activity = ParsedActivity {
        file_name: Some(raw_activity.file_name.clone()),
        file_format: Some(raw_activity.file_format.clone()),
        metadata,
        source_start_time: start_time,
        sample_elapsed_seconds: elapsed_series,
        sample_distance_progress: distance_progress_series,
        frame_elapsed_seconds: Vec::new(),
        frame_timestamps: Vec::new(),
        frame_distance_progress: Vec::new(),
        trim_start_seconds: 0.0,
        trim_end_seconds: round_f64(duration_seconds, 3).unwrap_or(0.0),
        sample_course_points: course_series.clone(),
        sample_elevations: metric(&metric_series_map, "elevation"),
        course: course_series,
        elevation: metric(&metric_series_map, "elevation"),
        speed: metric(&metric_series_map, "speed"),
        distance: metric(&metric_series_map, "distance"),
        heartrate: metric(&metric_series_map, "heartrate"),
        cadence: metric(&metric_series_map, "cadence"),
        power: metric(&metric_series_map, "power"),
        temperature: metric(&metric_series_map, "temperature"),
        pace: metric(&metric_series_map, "pace"),
        g_force: metric(&metric_series_map, "g_force"),
        air_pressure: metric(&metric_series_map, "air_pressure"),
        ground_contact_time: metric(&metric_series_map, "ground_contact_time"),
        left_right_balance: metric(&metric_series_map, "left_right_balance"),
        stride_length: metric(&metric_series_map, "stride_length"),
        stroke_rate: metric(&metric_series_map, "stroke_rate"),
        torque: metric(&metric_series_map, "torque"),
        vertical_speed: metric(&metric_series_map, "vertical_speed"),
        altitude: metric(&metric_series_map, "altitude"),
        iso: metric(&metric_series_map, "iso"),
        aperture: metric(&metric_series_map, "aperture"),
        shutter_speed: metric(&metric_series_map, "shutter_speed"),
        focal_length: metric(&metric_series_map, "focal_length"),
        ev: metric(&metric_series_map, "ev"),
        color_temperature: metric(&metric_series_map, "color_temperature"),
        gear_position: metric(&metric_series_map, "gear_position"),
        vertical_ratio: Vec::new(),
        vertical_oscillation: metric(&metric_series_map, "vertical_oscillation"),
        core_temperature: metric(&metric_series_map, "core_temperature"),
        gradient: metric(&metric_series_map, "gradient"),
        time: time_series,
        heading: metric(&metric_series_map, "heading"),
        extra,
    };

    FinalizedActivity {
        parsed_activity,
        gap_debug,
    }
}

/// Builds lat/lon tuples while preserving partial GPS samples.
///
/// Each coordinate is guarded independently so one bad component does not
/// poison the whole vector shape; downstream interpolation can still decide
/// whether enough course data exists to render.
fn build_course_series(raw_samples: &[RawSample]) -> Vec<(Option<f64>, Option<f64>)> {
    raw_samples
        .iter()
        .map(|sample| {
            (
                sample.latitude.and_then(finite_f64),
                sample.longitude.and_then(finite_f64),
            )
        })
        .collect()
}

/// Normalizes source timestamps into UTC millisecond RFC 3339 strings.
///
/// The frontend historically emitted `Date#toISOString()` values. Normalizing
/// here keeps backend-created payloads byte-stable enough for diagnostics and
/// avoids leaking local time-zone formatting into render data.
fn build_time_series(raw_samples: &[RawSample]) -> Vec<Option<String>> {
    raw_samples
        .iter()
        .map(|sample| {
            sample
                .timestamp
                .as_deref()
                .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
                .map(|value| {
                    value
                        .with_timezone(&Utc)
                        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
                })
        })
        .collect()
}

/// Computes the primary attribute list expected by legacy UI consumers.
///
/// Course and time are structural series rather than metric descriptors, so they
/// are checked directly; numeric attributes use the derived metric map to reflect
/// both direct and fallback-derived availability.
fn build_valid_attributes(
    metric_series_map: &BTreeMap<String, MetricDescriptor>,
    course_series: &[(Option<f64>, Option<f64>)],
    time_series: &[Option<String>],
) -> Vec<String> {
    CORE_ACTIVITY_ATTRIBUTES
        .iter()
        .filter(|attribute| {
            let key = **attribute;
            if key == "course" {
                return course_series
                    .iter()
                    .any(|(lat, lon)| lat.is_some() && lon.is_some());
            }
            if key == "time" {
                return time_series.iter().any(Option::is_some);
            }
            metric_series_map
                .get(key)
                .is_some_and(|descriptor| descriptor.series.iter().any(Option::is_some))
        })
        .map(|value| (*value).to_string())
        .collect()
}

/// Computes optional metric attributes from populated finalized series.
///
/// This is intentionally derived after metric combination so an attribute is
/// advertised only when the renderer can actually read a non-null value.
fn build_extended_attributes(
    metric_series_map: &BTreeMap<String, MetricDescriptor>,
) -> Vec<String> {
    EXTENDED_ACTIVITY_ATTRIBUTES
        .iter()
        .filter(|attribute| {
            let key = **attribute;
            metric_series_map[key].series.iter().any(Option::is_some)
        })
        .map(|value| (*value).to_string())
        .collect()
}

/// Clones one finalized metric vector from the descriptor map.
///
/// `ParsedActivity` stores each metric as a named field, while derivation works
/// over a map for uniform combination/coverage logic; this helper keeps that
/// impedance match local to final assembly.
fn metric(metric_series_map: &BTreeMap<String, MetricDescriptor>, name: &str) -> Vec<Option<f64>> {
    metric_series_map
        .get(name)
        .map(|descriptor| descriptor.series.clone())
        .unwrap_or_default()
}

/// Applies parser-requested smoothing after all direct/derived metrics exist.
///
/// Discrete metrics are ignored even if a parser lists them. Heading only uses
/// circular EMA so north-bound wraparound never goes through a linear average.
fn apply_metric_smoothing(
    metric_series_map: &mut BTreeMap<String, MetricDescriptor>,
    elapsed_series: &[f64],
    smoothing_options: &BTreeMap<String, crate::activity::schema::SmoothingOption>,
) {
    if smoothing_options.is_empty() {
        return;
    }

    let discrete_metrics = BTreeSet::from([
        "aperture",
        "color_temperature",
        "ev",
        "focal_length",
        "gear_position",
        "iso",
        "left_right_balance",
        "shutter_speed",
    ]);
    let zero_phase_metrics = BTreeSet::from([
        "elevation",
        "speed",
        "vertical_speed",
        "g_force",
        "gradient",
        "pace",
    ]);
    let sample_timestamps_ms: Vec<_> = elapsed_series
        .iter()
        .map(|seconds| seconds * 1000.0)
        .collect();

    for (metric_name, option) in smoothing_options {
        if !option.enabled || discrete_metrics.contains(metric_name.as_str()) {
            continue;
        }

        let Some(descriptor) = metric_series_map.get_mut(metric_name) else {
            continue;
        };

        match option.method.as_str() {
            "circular_ema" if metric_name == "heading" => {
                descriptor.series = circular_ema(&descriptor.series);
            }
            "zero_phase_ma" if zero_phase_metrics.contains(metric_name.as_str()) => {
                let window =
                    smoothing_window_for_seconds(&sample_timestamps_ms, option.window_seconds);
                descriptor.series = zero_phase_smooth(&descriptor.series, window);
            }
            _ => {}
        }
    }
}

/// Returns frontend-compatible units for every finalized metric.
///
/// Units stay backend-owned with the finalizer so raw extraction code only needs
/// to emit values in canonical units, not duplicate UI catalog metadata.
fn metric_units() -> Value {
    json!({
        "air_pressure": "bar",
        "altitude": "m",
        "aperture": "fnum",
        "cadence": "rpm",
        "color_temperature": "kelvin",
        "core_temperature": "celsius",
        "distance": "m",
        "elevation": "m",
        "ev": "ev",
        "focal_length": "mm",
        "g_force": "g",
        "gear_position": "raw",
        "gradient": "percent",
        "ground_contact_time": "ms",
        "heading": "degrees",
        "heartrate": "bpm",
        "iso": "iso",
        "left_right_balance": "raw",
        "pace": "seconds_per_km",
        "power": "watts",
        "shutter_speed": "seconds",
        "speed": "mps",
        "stride_length": "raw",
        "stroke_rate": "strokes_per_minute",
        "temperature": "celsius",
        "torque": "nm",
        "vertical_oscillation": "raw",
        "vertical_speed": "mps",
    })
}
