use ovrley_core::activity::{build_dense_activity_report, parse_activity_json};
use ovrley_core::paths::AppPaths;
use ovrley_core::config::parse_config_json;
use ovrley_core::encode::codec_detect::detect_codecs;
use ovrley_core::encode::video::{render_video, rendered_frame_count, RenderController};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

#[path = "../bin_common.rs"]
mod common;
use common::{
    format_mmss, read_optional_arg, read_positional, repo_root, resolve_path, unix_timestamp,
};

#[path = "../benchmark_common.rs"]
mod benchmark_common;
use benchmark_common::{
    average_successful_runs, file_size_mb, is_transparent_codec_available, prevent_sleep,
    sleep_between_benchmark_groups, sleep_between_benchmark_runs, summarize_run_outcome,
    CommonRunMetrics,
};

const TRANSPARENT_CODECS: &[&str] = &[
    //"prores_ks",
    //"prores_ks_vulkan",
    //"prores_videotoolbox",
    "qtrle",
];

fn parse_args(args: &[String]) -> Result<(PathBuf, PathBuf), String> {
    let program = &args[0];
    let rest = &args[1..];

    let activity = read_optional_arg("--activity", rest).or_else(|| read_positional(0, rest));
    let template = read_optional_arg("--template", rest).or_else(|| read_positional(1, rest));

    match (activity, template) {
        (Some(a), Some(t)) => Ok((PathBuf::from(a), PathBuf::from(t))),
        _ => Err(format!(
            "Usage: {program} <activity-path> <template-path>\n\
             Or:    {program} --activity <path> --template <path>"
        )),
    }
}

#[derive(Serialize)]
struct RunResult {
    run: u32,
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolution: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    widget_update_rate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total_frames: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    overlay_duration_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    job_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    job_time_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_size_mb: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct AverageResult {
    job_time: String,
    job_time_seconds: f64,
    file_size_mb: f64,
}

#[derive(Serialize)]
struct CodecResults {
    available: bool,
    runs: Vec<RunResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    average: Option<AverageResult>,
    successful_runs: u32,
    failed_runs: u32,
}

#[derive(Serialize)]
struct ConfigInfo {
    resolution: String,
    widget_update_rate: u32,
}

#[derive(Serialize)]
struct RenderWindow {
    start: f64,
    end: f64,
    duration_seconds: f64,
}

#[derive(Serialize)]
struct BenchmarkOutput {
    generated_at: String,
    template: String,
    activity: String,
    config: ConfigInfo,
    render_window: RenderWindow,
    results: BTreeMap<String, CodecResults>,
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let (activity_path, template_path) = parse_args(&args)?;

    prevent_sleep();

    let root = repo_root()?;
    let paths = AppPaths::from_repo_root(root.clone());
    paths.ensure_dirs().map_err(|e| e.to_string())?;

    let available = detect_codecs(&root).map_err(|e| e.to_string())?;

    let resolved_activity = resolve_path(&activity_path, &root);
    let resolved_template = resolve_path(&template_path, &root);

    let activity_json = fs::read_to_string(&resolved_activity).map_err(|e| {
        format!(
            "Failed to read activity {}: {e}",
            resolved_activity.display()
        )
    })?;

    let template_raw = fs::read_to_string(&resolved_template).map_err(|e| {
        format!(
            "Failed to read template {}: {e}",
            resolved_template.display()
        )
    })?;
    let template_value: serde_json::Value =
        serde_json::from_str(&template_raw).map_err(|e| format!("Invalid template JSON: {e}"))?;

    let template_name = template_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();
    let activity_name = activity_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let config_value = template_value
        .get("config")
        .ok_or_else(|| "Template missing 'config' key".to_string())?
        .clone();

    let settings_update_rate = template_value
        .get("settings")
        .and_then(|s| s.get("updateRate"))
        .and_then(serde_json::Value::as_u64)
        .map(|u| u as u32);

    let activity = parse_activity_json(&activity_json).map_err(|e| e.to_string())?;

    let res_width = config_value["scene"]["width"].as_u64().unwrap_or(3840);
    let res_height = config_value["scene"]["height"].as_u64().unwrap_or(2160);
    let base_update_rate = settings_update_rate
        .or_else(|| {
            config_value["scene"]["update_rate"]
                .as_u64()
                .map(|u| u as u32)
        })
        .unwrap_or(1);

    let mut results = BTreeMap::new();

