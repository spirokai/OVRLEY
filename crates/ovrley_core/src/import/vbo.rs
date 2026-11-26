//! Native Racelogic VBOX extraction.
//!
//! VBO values are validated once at this boundary, converted to canonical activity units and
//! assembled directly as [`ActivityColumns`] for the columnar finalizer. Channel names are
//! normalized and resolved to canonical metrics first.

use crate::activity::schema::{ActivityColumns, LapMarkers, ParsedActivity, RawActivityOptions, SmoothingOption, TimingMarker, TimingMarkerKind};
use crate::error::{CoreError, CoreResult};
use crate::import::backfill_lateral_g_from_lean_angle;
use crate::import::finalize::finalize_columns;
use chrono::{Duration, NaiveDate, NaiveTime, SecondsFormat, Utc};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
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
pub fn parse_path(
    path: &Path,
    repo_root: Option<&Path>,
) -> CoreResult<ParsedActivity> {
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

    parse_reader(file, file_name, repo_root)
}

/// Reads VBO data, converts it to canonical activity columns, and finalizes the activity.
///
/// The reader must contain UTF-8 VBO text. Required sections, numeric values, channel widths,
/// telemetry mappings, and timelines are validated at this boundary. `repo_root` is forwarded to
/// the shared activity finalizer for any repository-relative metadata it needs to resolve.
pub fn parse_reader<R: Read>(
    mut reader: R,
    file_name: &str,
    repo_root: Option<&Path>,
) -> CoreResult<ParsedActivity> {
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
        finalize_columns(&columns, repo_root)
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
    let layout = resolve(&sections.header, &identifiers)?;
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
    let lean_angle = series(layout.lean_angle, identity);
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
            smoothing: [
                (
                    "pace".to_string(),
                    SmoothingOption {
                        enabled: true,
                        method: "zero_phase_ma".to_string(),
                        window_seconds: 5.0,
                    },
                ),
                (
                    "heading".to_string(),
                    SmoothingOption {
                        enabled: true,
                        method: "circular_ema".to_string(),
                        window_seconds: 1.0,
                    },
                ),
            ]
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
        depth: empty(),
        first_stop_depth: empty(),
        first_stop_time: empty(),
        time_to_surface: empty(),
        average_ppo2: empty(),
        fraction_o2: empty(),
        fraction_he: empty(),
        water_temperature: empty(),
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
        engine_load: series(layout.engine_load, identity),
        coolant_temperature: empty(),
        boost_pressure: empty(),
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
    layout: &ChannelLayout,
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

/// Device/source suffixes that are retained as channel qualifiers during normalization.
const DEVICE_QUALIFIERS: &[&str] = &[
    "acc", "baro", "canbus", "data", "gps", "gyro", "hrm", "lap", "logger", "magn", "obd", "wheel",
];
/// Qualifiers indicating calculated or estimated telemetry.
const OTHER_QUALIFIERS: &[&str] = &["calc", "calculated", "estimate", "estimated"];
/// Qualifiers identifying a physical position or numbered channel.
const POSITION_QUALIFIERS: &[&str] = &["front", "rear", "rr", "rl", "fr", "fl"];
/// Tokens recognized as measurement-unit suffixes and removed from metric bases.
const UNIT_TOKENS: &[&str] = &[
    "c", "deg", "degree", "degrees", "g", "hz", "kmh", "kph", "knots", "knot", "kts", "m", "meter",
    "meters", "metre", "metres", "mph", "pct", "percent", "s", "sec", "second", "seconds", "utc",
];

/// Units supported by VBO speed-like channels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SpeedUnit {
    /// Kilometers per hour.
    KilometersPerHour,
    /// Meters per second.
    MetersPerSecond,
    /// Miles per hour.
    MilesPerHour,
    /// Nautical miles per hour.
    Knots,
}

impl SpeedUnit {
    /// Converts a value in this unit to meters per second.
    fn to_meters_per_second(self, value: f64) -> f64 {
        match self {
            Self::KilometersPerHour => value / 3.6,
            Self::MetersPerSecond => value,
            Self::MilesPerHour => value * 0.447_04,
            Self::Knots => value * 0.514_444_444_444_444_5,
        }
    }
}

