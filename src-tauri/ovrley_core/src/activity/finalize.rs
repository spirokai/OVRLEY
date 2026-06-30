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
    build_distance_series, build_progress_series, insert_idle_gap_samples, skipped_gap_debug,
};
use crate::activity::finalize::metrics::{
    build_metric_coverage, derive_activity_metric_series, MetricDescriptor,
};
use crate::activity::finalize::smoothing::{
    circular_ema, smoothing_window_for_seconds, zero_phase_smooth,
};
use crate::activity::schema::{ActivityColumns, ParsedActivity, RawActivity, RawSample};
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

impl FinalizedActivity {
    fn into_response(
        self,
        file_name: Option<&str>,
        file_format: Option<&str>,
        repo_root: Option<&std::path::Path>,
    ) -> FinalizeActivityResponse {
        let debug_payload = build_debug_payload(
            file_name,
            file_format,
            &self.gap_debug,
            &self.parsed_activity,
        );
        if let (Some(root), Some(payload)) = (repo_root, &debug_payload) {
            write_activity_debug_file(root, Some(file_name.unwrap_or("activity")), payload);
        }
        FinalizeActivityResponse {
            parsed_activity: self.parsed_activity,
            debug_payload,
        }
    }
}

/// Parses the backend raw contract before finalization.
///
/// This keeps Tauri command handlers thin: serde validation and activity-domain
/// error wording live beside the finalizer, while callers still pass plain JSON.
pub fn parse_raw_activity_json(input: &str) -> CoreResult<RawActivity> {
    serde_json::from_str(input)
        .map_err(|error| CoreError::Activity(format!("Invalid RawActivity payload: {error}")))
}

fn build_debug_payload(
    file_name: Option<&str>,
    file_format: Option<&str>,
    gap_debug: &gap::GapDebug,
    parsed_activity: &ParsedActivity,
) -> Option<Value> {
    cfg!(debug_assertions).then(|| {
        json!({
            "generated_at": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            "file_name": file_name,
            "file_format": file_format,
            "idle_gap_fill": gap_debug,
            "parsed_activity": parsed_activity,
        })
    })
}

/// Runs the command-facing finalization path from raw JSON to response JSON.
///
/// The parsed activity is always returned, but debug details are gated by
/// `debug_assertions` so production builds do not pay for or expose parser
/// internals. When `repo_root` is provided, writes the debug file to disk.
pub fn finalize_raw_activity_json(
    input: &str,
    repo_root: Option<&std::path::Path>,
) -> CoreResult<FinalizeActivityResponse> {
    let raw_activity = parse_raw_activity_json(input)?;
    let finalized = finalize_raw_activity_with_debug(&raw_activity)?;
    Ok(finalized.into_response(
        Some(&raw_activity.file_name),
        Some(&raw_activity.file_format),
        repo_root,
    ))
}

/// Finalizes columnar activity input and returns full response with debug payload.
///
/// Both FIT/GPX/SRT and MP4 telemetry paths go through this to produce
/// consistent debug output in dev builds. When `repo_root` is provided,
/// writes the debug file to disk.
pub fn finalize_activity_columns(
    columns: &ActivityColumns,
    repo_root: Option<&std::path::Path>,
) -> CoreResult<FinalizeActivityResponse> {
    let finalized = finalize_columns_with_debug(columns, skipped_gap_debug())?;
    Ok(finalized.into_response(
        Some(&columns.file_name),
        Some(&columns.file_format),
        repo_root,
    ))
}

/// Runs finalization while retaining intermediate diagnostics.
///
/// Public consumers only need `ParsedActivity`, but debug payload generation
/// must preserve gap-fill details from the normalization phase without
/// recomputing the activity or threading debug state through render-facing
/// schemas.
fn finalize_raw_activity_with_debug(
    raw_activity: &RawActivity,
) -> CoreResult<FinalizedActivity> {
    let (normalized_raw_samples, gap_debug) = if raw_activity.options.skip_idle_gap_fill {
        (raw_activity.raw_samples.clone(), skipped_gap_debug())
    } else {
        insert_idle_gap_samples(&raw_activity.raw_samples)
    };
    let columns = activity_columns_from_samples(
        raw_activity,
        normalized_raw_samples,
        raw_activity.raw_samples.len(),
    );
    finalize_columns_with_debug(&columns, gap_debug)
}