    for (codec_index, codec_name) in TRANSPARENT_CODECS.iter().enumerate() {
        println!("\n=== Codec: {codec_name} ===");

        if !is_transparent_codec_available(&available, codec_name) {
            println!("  → NOT AVAILABLE on this system");
            results.insert(
                codec_name.to_string(),
                CodecResults {
                    available: false,
                    runs: Vec::new(),
                    average: None,
                    successful_runs: 0,
                    failed_runs: 0,
                },
            );
            continue;
        }

        println!("  → Available, running 3 iterations...");

        let mut runs = Vec::with_capacity(3);
        let mut successful_run_data: Vec<CommonRunMetrics> = Vec::new();

        for run_num in 1..=3 {
            print!("    Run {run_num}/3... ");

            let mut run_config_value = config_value.clone();
            run_config_value["scene"]["start"] = serde_json::json!(300.0f64);
            run_config_value["scene"]["end"] = serde_json::json!(360.0f64);
            run_config_value["scene"]["ffmpeg"] = serde_json::json!({"codec": codec_name});

            let config_str = serde_json::to_string(&run_config_value)
                .map_err(|e| format!("Failed to serialize config: {e}"))?;
            let config = parse_config_json(&config_str).map_err(|e| e.to_string())?;
            let dense =
                build_dense_activity_report(&activity, &config).map_err(|e| e.to_string())?;

            let update_rate = config.widget_update_rate();
            let total_frames = rendered_frame_count(dense.frame_count, update_rate as usize) as u32;
            let overlay_duration = config.scene.end - config.scene.start;

            let controller = RenderController::default();
            if let Err(e) = controller.try_start(
                total_frames,
                &format!("Benchmark {codec_name} run {run_num}"),
            ) {
                println!("FAILED: {e}");
                runs.push(RunResult {
                    run: run_num,
                    success: false,
                    resolution: None,
                    widget_update_rate: None,
                    total_frames: None,
                    overlay_duration_seconds: None,
                    job_time: None,
                    job_time_seconds: None,
                    file_size_mb: None,
                    error: Some(e.to_string()),
                });
                continue;
            }

            let started = Instant::now();
            let render_result = render_video(&paths, &config, &activity, &dense, &controller);
            let elapsed_secs = started.elapsed().as_secs_f64();

            match render_result {
                Ok(filename) => {
                    let output_path = paths.downloads_dir.join(&filename);
                    let file_size = file_size_mb(&output_path);

                    println!(
                        "OK  job_time={}  file_size={:.1}MB",
                        format_mmss(elapsed_secs),
                        file_size
                    );

                    runs.push(RunResult {
                        run: run_num,
                        success: true,
                        resolution: Some(format!("{res_width}x{res_height}")),
                        widget_update_rate: Some(update_rate),
                        total_frames: Some(total_frames),
                        overlay_duration_seconds: Some(overlay_duration),
                        job_time: Some(format_mmss(elapsed_secs)),
                        job_time_seconds: Some(elapsed_secs),
                        file_size_mb: Some(file_size),
                        error: None,
                    });

                    successful_run_data.push(CommonRunMetrics {
                        job_time_seconds: elapsed_secs,
                        file_size_mb: file_size,
                    });

                    if run_num < 3 {
                        sleep_between_benchmark_runs();
                    }
                }
                Err(e) => {
                    println!("FAILED: {e}");
                    runs.push(RunResult {
                        run: run_num,
                        success: false,
                        resolution: None,
                        widget_update_rate: None,
                        total_frames: None,
                        overlay_duration_seconds: None,
                        job_time: None,
                        job_time_seconds: None,
                        file_size_mb: None,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        if !successful_run_data.is_empty() && codec_index + 1 < TRANSPARENT_CODECS.len() {
            sleep_between_benchmark_groups("Codec");
        }

        let (successful_count, failed_count) =
            summarize_run_outcome(successful_run_data.len(), runs.len());

        let average =
            average_successful_runs(&successful_run_data).map(|avg| AverageResult {
                job_time: avg.job_time,
                job_time_seconds: avg.job_time_seconds,
                file_size_mb: avg.file_size_mb,
            });

        results.insert(
            codec_name.to_string(),
            CodecResults {
                available: true,
                runs,
                average,
                successful_runs: successful_count,
                failed_runs: failed_count,
            },
        );
    }

    let output = BenchmarkOutput {
        generated_at: unix_timestamp(),
        template: template_name,
        activity: activity_name,
        config: ConfigInfo {
            resolution: format!("{res_width}x{res_height}"),
            widget_update_rate: base_update_rate,
        },
        render_window: RenderWindow {
            start: 300.0,
            end: 360.0,
            duration_seconds: 60.0,
        },
        results,
    };

    let output_dir = root.join("debug").join("benchmarks");
    fs::create_dir_all(&output_dir).map_err(|e| format!("Failed to create benchmarks dir: {e}"))?;
    let output_path = output_dir.join("transparent.json");
    let output_json = serde_json::to_string_pretty(&output)
        .map_err(|e| format!("Failed to serialize output: {e}"))?;
    fs::write(&output_path, &output_json)
        .map_err(|e| format!("Failed to write {}: {e}", output_path.display()))?;

    println!("\n=== Results written to {} ===", output_path.display());
    Ok(())
}
