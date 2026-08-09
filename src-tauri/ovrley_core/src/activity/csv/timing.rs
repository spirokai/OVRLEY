//! Elapsed-time and absolute-timestamp reconstruction for CSV activities.

use super::data::CsvColumnData;
use super::headers::{HeaderColumn, HeaderLayout, TimingKind};
use super::metrics::{column_unit, parse_number};
use super::units::{convert, Unit};
use super::Metric;
use chrono::{
    DateTime, Local, NaiveDate, NaiveDateTime, NaiveTime, SecondsFormat, TimeDelta, TimeZone, Utc,
};
use csv::StringRecord;

/// An absolute timestamp normalized to UTC.
#[derive(Clone)]
pub(super) struct AbsoluteTimestamp(DateTime<Utc>);

impl AbsoluteTimestamp {
    /// Returns the timestamp as fractional Unix seconds.
    pub(super) fn seconds(&self) -> f64 {
        self.0.timestamp() as f64 + self.0.timestamp_subsec_nanos() as f64 / 1_000_000_000.0
    }

    /// Formats the timestamp as UTC RFC 3339 with millisecond precision.
    pub(super) fn rfc3339(self) -> String {
        self.0.to_rfc3339_opts(SecondsFormat::Millis, true)
    }
}

/// Optional date/time facts collected from recognized exporter preambles.
///
/// These values are transient parser context. They are used only to
/// reconstruct absolute timestamps and are not emitted as activity metadata.
#[derive(Default)]
pub(super) struct LocalPreamble {
    /// Date emitted by AiM-style exports.
    aim_date: Option<NaiveDate>,
    /// Start time emitted by AiM-style exports.
    aim_time: Option<NaiveTime>,
    /// Date emitted by RaceChrono-style exports.
    racechrono_date: Option<NaiveDate>,
    /// Elapsed seconds of start/finish crossings from AiM `Beacon Markers`.
    pub(super) beacon_markers: Vec<f64>,
}

impl LocalPreamble {
    /// Observes one pre-header record for a supported date/time declaration.
    ///
    /// Malformed or unrelated records are ignored because preamble metadata is
    /// optional; the CSV still needs an independent usable timing basis.
    pub(super) fn observe(&mut self, record: &StringRecord) {
        match record
            .get(0)
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("date") => {
                self.aim_date = record.get(1).and_then(|value| {
                    NaiveDate::parse_from_str(value.trim(), "%A, %B %d, %Y").ok()
                });
            }
            Some("time") => {
                self.aim_time = record
                    .get(1)
                    .and_then(|value| NaiveTime::parse_from_str(value.trim(), "%I:%M %p").ok());
            }
            Some("created") => {
                self.racechrono_date = record
                    .get(1)
                    .and_then(|value| NaiveDate::parse_from_str(value.trim(), "%d/%m/%Y").ok());
            }
            Some("beacon markers") => {
                self.beacon_markers = record
                    .iter()
                    .skip(1)
                    .filter_map(|value| value.trim().parse::<f64>().ok())
                    .filter(|value| value.is_finite())
                    .collect();
            }
            _ => {}
        }
    }

    /// Returns the preferred date for local timestamp reconstruction.
    pub(super) fn date(&self) -> Option<NaiveDate> {
        self.racechrono_date.or(self.aim_date)
    }

    /// Returns the AiM local start date and time when both are valid.
    pub(super) fn start(&self) -> Option<NaiveDateTime> {
        self.aim_date
            .zip(self.aim_time)
            .map(|(date, time)| date.and_time(time))
    }
}

/// Selects and reconstructs the best absolute timestamp series.
///
/// Explicit UTC/Unix columns take precedence. Bare time-like columns are
/// accepted when their values are parseable as timestamps. When only elapsed
/// time is available, supported local date/time preamble values provide an
/// optional local-time reconstruction; ambiguous or invalid local times remain
/// missing rather than being guessed.
pub(super) fn selected_absolute_timestamps(
    header: &HeaderLayout,
    units_row: Option<&StringRecord>,
    data: &CsvColumnData,
    preamble: &LocalPreamble,
    elapsed_column: Option<&HeaderColumn>,
    elapsed_unit: Option<Unit>,
) -> Vec<Option<AbsoluteTimestamp>> {
    // Date + time-of-day pair: the date column supplies the calendar date and
    // the time column supplies the clock time. Both are required per row.
    if let Some(tod_column) = header
        .columns
        .iter()
        .find(|column| column.timing == Some(TimingKind::TimeOfDay))
    {
        if let Some(date_column) = header
            .columns
            .iter()
            .find(|column| column.metric == Metric::CompanionDate)
        {
            return (0..data.len())
                .map(|row| {
                    time_of_day_timestamp(
                        data.value(row, date_column.index),
                        data.value(row, tod_column.index),
                    )
                })
                .collect();
        }
    }

    let mut explicit_columns = header
        .columns
        .iter()
        .filter(|column| column.metric == Metric::Timestamp)
        .filter_map(|column| column_unit(column, units_row).map(|unit| (column, unit)))
        .collect::<Vec<_>>();
    explicit_columns.sort_by_key(|(column, _)| (column.priority, column.index));
    if let Some((column, unit)) = explicit_columns.iter().copied().find(|(column, unit)| {
        (0..data.len()).any(|row| {
            parse_absolute_timestamp(data.value(row, column.index), Some(*unit)).is_some()
        })
    }) {
        return (0..data.len())
            .map(|row| parse_absolute_timestamp(data.value(row, column.index), Some(unit)))
            .collect();
    }
    if let Some(column) = header.columns.iter().find(|column| {
        matches!(
            column.timing,
            Some(TimingKind::BareTime | TimingKind::BareTimestamp)
        ) && (0..data.len())
            .any(|row| parse_absolute_timestamp(data.value(row, column.index), None).is_some())
    }) {
        return (0..data.len())
            .map(|row| parse_absolute_timestamp(data.value(row, column.index), None))
            .collect();
    }

    if elapsed_column.is_some_and(|column| column.timing == Some(TimingKind::BareTimestamp)) {
        if let Some(date) = preamble.date() {
            return local_wall_timestamps_from_elapsed(
                date.and_time(NaiveTime::MIN),
                elapsed_column.expect("checked elapsed column"),
                elapsed_unit.expect("elapsed column has a unit"),
                data,
            );
        }
    }

    if let (Some(start), Some(column), Some(unit)) =
        (preamble.start(), elapsed_column, elapsed_unit)
    {
        return local_start_timestamps_from_elapsed(start, column, unit, data);
    }

    vec![None; data.len()]
}