fn finalize_columns_with_debug(
    columns: &ActivityColumns,
    gap_debug: gap::GapDebug,
) -> CoreResult<FinalizedActivity> {
    validate_column_lengths(columns)?;

    let time_series = build_time_series(columns);
    let course_series = build_course_series(columns);
    let direct_distance_series: Vec<Option<f64>> = columns
        .distance
        .iter()
        .map(|value| value.and_then(finite_f64))
        .collect();
    let distance_series = build_distance_series(&course_series, &direct_distance_series);
    let elapsed_series = build_elapsed_series(columns, &time_series);
    let elevation_base_series: Vec<Option<f64>> = columns
        .elevation
        .iter()
        .map(|value| value.and_then(finite_f64))
        .collect();
    let mut metric_series_map = derive_activity_metric_series(
        &course_series,
        &distance_series,
        &elevation_base_series,
        &elapsed_series,
        columns,
    );
    apply_metric_smoothing(
        &mut metric_series_map,
        &elapsed_series,
        &columns.options.smoothing,
    );

    let valid_attributes = build_valid_attributes(&metric_series_map, &course_series, &time_series);
    let extended_attributes = build_extended_attributes(&metric_series_map);
    let duration_seconds = elapsed_series.last().copied().unwrap_or(0.0);
    let total_distance_meters = distance_series.last().copied().flatten().unwrap_or(0.0);
    let first_sample_time = time_series.iter().find_map(Clone::clone);
    let sync_time = columns
        .metadata
        .get("sync_time")
        .and_then(|value| value.as_str())
        .filter(|value| DateTime::parse_from_rfc3339(value).is_ok())
        .map(ToOwned::to_owned)
        .or(first_sample_time);
    let end_time = time_series.iter().rev().find_map(Clone::clone);
    let coverage = build_metric_coverage(&metric_series_map);
    let distance_progress_series = build_progress_series(&distance_series);

    let mut metadata = columns.metadata.clone();
    if !metadata.is_object() {
        metadata = json!({});
    }
    if let Some(object) = metadata.as_object_mut() {
        object.remove("start_time");
        object.insert("sync_time".to_string(), json!(sync_time));
        object.insert(
            "duration_seconds".to_string(),
            json!(round_f64(duration_seconds, 3).unwrap_or(0.0)),
        );
        object.insert("end_time".to_string(), json!(end_time));
        object.insert(
            "total_distance_m".to_string(),
            json!(round_f64(total_distance_meters, 3).unwrap_or(0.0)),
        );
        object.insert("sample_count".to_string(), json!(columns.len()));
        object.insert(
            "original_sample_count".to_string(),
            json!(columns.original_sample_count),
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
        file_name: Some(columns.file_name.clone()),
        file_format: Some(columns.file_format.clone()),
        metadata,
        sync_time,
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

    Ok(FinalizedActivity {
        parsed_activity,
        gap_debug,
    })
}

fn activity_columns_from_samples(
    raw_activity: &RawActivity,
    raw_samples: Vec<RawSample>,
    original_sample_count: usize,
) -> ActivityColumns {
    macro_rules! collect {
        ($field:ident) => {
            raw_samples
                .iter()
                .map(|sample| sample.$field.and_then(finite_f64))
                .collect()
        };
    }

    ActivityColumns {
        file_name: raw_activity.file_name.clone(),
        file_format: raw_activity.file_format.clone(),
        metadata: raw_activity.metadata.clone(),
        options: raw_activity.options.clone(),
        timestamp: raw_samples
            .iter()
            .map(|sample| sample.timestamp.clone())
            .collect(),
        elapsed_seconds: collect!(elapsed_seconds),
        latitude: collect!(latitude),
        longitude: collect!(longitude),
        elevation: collect!(elevation),
        altitude: collect!(altitude),
        speed: collect!(speed),
        heading: collect!(heading),
        heartrate: collect!(heartrate),
        cadence: collect!(cadence),
        power: collect!(power),
        temperature: collect!(temperature),
        gradient: collect!(gradient),
        pace: collect!(pace),
        distance: collect!(distance),
        g_force: collect!(g_force),
        vertical_speed: collect!(vertical_speed),
        torque: collect!(torque),
        stroke_rate: collect!(stroke_rate),
        stride_length: collect!(stride_length),
        vertical_oscillation: collect!(vertical_oscillation),
        ground_contact_time: collect!(ground_contact_time),
        left_right_balance: collect!(left_right_balance),
        core_temperature: collect!(core_temperature),
        air_pressure: collect!(air_pressure),
        gear_position: collect!(gear_position),
        iso: collect!(iso),
        aperture: collect!(aperture),
        shutter_speed: collect!(shutter_speed),
        focal_length: collect!(focal_length),
        ev: collect!(ev),
        color_temperature: collect!(color_temperature),
        original_sample_count,
    }
}

impl ActivityColumns {
    pub fn len(&self) -> usize {
        self.timestamp.len()
    }
}

fn validate_column_lengths(columns: &ActivityColumns) -> CoreResult<()> {
    let expected = columns.timestamp.len();
    let lengths = [
        ("elapsed_seconds", columns.elapsed_seconds.len()),
        ("latitude", columns.latitude.len()),
        ("longitude", columns.longitude.len()),
        ("elevation", columns.elevation.len()),
        ("altitude", columns.altitude.len()),
        ("speed", columns.speed.len()),
        ("heading", columns.heading.len()),
        ("heartrate", columns.heartrate.len()),
        ("cadence", columns.cadence.len()),
        ("power", columns.power.len()),
        ("temperature", columns.temperature.len()),
        ("gradient", columns.gradient.len()),
        ("pace", columns.pace.len()),
        ("distance", columns.distance.len()),
        ("g_force", columns.g_force.len()),
        ("vertical_speed", columns.vertical_speed.len()),
        ("torque", columns.torque.len()),
        ("stroke_rate", columns.stroke_rate.len()),
        ("stride_length", columns.stride_length.len()),
        ("vertical_oscillation", columns.vertical_oscillation.len()),
        ("ground_contact_time", columns.ground_contact_time.len()),
        ("left_right_balance", columns.left_right_balance.len()),
        ("core_temperature", columns.core_temperature.len()),
        ("air_pressure", columns.air_pressure.len()),
        ("gear_position", columns.gear_position.len()),
        ("iso", columns.iso.len()),
        ("aperture", columns.aperture.len()),
        ("shutter_speed", columns.shutter_speed.len()),
        ("focal_length", columns.focal_length.len()),
        ("ev", columns.ev.len()),
        ("color_temperature", columns.color_temperature.len()),
    ];

    for (name, len) in lengths {
        if len != expected {
            return Err(CoreError::Activity(format!(
                "Invalid ActivityColumns payload: {name} length {len} does not match timestamp length {expected}"
            )));
        }
    }
    Ok(())
}

/// Builds lat/lon tuples while preserving partial GPS samples.
///
/// Each coordinate is guarded independently so one bad component does not
/// poison the whole vector shape; downstream interpolation can still decide
/// whether enough course data exists to render.
fn build_course_series(columns: &ActivityColumns) -> Vec<(Option<f64>, Option<f64>)> {
    columns
        .latitude
        .iter()
        .zip(&columns.longitude)
        .map(|(latitude, longitude)| {
            (
                latitude.and_then(finite_f64),
                longitude.and_then(finite_f64),
            )
        })
        .collect()
}

/// Normalizes source timestamps into UTC millisecond RFC 3339 strings.
///
/// The frontend historically emitted `Date#toISOString()` values. Normalizing
/// here keeps backend-created payloads byte-stable enough for diagnostics and
/// avoids leaking local time-zone formatting into render data.
fn build_time_series(columns: &ActivityColumns) -> Vec<Option<String>> {
    columns
        .timestamp
        .iter()
        .map(|timestamp| {
            timestamp
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

fn build_elapsed_series(columns: &ActivityColumns, time_series: &[Option<String>]) -> Vec<f64> {
    let explicit_elapsed: Vec<Option<f64>> = columns
        .elapsed_seconds
        .iter()
        .map(|value| value.and_then(finite_f64))
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
        let mut elapsed_series = Vec::with_capacity(columns.len());
        let mut last_value: f64 = 0.0;
        let preserve_precision = columns.file_format == "mp4_telemetry";
        for index in 0..explicit_elapsed.len() {
            if let Some(current) = explicit_elapsed[index] {
                last_value = last_value.max(current);
                elapsed_series.push(if preserve_precision {
                    last_value
                } else {
                    round_f64(last_value, 3).unwrap_or(0.0)
                });
                continue;
            }
            if let (Some(origin), Some(timestamp)) = (origin, valid_timestamps[index]) {
                let computed = ((timestamp - origin).num_milliseconds() as f64 / 1000.0).max(0.0);
                last_value = last_value.max(computed);
                elapsed_series.push(if preserve_precision {
                    last_value
                } else {
                    round_f64(last_value, 3).unwrap_or(0.0)
                });
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
        return (0..columns.len())
            .map(|index| round_f64(index as f64, 3).unwrap_or(index as f64))
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
    let series = metric_series_map
        .get(name)
        .map(|descriptor| descriptor.series.clone())
        .unwrap_or_default();
    strip_all_none(series)
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

/// Writes debug payload to `debug/activities/` in dev mode.
pub fn write_activity_debug_file(
    repo_root: &std::path::Path,
    activity_filename: Option<&str>,
    debug_payload: &Value,
) {
    if !cfg!(debug_assertions) {
        return;
    }
    let name = activity_filename.unwrap_or("activity");
    let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
    let path = repo_root
        .join("debug")
        .join("activities")
        .join(format!("{stem}-parse-debug.json"));
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    let _ = std::fs::write(&path, serde_json::to_string_pretty(debug_payload).unwrap());
}

/// Replaces an all-`None` series with an empty `Vec` to avoid shipping
/// useless null-filled arrays to the frontend.
fn strip_all_none(series: Vec<Option<f64>>) -> Vec<Option<f64>> {
    if series.iter().all(Option::is_none) {
        Vec::new()
    } else {
        series
    }
}
