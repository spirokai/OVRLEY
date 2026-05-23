//! Composite (overlay-on-video) encoding benchmark binary.
//!
//! Iterates over every available composite codec profile (software, NVENC,
//! QSV, VideoToolbox, VAAPI, AMF), runs 3 full composite renders per codec,
//! and writes aggregated timing and file-size results to
//! `debug/benchmarks/composite.json`.
//!
//! The binary sleep-cooldowns between codec groups to reduce thermal throttling
//! bias. On Windows, it also prevents system sleep for the duration of the run.
//!
//! Does not own: rendering or encoding — delegates to `ovrley_core`.

use ovrley_core::activity::{build_dense_activity_report, parse_activity_json};
use ovrley_core::config::parse_config_json;
use ovrley_core::encode::codec_detect::detect_codecs;
use ovrley_core::encode::video::{
    render_composite_video, CompositeRenderRequest, RenderController,
};
use ovrley_core::encode::video_probe::probe_video;
use ovrley_core::paths::AppPaths;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

#[path = "../bin_common.rs"]
mod common;
use common::{format_mmss, read_positional, repo_root, resolve_path, unix_timestamp};

#[path = "../benchmark_common.rs"]
mod benchmark_common;
use benchmark_common::{
    average_successful_runs, file_size_mb, is_composite_codec_available, prevent_sleep,
    sleep_between_benchmark_groups, sleep_between_benchmark_runs, summarize_run_outcome,
    CommonRunMetrics,
};

/// All composite codec profiles exercised by this benchmark.
///
/// Each entry pairs a human-readable display name with the ffmpeg encoder
/// name so availability can be checked via the shared codec catalog.
const COMPOSITE_CODECS: &[(&str, &str)] = &[
    ("software_h264", "libx264"),
    //("software_hevc", "libx265"),
    //("nvgpu_h264", "h264_nvenc"),
    // ("nvgpu_hevc", "hevc_nvenc"),
    //("nnvgpu_h264", "nnvgpu_h264"),
    // ("nnvgpu_hevc", "nnvgpu_hevc"),
    // ("qsv_h264", "h264_qsv"),
    // ("qsv_hevc", "hevc_qsv"),
    // ("qsv_full_h264", "qsv_full_h264"),
    // ("qsv_full_hevc", "qsv_full_hevc"),
    // ("mac_h264", "h264_videotoolbox"),
    // ("mac_hevc", "hevc_videotoolbox"),
    // ("vaapi_h264", "h264_vaapi"),
    // ("vaapi_hevc", "hevc_vaapi"),
    // ("amd_h264", "h264_amf"),
    // ("amd_hevc", "hevc_amf"),
];

/// Parses three positional CLI arguments: activity path, template path, video path.
fn parse_args(args: &[String]) -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let program = &args[0];
    let rest = &args[1..];

    let activity = read_positional(0, rest);
    let template = read_positional(1, rest);
    let video = read_positional(2, rest);

    match (activity, template, video) {
        (Some(a), Some(t), Some(v)) => Ok((PathBuf::from(a), PathBuf::from(t), PathBuf::from(v))),
        _ => Err(format!(
            "Usage: {program} <activity-path> <template-path> <video-path>\n"
        )),
    }
}

/// Per-run result for a single composite encode iteration.
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

/// Averaged metrics over successful runs for one codec.
#[derive(Serialize)]
struct AverageResult {
    job_time: String,
    job_time_seconds: f64,
    file_size_mb: f64,
}

/// Aggregated results for one codec across all runs.
#[derive(Serialize)]
struct CodecResults {
    available: bool,
    profile_name: String,
    runs: Vec<RunResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    average: Option<AverageResult>,
    successful_runs: u32,
    failed_runs: u32,
}

/// Resolution and update-rate config extracted from the template.
#[derive(Serialize)]
struct ConfigInfo {
    resolution: String,
    widget_update_rate: u32,
}

/// Activity and video trim window used for the render.
#[derive(Serialize)]
struct RenderWindow {
    start: f64,
    end: f64,
    duration_seconds: f64,
}

/// Source video metadata included in the benchmark output.
#[derive(Serialize)]
struct VideoInfo {
    path: String,
    fps: String,
    fps_num: u32,
    fps_den: u32,
    duration_seconds: f64,
}

