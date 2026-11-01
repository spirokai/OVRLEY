//! Native activity file extraction into the canonical raw activity contract.

use crate::error::{CoreError, CoreResult};
use std::borrow::Cow;
use std::path::Path;

mod dive_xml;
/// Native GPX extraction.
pub mod gpx;
/// Native IGC flight-log extraction.
pub mod igc;
/// Lenient readers for sensor values in external activity files.
pub mod raw_values;
/// Native Subsurface XML dive extraction.
pub mod subsurface;
/// Native Garmin TCX extraction into canonical raw activity samples.
pub mod tcx;
/// Native Universal Dive Data Format extraction.
pub mod uddf;

/// Reads a native activity file and its UTF-8 file name.
fn read_activity_file<'a>(path: &'a Path, format: &str) -> CoreResult<(Vec<u8>, &'a str)> {
    let bytes = std::fs::read(path).map_err(|source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            CoreError::Activity(format!(
                "{format} path has no valid UTF-8 filename: {}",
                path.display()
            ))
        })?;
    Ok((bytes, file_name))
}

/// Decodes file bytes as UTF-8 with replacement, dropping a leading byte order mark.
fn decode_file_text(bytes: &[u8]) -> Cow<'_, str> {
    String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes))
}
