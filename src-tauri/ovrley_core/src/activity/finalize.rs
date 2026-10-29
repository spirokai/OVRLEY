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
    build_metric_coverage, derive_activity_metric_series, MetricCoverage, MetricDescriptor,
    MetricSeries,
};
use crate::activity::finalize::smoothing::{
    circular_ema, smoothing_window_for_seconds, zero_phase_smooth,
};
use crate::activity::lap_timing::derive_lap_timing;
use crate::activity::schema::{
    ActivityColumns, GearSeries, ParsedActivity, RawActivity, RawSample,
};
use crate::error::{CoreError, CoreResult};
use crate::media::telemetry_math::{finite_f64, round_f64};
use crate::media::timezone::timezone_for_coordinates;
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use ovrley_widgets::model::telemetry::{
    ActivitySeries, ACTIVITY_COVERAGE_SOURCES, CORE_ACTIVITY_ATTRIBUTES,
    EXTENDED_ACTIVITY_ATTRIBUTES,
};
use ovrley_widgets::widgets::elevation::sampling::preferred_elevation_series;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Zero-phase moving-average horizon applied to every source's altitude at ingress.
const ALTITUDE_SMOOTHING_SECONDS: f64 = 1.0;

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
    finalize_raw_activity(&raw_activity, repo_root)
}

