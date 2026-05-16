use ovrley_core::activity::{build_dense_activity_report, parse_activity_json};
use ovrley_core::commands::AppPaths;
use ovrley_core::config::parse_config_json;
use std::fs;
use std::path::PathBuf;

fn repo_root() -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| "Failed to resolve repo root".to_string())
}

fn estimate_max_parallelism() -> usize {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    // Heuristic: 4K render needs ~4 logical cores for peak efficiency (Skia + FFmpeg)
    let by_cores = cores / 4;

    // Hard limit for hardware encoders (usually 3 on consumer cards)
    let gpu_limit = 3;

    let estimate = std::cmp::min(by_cores, gpu_limit).max(1);

    println!("--- Resource Estimation ---");
    println!("Logical Cores: {}", cores);
    println!("Estimated Max Parallel Sessions: {}", estimate);
    println!("---------------------------");

    estimate
}

fn main() -> Result<(), String> {
    let _ = estimate_max_parallelism();
    let root = repo_root()?;
    let paths = AppPaths::from_repo_root(root.clone());
    paths.ensure_dirs()?;

    let payload_path = root.join("debug/activities/Test_FIT-parse-debug.json");

    println!("Loading payload: {}", payload_path.display());
    let payload_json = fs::read_to_string(&payload_path)
        .map_err(|error| format!("Failed to read payload: {error}"))?;
    let activity = parse_activity_json(&payload_json)?;

    println!("Loading configs...");
    let mut configs = Vec::new();
    let mut reports = Vec::new();

    for i in 1..=2 {
        let config_path = root.join(format!("templates/parallel{}.json", i));
        let config_json = fs::read_to_string(&config_path)
            .map_err(|error| format!("Failed to read config{}: {error}", i))?;
        let config = parse_config_json(&config_json)?;
        let dense = build_dense_activity_report(&activity, &config)?;
        configs.push(config);
        reports.push(dense);
    }

    println!("Starting parallel renders (2 sessions)...");
    let duration =
        ovrley_core::encode::video::run_parallel_renders(&paths, configs, &activity, reports)?;

    println!("\nTotal execution time: {:.2}s", duration.as_secs_f64());

    Ok(())
}