/// Normalized representation of one source channel from the VBO declarations.
#[derive(Clone, Debug)]
struct ChannelName {
    /// Original identifier from `[column names]` or the header fallback.
    identifier: String,
    /// One or more normalized metric bases from the identifier and optional header.
    bases: Vec<String>,
    /// Normalized source, calculation, unit, and position qualifiers.
    qualifiers: BTreeSet<String>,
    /// Original `[header]` text, when available for unit resolution and diagnostics.
    header: Option<String>,
}

/// Resolved source-column indexes and canonical units for supported VBO metrics.
#[derive(Clone, Debug)]
pub(crate) struct ChannelLayout {
    /// Number of values required in every data row.
    pub(super) column_count: usize,
    /// Source index of the time-of-day channel.
    pub(super) time: Option<usize>,
    /// Source index of the Unix timestamp channel.
    pub(super) timestamp: Option<usize>,
    /// Source index of the relative elapsed-time channel.
    pub(super) elapsed_time: Option<usize>,
    /// Source index of latitude in VBO minutes.
    pub(super) latitude: Option<usize>,
    /// Source index of longitude in VBO minutes.
    pub(super) longitude: Option<usize>,
    /// Source index of speed.
    pub(super) speed: Option<usize>,
    /// Source index of heading.
    pub(super) heading: Option<usize>,
    /// Source index of elevation.
    pub(super) elevation: Option<usize>,
    /// Source index of vertical speed.
    pub(super) vertical_speed: Option<usize>,
    /// Source index of cumulative distance.
    pub(super) distance: Option<usize>,
    /// Source index of combined g-force.
    pub(super) g_force: Option<usize>,
    /// Source index of lateral or X-axis g-force.
    pub(super) g_force_x: Option<usize>,
    /// Source index of longitudinal or Y-axis g-force.
    pub(super) g_force_y: Option<usize>,
    /// Source index of vertical or Z-axis g-force.
    pub(super) g_force_z: Option<usize>,
    /// Source index of engine RPM.
    pub(super) rpm: Option<usize>,
    /// Source index of throttle position.
    pub(super) throttle_position: Option<usize>,
    /// Source index of brake position.
    pub(super) brake_position: Option<usize>,
    /// Source index of engine load.
    pub(super) engine_load: Option<usize>,
    /// Source index of lean angle.
    pub(super) lean_angle: Option<usize>,
    /// Source index of gear position.
    pub(super) gear_position: Option<usize>,
    /// Unit used to convert the selected speed channel.
    speed_unit: SpeedUnit,
    /// Unit used to convert the selected vertical-speed channel.
    vertical_speed_unit: SpeedUnit,
}

impl ChannelLayout {
    /// Converts a speed-channel value to meters per second.
    fn speed_to_meters_per_second(&self, value: f64) -> f64 {
        self.speed_unit.to_meters_per_second(value)
    }

    /// Converts a vertical-speed-channel value to meters per second.
    fn vertical_speed_to_meters_per_second(&self, value: f64) -> f64 {
        self.vertical_speed_unit.to_meters_per_second(value)
    }
}

/// Canonical telemetry metric that can be selected from a VBO channel declaration.
#[derive(Clone, Copy)]
enum Metric {
    /// Time of day encoded as `HHMMSS.ss`.
    Time,
    /// Unix timestamp in seconds.
    Timestamp,
    /// Relative elapsed time.
    ElapsedTime,
    /// Latitude in VBO minutes.
    Latitude,
    /// Longitude in VBO minutes.
    Longitude,
    /// Horizontal speed.
    Speed,
    /// Direction of travel.
    Heading,
    /// Elevation or altitude.
    Elevation,
    /// Vertical speed.
    VerticalSpeed,
    /// Cumulative traveled distance.
    Distance,
    /// Combined acceleration magnitude.
    GForce,
    /// Lateral or X-axis acceleration.
    GForceX,
    /// Longitudinal or Y-axis acceleration.
    GForceY,
    /// Vertical or Z-axis acceleration.
    GForceZ,
    /// Engine revolutions per minute.
    Rpm,
    /// Throttle or accelerator position.
    ThrottlePosition,
    /// Brake position.
    BrakePosition,
    /// Engine load as a percentage from zero to one hundred.
    EngineLoad,
    /// Vehicle lean angle.
    LeanAngle,
    /// Gear position.
    GearPosition,
}

