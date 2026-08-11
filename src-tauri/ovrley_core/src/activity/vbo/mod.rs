//! Native Racelogic VBOX activity extraction.
//!
//! VBO values are validated once at this boundary, converted to canonical
//! activity units, assembled directly as [`ActivityColumns`], and passed to the
//! shared columnar finalizer without a row-oriented JSON intermediate.

mod channels;

use crate::activity::finalize::{finalize_activity_columns, FinalizeActivityResponse};
use crate::activity::schema::{
    ActivityColumns, LapMarkers, RawActivityOptions, SmoothingOption, TimingMarker,
    TimingMarkerKind,
};
use crate::error::{CoreError, CoreResult};
use crate::media::telemetry_math::{
    backfill_lateral_g_from_lean_angle, derive_lean_from_speed_heading, lean_angle_from_lateral_g,
};
use chrono::{Duration, NaiveDate, NaiveTime, SecondsFormat, Utc};
use serde_json::json;
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Largest backward jump that is treated as a midnight rollover rather than invalid time data.
const MIDNIGHT_ROLLOVER_THRESHOLD_SECONDS: f64 = 12.0 * 60.0 * 60.0;
/// Number of seconds added when a time-of-day value rolls over to the next day.
const SECONDS_PER_DAY: f64 = 24.0 * 60.0 * 60.0;
/// Width of the timing gate constructed perpendicular to a VBO lap-marker direction pair.
const TIMING_GATE_WIDTH_METERS: f64 = 40.0;
/// Spherical-earth radius used only for local timing-gate projection.
const EARTH_RADIUS_METERS: f64 = 6_371_000.0;

/// Sections extracted from the text-based VBO container.
#[derive(Default)]
struct Sections {
    /// Date declared by the file-creation line, when present.
    creation_date: Option<NaiveDate>,
    /// Raw channel declarations from the `[header]` section.
    header: Vec<String>,
    /// Canonical channel identifiers from the `[column names]` section.
    column_names: Vec<String>,
    /// Numeric rows from the `[data]` section.
    data: Vec<DataRow>,
    /// Raw marker lines from the `[laptiming]` section.
    laptiming: Vec<String>,
}

/// One parsed VBO data row, retaining its source line for diagnostics.
struct DataRow {
    /// One-based line number in the original VBO text.
    line_number: usize,
    /// Finite numeric values in source-column order.
    values: Vec<f64>,
}

/// Opens a native VBO file, parses it into canonical columns, and finalizes it.
///
/// The path is also used to derive the activity filename and to report file-opening errors.
/// Parsing errors identify the supplied filename in their outer error message.
pub fn parse_vbo_activity_path(
    path: &Path,
    repo_root: Option<&Path>,
) -> CoreResult<FinalizeActivityResponse> {
    let file = File::open(path).map_err(|source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            CoreError::Activity(format!(
                "VBO path has no valid UTF-8 filename: {}",
                path.display()
            ))
        })?;

    parse_vbo_activity_reader(file, file_name, repo_root)
}

/// Reads VBO data, converts it to canonical activity columns, and finalizes the activity.
///
/// The reader must contain UTF-8 VBO text. Required sections, numeric values, channel widths,
/// telemetry mappings, and timelines are validated at this boundary. `repo_root` is forwarded to
/// the shared activity finalizer for any repository-relative metadata it needs to resolve.
pub fn parse_vbo_activity_reader<R: Read>(
    mut reader: R,
    file_name: &str,
    repo_root: Option<&Path>,
) -> CoreResult<FinalizeActivityResponse> {
    let result = (|| {
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(|error| CoreError::Activity(format!("Failed to read VBO input: {error}")))?;
        if bytes.is_empty() {
            return Err(CoreError::Activity("VBO file is empty".to_string()));
        }

        let text = String::from_utf8(bytes).map_err(|error| {
            CoreError::Activity(format!(
                "VBO input is not valid UTF-8 at byte {}",
                error.utf8_error().valid_up_to()
            ))
        })?;
        let sections = parse_sections(&text)?;
        let columns = build_activity_columns(sections, file_name)?;
        finalize_activity_columns(&columns, repo_root)
    })();

    result.map_err(|error| CoreError::Activity(format!("VBO import '{file_name}': {error}")))
}