/// Root JSON schema written to `debug/benchmarks/composite.json`.
#[derive(Serialize)]
struct BenchmarkOutput {
    generated_at: String,
    template: String,
    activity: String,
    video: VideoInfo,
    bitrate: String,
    config: ConfigInfo,
    render_window: RenderWindow,
    results: BTreeMap<String, CodecResults>,
}

/// Runs the composite overlay encoding benchmark across all available codecs.
///
/// # Phases
///
/// 1. **Setup** — parse CLI args, prevent system sleep, resolve paths, detect
///    available codecs, probe the source video for FPS and duration.
/// 2. **Render window calculation** — choose a 60-second activity segment
///    (preferably 300–360s) aligned with the video timeline.
/// 3. **Per-codec loop** — for each composite codec profile, check availability
///    and run 3 full composite render iterations with cooldowns between groups.
/// 4. **Per-run loop** — serialize config, build dense activity report, create a
///    `RenderController`, call `render_composite_video`, measure elapsed time
///    and output file size, and record success/failure.
/// 5. **Output** — aggregate results into `BenchmarkOutput` and write to
///    `debug/benchmarks/composite.json`.
fn main() -> Result<(), String> {
    // -- Phase 1: setup and codec detection --
    prevent_sleep();

    let args: Vec<String> = std::env::args().collect();
    let (activity_path, template_path, video_path) = parse_args(&args)?;

    let root = repo_root()?;
    let paths = AppPaths::from_repo_root(root.clone());
    paths.ensure_dirs().map_err(|e| e.to_string())?;

    let available = detect_codecs(&root).map_err(|e| e.to_string())?;

    let resolved_video = resolve_path(&video_path, &root);
    let metadata =
        probe_video(&root, &resolved_video.to_string_lossy()).map_err(|e| e.to_string())?;
    let fps_num = metadata.fps_num.unwrap_or(30);
    let fps_den = metadata.fps_den.unwrap_or(1);
    let video_duration = metadata
        .duration
        .ok_or_else(|| "Could not determine video duration".to_string())?;
    let video_res_text = metadata
        .resolution
        .as_ref()
        .map(|r| format!("{}x{}", r.width, r.height))
        .unwrap_or_else(|| "unknown".to_string());

    // -- Phase 2: compute a 60-second render window from video duration --
    let (activity_start, activity_end, render_duration, trim_start) = if video_duration >= 360.0 {
        (300.0, 360.0, 60.0, 300.0)
    } else if video_duration >= 60.0 {
        let offset = video_duration - 60.0;
        (offset, offset + 60.0, 60.0, offset)
    } else {
        (0.0, video_duration, video_duration, 0.0)
    };

    println!(
        "  Render window: activity {:.0}s-{:.0}s, video trim {:.1}s, duration {:.1}s",
        activity_start, activity_end, trim_start, render_duration
    );

    let activity_json = fs::read_to_string(resolve_path(&activity_path, &root))
        .map_err(|e| format!("Failed to read activity: {e}"))?;

    let template_raw = fs::read_to_string(resolve_path(&template_path, &root))
        .map_err(|e| format!("Failed to read template: {e}"))?;
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
    let video_name = resolved_video
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

    let res_width = config_value["scene"]["width"].as_u64().unwrap_or(1920);
    let res_height = config_value["scene"]["height"].as_u64().unwrap_or(1080);
    let base_update_rate = settings_update_rate
        .or_else(|| {
            config_value["scene"]["update_rate"]
                .as_u64()
                .map(|u| u as u32)
        })
        .unwrap_or(1);

    let mut results = BTreeMap::new();

    // -- Phase 3: iterate over every composite codec profile --
    for (codec_index, &(display_name, codec_key)) in COMPOSITE_CODECS.iter().enumerate() {
        println!("\n=== Codec: {display_name} ===");

        if !is_composite_codec_available(&available, display_name) {
            println!("  → NOT AVAILABLE on this system");
            results.insert(
                display_name.to_string(),
                CodecResults {
                    available: false,
                    profile_name: display_name.to_string(),
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

        // -- Phase 4: run 3 full composite render iterations for this codec --
        for run_num in 1..=3 {
            print!("    Run {run_num}/3... ");

            let mut run_config_value = config_value.clone();
            run_config_value["scene"]["start"] = serde_json::json!(activity_start);
            run_config_value["scene"]["end"] = serde_json::json!(activity_end);

            let mut ffmpeg_config = serde_json::json!({"codec": codec_key});
            if display_name.starts_with("qsv_full_") && !available.qsv_full_init_args.is_empty() {
                let init_args: Vec<serde_json::Value> = available
                    .qsv_full_init_args
                    .iter()
                    .map(|a| serde_json::Value::String(a.clone()))
                    .collect();
                ffmpeg_config["qsv_full_init_args"] = serde_json::Value::Array(init_args);
            }
            run_config_value["scene"]["ffmpeg"] = ffmpeg_config;

            let config_str = serde_json::to_string(&run_config_value)
                .map_err(|e| format!("Failed to serialize config: {e}"))?;
            let config = parse_config_json(&config_str).map_err(|e| e.to_string())?;
            let dense =
                build_dense_activity_report(&activity, &config).map_err(|e| e.to_string())?;

            let update_rate = config.widget_update_rate();
            let overlay_duration = config.scene.end - config.scene.start;

            let output_frame_count = (overlay_duration * fps_num as f64 / fps_den as f64)
                .ceil()
                .max(1.0) as u32;

            let controller = RenderController::default();
            if let Err(e) = controller.try_start(
                output_frame_count,
                &format!("Benchmark composite {display_name} run {run_num}"),
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
            let render_result = render_composite_video(&CompositeRenderRequest {
                paths: &paths,
                config: &config,
                activity: &activity,
                dense_activity: &dense,
                controller: &controller,
                composite_video_path: &resolved_video.to_string_lossy(),
                composite_bitrate: "40M",
                composite_sync_offset: 0.0,
                composite_video_fps_num: fps_num,
                composite_video_fps_den: fps_den,
                composite_video_duration: video_duration,
                composite_render_duration: Some(render_duration),
                composite_video_trim_start: Some(trim_start),
                composite_widget_update_rate: Some(update_rate),
            });
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
                        total_frames: Some(output_frame_count),
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

        if !successful_run_data.is_empty() && codec_index + 1 < COMPOSITE_CODECS.len() {
            sleep_between_benchmark_groups("Codec");
        }

        let (successful_count, failed_count) =
            summarize_run_outcome(successful_run_data.len(), runs.len());

        let average = average_successful_runs(&successful_run_data).map(|avg| AverageResult {
            job_time: avg.job_time,
            job_time_seconds: avg.job_time_seconds,
            file_size_mb: avg.file_size_mb,
        });

        results.insert(
            display_name.to_string(),
            CodecResults {
                available: true,
                profile_name: display_name.to_string(),
                runs,
                average,
                successful_runs: successful_count,
                failed_runs: failed_count,
            },
        );
    }

    // -- Phase 5: serialize results and write to debug/benchmarks/composite.json --
    let output = BenchmarkOutput {
        generated_at: unix_timestamp(),
        template: template_name,
        activity: activity_name,
        video: VideoInfo {
            path: video_name,
            fps: if fps_den == 1 {
                format!("{fps_num}")
            } else {
                format!("{fps_num}/{fps_den}")
            },
            fps_num,
            fps_den,
            duration_seconds: video_duration,
        },
        bitrate: "40M".to_string(),
        config: ConfigInfo {
            resolution: format!("{res_width}x{res_height}"),
            widget_update_rate: base_update_rate,
        },
        render_window: RenderWindow {
            start: activity_start,
            end: activity_end,
            duration_seconds: render_duration,
        },
        results,
    };

    let output_dir = root.join("debug").join("benchmarks");
    fs::create_dir_all(&output_dir).map_err(|e| format!("Failed to create benchmarks dir: {e}"))?;
    let output_path = output_dir.join("composite.json");
    let output_json = serde_json::to_string_pretty(&output)
        .map_err(|e| format!("Failed to serialize output: {e}"))?;
    fs::write(&output_path, &output_json)
        .map_err(|e| format!("Failed to write {}: {e}", output_path.display()))?;

    println!("\n=== Results written to {} ===", output_path.display());
    Ok(())
}
