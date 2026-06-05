//! Activity parsing validation binary.
//!
//! Reads a raw activity payload and a render config, then produces the
//! dense activity report (`DenseActivityReport`) that the render pipeline
//! consumes. Writes the report as JSON to the `--out` path so developers
//! can inspect the internal frame-by-frame data.
//!
//! This binary is a diagnostic tool — it does no rendering.

use ovrley_core::activity::{build_dense_activity_report_validated, parse_activity_json};
use ovrley_core::commands::parse_and_validate_config;
use std::fs;
use std::path::PathBuf;

use ovrley_core::bin_common::read_arg;

/// Validates that a given activity file and config produce a parse-able
/// dense activity report, and writes the report to disk.
///
/// # Arguments
///
/// * `--payload <path>` — activity JSON (required).
/// * `--config <path>` — render config JSON (required).
/// * `--out <path>` — output path for the dense report JSON (required).
fn main() -> Result<(), String> {
    let args = std::env::args().collect::<Vec<_>>();
    let payload_path = PathBuf::from(read_arg("--payload", &args)?);
    let config_path = PathBuf::from(read_arg("--config", &args)?);
    let out_path = PathBuf::from(read_arg("--out", &args)?);

    let payload_json = fs::read_to_string(&payload_path)
        .map_err(|error| format!("Failed to read {}: {error}", payload_path.display()))?;
    let config_json = fs::read_to_string(&config_path)
        .map_err(|error| format!("Failed to read {}: {error}", config_path.display()))?;

    let activity = parse_activity_json(&payload_json).map_err(|e| e.to_string())?;
    let config = parse_and_validate_config(&config_json).map_err(|e| e.to_string())?;
    let report = build_dense_activity_report_validated(&activity, &config).map_err(|e| e.to_string())?;
    let output = serde_json::to_string_pretty(&report)
        .map_err(|error| format!("Failed to serialize report: {error}"))?;

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create {}: {error}", parent.display()))?;
    }
    fs::write(&out_path, output)
        .map_err(|error| format!("Failed to write {}: {error}", out_path.display()))?;
    Ok(())
}