/// Parses VBO section headers, channel declarations, creation date, and data rows.
///
/// Section names are matched case-insensitively. Data values are parsed and checked for finite
/// floating-point values here so later column construction can rely on numeric input.
fn parse_sections(text: &str) -> CoreResult<Sections> {
    let mut sections = Sections::default();
    let mut current = String::new();

    for (line_index, line) in text.lines().enumerate() {
        let line_number = line_index + 1;
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current = trimmed[1..trimmed.len() - 1].to_ascii_lowercase();
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("File created on ") {
            if sections.creation_date.is_some() {
                return Err(CoreError::Activity(format!(
                    "VBO line {line_number} declares the file creation timestamp more than once"
                )));
            }
            sections.creation_date = Some(parse_creation_date(trimmed, line_number)?);
            continue;
        }

        match current.as_str() {
            "header" => sections.header.push(trimmed.to_string()),
            "column names" => {
                if !sections.column_names.is_empty() {
                    return Err(CoreError::Activity(format!(
                        "VBO line {line_number} declares column names more than once"
                    )));
                }
                sections.column_names = trimmed.split_whitespace().map(str::to_string).collect();
                current.clear();
            }
            "data" => sections.data.push(DataRow {
                line_number,
                values: trimmed
                    .split_whitespace()
                    .map(|token| parse_data_value(token, line_number))
                    .collect::<CoreResult<Vec<_>>>()?,
            }),
            "laptiming" => sections.laptiming.push(trimmed.to_string()),
            _ => {}
        }
    }

    if sections.data.is_empty() {
        return Err(CoreError::Activity(
            "VBO file has no data rows in a [data] section".to_string(),
        ));
    }
    Ok(sections)
}

/// Parses the date portion of a VBO `File created on DD/MM/YYYY at HH:MM:SS` line.
fn parse_creation_date(line: &str, line_number: usize) -> CoreResult<NaiveDate> {
    let value = line
        .strip_prefix("File created on ")
        .expect("VBO creation timestamp prefix was checked");
    let mut tokens = value.split_whitespace();
    let date = tokens.next();
    let separator = tokens.next();
    let time = tokens.next();
    let parsed = date
        .and_then(|date| NaiveDate::parse_from_str(date, "%d/%m/%Y").ok())
        .zip(time.and_then(|time| {
            NaiveTime::parse_from_str(time, "%H:%M:%S")
                .or_else(|_| NaiveTime::parse_from_str(time, "%H:%M"))
                .ok()
        }))
        .filter(|_| matches!(separator, Some("at" | "@")));
    parsed.map(|(date, _)| date).ok_or_else(|| {
        CoreError::Activity(format!(
            "VBO line {line_number} contains invalid file creation timestamp '{value}'"
        ))
    })
}

/// Parses one VBO data token as a finite floating-point value.
fn parse_data_value(token: &str, line_number: usize) -> CoreResult<f64> {
    let value = token.parse::<f64>().map_err(|_| {
        CoreError::Activity(format!(
            "VBO line {line_number} contains invalid numeric value '{token}'"
        ))
    })?;
    if !value.is_finite() {
        return Err(CoreError::Activity(format!(
            "VBO line {line_number} contains non-finite numeric value '{token}'"
        )));
    }
    Ok(value)
}

/// Parses VBO `[laptiming]` rows into canonical finite timing gates.
///
/// Each row contains a marker kind (`Start` or `Split`) followed by four
/// coordinate values in VBO minute format: `lon_a lat_a lon_b lat_b`. The two
/// source points define the timing location followed by a heading reference.
/// The canonical gate is centred on the first point and constructed
/// perpendicular to that direction with a 20-metre reach on either side.
fn parse_laptiming_markers(lines: &[String]) -> Vec<TimingMarker> {
    lines
        .iter()
        .filter_map(|line| {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            if tokens.len() < 5 {
                return None;
            }
            let kind = match tokens[0].to_ascii_lowercase().as_str() {
                "start" => TimingMarkerKind::Start,
                _ => TimingMarkerKind::Split,
            };
            let lon_a = tokens[1].parse::<f64>().ok()?;
            let lat_a = tokens[2].parse::<f64>().ok()?;
            let lon_b = tokens[3].parse::<f64>().ok()?;
            let lat_b = tokens[4].parse::<f64>().ok()?;
            timing_gate_from_direction(
                kind,
                minutes_to_degrees(lat_a),
                longitude_minutes_to_degrees(lon_a),
                minutes_to_degrees(lat_b),
                longitude_minutes_to_degrees(lon_b),
            )
        })
        .collect()
}

