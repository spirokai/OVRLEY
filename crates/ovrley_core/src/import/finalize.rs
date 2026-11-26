//! Finalization of extracted samples into the canonical [`ParsedActivity`].
//!
//! Format extraction stays at the edge and hands normalized [`RawActivity`] samples or
//! [`ActivityColumns`] here. Finalization owns the shared path: idle-gap filling, the
//! timeline, the timezone, smoothing, metric derivation, coverage bookkeeping and assembly.

use crate::activity::laps;
use crate::activity::schema::{
    ActivityColumns, GearSeries, NumericSeries, ParsedActivity, RawActivity, RawSample,
    };
use crate::error::{CoreError, CoreResult};
use crate::import::metrics::{
    build_metric_coverage, derive_activity_metric_series, MetricCoverage, MetricDescriptor,
    MetricSeries,
};
use crate::import::{finite_f64, haversine_distance, round_f64};
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use ovrley_widgets::model::telemetry::{
    ActivitySeries, ACTIVITY_COVERAGE_SOURCES, CORE_ACTIVITY_ATTRIBUTES,
    EXTENDED_ACTIVITY_ATTRIBUTES,
};
use ovrley_widgets::widgets::elevation::preferred_elevation_series;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;
use tzf_rs::DefaultFinder;



/// Zero-phase moving-average horizon applied to every source's altitude at ingress.
const ALTITUDE_SMOOTHING_SECONDS: f64 = 1.0;

/// Finalizes row samples: equal-time samples merge, stationary gaps fill with idle samples
/// unless the parser skips that, then the columns finalize. In debug builds with a `debug_root`,
/// the result and the gap diagnostics are written under `debug/activities/`.
pub fn finalize_samples(raw_activity: &RawActivity, debug_root: Option<&Path>) -> CoreResult<ParsedActivity> {
    let (activity, gaps) = finalize_raw_activity_with_debug(raw_activity)?;
    write_finalize_debug(debug_root, &activity, &gaps);
    Ok(activity)
}

/// Finalizes columnar extraction, which never fills gaps.
pub fn finalize_columns(columns: &ActivityColumns, debug_root: Option<&Path>) -> CoreResult<ParsedActivity> {
    let gaps = skipped_gap_debug();
    let activity = finalize_columns_with_debug(columns, &gaps)?;
    write_finalize_debug(debug_root, &activity, &gaps);
    Ok(activity)
}

fn write_finalize_debug(root: Option<&Path>, activity: &ParsedActivity, gaps: &GapDebug) {
    let Some(root) = root.filter(|_| cfg!(debug_assertions)) else {
        return;
    };
    let payload = json!({
        "generated_at": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "file_name": activity.file_name,
        "file_format": activity.file_format,
        "idle_gap_fill": gaps,
        "parsed_activity": activity,
    });
    write_activity_debug_file(root, activity.file_name.as_deref().unwrap_or("activity"), &payload);
}

/// Finalizes row samples and returns the gap diagnostics with the activity.
fn finalize_raw_activity_with_debug(raw_activity: &RawActivity) -> CoreResult<(ParsedActivity, GapDebug)> {
    // Gap filling compares neighbouring samples, so it must see each instant once.
    let merged_raw_samples = merge_equal_time_samples(&raw_activity.raw_samples);
    let (normalized_raw_samples, gap_debug) = if raw_activity.options.skip_idle_gap_fill {
        (merged_raw_samples, skipped_gap_debug())
    } else {
        insert_idle_gap_samples(&merged_raw_samples)
    };
    let columns = activity_columns_from_samples(
        raw_activity,
        normalized_raw_samples,
        raw_activity.raw_samples.len(),
    );
    let activity = finalize_columns_with_debug(&columns, &gap_debug)?;
    Ok((activity, gap_debug))
}