/// Reconstructs a UTC timestamp from a calendar date and a time-of-day string.
///
/// The date is expected in `YYYY-MM-DD` form. The time accepts both
/// `HH:MM:SS` and `HH:MM:SS.fff` shapes. When no timezone is supplied by the
/// source, the combination is interpreted as GMT0/UTC.
fn time_of_day_timestamp(date: Option<&str>, time: Option<&str>) -> Option<AbsoluteTimestamp> {
    let date = NaiveDate::parse_from_str(date?.trim(), "%Y-%m-%d").ok()?;
    let time_str = time?.trim();
    let time = NaiveTime::parse_from_str(time_str, "%H:%M:%S%.f")
        .or_else(|_| NaiveTime::parse_from_str(time_str, "%H:%M:%S"))
        .ok()?;
    Some(AbsoluteTimestamp(
        Utc.from_utc_datetime(&date.and_time(time)),
    ))
}

/// Parses an RFC 3339, ISO 8601 (assumed UTC), English Java date string, or
/// numeric Unix timestamp.
///
/// Numeric values require an explicit `numeric_unit`; this prevents elapsed
/// values from being reinterpreted as absolute time based on their magnitude.
fn parse_absolute_timestamp(
    value: Option<&str>,
    numeric_unit: Option<Unit>,
) -> Option<AbsoluteTimestamp> {
    let value = value?.trim();
    if let Ok(timestamp) = DateTime::parse_from_rfc3339(value) {
        return Some(AbsoluteTimestamp(timestamp.with_timezone(&Utc)));
    }
    if let Ok(timestamp) = DateTime::parse_from_str(value, "%a %b %d %H:%M:%S GMT%:z %Y") {
        return Some(AbsoluteTimestamp(timestamp.with_timezone(&Utc)));
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f") {
        return Some(AbsoluteTimestamp(Utc.from_utc_datetime(&naive)));
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f") {
        return Some(AbsoluteTimestamp(Utc.from_utc_datetime(&naive)));
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
        return Some(AbsoluteTimestamp(Utc.from_utc_datetime(&naive)));
    }
    let seconds = convert(parse_number(Some(value))?, numeric_unit?);
    unix_timestamp(seconds)
}

/// Converts fractional Unix seconds into a UTC timestamp.
///
/// Returns `None` when the value is outside chrono's representable range.
fn unix_timestamp(seconds: f64) -> Option<AbsoluteTimestamp> {
    let whole = seconds.floor() as i64;
    let nanos = ((seconds - whole as f64) * 1_000_000_000.0).round() as u32;
    DateTime::<Utc>::from_timestamp(whole, nanos).map(AbsoluteTimestamp)
}

/// Adds elapsed values to the start of a preamble date in local wall time.
///
/// Each local value is resolved independently so a daylight-saving transition
/// can advance using the timezone's historical rules. Nonexistent or ambiguous
/// local instants produce missing timestamps.
fn local_wall_timestamps_from_elapsed(
    start: NaiveDateTime,
    column: &HeaderColumn,
    unit: Unit,
    data: &CsvColumnData,
) -> Vec<Option<AbsoluteTimestamp>> {
    (0..data.len())
        .map(|row| {
            let elapsed = elapsed_duration(data.value(row, column.index), unit)?;
            let local = start.checked_add_signed(elapsed)?;
            Local
                .from_local_datetime(&local)
                .single()
                .map(|value| AbsoluteTimestamp(value.with_timezone(&Utc)))
        })
        .collect()
}

/// Adds elapsed values to a fully specified local preamble start.
///
/// If the start itself is not unique in the local timezone, the entire derived
/// timestamp series is missing. Otherwise, elapsed values are added on the
/// resolved instant timeline, preserving DST transitions.
fn local_start_timestamps_from_elapsed(
    start: NaiveDateTime,
    column: &HeaderColumn,
    unit: Unit,
    data: &CsvColumnData,
) -> Vec<Option<AbsoluteTimestamp>> {
    let Some(start) = Local.from_local_datetime(&start).single() else {
        return vec![None; data.len()];
    };
    (0..data.len())
        .map(|row| {
            let elapsed = elapsed_duration(data.value(row, column.index), unit)?;
            start
                .checked_add_signed(elapsed)
                .map(|value| AbsoluteTimestamp(value.with_timezone(&Utc)))
        })
        .collect()
}

/// Converts a numeric elapsed value to a millisecond `TimeDelta`.
///
/// Fractional milliseconds are rounded and values outside `TimeDelta`'s
/// representable millisecond range are rejected.
fn elapsed_duration(value: Option<&str>, unit: Unit) -> Option<TimeDelta> {
    let elapsed = convert(parse_number(value)?, unit);
    let milliseconds = (elapsed * 1000.0).round();
    if !(i64::MIN as f64..=i64::MAX as f64).contains(&milliseconds) {
        return None;
    }
    Some(TimeDelta::milliseconds(milliseconds as i64))
}