/// Constructs a finite gate perpendicular to a direction pair in local metre coordinates.
fn timing_gate_from_direction(
    kind: TimingMarkerKind,
    latitude_a: f64,
    longitude_a: f64,
    latitude_b: f64,
    longitude_b: f64,
) -> Option<TimingMarker> {
    let meters_per_degree_latitude = EARTH_RADIUS_METERS * std::f64::consts::PI / 180.0;
    let meters_per_degree_longitude = meters_per_degree_latitude * latitude_a.to_radians().cos();
    let direction_east = (longitude_b - longitude_a) * meters_per_degree_longitude;
    let direction_north = (latitude_b - latitude_a) * meters_per_degree_latitude;
    let direction_length = direction_east.hypot(direction_north);
    if direction_length <= f64::EPSILON || meters_per_degree_longitude.abs() <= f64::EPSILON {
        return None;
    }

    let half_width = TIMING_GATE_WIDTH_METERS / 2.0;
    let gate_offset_east = -direction_north / direction_length * half_width;
    let gate_offset_north = direction_east / direction_length * half_width;
    let gate_offset_latitude = gate_offset_north / meters_per_degree_latitude;
    let gate_offset_longitude = gate_offset_east / meters_per_degree_longitude;

    Some(TimingMarker {
        kind,
        latitude_a: latitude_a - gate_offset_latitude,
        longitude_a: longitude_a - gate_offset_longitude,
        latitude_b: latitude_a + gate_offset_latitude,
        longitude_b: longitude_a + gate_offset_longitude,
    })
}