/// Finalizes native extraction through the same gap-filling path as raw JSON input.
pub fn finalize_raw_activity(
    raw_activity: &RawActivity,
    repo_root: Option<&std::path::Path>,
) -> CoreResult<FinalizeActivityResponse> {
    let finalized = finalize_raw_activity_with_debug(raw_activity)?;
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
fn finalize_raw_activity_with_debug(raw_activity: &RawActivity) -> CoreResult<FinalizedActivity> {
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

    let course_series = build_course_series(columns);
    validate_course_coordinate_ranges(&course_series)?;
    let timezone = course_series.iter().find_map(|(latitude, longitude)| {
        let (Some(latitude), Some(longitude)) = (*latitude, *longitude) else {
            return None;
        };
        timezone_for_coordinates(longitude, latitude)
    });
    let timezone_tz = timezone
        .as_deref()
        .map(str::parse::<Tz>)
        .transpose()
        .map_err(|_| {
            CoreError::Activity("Timezone finder returned an invalid IANA timezone".into())
        })?;
    let time_series = build_time_series(columns, timezone_tz);
    let elapsed_series = build_elapsed_series(columns, &time_series);
    let direct_distance_series: Vec<Option<f64>> = columns
        .distance
        .iter()
        .map(|value| value.and_then(finite_f64))
        .collect();
    let distance_series =
        build_distance_series(&course_series, &direct_distance_series, &elapsed_series);
    // Altitude is smoothed once here for every source, so derived vertical speed, gradient,
    // total ascent and the elevation profile all read the same series.
    let altitude_window = smoothing_window_for_seconds(
        &elapsed_series.iter().map(|second| second * 1000.0).collect::<Vec<_>>(),
        ALTITUDE_SMOOTHING_SECONDS,
    );
    let smooth_altitude = |series: &[Option<f64>]| {
        let finite: Vec<_> = series.iter().map(|value| value.and_then(finite_f64)).collect();
        zero_phase_smooth(&finite, altitude_window)
    };
    let elevation_base_series = smooth_altitude(&columns.elevation);
    let barometric_altitude_series = smooth_altitude(&columns.barometric_altitude);
    let elevation_profile_series =
        preferred_elevation_series(&barometric_altitude_series, &elevation_base_series).to_vec();

    let mut metric_series_map = derive_activity_metric_series(
        &course_series,
        &distance_series,
        &elevation_base_series,
        &barometric_altitude_series,
        &elapsed_series,
        columns,
    );
    apply_metric_smoothing(
        &mut metric_series_map,
        &elapsed_series,
        &columns.options.smoothing,
    );

    let mut coverage = build_metric_coverage(&metric_series_map);
    insert_core_attribute_coverage(&mut coverage, &course_series, &time_series);
    let valid_attributes = available_attributes(&coverage, CORE_ACTIVITY_ATTRIBUTES);
    let extended_attributes = available_attributes(&coverage, EXTENDED_ACTIVITY_ATTRIBUTES);
    let duration_seconds = elapsed_series.last().copied().unwrap_or(0.0);
    let total_distance_meters = distance_series.last().copied().flatten().unwrap_or(0.0);
    let first_sample_time = time_series.iter().find_map(Clone::clone);
    let explicit_sync_time = columns
        .sync_time
        .as_deref()
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|parsed| {
                    parsed
                        .with_timezone(&Utc)
                        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
                })
                .map_err(|error| {
                    CoreError::Activity(format!(
                        "Activity sync_time must be a valid RFC3339 timestamp: {error}"
                    ))
                })
        })
        .transpose()?;
    let sync_time = explicit_sync_time.or(first_sample_time);
    let end_time = time_series.iter().rev().find_map(Clone::clone);
    let distance_progress_series = build_progress_series(&distance_series);
    let lap_timing = derive_lap_timing(
        &elapsed_series,
        &distance_series,
        &course_series,
        &columns.lap_number,
        &columns.lap_markers,
    );

    let mut metadata = columns.metadata.clone();
    if !metadata.is_object() {
        metadata = json!({});
    }
    if let Some(object) = metadata.as_object_mut() {
        object.remove("start_time");
        object.remove("timezone");
        object.remove("sync_time");
        if let Some(timezone) = &timezone {
            object.insert("timezone".to_string(), json!(timezone));
        }
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
        if columns.include_original_sample_count_metadata {
            object.insert(
                "original_sample_count".to_string(),
                json!(columns.original_sample_count),
            );
        } else {
            object.remove("original_sample_count");
        }
        object.remove("best_lap_time_seconds");
        object.insert(
            "inserted_idle_sample_count".to_string(),
            json!(gap_debug.inserted_sample_count),
        );
    }

    let mut extra = BTreeMap::new();
    extra.insert("metric_units".to_string(), metric_units());
    extra.insert("coverage".to_string(), json!(coverage));
    extra.insert("valid_attributes".to_string(), json!(valid_attributes));
    extra.insert(
        "extended_attributes".to_string(),
        json!(extended_attributes),
    );

    let parsed_activity = ovrley_widgets::numeric_activity!(ParsedActivity, FinalizedNumericSeries(&metric_series_map), series, {
        file_name: Some(columns.file_name.clone()),
        file_format: Some(columns.file_format.clone()),
        metadata: metadata,
        timezone: timezone_tz,
        sync_time: sync_time,
        sample_elapsed_seconds: elapsed_series,
        sample_distance_progress: distance_progress_series,
        frame_elapsed_seconds: Vec::new(),
        frame_timestamps: Vec::new(),
        frame_distance_progress: Vec::new(),
        trim_start_seconds: 0.0,
        trim_end_seconds: round_f64(duration_seconds, 3).unwrap_or(0.0),
        sample_course_points: course_series.clone(),
        sample_elevations: elevation_profile_series.to_vec(),
        course: course_series,
        gear_position: gear_metric(&metric_series_map),
        time: time_series,
        lap_number: lap_timing.lap_number,
        lap_time_seconds: lap_timing.lap_time_seconds,
        lap_start_elapsed_seconds: lap_timing.lap_start_elapsed_seconds,
        delta_to_best_lap_seconds: lap_timing.delta_to_best_lap_seconds,
        best_lap_time_seconds: lap_timing.best_lap_time_seconds,
        lap_durations_seconds: lap_timing.lap_durations_seconds,
        lap_durations_best_so_far_seconds: lap_timing.lap_durations_best_so_far_seconds,
        extra: extra,
    });

    Ok(FinalizedActivity {
        parsed_activity,
        gap_debug,
    })
}