impl Metric {
    /// Returns accepted normalized source-name aliases in priority order.
    fn aliases(self) -> &'static [&'static str] {
        match self {
            Self::Time => &["time", "utc_time"],
            Self::Timestamp => &["timestamp"],
            Self::ElapsedTime => &["elapsed_time"],
            Self::Latitude => &["lat", "latitude"],
            Self::Longitude => &["long", "lon", "longitude"],
            Self::Speed => &["velocity", "speed"],
            Self::Heading => &["heading", "bearing"],
            Self::Elevation => &["height", "altitude"],
            Self::VerticalSpeed => &["vertvel", "vert_vel", "vertical_velocity", "vertical_speed"],
            Self::Distance => &["distance", "distance_traveled", "distance_travelled"],
            Self::GForce => &["combined_acc", "g_force"],
            Self::GForceX => &[
                "lateral_acc",
                "lat_acc",
                "latacc",
                "lateral_accel",
                "lat_accel",
                "x_acc",
            ],
            Self::GForceY => &[
                "longitudinal_acc",
                "long_acc",
                "longacc",
                "longitudinal_accel",
                "long_accel",
                "y_acc",
            ],
            Self::GForceZ => &["vertical_acc", "vert_acc", "vertacc", "z_acc"],
            Self::Rpm => &["rpm", "engine_rpm"],
            Self::ThrottlePosition => &[
                "accelerator_pos",
                "accelerator_position",
                "throttle_pos",
                "throttle_position",
            ],
            Self::BrakePosition => &["brake_pos", "brake_position"],
            Self::EngineLoad => &["engine_load", "engine load"],
            Self::LeanAngle => &["lean_angle"],
            Self::GearPosition => &["gear", "gear_position"],
        }
    }

    /// Ranks a channel by the preferred source for this metric.
    fn source_rank(self, channel: &ChannelName) -> usize {
        let has = |qualifier| channel.qualifiers.contains(qualifier);
        match self {
            Self::GForce
            | Self::GForceX
            | Self::GForceY
            | Self::GForceZ => {
                if has("canbus") {
                    0
                } else if has("acc") {
                    1
                } else if has("gps") {
                    2
                } else if has("calc") || has("calculated") {
                    3
                } else {
                    4
                }
            }
            Self::Rpm
            | Self::ThrottlePosition
            | Self::BrakePosition
            | Self::EngineLoad
            | Self::GearPosition => {
                if has("canbus") {
                    0
                } else if has("obd") {
                    1
                } else if has("calc") || has("calculated") {
                    3
                } else {
                    2
                }
            }
            _ => {
                if has("gps") {
                    0
                } else if has("calc") || has("calculated") {
                    2
                } else if has("canbus") || has("obd") {
                    3
                } else {
                    1
                }
            }
        }
    }

    /// Indicates whether source priority should be compared before alias priority.
    fn source_precedes_alias(self) -> bool {
        matches!(
            self,
            Self::GForce
                | Self::GForceX
                    | Self::GForceY
                | Self::GForceZ
                | Self::Rpm
                | Self::ThrottlePosition
                | Self::BrakePosition
                | Self::EngineLoad
                | Self::GearPosition
        )
    }
}

