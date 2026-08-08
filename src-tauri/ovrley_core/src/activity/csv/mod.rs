//! Native CSV activity extraction.
//!
//! This module imports exporter-specific CSV files through one capability-based
//! path. It discovers a telemetry header after any exporter preamble, resolves
//! recognized columns to the canonical activity metrics, converts values to the
//! units expected by the shared finalizer, and rejects rows that cannot be
//! placed on a valid timeline.
//!
//! Header aliases, source precedence, and unit compatibility live in
//! [`headers`]. Column storage, timing reconstruction, duplicate-time
//! coalescing, and metric validation live in [`columns`]. The [`units`] module
//! owns dimensional checks and canonical-unit conversion. The resulting
//! [`ActivityColumns`](crate::activity::schema::ActivityColumns) is finalized
//! through the same path as other native activity sources.

mod columns;
mod data;
mod headers;
mod metrics;
mod parser;
mod timing;
mod types;
mod units;

use crate::activity::finalize::{finalize_activity_columns, FinalizeActivityResponse};
use crate::error::{CoreError, CoreResult};
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Metric {
    /// Elapsed time from the beginning of the activity, in seconds.
    ElapsedSeconds,
    /// An absolute activity timestamp, represented internally as UTC.
    Timestamp,
    /// GPS latitude in decimal degrees.
    Latitude,
    /// GPS longitude in decimal degrees.
    Longitude,
    /// GPS or sensor elevation in metres.
    Elevation,
    /// Pressure or barometric altitude in metres.
    BarometricAltitude,
    /// Ground speed in metres per second.
    Speed,
    /// Course or bearing in degrees clockwise from north.
    Heading,
    /// Cumulative travelled distance in metres.
    Distance,
    /// Distance from the starting home point in metres.
    DistanceToHome,
    /// Scalar acceleration magnitude in standard gravity units.
    GForce,
    /// Lateral or literal X acceleration in standard gravity units.
    GForceX,
    /// Longitudinal or literal Y acceleration in standard gravity units.
    GForceY,
    /// Vertical or literal Z acceleration in standard gravity units.
    GForceZ,
    /// Engine speed in revolutions per minute.
    Rpm,
    /// Throttle position as a percentage from zero to one hundred.
    ThrottlePosition,
    /// Brake position as a percentage from zero to one hundred.
    BrakePosition,
    /// Signed vehicle lean angle in degrees.
    LeanAngle,
    /// Gear position as an unscaled numeric value.
    GearPosition,
    /// Raw date string companion for time-of-day timestamp reconstruction.
    ///
    /// This is not emitted as an activity metric; it supplies the calendar date
    /// paired with a [`Timestamp`](TimingKind::TimeOfDay) column.
    CompanionDate,
    /// Combined GPS coordinate as a space-separated "lat lon" string.
    ///
    /// This is not emitted as an activity metric; it is split into separate
    /// latitude and longitude series during column assembly.
    GpsCoordinate,
    /// Lap number from the source file.
    LapNumber,
}

impl Metric {
    /// Returns whether this metric can establish the source timeline.
    fn is_timing(self) -> bool {
        matches!(self, Self::ElapsedSeconds | Self::Timestamp)
    }

    /// Returns whether a TrackAddict GPS update flag governs this metric.
    fn uses_gps_update(self) -> bool {
        matches!(
            self,
            Self::Latitude
                | Self::Longitude
                | Self::Elevation
                | Self::Speed
                | Self::Heading
                | Self::Distance
                | Self::DistanceToHome
        )
    }
}

/// Opens and parses a native CSV activity path.
///
/// The path's final UTF-8 filename is retained in the finalized activity. An
/// invalid path or filename is returned as a [`CoreError`](crate::error::CoreError),
/// while CSV, header, and timeline failures include CSV import context.
pub fn parse_csv_activity_path(
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
                "CSV path has no valid UTF-8 filename: {}",
                path.display()
            ))
        })?;

    parse_csv_activity(file, file_name, repo_root)
}

/// Parses CSV data from a reader and finalizes canonical columns.
///
/// The reader may contain arbitrary preamble records, an optional compatible
/// units row, blank/comment records, and variable-width data rows. A usable
/// header must contain both a recognized timing basis and at least one
/// recognized telemetry metric. Data is normalized to canonical activity
/// columns and passed through the shared finalizer before being returned.
///
/// Errors include the supplied `file_name` so callers can identify the source
/// even when the input came from an in-memory reader.
pub fn parse_csv_activity_reader<R: Read>(
    reader: R,
    file_name: &str,
) -> CoreResult<FinalizeActivityResponse> {
    parse_csv_activity(reader, file_name, None)
}

fn parse_csv_activity<R: Read>(
    reader: R,
    file_name: &str,
    repo_root: Option<&Path>,
) -> CoreResult<FinalizeActivityResponse> {
    let result = (|| {
        let mut records = csv::ReaderBuilder::new()
            .delimiter(b',')
            .has_headers(false)
            .flexible(true)
            .from_reader(reader)
            .into_records();
        let mut header: Option<headers::HeaderLayout> = None;
        let mut units_row = None;
        let mut data = None;
        let mut preamble = columns::LocalPreamble::default();
        let mut awaiting_units_row = false;
        let mut record_count = 0;

        for (record_index, record) in records.by_ref().enumerate() {
            let record = record.map_err(csv_error)?;
            record_count = record_index + 1;
            if header.is_none() {
                if let Some(candidate) = headers::parse_header_candidate(&record)? {
                    data = Some(columns::CsvColumnData::new(&candidate));
                    header = Some(candidate);
                    awaiting_units_row = true;
                    continue;
                }
            }

            let Some(header) = &header else {
                preamble.observe(&record);
                continue;
            };
            if awaiting_units_row {
                awaiting_units_row = false;
                if headers::is_compatible_units_row(&record, &header.columns) {
                    units_row = Some(record);
                    continue;
                }
            }
            if record.iter().all(|cell| cell.trim().is_empty())
                || record
                    .get(0)
                    .is_some_and(|cell| cell.trim_start().starts_with('#'))
            {
                continue;
            }
            data.as_mut()
                .expect("CSV data storage exists after header discovery")
                .push(record_index, &record);
        }

        let header = header.ok_or_else(|| {
            CoreError::Activity(format!(
                "Unsupported CSV: no usable telemetry header in {record_count} records"
            ))
        })?;
        let data = data.expect("CSV data storage exists after header discovery");
        let columns = columns::build_activity_columns(
            &header,
            units_row.as_ref(),
            &data,
            &preamble,
            file_name,
        )?;

        finalize_activity_columns(&columns, repo_root)
    })();

    result.map_err(|error| CoreError::Activity(format!("CSV import '{file_name}': {error}")))
}

/// Converts a low-level CSV reader error into an activity-domain error.
fn csv_error(error: csv::Error) -> CoreError {
    CoreError::Activity(format!("Invalid CSV: {error}"))
}
