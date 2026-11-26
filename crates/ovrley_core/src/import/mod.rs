//! Activity file import: picks the parser by extension and finalizes its samples.
//!
//! Every format parser extracts a [`RawActivity`] or [`ActivityColumns`]; [`finalize`] turns
//! it into the canonical [`ParsedActivity`](crate::activity::schema::ParsedActivity). This
//! module also holds the lenient value readers and numeric helpers the parsers share.

use crate::import::finalize::finalize_samples;
use crate::activity::schema::{ParsedActivity, RawActivity, SmoothingOption};
use crate::error::{CoreError, CoreResult};
use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use roxmltree::{Document, Node};

pub mod csv;
pub mod dive;
pub mod finalize;
pub mod fit;
pub mod gpx;
pub mod igc;
mod metrics;
pub mod srt;
pub mod tcx;
pub mod vbo;

/// The activity file extensions OVRLEY imports, lowercase. The file dialogs offer them
/// case-insensitively, and the drop classifier uses the same list.
pub const ACTIVITY_EXTENSIONS: [&str; 10] = [
    "gpx", "fit", "tcx", "srt", "igc", "csv", "vbo", "uddf", "ssrf", "xml",
];

/// Imports an activity file, picking the parser by its extension case-insensitively.
pub fn load_activity(paths: &crate::paths::AppPaths, path: &Path) -> CoreResult<ParsedActivity> {
    let invalid = || CoreError::Activity(format!("Invalid activity path: {}", path.display()));
    let text = path.to_str().ok_or_else(invalid)?;
    if text.ends_with(['/', '\\']) {
        return Err(invalid());
    }
    let file_name = path.file_name().ok_or_else(invalid)?;
    let extension = Path::new(file_name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let debug_root = Some(paths.repo_root.as_path());
    match extension.as_str() {
        "csv" => csv::parse_path(path, debug_root),
        "fit" => fit::parse_path(path, debug_root),
        "gpx" => gpx::parse_path(path, debug_root),
        "igc" => igc::parse_path(path, debug_root),
        "srt" => srt::parse_path(path, debug_root),
        "ssrf" | "xml" => dive::parse_subsurface_path(path, debug_root),
        "tcx" => tcx::parse_path(path, debug_root),
        "uddf" => dive::parse_uddf_path(path, debug_root),
        "vbo" => vbo::parse_path(path, debug_root),
        _ => Err(CoreError::Activity(format!(
            "Unsupported activity file format: {}",
            file_name.to_string_lossy()
        ))),
    }
}

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
) -> CoreResult<ParsedActivity> {
    let bytes = std::fs::read(path).map_err(|source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    finalize_samples(
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
) -> CoreResult<ParsedActivity> {
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
) -> CoreResult<ParsedActivity> {
    let result = (|| {
        let mut text = String::new();
        reader.read_to_string(&mut text).map_err(|error| {
            CoreError::Activity(format!("Failed to read {format} input: {error}"))
        })?;
        finalize_samples(&extract(&text, file_name)?, repo_root)
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

/// The largest integer below which `f64` represents every integer exactly (2^53 - 1).
pub(super) const MAX_EXACT_INTEGER: f64 = 9_007_199_254_740_991.0;

/// Returns whether every byte is an ASCII digit, which empty input satisfies.
pub(super) fn all_digits(text: impl AsRef<[u8]>) -> bool {
    text.as_ref().iter().all(u8::is_ascii_digit)
}

/// Reads a finite decimal number from trimmed text. Empty, malformed and non-finite text is
/// `None`, and negative zero reads as zero because raw activity values are never signed zeros.
pub(super) fn parse_number(text: &str) -> Option<f64> {
    let value: f64 = text.trim().parse().ok()?;
    value
        .is_finite()
        .then_some(if value == 0.0 { 0.0 } else { value })
}

/// Reads a gear label: numeric text takes its canonical number form, other text is kept trimmed.
pub(super) fn parse_gear(text: &str) -> Option<String> {
    let label = text.trim();
    if label.is_empty() || matches!(label.to_lowercase().as_str(), "n/a" | "na" | "null") {
        return None;
    }
    Some(parse_number(label).map_or_else(|| label.to_string(), number_label))
}

/// The shortest text that reads back to `value`: `3` for 3.0, and `0` for negative zero.
pub(super) fn number_label(value: f64) -> String {
    if value == 0.0 {
        "0".to_string()
    } else {
        value.to_string()
    }
}

/// The bytes `start..end` of a fixed-column record, clamped to the line; empty when the range
/// is empty or splits a character.
pub(super) fn columns(line: &str, start: usize, end: usize) -> &str {
    let end = end.min(line.len());
    line.get(start.min(end)..end).unwrap_or_default()
}

/// Parses a document, naming the format in the error.
pub(super) fn parse_document<'input>(
    text: &'input str,
    format: &str,
) -> CoreResult<Document<'input>> {
    Document::parse(text).map_err(|error| {
        CoreError::Activity(format!("The {format} file could not be parsed: {error}"))
    })
}

pub(super) fn children<'a, 'input>(
    node: Node<'a, 'input>,
    name: &'a str,
    namespace: Option<&'a str>,
) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(move |child| {
        child.is_element()
            && child.tag_name().name() == name
            && child.tag_name().namespace() == namespace
    })
}

/// Returns the first matching child's trimmed text, when it has any.
pub(super) fn child_text<'a>(
    node: Node<'a, '_>,
    name: &'a str,
    namespace: Option<&'a str>,
) -> Option<&'a str> {
    children(node, name, namespace)
        .next()?
        .text()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

/// Returns `Some(value)` when the float is finite, `None` otherwise.
///
/// Every numeric value extracted from telemetry passes through this guard to
/// prevent NaN and infinity from reaching JSON serialization.
#[inline]
pub(crate) fn finite_f64(value: f64) -> Option<f64> {
    value.is_finite().then_some(value)
}

/// Great-circle distance between two GPS coordinates in meters.
///
/// Uses the Haversine formula with Earth radius 6,371,000 m.
pub(crate) fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 6_371_000.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    R * c
}

