use ovrley_core::activity::{build_dense_activity_report, parse_activity_json};
use ovrley_core::config::parse_config_json;
use std::fs;
use std::path::PathBuf;

fn read_arg(flag: &str, args: &[String]) -> Result<String, String> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
        .ok_or_else(|| format!("Missing required argument: {flag}"))
}

fn main() -> Result<(), String> {
    let args = std::env::args().collect::<Vec<_>>();
    let payload_path = PathBuf::from(read_arg("--payload", &args)?);
    let config_path = PathBuf::from(read_arg("--config", &args)?);
    let out_path = PathBuf::from(read_arg("--out", &args)?);

    let payload_json = fs::read_to_string(&payload_path)
        .map_err(|error| format!("Failed to read {}: {error}", payload_path.display()))?;
    let config_json = fs::read_to_string(&config_path)
        .map_err(|error| format!("Failed to read {}: {error}", config_path.display()))?;

    let activity = parse_activity_json(&payload_json)?;
    let config = parse_config_json(&config_json)?;
    let report = build_dense_activity_report(&activity, &config)?;
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
