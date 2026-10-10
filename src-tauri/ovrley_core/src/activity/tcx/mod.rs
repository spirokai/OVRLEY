//! Native TCX ingestion through the shared raw-activity finalizer.
//!
//! Extraction is based on python-tcxparser by Vinod Kurup:
//! https://github.com/vkurup/python-tcxparser/tree/master/tcxparser

mod parser;

pub use parser::extract_tcx_activity;

use crate::activity::finalize::{finalize_raw_activity, FinalizeActivityResponse};
use crate::error::{CoreError, CoreResult};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Opens a native TCX file and returns the shared finalized activity response.
pub fn parse_tcx_activity_path(
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
                "TCX path has no valid UTF-8 filename: {}",
                path.display()
            ))
        })?;
    parse_tcx_activity_reader(file, file_name, repo_root)
}

/// Reads UTF-8 TCX XML, extracts aligned samples, and finalizes them exactly once.
pub fn parse_tcx_activity_reader<R: Read>(
    mut reader: R,
    file_name: &str,
    repo_root: Option<&Path>,
) -> CoreResult<FinalizeActivityResponse> {
    let result = (|| {
        let mut text = String::new();
        reader
            .read_to_string(&mut text)
            .map_err(|error| CoreError::Activity(format!("Failed to read TCX input: {error}")))?;
        let raw_activity = extract_tcx_activity(&text, file_name)?;
        finalize_raw_activity(&raw_activity, repo_root)
    })();
    result.map_err(|error| CoreError::Activity(format!("TCX import '{file_name}': {error}")))
}
