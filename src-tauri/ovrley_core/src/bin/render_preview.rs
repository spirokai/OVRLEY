//! Offline preview render binary.
//!
//! Renders one or more still-frame preview images (PNG) from an activity
//! payload and overlay config. Used as a subprocess by the frontend to
//! generate the preview thumbnails shown in the timeline scrubber.
//!
//! Supports single-second (`--second`) and batch (`--seconds a,b,c`)
//! previews. Outputs a timing summary JSON to stdout, and optionally a
//! separate timing file via `--timing-out`.
//!
//! Does not own: rendering — delegates to `ovrley_core::render`.

use ovrley_core::activity::{build_dense_activity_report_validated, parse_activity_json};
use ovrley_core::commands::parse_and_validate_config;
use ovrley_core::paths::AppPaths;
use ovrley_core::render::{render_preview_frames, PreviewFrame};
use std::fs;
use std::path::PathBuf;

use ovrley_core::bin_common::{read_arg, read_optional_arg, repo_root};

/// Parses `--seconds a,b,c` or `--second N` into a sorted list of activity
/// seconds to render. Returns an error when no valid seconds are provided.
fn parse_seconds(args: &[String]) -> Result<Vec<f64>, String> {
    if let Some(seconds) = read_optional_arg("--seconds", args) {
        let parsed = seconds
            .split(',')
            .filter(|value| !value.trim().is_empty())
            .map(|value| {
                value
                    .trim()
                    .parse::<f64>()
                    .map_err(|error| format!("Invalid second value '{value}': {error}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if parsed.is_empty() {
            return Err("--seconds must contain at least one value".to_string());
        }
        return Ok(parsed);
    }

    Ok(vec![read_arg("--second", args)?.parse::<f64>().map_err(
        |error| format!("Invalid --second value: {error}"),
    )?])
}

/// Renders one or more preview frames and outputs their timing report.
///
/// The first frame is written to `--out` verbatim; later frames append
/// `_<second>` so multi-frame batches don't overwrite each other.
fn main() -> Result<(), String> {
    let args = std::env::args().collect::<Vec<_>>();
    let payload_path = PathBuf::from(read_arg("--payload", &args)?);
    let config_path = PathBuf::from(read_arg("--config", &args)?);
    let out_path = PathBuf::from(read_arg("--out", &args)?);
    let timing_out_path = read_optional_arg("--timing-out", &args).map(PathBuf::from);
    let seconds = parse_seconds(&args)?;

    let payload_json = fs::read_to_string(&payload_path)
        .map_err(|error| format!("Failed to read {}: {error}", payload_path.display()))?;
    let config_json = fs::read_to_string(&config_path)
        .map_err(|error| format!("Failed to read {}: {error}", config_path.display()))?;
    let activity = parse_activity_json(&payload_json).map_err(|e| e.to_string())?;
    let config = parse_and_validate_config(&config_json).map_err(|e| e.to_string())?;
    let dense_activity =
        build_dense_activity_report_validated(&activity, &config).map_err(|e| e.to_string())?;
    let paths = AppPaths::from_repo_root(repo_root()?);

    let stem = out_path.file_stem().and_then(|value| value.to_str()).unwrap_or("preview");
    let extension = out_path.extension().and_then(|value| value.to_str()).unwrap_or("png");
    let frames = seconds
        .into_iter()
        .enumerate()
        .map(|(index, second)| PreviewFrame {
            second,
            out_path: if index == 0 {
                out_path.clone()
            } else {
                out_path.with_file_name(format!("{stem}_{second}.{extension}"))
            },
        })
        .collect::<Vec<_>>();
    for frame in &frames {
        if let Some(parent) = frame.out_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Failed to create {}: {error}", parent.display()))?;
        }
    }
    let report = render_preview_frames(&paths, &config, &activity, &dense_activity, &frames)
        .map_err(|e| e.to_string())?;

    let json = serde_json::to_string_pretty(&report)
        .map_err(|error| format!("Failed to serialize timing report: {error}"))?;
    if let Some(timing_out_path) = timing_out_path {
        if let Some(parent) = timing_out_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Failed to create {}: {error}", parent.display()))?;
        }
        fs::write(&timing_out_path, &json)
            .map_err(|error| format!("Failed to write {}: {error}", timing_out_path.display()))?;
    }
    println!("{json}");
    Ok(())
}