/// Maps parsed VBO rows into the shared canonical activity-column representation.
///
/// This validates row widths, resolves channel names and units, constructs the timeline, converts
/// coordinates and measurements, and fills unsupported metrics with explicit absence.
fn build_activity_columns(sections: Sections, file_name: &str) -> CoreResult<ActivityColumns> {
    let identifiers = resolve_identifiers(&sections)?;
    let layout = channels::resolve(&sections.header, &identifiers)?;
    for row in &sections.data {
        if row.values.len() != layout.column_count {
            return Err(CoreError::Activity(format!(
                "VBO line {} has {} values but the column layout declares {}",
                row.line_number,
                row.values.len(),
                layout.column_count
            )));
        }
    }

    let (timestamp, elapsed_seconds) =
        build_timeline(&sections.data, &layout, sections.creation_date)?;
    let sample_count = elapsed_seconds.len();
    if sample_count < 2 {
        return Err(CoreError::Activity(
            "VBO activity must contain at least two timed samples".to_string(),
        ));
    }

    let empty = || vec![None; sample_count];
    let series =
        |index, conversion: fn(f64) -> f64| selected_series(&sections.data, index, conversion);
    let latitude = series(layout.latitude, minutes_to_degrees);
    let longitude = series(layout.longitude, longitude_minutes_to_degrees);
    validate_coordinates(&latitude, &longitude, &sections.data)?;
    let g_force_x = series(layout.g_force_x, identity);
    let g_force_y = series(layout.g_force_y, identity);
    let mut lean_angle = series(layout.lean_angle, identity);
    if lean_angle.iter().all(Option::is_none) {
        let speed_mps = selected_series(&sections.data, layout.speed, |value| {
            layout.speed_to_meters_per_second(value)
        });
        lean_angle = derive_lean_from_speed_heading(
            &speed_mps,
            &series(layout.heading, identity),
            &elapsed_seconds,
        );
    }
    if lean_angle.iter().all(Option::is_none) {
        lean_angle = series(layout.lateral_acceleration, identity)
            .into_iter()
            .map(|value| value.and_then(lean_angle_from_lateral_g))
            .collect();
    }
    let (g_force_x, g_force_y) =
        backfill_lateral_g_from_lean_angle(&g_force_x, &g_force_y, &lean_angle);
    let mut g_force = series(layout.g_force, identity);
    if g_force.iter().all(Option::is_none) {
        g_force = g_force_x
            .iter()
            .zip(&g_force_y)
            .map(|(x, y)| (*x).zip(*y).map(|(x, y)| x.hypot(y)))
            .collect();
    }

    Ok(ActivityColumns {
        file_name: file_name.to_string(),
        file_format: "vbo".to_string(),
        metadata: json!({}),
        sync_time: None,
        options: RawActivityOptions {
            smoothing: [(
                "heading".to_string(),
                SmoothingOption {
                    enabled: true,
                    method: "circular_ema".to_string(),
                    window_seconds: 0.0,
                },
            )]
            .into(),
            ..RawActivityOptions::default()
        },
        preserve_direct_metric_gaps: Default::default(),
        timestamp,
        elapsed_seconds: elapsed_seconds.into_iter().map(Some).collect(),
        latitude,
        longitude,
        elevation: series(layout.elevation, identity),
        barometric_altitude: empty(),
        speed: selected_series(&sections.data, layout.speed, |value| {
            layout.speed_to_meters_per_second(value)
        }),
        heading: series(layout.heading, identity),
        heartrate: empty(),
        cadence: empty(),
        power: empty(),
        engine_power: empty(),
        temperature: empty(),
        calories: empty(),
        gradient: empty(),
        pace: empty(),
        distance: build_distance(&sections.data, layout.distance)?,
        distance_to_home: empty(),
        g_force,
        g_force_x,
        g_force_y,
        g_force_z: series(layout.g_force_z, identity),
        rpm: series(layout.rpm, identity),
        throttle_position: series(layout.throttle_position, identity),
        brake_position: series(layout.brake_position, identity),
        lean_angle,
        vertical_speed: selected_series(&sections.data, layout.vertical_speed, |value| {
            layout.vertical_speed_to_meters_per_second(value)
        }),
        torque: empty(),
        stroke_rate: empty(),
        stride_length: empty(),
        vertical_oscillation: empty(),
        ground_contact_time: empty(),
        left_right_balance: empty(),
        core_temperature: empty(),
        air_pressure: empty(),
        gear_position: series(layout.gear_position, identity)
            .into_iter()
            .map(|value| value.map(|number| number.to_string()))
            .collect(),
        iso: empty(),
        aperture: empty(),
        shutter_speed: empty(),
        focal_length: empty(),
        ev: empty(),
        color_temperature: empty(),
        original_sample_count: sample_count,
        include_original_sample_count_metadata: false,
        lap_number: vec![None; sample_count],
        lap_markers: if sections.laptiming.is_empty() {
            LapMarkers::None
        } else {
            LapMarkers::TimingMarkers(parse_laptiming_markers(&sections.laptiming))
        },
    })
}

/// Chooses `[column names]` as the channel source, falling back to `[header]` when it is absent.
fn resolve_identifiers(sections: &Sections) -> CoreResult<Vec<String>> {
    if !sections.column_names.is_empty() {
        return Ok(sections.column_names.clone());
    }
    if sections.header.is_empty() {
        return Err(CoreError::Activity(
            "VBO file has neither [column names] nor [header] channel declarations".to_string(),
        ));
    }

    Ok(sections.header.clone())
}