macro_rules! activity_input_operations {
    (
        numeric { $($numeric:ident,)* } text { $($text:ident,)* } integer { $($integer:ident,)* }
        samples { numeric { $($sample:ident,)* } text { $($sample_text:ident,)* } integer { $($sample_integer:ident,)* } }
        sources { $($source:ident: $variant:ident,)* } absent { $($absent:ident,)* }
    ) => {
        fn activity_columns_from_samples(
            raw_activity: &RawActivity, raw_samples: Vec<RawSample>, original_sample_count: usize,
        ) -> ActivityColumns {
            let sample_count = raw_samples.len();
            let mut columns = ActivityColumns {
                file_name: raw_activity.file_name.clone(),
                file_format: raw_activity.file_format.clone(),
                metadata: raw_activity.metadata.clone(),
                sync_time: raw_activity.sync_time.clone(),
                options: raw_activity.options.clone(),
                preserve_direct_metric_gaps: Default::default(),
                $($numeric: Vec::with_capacity(sample_count),)*
                $($text: Vec::with_capacity(sample_count),)*
                $($integer: Vec::with_capacity(sample_count),)*
                original_sample_count,
                include_original_sample_count_metadata: true,
                lap_markers: raw_activity.lap_markers.clone(),
            };
            for sample in raw_samples {
                $(columns.$sample.push(sample.$sample.and_then(finite_f64));)*
                $(columns.$sample_text.push(sample.$sample_text);)*
                $(columns.$sample_integer.push(sample.$sample_integer);)*
                $(columns.$absent.push(None);)*
            }
            columns
        }

        fn validate_column_lengths(columns: &ActivityColumns) -> CoreResult<()> {
            let expected = columns.timestamp.len();
            for (name, len) in [
                $((stringify!($numeric), columns.$numeric.len()),)*
                $((stringify!($text), columns.$text.len()),)*
                $((stringify!($integer), columns.$integer.len()),)*
            ] {
                if len != expected {
                    return Err(CoreError::Activity(format!(
                        "Invalid ActivityColumns payload: {name} length {len} does not match timestamp length {expected}"
                    )));
                }
            }
            Ok(())
        }
    };
}

ovrley_widgets::activity_input_columns!(activity_input_operations);

impl ActivityColumns {
    pub fn len(&self) -> usize {
        self.timestamp.len()
    }