fn finalize_columns_with_debug(
    columns: &ActivityColumns,
    gap_debug: &GapDebug,
) -> CoreResult<ParsedActivity> {
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
    let lap_timing = laps::derive_series(
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

    Ok(parsed_activity)
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

        /// Merges each run of samples sharing one instant, so the timeline strictly increases.
        /// Formats such as FIT may split one instant across records (GPS in one, heart rate in
        /// the next); a later sample fills in the fields it carries, as CSV rows at one time do.
        /// A sample with neither an elapsed time nor a timestamp has no instant to share.
        fn merge_equal_time_samples(raw_samples: &[RawSample]) -> Vec<RawSample> {
            let mut merged: Vec<RawSample> = Vec::with_capacity(raw_samples.len());
            for sample in raw_samples {
                let has_time = sample.elapsed_seconds.is_some() || sample.timestamp.is_some();
                match merged.last_mut() {
                    Some(last)
                        if has_time
                            && last.elapsed_seconds == sample.elapsed_seconds
                            && last.timestamp == sample.timestamp =>
                    {
                        $(if sample.$sample.is_some() { last.$sample = sample.$sample; })*
                        $(if sample.$sample_text.is_some() { last.$sample_text = sample.$sample_text.clone(); })*
                        $(if sample.$sample_integer.is_some() { last.$sample_integer = sample.$sample_integer; })*
                    }
                    _ => merged.push(sample.clone()),
                }
            }
            merged
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

/// Recorded distance between two samples above which a repeated GPS fix counts as stale.
const STALE_FIX_DISTANCE_METERS: f64 = 1.0;

/// Builds lat/lon tuples while preserving partial GPS samples.
///
/// Each coordinate is guarded independently so one bad component does not
/// poison the whole vector shape; downstream interpolation can still decide
/// whether enough course data exists to render.
///
/// A receiver that misses a fix can repeat the last one while the recorded distance keeps growing.
/// That repeat is a missing fix rather than a stop, so it becomes a gap that route interpolation
/// bridges, instead of stalling the rider on the map. The distance is compared sample to sample, so
/// creep while stopped never adds up to movement. Without recorded distance a repeat stays.
fn build_course_series(columns: &ActivityColumns) -> Vec<(Option<f64>, Option<f64>)> {
    let mut last_fix = None;
    let mut last_distance = None;
    columns
        .latitude
        .iter()
        .zip(&columns.longitude)
        .zip(&columns.distance)
        .map(|((latitude, longitude), distance)| {
            let point = (
                latitude.and_then(finite_f64),
                longitude.and_then(finite_f64),
            );
            let distance = distance.and_then(finite_f64);
            let moved = distance
                .zip(last_distance)
                .is_some_and(|(now, then)| now - then > STALE_FIX_DISTANCE_METERS);
            if distance.is_some() {
                last_distance = distance;
            }
            let Some(fix) = point.0.zip(point.1) else {
                return point;
            };
            if moved && last_fix == Some(fix) {
                return (None, None);
            }
            last_fix = Some(fix);
            point
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

/// The attributes of `attributes` whose coverage has usable values.
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

/// Adds coverage for the attributes backed by the course and time series or by another metric,
/// so every advertised attribute has its availability and provenance in one place.
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

/// The unit of every finalized metric. Parsers emit canonical units, so the finalizer owns them.
fn metric_units() -> Value {
    let mut units = serde_json::Map::new();
    for &column in ActivitySeries::ALL {
        if let Some(unit) = column.unit() {
            units.insert(column.as_str().into(), Value::String(unit.into()));
        }
    }
    Value::Object(units)
}

/// Writes an import debug payload to `debug/activities/` in debug builds, best-effort.
pub(crate) fn write_activity_debug_file(repo_root: &Path, name: &str, debug_payload: &Value) {
    if !cfg!(debug_assertions) {
        return;
    }
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    let directory = repo_root.join("debug").join("activities");
    let path = directory.join(format!("{stem}-parse-debug.json"));
    let written = std::fs::create_dir_all(&directory)
        .and_then(|()| std::fs::write(&path, serde_json::to_vec_pretty(debug_payload)?));
    if let Err(error) = written {
        log::warn!("Could not write {}: {error}", path.display());
    }
}

/// Replaces an all-`None` series with an empty `Vec`, the shape of an absent series.
fn strip_all_none<T>(series: Vec<Option<T>>) -> Vec<Option<T>> {
    if series.iter().all(Option::is_none) {
        Vec::new()
    } else {
        series
    }
}

#[derive(Clone, Debug, Serialize)]
struct GapDebug {
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
struct DetectedGap {
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

/// The gap diagnostics of an activity whose gaps were not filled.
fn skipped_gap_debug() -> GapDebug {
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
/// Gap detection ignores malformed timestamps instead of failing the import.
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
    if sorted.len().is_multiple_of(2) {
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
fn estimate_recording_interval_seconds(raw_samples: &[RawSample]) -> f64 {
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
    synthetic.apply_idle_values();
    synthetic.synthetic_idle = true;
    synthetic
}

/// Inserts synthetic samples for stationary recording gaps.
///
/// A gap is filled only when elapsed time exceeds the cadence-based threshold
/// and physical movement stays below the stationary threshold. This preserves
/// real travel gaps while preventing stopped periods from collapsing to one
/// long interpolation jump.
fn insert_idle_gap_samples(raw_samples: &[RawSample]) -> (Vec<RawSample>, GapDebug) {
    if raw_samples.len() < 2 {
        return (raw_samples.to_vec(), skipped_gap_debug());
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
/// segment distance fills interior gaps. Distance stays missing before the first observation.
fn build_distance_series(
    course_points: &[(Option<f64>, Option<f64>)],
    direct_distance_series: &[Option<f64>],
    elapsed_seconds: &[f64],
) -> Vec<Option<f64>> {
    let has_direct_distance = direct_distance_series.iter().any(|value| value.is_some());
    let has_course = course_points
        .iter()
        .any(|(latitude, longitude)| latitude.is_some() && longitude.is_some());
    if !has_direct_distance && !has_course {
        return course_points.iter().map(|_| None).collect();
    }

    let mut sparse_distance_series = Vec::with_capacity(course_points.len());
    let mut total_distance_meters: f64 = 0.0;
    let mut previous_course = None;

    for (index, (latitude, longitude)) in course_points.iter().enumerate() {
        let current_course = latitude.zip(*longitude);
        if let Some(direct_distance) = direct_distance_series
            .get(index)
            .copied()
            .flatten()
            .and_then(finite_f64)
        {
            total_distance_meters = total_distance_meters.max(direct_distance);
            sparse_distance_series.push(round_f64(total_distance_meters, 3));
            if current_course.is_some() {
                previous_course = current_course;
            }
            continue;
        }

        let Some((latitude, longitude)) = current_course else {
            sparse_distance_series.push(None);
            continue;
        };
        if let Some((previous_latitude, previous_longitude)) = previous_course {
            total_distance_meters +=
                haversine_distance(previous_latitude, previous_longitude, latitude, longitude);
        }
        previous_course = current_course;
        sparse_distance_series.push(round_f64(total_distance_meters, 3));
    }

    let points = ovrley_widgets::series::collect_valid_numeric_points(
        elapsed_seconds,
        &sparse_distance_series,
    );
    let first_observation = points.first().map(|(elapsed, _)| *elapsed);
    elapsed_seconds
        .iter()
        .map(|elapsed| {
            if first_observation.is_some_and(|first| *elapsed < first) {
                return None;
            }
            ovrley_widgets::series::interpolate_points(&points, *elapsed)
                .and_then(|distance| round_f64(distance, 3))
        })
        .collect()
}

/// Normalizes cumulative distance into full-activity progress.
///
/// Route and elevation widgets consume progress as `0..1`; zero-distance
/// activities return zeros so callers never divide by an unusable total.
fn build_progress_series(distance_series: &[Option<f64>]) -> Vec<f64> {
    let total_distance_meters = distance_series.last().copied().flatten().unwrap_or(0.0);
    if !total_distance_meters.is_finite() || total_distance_meters <= 0.0 {
        return distance_series.iter().map(|_| 0.0).collect();
    }
    distance_series
        .iter()
        .map(|value| round_f64(value.unwrap_or(0.0) / total_distance_meters, 6).unwrap_or(0.0))
        .collect()
}

/// A null-aware centered moving average for sparse telemetry series.
pub(super) fn moving_average(data: &[Option<f64>], window: usize) -> Vec<Option<f64>> {
    if window <= 1 || data.is_empty() {
        return data.to_vec();
    }

    let half = window / 2;
    let mut result = Vec::with_capacity(data.len());
    for index in 0..data.len() {
        if data[index].is_none() {
            result.push(None);
            continue;
        }

        let start = index.saturating_sub(half);
        let end = (index + half + 1).min(data.len());
        let mut sum = 0.0;
        let mut count = 0;

        for value in &data[start..end] {
            if let Some(value) = value.and_then(finite_f64) {
                sum += value;
                count += 1;
            }
        }

        result.push((count > 0).then_some(sum / count as f64));
    }
    result
}

/// Forward/backward (zero-phase) smoothing to avoid shifting events.
pub(crate) fn zero_phase_smooth(data: &[Option<f64>], window: usize) -> Vec<Option<f64>> {
    if window <= 1 || data.len() < 2 {
        return data.to_vec();
    }

    let forward = moving_average(data, window);
    let reversed: Vec<_> = forward.into_iter().rev().collect();
    let backward = moving_average(&reversed, window);
    backward.into_iter().rev().collect()
}

/// Converts a time horizon into a sample window using observed cadence.
pub(crate) fn smoothing_window_for_seconds(sample_timestamps_ms: &[f64], seconds: f64) -> usize {
    if sample_timestamps_ms.len() < 2 || !seconds.is_finite() || seconds <= 0.0 {
        return 1;
    }

    let mut deltas_ms: Vec<_> = sample_timestamps_ms
        .windows(2)
        .filter_map(|pair| finite_f64(pair[1] - pair[0]))
        .filter(|delta| *delta > 0.0)
        .collect();
    if deltas_ms.is_empty() {
        return 1;
    }

    deltas_ms.sort_by(f64::total_cmp);
    let median_delta_ms = deltas_ms[deltas_ms.len() / 2];
    if median_delta_ms <= 0.0 {
        return 1;
    }

    ((seconds * 1000.0) / median_delta_ms).round().max(1.0) as usize
}

/// Smooths heading as a time-aware circular EMA over unit vectors.
///
/// `window_seconds` is the EMA time constant. The per-sample alpha is derived
/// from the observed elapsed time, so the same configured smoothing horizon
/// behaves consistently at different sample rates. Missing samples reset the
/// filter so it never carries a pre-gap heading into a new run.
pub(crate) fn circular_ema(
    heading_series: &NumericSeries,
    elapsed_seconds: &[f64],
    window_seconds: f64,
) -> NumericSeries {
    let mut smoothed_series = Vec::with_capacity(heading_series.len());
    let mut smoothed_x = None;
    let mut smoothed_y = None;
    let mut previous_elapsed: Option<f64> = None;

    for (index, heading) in heading_series.iter().enumerate() {
        let Some(heading) = heading.and_then(finite_f64) else {
            smoothed_series.push(None);
            smoothed_x = None;
            smoothed_y = None;
            previous_elapsed = None;
            continue;
        };
        let radians = heading.to_radians();
        let next_x = radians.cos();
        let next_y = radians.sin();
        let alpha = previous_elapsed
            .map(|previous| {
                let elapsed = elapsed_seconds[index] - previous;
                if elapsed.is_finite()
                    && elapsed > 0.0
                    && window_seconds.is_finite()
                    && window_seconds > 0.0
                {
                    1.0 - (-elapsed / window_seconds).exp()
                } else {
                    1.0
                }
            })
            .unwrap_or(1.0);

        match (smoothed_x, smoothed_y) {
            (Some(x), Some(y)) => {
                smoothed_x = Some(alpha * next_x + (1.0 - alpha) * x);
                smoothed_y = Some(alpha * next_y + (1.0 - alpha) * y);
            }
            _ => {
                smoothed_x = Some(next_x);
                smoothed_y = Some(next_y);
            }
        }

        let smoothed_heading = smoothed_y.unwrap().atan2(smoothed_x.unwrap()).to_degrees();
        smoothed_series.push(round_f64((smoothed_heading + 360.0) % 360.0, 3));
        previous_elapsed = Some(elapsed_seconds[index]);
    }

    smoothed_series
}

static FINDER: OnceLock<DefaultFinder> = OnceLock::new();

/// Finds the timezone containing one validated GPS coordinate.
pub(crate) fn timezone_for_coordinates(longitude: f64, latitude: f64) -> Option<String> {
    if !longitude.is_finite()
        || !latitude.is_finite()
        || !(-180.0..=180.0).contains(&longitude)
        || !(-90.0..=90.0).contains(&latitude)
        || (longitude == 0.0 && latitude == 0.0)
    {
        return None;
    }
    let timezone = FINDER
        .get_or_init(DefaultFinder::new)
        .get_tz_name(longitude, latitude);

    (!timezone.is_empty()).then(|| timezone.to_string())
}

#[cfg(test)]
mod tests {
    use super::zero_filled_idle_sample;
    use crate::activity::schema::RawSample;
    use super::circular_ema;
    use super::timezone_for_coordinates;

    #[test]
    fn idle_sample_preserves_context_and_resets_motion_values() {
        let sample = RawSample {
            elevation: Some(80.0),
            heartrate: Some(140.0),
            engine_power: Some(500.0),
            speed: Some(5.0),
            gradient: Some(8.0),
            g_force_y: Some(0.4),
            stride_length: Some(1.2),
            ..Default::default()
        };
        let idle = zero_filled_idle_sample(&sample, 2.5, None);
        assert_eq!(idle.elevation, sample.elevation);
        assert_eq!(idle.heartrate, sample.heartrate);
        assert_eq!(idle.engine_power, sample.engine_power);
        assert_eq!(idle.speed, Some(0.0));
        assert_eq!(idle.gradient, Some(0.0));
        assert_eq!(idle.g_force_y, Some(0.0));
        assert_eq!(idle.stride_length, None);
        assert_eq!(idle.elapsed_seconds, Some(2.5));
        assert!(idle.synthetic_idle);
    }

    #[test]
    fn time_aware_circular_ema_preserves_a_short_heading_peak() {
        let result = circular_ema(
            &vec![Some(0.0), Some(30.0), Some(60.0), Some(90.0), Some(90.0)],
            &[0.0, 1.0, 2.0, 3.0, 4.0],
            0.5,
        );

        assert!(result[3].expect("heading is present") > 75.0);
    }

    #[test]
    fn circular_ema_keeps_heading_wraparound_on_the_short_arc() {
        let result = circular_ema(
            &vec![Some(350.0), Some(355.0), Some(5.0), Some(10.0)],
            &[0.0, 1.0, 2.0, 3.0],
            0.5,
        );

        assert!(result
            .iter()
            .flatten()
            .all(|heading| *heading >= 340.0 || *heading <= 20.0));
    }

    #[test]
    fn finds_timezone_from_coordinates() {
        assert_eq!(
            timezone_for_coordinates(14.421_254, 50.087_465).as_deref(),
            Some("Europe/Prague")
        );
    }

    #[test]
    fn returns_no_timezone_for_unmapped_coordinates() {
        assert_eq!(timezone_for_coordinates(0.0, 0.0), None);
    }
}