/// Builds timestamps and zero-based elapsed seconds from the available VBO time channels.
///
/// Explicit elapsed time takes precedence for elapsed values, followed by time-of-day and then
/// Unix timestamps. Creation dates are used to turn time-of-day values into RFC 3339 timestamps;
/// Unix timestamp channels already provide absolute timestamps.
fn build_timeline(
    rows: &[DataRow],
    layout: &channels::ChannelLayout,
    creation_date: Option<NaiveDate>,
) -> CoreResult<(Vec<Option<String>>, Vec<f64>)> {
    let time_elapsed = layout
        .time
        .map(|index| build_time_of_day_elapsed(rows, index))
        .transpose()?;
    let explicit_elapsed = layout
        .elapsed_time
        .map(|index| build_relative_elapsed(rows, index, "elapsed_time"))
        .transpose()?;
    let unix_timeline = layout
        .timestamp
        .map(|index| build_unix_timeline(rows, index))
        .transpose()?;

    let timestamps = if let (Some(date), Some(elapsed)) = (creation_date, time_elapsed.as_ref()) {
        elapsed
            .iter()
            .map(|seconds| Some(date_seconds_to_rfc3339(date, *seconds)))
            .collect()
    } else if let Some((timestamps, _)) = unix_timeline.as_ref() {
        timestamps.iter().cloned().map(Some).collect()
    } else {
        vec![None; rows.len()]
    };
    let elapsed = explicit_elapsed
        .or_else(|| time_elapsed.as_ref().map(|values| rebase_values(values)))
        .or_else(|| unix_timeline.map(|(_, elapsed)| elapsed))
        .ok_or_else(|| {
            CoreError::Activity(
                "VBO column layout has no time, timestamp, or elapsed_time column".to_string(),
            )
        })?;
    Ok((timestamps, elapsed))
}

/// Converts a VBO time-of-day channel into strictly increasing elapsed seconds.
///
/// A sufficiently large backward jump is interpreted as a midnight rollover. Smaller backward
/// jumps and duplicate values are rejected as malformed timeline data.
fn build_time_of_day_elapsed(rows: &[DataRow], time_index: usize) -> CoreResult<Vec<f64>> {
    let first = time_of_day_seconds(rows[0].values[time_index], rows[0].line_number)?;
    let mut previous = first;
    let mut day_offset = 0.0;
    let mut absolute_seconds = Vec::with_capacity(rows.len());
    absolute_seconds.push(first);

    for row in &rows[1..] {
        let time_of_day = time_of_day_seconds(row.values[time_index], row.line_number)?;
        let mut absolute = time_of_day + day_offset;
        if absolute < previous && previous - absolute > MIDNIGHT_ROLLOVER_THRESHOLD_SECONDS {
            day_offset += SECONDS_PER_DAY;
            absolute = time_of_day + day_offset;
        }
        if absolute <= previous {
            return Err(CoreError::Activity(format!(
                "VBO line {} time must be strictly increasing",
                row.line_number
            )));
        }
        absolute_seconds.push(absolute);
        previous = absolute;
    }

    Ok(absolute_seconds)
}

/// Validates and rebases a relative elapsed-time channel to start at zero.
fn build_relative_elapsed(
    rows: &[DataRow],
    index: usize,
    channel_name: &str,
) -> CoreResult<Vec<f64>> {
    let values = rows
        .iter()
        .map(|row| (row.values[index], row.line_number))
        .collect::<Vec<_>>();
    if values[0].0 < 0.0 {
        return Err(CoreError::Activity(format!(
            "VBO line {} {channel_name} must be non-negative",
            values[0].1
        )));
    }
    for pair in values.windows(2) {
        if pair[1].0 <= pair[0].0 {
            return Err(CoreError::Activity(format!(
                "VBO line {} {channel_name} must be strictly increasing",
                pair[1].1
            )));
        }
    }
    Ok(rebase_values(
        &values.iter().map(|(value, _)| *value).collect::<Vec<_>>(),
    ))
}

/// Converts Unix-second timestamps into RFC 3339 strings and zero-based elapsed seconds.
fn build_unix_timeline(rows: &[DataRow], index: usize) -> CoreResult<(Vec<String>, Vec<f64>)> {
    let elapsed = build_relative_elapsed(rows, index, "timestamp")?;
    let timestamps = rows
        .iter()
        .map(|row| {
            let milliseconds = (row.values[index] * 1_000.0).round() as i64;
            chrono::DateTime::<Utc>::from_timestamp_millis(milliseconds)
                .map(|timestamp| timestamp.to_rfc3339_opts(SecondsFormat::Millis, true))
                .ok_or_else(|| {
                    CoreError::Activity(format!(
                        "VBO line {} timestamp is outside the supported Unix range",
                        row.line_number
                    ))
                })
        })
        .collect::<CoreResult<Vec<_>>>()?;
    Ok((timestamps, elapsed))
}

