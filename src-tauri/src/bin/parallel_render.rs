//! Parallel render stress-test binary.
//!
//! Loads two overlay configs (`parallel1.json`, `parallel2.json`) and a
//! shared activity payload, then dispatches simultaneous ffmpeg render
//! sessions to test encoder concurrency and resource contention.
//!
//! Does not accept CLI arguments — paths are hardcoded relative to the repo
//! root for reproducible stress testing.

use ovrley_core::activity::{build_dense_activity_report, parse_activity_json};
use ovrley_core::config::parse_config_json;
use ovrley_core::paths::AppPaths;

#[path = "../bin_common.rs"]
mod common;
use common::repo_root;

/// Estimates a safe maximum for concurrent render sessions.
///
/// Caps at 3 to avoid GPU context exhaustion; floors at 1. Uses logical core
/// count divided by 4 as a rough proxy for GPU encoder pipeline depth.
fn estimate_max_parallelism() -> usize {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    let by_cores = cores / 4;
    let gpu_limit = 3;
    let estimate = std::cmp::min(by_cores, gpu_limit).max(1);

    println!("--- Resource Estimation ---");
    println!("Logical Cores: {}", cores);
    println!("Estimated Max Parallel Sessions: {}", estimate);
    println!("---------------------------");

    estimate
}

/// Loads two overlay configs, builds dense activity reports for each, and
/// runs them concurrently through the video render pipeline. Prints combined
/// elapsed time.
fn main() -> Result<(), String> {
    let _ = estimate_max_parallelism();
    let root = repo_root()?;
    let paths = AppPaths::from_repo_root(root.clone());
    paths.ensure_dirs().map_err(|e| e.to_string())?;

    let payload_path = root.join("debug/activities/Test_FIT-parse-debug.json");

    println!("Loading payload: {}", payload_path.display());
    let payload_json = std::fs::read_to_string(&payload_path)
        .map_err(|error| format!("Failed to read payload: {error}"))?;
    let activity = parse_activity_json(&payload_json).map_err(|e| e.to_string())?;

    println!("Loading configs...");
    let mut configs = Vec::new();
    let mut reports = Vec::new();

    for i in 1..=2 {
        let config_path = root.join(format!("templates/parallel{}.json", i));
        let config_json = std::fs::read_to_string(&config_path)
            .map_err(|error| format!("Failed to read config{}: {error}", i))?;
        let config = parse_config_json(&config_json).map_err(|e| e.to_string())?;
        let dense = build_dense_activity_report(&activity, &config).map_err(|e| e.to_string())?;
        configs.push(config);
        reports.push(dense);
    }

    println!("Starting parallel renders (2 sessions)...");
    let duration =
        ovrley_core::encode::video::run_parallel_renders(&paths, configs, &activity, reports)
            .map_err(|e| e.to_string())?;

    println!("\nTotal execution time: {:.2}s", duration.as_secs_f64());

    Ok(())
}