/// Resolves VBO channel declarations into one unambiguous canonical layout.
///
/// The resolver validates declaration width, duplicate identifiers, required timeline and paired
/// coordinate channels, metric conflicts, supported telemetry presence, and declared speed units.
pub(super) fn resolve(header: &[String], identifiers: &[String]) -> CoreResult<ChannelLayout> {
    if !header.is_empty() && header.len() != identifiers.len() {
        return Err(CoreError::Activity(format!(
            "VBO header declares {} channels but the column layout declares {}",
            header.len(),
            identifiers.len()
        )));
    }

    let mut unique_identifiers = BTreeSet::new();
    for identifier in identifiers {
        if !unique_identifiers.insert(identifier.to_ascii_lowercase()) {
            return Err(CoreError::Activity(format!(
                "VBO column layout declares '{identifier}' more than once"
            )));
        }
    }

    let channels = identifiers
        .iter()
        .enumerate()
        .map(|(index, identifier)| channel_name(identifier, header.get(index)))
        .collect::<Vec<_>>();
    let time = resolve_metric(&channels, Metric::Time);
    let timestamp = resolve_metric(&channels, Metric::Timestamp);
    let elapsed_time = resolve_metric(&channels, Metric::ElapsedTime);
    if time.is_none() && timestamp.is_none() && elapsed_time.is_none() {
        return Err(CoreError::Activity(
            "VBO column layout has no time, timestamp, or elapsed_time column".to_string(),
        ));
    }
    let latitude = resolve_metric(&channels, Metric::Latitude);
    let longitude = resolve_metric(&channels, Metric::Longitude);
    if latitude.is_some() != longitude.is_some() {
        return Err(CoreError::Activity(
            "VBO column layout must declare latitude and longitude together".to_string(),
        ));
    }

    let speed = resolve_metric(&channels, Metric::Speed);
    let heading = resolve_metric(&channels, Metric::Heading);
    let elevation = resolve_metric(&channels, Metric::Elevation);
    let vertical_speed = resolve_metric(&channels, Metric::VerticalSpeed);
    let distance = resolve_metric(&channels, Metric::Distance);
    let g_force = resolve_metric(&channels, Metric::GForce);
    let g_force_x = resolve_metric(&channels, Metric::GForceX);
    let g_force_y = resolve_metric(&channels, Metric::GForceY);
    let g_force_z = resolve_metric(&channels, Metric::GForceZ);
    let rpm = resolve_metric(&channels, Metric::Rpm);
    let throttle_position = resolve_metric(&channels, Metric::ThrottlePosition);
    let brake_position = resolve_metric(&channels, Metric::BrakePosition);
    let engine_load = resolve_metric(&channels, Metric::EngineLoad);
    let lean_angle = resolve_metric(&channels, Metric::LeanAngle);
    let gear_position = resolve_metric(&channels, Metric::GearPosition);
    let mut metric_owners = BTreeMap::new();
    for (metric, index) in [
        ("time", time),
        ("timestamp", timestamp),
        ("elapsed_time", elapsed_time),
        ("latitude", latitude),
        ("longitude", longitude),
        ("speed", speed),
        ("heading", heading),
        ("elevation", elevation),
        ("vertical_speed", vertical_speed),
        ("distance", distance),
        ("g_force", g_force),
        ("g_force_x", g_force_x),
        ("g_force_y", g_force_y),
        ("g_force_z", g_force_z),
        ("rpm", rpm),
        ("throttle_position", throttle_position),
        ("brake_position", brake_position),
        ("engine_load", engine_load),
        ("lean_angle", lean_angle),
        ("gear_position", gear_position),
    ] {
        let Some(index) = index else {
            continue;
        };
        if let Some(existing) = metric_owners.insert(index, metric) {
            return Err(CoreError::Activity(format!(
                "VBO channel '{}' resolves to conflicting metrics {existing} and {metric}",
                identifiers[index]
            )));
        }
    }
    if [
        latitude,
        speed,
        heading,
        elevation,
        vertical_speed,
        distance,
        g_force,
        g_force_x,
        g_force_y,
        g_force_z,
        rpm,
        throttle_position,
        brake_position,
        engine_load,
        lean_angle,
        gear_position,
    ]
    .iter()
    .all(Option::is_none)
    {
        return Err(CoreError::Activity(
            "VBO column layout has no supported telemetry columns".to_string(),
        ));
    }

    Ok(ChannelLayout {
        column_count: identifiers.len(),
        time,
        timestamp,
        elapsed_time,
        latitude,
        longitude,
        speed,
        heading,
        elevation,
        vertical_speed,
        distance,
        g_force,
        g_force_x,
        g_force_y,
        g_force_z,
        rpm,
        throttle_position,
        brake_position,
        engine_load,
        lean_angle,
        gear_position,
        speed_unit: speed
            .map(|index| speed_unit(&channels[index], SpeedUnit::KilometersPerHour))
            .transpose()?
            .unwrap_or(SpeedUnit::KilometersPerHour),
        vertical_speed_unit: vertical_speed
            .map(|index| {
                let default = if channels[index]
                    .bases
                    .iter()
                    .any(|base| matches!(base.as_str(), "vertvel" | "vert_vel"))
                {
                    SpeedUnit::KilometersPerHour
                } else {
                    SpeedUnit::MetersPerSecond
                };
                speed_unit(&channels[index], default)
            })
            .transpose()?
            .unwrap_or(SpeedUnit::MetersPerSecond),
    })
}

