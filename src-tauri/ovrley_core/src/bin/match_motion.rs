//! Standalone motion matching tool.
//!
//! Reads a dev motion dump (`{video}.motion.jsonl`) and a parse-debug JSON
//! that contains a `parsed_activity` field, runs the active matcher, and writes
//! its canonical result as JSON.
//!
//! # Arguments
//!
//! * `--motion <path>` — motion JSONL dump from dev decoding (required).
//! * `--activity <path>` — parse-debug JSON containing `parsed_activity` (required).
//! * `--out <path>` — output path for the comparison JSON (required).

use ovrley_core::activity::parse_activity_json;
use ovrley_core::bin_common::read_arg;
use ovrley_core::synchronization::fingerprint::VideoFingerprintBuilder;
use ovrley_core::synchronization::matcher::match_fingerprints;
use ovrley_core::synchronization::motion_estimation::MotionInterval;
use ovrley_core::synchronization::{fingerprint, Cancellation};
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), String> {
    let args = std::env::args().collect::<Vec<_>>();
    let motion_path = PathBuf::from(read_arg("--motion", &args)?);
    let activity_path = PathBuf::from(read_arg("--activity", &args)?);
    let out_path = PathBuf::from(read_arg("--out", &args)?);

    let motion_jsonl = fs::read_to_string(&motion_path)
        .map_err(|error| format!("Failed to read {}: {error}", motion_path.display()))?;
    let mut intervals = Vec::new();
    for (index, line) in motion_jsonl.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let interval: MotionInterval = serde_json::from_str(line)
            .map_err(|error| format!("Failed to parse motion line {}: {error}", index + 1))?;
        intervals.push(interval);
    }

    let activity_debug_json = fs::read_to_string(&activity_path)
        .map_err(|error| format!("Failed to read {}: {error}", activity_path.display()))?;
    let activity_debug: serde_json::Value = serde_json::from_str(&activity_debug_json)
        .map_err(|error| format!("Failed to parse activity debug JSON: {error}"))?;
    let parsed_activity_value = activity_debug
        .get("parsed_activity")
        .ok_or("Activity debug JSON is missing 'parsed_activity' field")?;
    let parsed_activity_json = serde_json::to_string(parsed_activity_value)
        .map_err(|error| format!("Failed to re-serialize parsed_activity: {error}"))?;
    let parsed_activity =
        parse_activity_json(&parsed_activity_json).map_err(|error| error.to_string())?;

    let mut video = VideoFingerprintBuilder::default();
    for interval in intervals {
        video.push(interval);
    }
    let video = video.finish();
    let activity = fingerprint::prepare_activity(&parsed_activity);

    let cancellation = Cancellation::default();
    let output = match_fingerprints(&activity, &video, &cancellation)
        .map_err(|error| format!("Motion matching failed: {error}"))?;
    let output_json = serde_json::to_string_pretty(&output)
        .map_err(|error| format!("Failed to serialize results: {error}"))?;

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create {}: {error}", parent.display()))?;
    }
    fs::write(&out_path, output_json)
        .map_err(|error| format!("Failed to write {}: {error}", out_path.display()))?;

    eprintln!("Wrote match results to {}", out_path.display());
    Ok(())
}