    pub fn is_empty(&self) -> bool {
        self.timestamp.is_empty()
    }
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

fn validate_course_coordinate_ranges(
    course_series: &[(Option<f64>, Option<f64>)],
) -> CoreResult<()> {
    for (index, (latitude, longitude)) in course_series.iter().enumerate() {
        if latitude.is_some_and(|value| !(-90.0..=90.0).contains(&value)) {
            return Err(CoreError::Activity(format!(
                "Invalid latitude at activity sample {index}: expected -90..=90 degrees"
            )));
        }
        if longitude.is_some_and(|value| !(-180.0..=180.0).contains(&value)) {
            return Err(CoreError::Activity(format!(
                "Invalid longitude at activity sample {index}: expected -180..=180 degrees"
            )));
        }
    }
    Ok(())
}

/// Normalizes source timestamps into UTC millisecond RFC 3339 strings.
///
/// The frontend historically emitted `Date#toISOString()` values. Normalizing
/// here keeps backend-created payloads byte-stable enough for diagnostics and
/// avoids leaking local time-zone formatting into render data.
///
/// When a timestamp has no timezone offset (naive local time such as SRT
/// camera timestamps), it is interpreted in the GPS-derived activity timezone
/// and converted to UTC. Without a timezone, naive timestamps are treated as
/// UTC.
fn build_time_series(columns: &ActivityColumns, timezone: Option<Tz>) -> Vec<Option<String>> {
    columns
        .timestamp
        .iter()
        .map(|timestamp| {
            let value = timestamp.as_deref()?;
            if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
                return Some(
                    parsed
                        .with_timezone(&Utc)
                        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                );
            }
            let naive = NaiveDateTime::parse_from_str(value.trim(), "%Y-%m-%dT%H:%M:%S%.f")
                .or_else(|_| NaiveDateTime::parse_from_str(value.trim(), "%Y-%m-%dT%H:%M:%S"))
                .ok()?;
            let utc = match timezone {
                Some(tz) => tz
                    .from_local_datetime(&naive)
                    .single()
                    .map(|dt| dt.with_timezone(&Utc)),
                None => Some(Utc.from_utc_datetime(&naive)),
            }?;
            Some(utc.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
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
/// Structural and numeric attributes share the same coverage contract by this
/// stage, so advertised availability has one authoritative appraisal.
fn available_attributes(
    coverage: &BTreeMap<String, MetricCoverage>,
    attributes: &[&str],
) -> Vec<String> {
    let mut available = Vec::new();
    for &attribute in attributes {
        if coverage[attribute].is_available() {
            available.push(attribute.to_owned());
        }
    }
    available
}

/// Adds coverage for core widget attributes represented by structural series or
/// an existing canonical metric. This keeps availability and provenance in one
/// backend-owned contract for every advertised attribute.
fn insert_core_attribute_coverage(
    coverage: &mut BTreeMap<String, MetricCoverage>,
    course_series: &[(Option<f64>, Option<f64>)],
    time_series: &[Option<String>],
) {
    let coordinate_count = course_series
        .iter()
        .filter(|(latitude, longitude)| latitude.is_some() && longitude.is_some())
        .count();
    let coordinate_coverage =
        MetricCoverage::from_direct_presence(coordinate_count, course_series.len());
    coverage.insert(
        ActivitySeries::Course.as_str().to_string(),
        coordinate_coverage,
    );

    let time_count = time_series.iter().filter(|value| value.is_some()).count();
    coverage.insert(
        ActivitySeries::Time.as_str().to_string(),
        MetricCoverage::from_direct_presence(time_count, time_series.len()),
    );

    for &(widget, source) in ACTIVITY_COVERAGE_SOURCES {
        coverage.insert(widget.to_owned(), coverage[source].clone());
    }
}

struct FinalizedNumericSeries<'a>(&'a BTreeMap<String, MetricDescriptor>);

impl FinalizedNumericSeries<'_> {
    fn series(&self, series: ActivitySeries) -> Vec<Option<f64>> {
        if !series.finalized() {
            // The activity contract reserves this column; extraction does not produce it.
            return Vec::new();
        }
        let MetricSeries::Numeric(values) = &self.0[series.as_str()].series else {
            unreachable!("numeric activity column mapped to gear series")
        };
        strip_all_none(values.clone())
    }
}

fn gear_metric(metric_series_map: &BTreeMap<String, MetricDescriptor>) -> GearSeries {
    let MetricSeries::Gear(series) = &metric_series_map["gear_position"].series else {
        unreachable!("gear_position mapped to numeric series")
    };
    strip_all_none(series.clone())
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

    let sample_timestamps_ms: Vec<_> = elapsed_series
        .iter()
        .map(|seconds| seconds * 1000.0)
        .collect();

    for (metric_name, option) in smoothing_options {
        if !option.enabled {
            continue;
        }

        let Some(descriptor) = metric_series_map.get_mut(metric_name) else {
            continue;
        };
        let MetricSeries::Numeric(series) = &mut descriptor.series else {
            continue;
        };

        let column: ActivitySeries = serde_json::from_value(Value::String(metric_name.clone()))
            .expect("finalized numeric metrics must declare their activity columns");
        if column.smoothing() != Some(option.method.as_str()) {
            continue;
        }
        match option.method.as_str() {
            "circular_ema" => {
                *series = circular_ema(series, elapsed_series, option.window_seconds);
            }
            "zero_phase_ma" => {
                let window =
                    smoothing_window_for_seconds(&sample_timestamps_ms, option.window_seconds);
                *series = zero_phase_smooth(series, window);
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
    let mut units = serde_json::Map::new();
    for &column in ActivitySeries::ALL {
        if let Some(unit) = column.unit() {
            units.insert(column.as_str().into(), Value::String(unit.into()));
        }
    }
    Value::Object(units)
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
fn strip_all_none<T>(series: Vec<Option<T>>) -> Vec<Option<T>> {
    if series.iter().all(Option::is_none) {
        Vec::new()
    } else {
        series
    }
}