/// Combines an identifier and optional header entry into one normalized channel description.
fn channel_name(identifier: &str, header: Option<&String>) -> ChannelName {
    let identifier_name = parse_name(identifier);
    let header_name = header.map(|value| parse_name(value));
    let mut bases = vec![identifier_name.0];
    let mut qualifiers = identifier_name.1;
    if let Some((base, header_qualifiers)) = header_name {
        if !bases.contains(&base) {
            bases.push(base);
        }
        qualifiers.extend(header_qualifiers);
    }
    ChannelName {
        identifier: identifier.to_string(),
        bases,
        qualifiers,
        header: header.cloned(),
    }
}

/// Normalizes a source channel name into a metric base and qualifier set.
///
/// RaceChrono prefixes, separator variants, unit suffixes, source qualifiers, and supported
/// numbered-position suffixes are handled here. Names containing unsupported punctuation remain
/// opaque instead of being repaired into a potentially different metric.
fn parse_name(raw: &str) -> (String, BTreeSet<String>) {
    if !raw.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '/' | ' ' | '\t')
    }) {
        return (raw.to_ascii_lowercase(), BTreeSet::new());
    }
    let lower = raw.to_ascii_lowercase();
    let without_prefix = lower.strip_prefix("rc_").unwrap_or(&lower);
    let normalized = normalize_separators(without_prefix);
    if matches!(normalized.as_str(), "event1" | "event_1") {
        return ("event".to_string(), BTreeSet::new());
    }
    let mut tokens = normalized
        .split('_')
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut qualifiers = BTreeSet::new();
    while tokens.len() > 1 && has_trailing_unit(&tokens) {
        if tokens.len() > 2
            && is_compound_unit_pair(
                tokens[tokens.len() - 2].as_str(),
                tokens[tokens.len() - 1].as_str(),
            )
        {
            tokens.truncate(tokens.len() - 2);
        } else {
            tokens.pop();
        }
    }
    let token_count = tokens.len();
    if let Some(qualifier) = tokens.pop_if(|last| {
        DEVICE_QUALIFIERS.contains(&last.as_str()) && (last != "acc" || token_count > 2)
    }) {
        qualifiers.insert(qualifier);
    }
    if let Some(qualifier) = tokens.pop_if(|last| OTHER_QUALIFIERS.contains(&last.as_str())) {
        qualifiers.insert(qualifier);
    }
    if let Some(number) = tokens.last().and_then(|last| last.parse::<u8>().ok()) {
        if (1..=63).contains(&number) {
            tokens.pop();
            if tokens
                .last()
                .is_some_and(|last| POSITION_QUALIFIERS.contains(&last.as_str()))
            {
                tokens.pop();
            }
        }
    } else if tokens
        .last()
        .is_some_and(|last| POSITION_QUALIFIERS.contains(&last.as_str()))
    {
        tokens.pop();
    }
    if tokens.len() > 1 && tokens[0] == "gps" {
        qualifiers.insert(tokens.remove(0));
    }
    (tokens.join("_"), qualifiers)
}

/// Reports whether the token list ends in a recognized unit or compound unit.
fn has_trailing_unit(tokens: &[String]) -> bool {
    tokens
        .last()
        .is_some_and(|last| UNIT_TOKENS.contains(&last.as_str()))
        || (tokens.len() > 2
            && is_compound_unit_pair(
                tokens[tokens.len() - 2].as_str(),
                tokens[tokens.len() - 1].as_str(),
            ))
}