/// Subtracts the first sample from every value in a monotonic timeline.
fn rebase_values(values: &[f64]) -> Vec<f64> {
    let origin = values[0];
    values.iter().map(|value| value - origin).collect()
}

/// Combines a calendar date and seconds since midnight into a millisecond-precision RFC 3339 timestamp.
fn date_seconds_to_rfc3339(date: NaiveDate, seconds: f64) -> String {
    let midnight = date
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid for every VBO creation date")
        .and_utc();
    let milliseconds = (seconds * 1_000.0).round() as i64;
    (midnight + Duration::milliseconds(milliseconds)).to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Decodes the VBO numeric `HHMMSS.ss` time-of-day representation into seconds since midnight.
fn time_of_day_seconds(raw: f64, line_number: usize) -> CoreResult<f64> {
    let hours = (raw / 10_000.0).floor();
    let minutes = ((raw % 10_000.0) / 100.0).floor();
    let seconds = raw % 100.0;
    if !(0.0..=23.0).contains(&hours)
        || !(0.0..=59.0).contains(&minutes)
        || !(0.0..60.0).contains(&seconds)
    {
        return Err(CoreError::Activity(format!(
            "VBO line {line_number} contains invalid UTC time-of-day {raw}"
        )));
    }
    Ok(hours * 3600.0 + minutes * 60.0 + seconds)
}

/// Checks that every present latitude and longitude is within its geographic range.
fn validate_coordinates(
    latitude: &[Option<f64>],
    longitude: &[Option<f64>],
    rows: &[DataRow],
) -> CoreResult<()> {
    for ((latitude, longitude), row) in latitude.iter().zip(longitude).zip(rows) {
        if latitude.is_some_and(|value| !(-90.0..=90.0).contains(&value)) {
            return Err(CoreError::Activity(format!(
                "VBO line {} latitude is outside -90..=90 degrees",
                row.line_number
            )));
        }
        if longitude.is_some_and(|value| !(-180.0..=180.0).contains(&value)) {
            return Err(CoreError::Activity(format!(
                "VBO line {} longitude is outside -180..=180 degrees",
                row.line_number
            )));
        }
    }
    Ok(())
}

/// Selects one source column, applies its canonical-unit conversion, and preserves optional absence.
fn selected_series<F>(rows: &[DataRow], index: Option<usize>, conversion: F) -> Vec<Option<f64>>
where
    F: Fn(f64) -> f64,
{
    index
        .map(|index| {
            rows.iter()
                .map(|row| Some(conversion(row.values[index])))
                .collect()
        })
        .unwrap_or_else(|| vec![None; rows.len()])
}

/// Returns a value unchanged for channels already expressed in canonical units.
fn identity(value: f64) -> f64 {
    value
}

/// Converts VBO latitude minutes to decimal degrees.
fn minutes_to_degrees(value: f64) -> f64 {
    value / 60.0
}

/// Converts VBO longitude minutes, which are positive west in VBO data, to canonical degrees.
fn longitude_minutes_to_degrees(value: f64) -> f64 {
    -value / 60.0
}

/// Rebases a cumulative distance channel to zero and rejects decreasing samples.
fn build_distance(rows: &[DataRow], index: Option<usize>) -> CoreResult<Vec<Option<f64>>> {
    let Some(index) = index else {
        return Ok(vec![None; rows.len()]);
    };
    for pair in rows.windows(2) {
        if pair[1].values[index] < pair[0].values[index] {
            return Err(CoreError::Activity(format!(
                "VBO line {} distance must be non-decreasing",
                pair[1].line_number
            )));
        }
    }
    let origin = rows[0].values[index];
    Ok(rows
        .iter()
        .map(|row| Some(row.values[index] - origin))
        .collect())
}
