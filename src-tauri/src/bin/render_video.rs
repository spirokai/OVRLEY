use cyclemetry_core::activity::{build_dense_activity_report, parse_activity_json};
use cyclemetry_core::commands::AppPaths;
use cyclemetry_core::config::parse_config_json;
use cyclemetry_core::encode::video::{render_video, RenderController};
use std::fs;
use std::path::PathBuf;

fn read_arg(flag: &str, args: &[String]) -> Result<String, String> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
        .ok_or_else(|| format!("Missing required argument: {flag}"))
}

fn repo_root() -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| "Failed to resolve repo root".to_string())
}

fn main() -> Result<(), String> {
    let args = std::env::args().collect::<Vec<_>>();
    let payload_path = PathBuf::from(read_arg("--payload", &args)?);
    let config_path = PathBuf::from(read_arg("--config", &args)?);

    let payload_json = fs::read_to_string(&payload_path)
        .map_err(|error| format!("Failed to read {}: {error}", payload_path.display()))?;
    let config_json = fs::read_to_string(&config_path)
        .map_err(|error| format!("Failed to read {}: {error}", config_path.display()))?;

    let activity = parse_activity_json(&payload_json)?;
    let config = parse_config_json(&config_json)?;
    let dense_activity = build_dense_activity_report(&activity, &config)?;
    let paths = AppPaths::from_repo_root(repo_root()?);
    paths.ensure_dirs()?;

    let controller = RenderController::default();
    controller.try_start(dense_activity.frame_count as u32, "Preparing render assets...")?;
    let filename = render_video(&paths, &config, &activity, &dense_activity, &controller)?;
    controller.finish_success(filename.clone());
    println!("{{\"filename\":\"{filename}\"}}");
    Ok(())
}