/// Lowercases a channel name and collapses non-alphanumeric runs into underscores.
fn normalize_separators(raw: &str) -> String {
    let mut normalized = String::with_capacity(raw.len());
    let mut separator_pending = false;
    for character in raw.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            if separator_pending && !normalized.is_empty() {
                normalized.push('_');
            }
            normalized.push(character);
            separator_pending = false;
        } else {
            separator_pending = true;
        }
    }
    normalized
}

/// Reports whether two adjacent tokens form a supported compound unit such as `km/h`.
fn is_compound_unit_pair(left: &str, right: &str) -> bool {
    matches!(
        (left, right),
        ("km", "h") | ("km", "hr") | ("m", "s") | ("m", "sec")
    )
}

/// Selects the best source column for a metric using alias, source, and declaration order.
fn resolve_metric(channels: &[ChannelName], metric: Metric) -> Option<usize> {
    channels
        .iter()
        .enumerate()
        .filter_map(|(index, channel)| {
            let alias_rank = metric
                .aliases()
                .iter()
                .position(|alias| channel.bases.iter().any(|base| base == alias))?;
            let source_rank = metric.source_rank(channel);
            let score = if metric.source_precedes_alias() {
                (source_rank, alias_rank, index)
            } else {
                (alias_rank, source_rank, index)
            };
            Some((score, index))
        })
        .min_by_key(|(score, _)| *score)
        .map(|(_, index)| index)
}

/// Resolves a channel's declared speed-like unit from its identifier and header.
///
/// Conflicting declarations are rejected; an absent declaration uses the metric-specific default.
fn speed_unit(channel: &ChannelName, default: SpeedUnit) -> CoreResult<SpeedUnit> {
    let identifier_unit = declared_speed_unit(&channel.identifier);
    let header_unit = channel.header.as_deref().and_then(declared_speed_unit);
    if identifier_unit.is_some() && header_unit.is_some() && identifier_unit != header_unit {
        return Err(CoreError::Activity(format!(
            "VBO channel '{}' declares conflicting units in [column names] and [header]",
            channel.identifier
        )));
    }
    Ok(identifier_unit.or(header_unit).unwrap_or(default))
}

/// Extracts a supported speed-like unit declaration from a raw channel name.
fn declared_speed_unit(name: &str) -> Option<SpeedUnit> {
    let normalized = normalize_separators(name);
    let tokens = normalized.split('_').collect::<Vec<_>>();
    if tokens.iter().any(|token| matches!(*token, "mph")) {
        Some(SpeedUnit::MilesPerHour)
    } else if tokens
        .iter()
        .any(|token| matches!(*token, "knot" | "knots" | "kts"))
    {
        Some(SpeedUnit::Knots)
    } else if tokens.iter().any(|token| matches!(*token, "kmh" | "kph"))
        || tokens
            .windows(2)
            .any(|pair| is_compound_unit_pair(pair[0], pair[1]) && pair[0] == "km")
    {
        Some(SpeedUnit::KilometersPerHour)
    } else if tokens
        .windows(2)
        .any(|pair| is_compound_unit_pair(pair[0], pair[1]) && pair[0] == "m")
        || tokens.contains(&"mps")
    {
        Some(SpeedUnit::MetersPerSecond)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::parse_name;

    /// Keeps RaceChrono suffix parsing in its documented order and range.
    #[test]
    fn racechrono_suffixes_follow_the_documented_order_and_range() {
        assert_eq!(parse_name("speed_front_1-lap").0, "speed");
        assert_eq!(parse_name("speed_63-canbus").0, "speed");
        assert_eq!(parse_name("speed_0-canbus").0, "speed_0");
        assert_eq!(parse_name("speed_64-canbus").0, "speed_64");
        assert_eq!(parse_name("speed_1_front").0, "speed_1");
    }

    /// Applies the RaceChrono prefix and historical event aliases exactly.
    #[test]
    fn racechrono_prefix_and_historical_event_aliases_are_exact() {
        assert_eq!(parse_name("rc_brake_pos").0, "brake_pos");
        assert_eq!(parse_name("rc-brake-pos").0, "rc_brake_pos");
        assert_eq!(parse_name("event1").0, "event");
        assert_eq!(parse_name("event-1").0, "event");
        assert_ne!(parse_name("speed!!!gps").0, "speed");
    }
}
