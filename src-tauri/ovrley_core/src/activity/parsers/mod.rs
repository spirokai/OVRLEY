//! Native activity file extraction into the canonical raw activity contract.

use crate::activity::finalize::{finalize_raw_activity, FinalizeActivityResponse};
use crate::activity::schema::{RawActivity, SmoothingOption};
use crate::error::{CoreError, CoreResult};
use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

mod dive_xml;
mod helpers;
/// Native FIT extraction.
pub mod fit;
/// Native GPX extraction.
pub mod gpx;
/// Native IGC flight-log extraction.
pub mod igc;
/// Native DJI SRT telemetry extraction.
pub mod srt;
/// Native Subsurface XML dive extraction.
pub mod subsurface;
/// Native Garmin TCX extraction into canonical raw activity samples.
pub mod tcx;
/// Native Universal Dive Data Format extraction.
pub mod uddf;

const BAR_PER_ATMOSPHERE: f64 = 1.01325;

/// `(metric, method, window_seconds)` smoothing requests for GPS track formats.
const TRACK_SMOOTHING: [(&str, &str, f64); 2] = [
    ("pace", "zero_phase_ma", 5.0),
    ("heading", "circular_ema", 3.0),
];

/// Reads a whole file, leaving its decoding to a lenient format parser.
fn parse_bytes_path(
    path: &Path,
    repo_root: Option<&Path>,
    format: &str,
    extract: fn(&[u8], &str) -> CoreResult<RawActivity>,
) -> CoreResult<FinalizeActivityResponse> {
    let bytes = std::fs::read(path).map_err(|source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    finalize_raw_activity(
        &extract(&bytes, activity_file_name(path, format)?)?,
        repo_root,
    )
}

/// Opens a file for a format that must be valid UTF-8.
fn parse_text_path(
    path: &Path,
    repo_root: Option<&Path>,
    format: &str,
    extract: fn(&str, &str) -> CoreResult<RawActivity>,
) -> CoreResult<FinalizeActivityResponse> {
    let file = File::open(path).map_err(|source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let file_name = activity_file_name(path, format)?;
    parse_text_reader(file, file_name, repo_root, format, extract)
}

/// Reads valid UTF-8 and finalizes it, naming the format and file in every error.
fn parse_text_reader<R: Read>(
    mut reader: R,
    file_name: &str,
    repo_root: Option<&Path>,
    format: &str,
    extract: fn(&str, &str) -> CoreResult<RawActivity>,
) -> CoreResult<FinalizeActivityResponse> {
    let result = (|| {
        let mut text = String::new();
        reader.read_to_string(&mut text).map_err(|error| {
            CoreError::Activity(format!("Failed to read {format} input: {error}"))
        })?;
        finalize_raw_activity(&extract(&text, file_name)?, repo_root)
    })();
    result.map_err(|error| CoreError::Activity(format!("{format} import '{file_name}': {error}")))
}

fn activity_file_name<'a>(path: &'a Path, format: &str) -> CoreResult<&'a str> {
    path.file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            CoreError::Activity(format!(
                "{format} path has no valid UTF-8 filename: {}",
                path.display()
            ))
        })
}

/// Decodes file bytes as UTF-8 with replacement, dropping a leading byte order mark.
fn decode_file_text(bytes: &[u8]) -> Cow<'_, str> {
    String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes))
}

/// Builds enabled smoothing options from `(metric, method, window_seconds)` requests.
fn smoothing_options(requests: &[(&str, &str, f64)]) -> BTreeMap<String, SmoothingOption> {
    requests
        .iter()
        .map(|(metric, method, window_seconds)| {
            (
                metric.to_string(),
                SmoothingOption {
                    enabled: true,
                    method: method.to_string(),
                    window_seconds: *window_seconds,
                },
            )
        })
        .collect()
}

fn format_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn seconds_between<Tz: TimeZone>(start: DateTime<Tz>, end: DateTime<Tz>) -> f64 {
    end.signed_duration_since(start).num_milliseconds() as f64 / 1000.0
}