/// Computes initial GPS bearing in degrees using the great-circle formula.
///
/// Heading derivation needs wrap-safe course direction from two coordinates;
/// invalid inputs return `None` so sparse GPS samples can be skipped without
/// failing the entire activity import.
pub(crate) fn bearing_degrees(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> Option<f64> {
    if !lat1.is_finite() || !lon1.is_finite() || !lat2.is_finite() || !lon2.is_finite() {
        return None;
    }

    let from_lat = lat1.to_radians();
    let to_lat = lat2.to_radians();
    let delta_lon = (lon2 - lon1).to_radians();
    let y = delta_lon.sin() * to_lat.cos();
    let x = from_lat.cos() * to_lat.sin() - from_lat.sin() * to_lat.cos() * delta_lon.cos();
    let bearing = y.atan2(x).to_degrees();
    Some((bearing + 360.0) % 360.0)
}

/// Rounds finite telemetry values to a fixed decimal precision.
///
/// Rounding at fixed assembly points keeps serialized activities stable; NaN and infinity
/// return `None` so invalid math never reaches serialization.
pub(crate) fn round_f64(value: f64, digits: i32) -> Option<f64> {
    if !value.is_finite() {
        return None;
    }
    let scale = 10_f64.powi(digits);
    Some((value * scale).round() / scale)
}

/// Dynamic g-force magnitude from three-axis accelerometer components.
///
/// Returns `magnitude - 1g` so a resting sensor reports 0.0 and positive values
/// mean extra dynamic load. The caller is responsible for applying any unit
/// conversion (`m/s² → g`) or scale factor before passing components in.
/// Returns `None` when the result is non-finite.
pub(crate) fn g_force_from_components(x: f64, y: f64, z: f64) -> Option<f64> {
    let g = (x.powi(2) + y.powi(2) + z.powi(2)).sqrt() - 1.0;
    finite_f64(g)
}

/// Back-fills the missing lateral g-force axis from a lean-angle series.
///
/// When exactly one of `g_force_x` or `g_force_y` is entirely absent and
/// `lean_angle` is present, the missing axis is filled with lateral
/// acceleration computed from lean angle via `tan(angle_rad)`.
/// When both axes are absent, only `g_force_x` is backfilled.
pub(crate) fn backfill_lateral_g_from_lean_angle(
    g_force_x: &[Option<f64>],
    g_force_y: &[Option<f64>],
    lean_angle: &[Option<f64>],
) -> (Vec<Option<f64>>, Vec<Option<f64>>) {
    let x_missing = g_force_x.iter().all(Option::is_none);
    let y_missing = g_force_y.iter().all(Option::is_none);

    let backfill = |lateral: &[Option<f64>]| -> Vec<Option<f64>> {
        lateral
            .iter()
            .zip(lean_angle)
            .map(|(value, lean)| {
                value.or_else(|| lean.and_then(|angle| finite_f64(angle.to_radians().tan())))
            })
            .collect()
    };

    let new_x = if x_missing {
        backfill(g_force_x)
    } else {
        g_force_x.to_vec()
    };

    let new_y = if y_missing && !x_missing {
        backfill(g_force_y)
    } else {
        g_force_y.to_vec()
    };

    (new_x, new_y)
}
