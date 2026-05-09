use crate::activity::schema::DenseActivityReport;
use crate::commands::AppPaths;
use crate::config::RenderConfig;
use crate::debug::TimingBucket;
use crate::render::LabelCacheStatus;
use chrono::Local;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
struct TimingSummary {
    phase: String,
    timestamp: String,
    overlay_filename: String,
    fps: f64,
    width: u32,
    height: u32,
    total_frames: u32,
    rendered_frames: u32,
    total_time_taken: f64,
    sample_frame_indices: Vec<usize>,
    timings: BTreeMap<String, TimingBucket>,
}

#[derive(Serialize)]
struct PrepareTimingSummary {
    total_ms: f64,
    timings: BTreeMap<String, TimingBucket>,
    label_cache_status: String,
}

#[derive(Serialize)]
struct StitchSummary {
    timestamp: String,
    codec: String,
    total_frames: u32,
    segments: Vec<SegmentSummary>,
    concat_output: String,
    concat_duration_ms: f64,
}

#[derive(Serialize)]
struct SegmentSummary {
    index: usize,
    start_seconds: f64,
    end_seconds: f64,
    frames: u32,
    filename: String,
}

pub(crate) fn concat_video_segments(
    paths: &AppPaths,
    ffmpeg_bin: &Path,
    filenames: &[String],
    output_path: &Path,
) -> Result<(), String> {
    let list_path = paths
        .temp_dir
        .join(format!("concat_list_{}.txt", timestamp_nanos()?));
    let mut list_content = String::new();
    for filename in filenames {
        list_content.push_str(&format!(
            "file '{}'\n",
            paths
                .downloads_dir
                .join(filename)
                .display()
                .to_string()
                .replace('\\', "/")
        ));
    }
    fs::write(&list_path, list_content).map_err(|e| format!("Failed to write concat list: {e}"))?;

    let status = Command::new(ffmpeg_bin)
        .arg("-f")
        .arg("concat")
        .arg("-safe")
        .arg("0")
        .arg("-i")
        .arg(&list_path)
        .arg("-c")
        .arg("copy")
        .arg("-y")
        .arg(output_path)
        .status()
        .map_err(|e| format!("Failed to run ffmpeg concat: {e}"))?;

    let _ = fs::remove_file(&list_path);

    if status.success() {
        Ok(())
    } else {
        Err(format!("FFmpeg concat failed with status {status}"))
    }
}

pub(crate) fn write_prepare_summary(
    debug_dir: &Path,
    total_ms: f64,
    timings: &BTreeMap<String, TimingBucket>,
    label_cache_status: LabelCacheStatus,
) -> Result<(), String> {
    let summary = PrepareTimingSummary {
        total_ms,
        timings: timings.clone(),
        label_cache_status: match label_cache_status {
            LabelCacheStatus::None => "none".to_string(),
            LabelCacheStatus::Hit => "hit".to_string(),
            LabelCacheStatus::Miss => "miss".to_string(),
        },
    };
    write_json(
        debug_dir.join("prepare_render_assets_timing.json"),
        &summary,
    )
}

pub(crate) fn write_timing_summary_with_phase(
    debug_dir: &Path,
    config: &RenderConfig,
    output_path: &Path,
    phase: &str,
    total_frames: u32,
    rendered_frames: u32,
    total_time_taken: f64,
    sample_frame_indices: Vec<usize>,
    timings: BTreeMap<String, TimingBucket>,
) -> Result<(), String> {
    let summary = TimingSummary {
        phase: phase.to_string(),
        timestamp: iso_timestamp_now(),
        overlay_filename: output_path.to_string_lossy().to_string(),
        fps: config.scene.fps,
        width: config.scene.width.unwrap_or(1920),
        height: config.scene.height.unwrap_or(1080),
        total_frames,
        rendered_frames,
        total_time_taken: round3(total_time_taken),
        sample_frame_indices,
        timings,
    };
    write_json(debug_dir.join("timing_summary.json"), &summary)
}

pub(crate) fn write_stitch_summary(
    paths: &AppPaths,
    config: &RenderConfig,
    public_filename: &str,
    concat_duration_ms: f64,
    segment_configs: &[RenderConfig],
    segment_reports: &[DenseActivityReport],
    filenames: &[String],
) -> Result<(), String> {
    let debug_dir = create_debug_dir(paths, "phase_6_stitch")?;
    let codec = config
        .scene
        .ffmpeg
        .as_object()
        .and_then(|map| map.get("codec"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let segment_summaries = segment_configs
        .iter()
        .enumerate()
        .map(|(index, segment_config)| SegmentSummary {
            index,
            start_seconds: round3(segment_config.scene.start),
            end_seconds: round3(segment_config.scene.end),
            frames: segment_reports[index].frame_count as u32,
            filename: filenames[index].clone(),
        })
        .collect::<Vec<_>>();
    let total_frames = segment_summaries.iter().map(|segment| segment.frames).sum();
    let summary = StitchSummary {
        timestamp: iso_timestamp_now(),
        codec,
        total_frames,
        segments: segment_summaries,
        concat_output: public_filename.to_string(),
        concat_duration_ms: round3(concat_duration_ms),
    };
    write_json(debug_dir.join("stitch_summary.json"), &summary)
}

pub(crate) fn create_debug_dir(paths: &AppPaths, phase: &str) -> Result<PathBuf, String> {
    let dir = paths.debug_render_dir.join(phase).join(timestamp_slug()?);
    fs::create_dir_all(&dir)
        .map_err(|error| format!("Failed to create {}: {error}", dir.display()))?;
    Ok(dir)
}

pub(crate) fn timestamp_nanos() -> Result<u128, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .map_err(|error| error.to_string())
}

pub(crate) fn sample_frame_indices(total_frames: usize) -> Vec<usize> {
    if total_frames == 0 {
        return Vec::new();
    }
    let mut indices = vec![
        0,
        total_frames / 4,
        total_frames / 2,
        (total_frames * 3) / 4,
        total_frames.saturating_sub(1),
    ];
    indices.sort_unstable();
    indices.dedup();
    indices
}

pub(crate) fn render_sample_frames_enabled() -> bool {
    matches!(
        std::env::var("OVRLEY_SAMPLE_FRAMES").ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "YES")
    )
}

pub(crate) fn write_sample_frame(
    ffmpeg_bin: &Path,
    debug_dir: &Path,
    width: u32,
    height: u32,
    rgba: &[u8],
    frame_index: usize,
    input_pix_fmt: &str,
) -> Result<(), String> {
    let png_path = debug_dir.join(format!("sample_{frame_index:04}.png"));
    let mut command = Command::new(ffmpeg_bin);
    command
        .arg("-loglevel")
        .arg("error")
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg(input_pix_fmt)
        .arg("-s")
        .arg(format!("{width}x{height}"))
        .arg("-i")
        .arg("-")
        .arg("-frames:v")
        .arg("1")
        .arg("-y")
        .arg(&png_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|error| format!("Failed to spawn ffmpeg for sample frame: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(rgba).map_err(|error| error.to_string())?;
    }
    let status = child.wait().map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "Failed to write sample frame {}",
            png_path.display()
        ))
    }
}

fn iso_timestamp_now() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

fn write_json<T: Serialize>(path: PathBuf, payload: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(payload).map_err(|error| error.to_string())?;
    fs::write(&path, json).map_err(|error| format!("Failed to write {}: {error}", path.display()))
}

fn timestamp_slug() -> Result<String, String> {
    Ok(timestamp_nanos()?.to_string())
}
