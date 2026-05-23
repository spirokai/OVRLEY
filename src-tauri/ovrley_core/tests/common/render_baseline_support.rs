//! Shared fixture-driven render baseline helpers for `ovrley_core` integration
//! tests.
//!
//! This module keeps the baseline suite readable by separating the three
//! high-level test layers:
//! 1. Fixture loading and per-case runtime setup.
//! 2. Real render/encode execution through `ovrley_core`.
//! 3. Baseline recording/comparison plus failure artifact generation.
//!
//! The goal is that a junior developer can trace a failing test from the
//! top-level suite file into one helper at a time without having to understand
//! the entire rendering subsystem at once.

use crate::common::test_config;
use anyhow::{anyhow, bail, Context, Result};
use ovrley_core::activity::schema::ParsedActivity;
use ovrley_core::activity::{build_dense_activity_report, parse_activity_json};
use ovrley_core::config::{parse_config_json, RenderConfig};
use ovrley_core::encode::ffmpeg::resolve_ffmpeg_binary;
use ovrley_core::encode::video::{
    render_composite_video, render_video, rendered_frame_count, CompositeRenderRequest,
    RenderController,
};
use ovrley_core::encode::video_probe::{probe_video, VideoMetadata};
use ovrley_core::paths::AppPaths;
use ovrley_core::render::render_preview_to_path;
use serde::Deserialize;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

/// Shared lock so the rendering suite runs one heavy ffmpeg/skia job at a time.
///
/// The tests use deterministic per-case output directories. Serializing access
/// avoids overlapping ffmpeg work, keeps CI logs easier to interpret, and
/// removes file-system races when the suite is run as a single `cargo test`
/// command.
static SUITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Parsed JSON manifest describing the frame, transparent, and composite cases.
#[derive(Debug, Deserialize)]
pub struct RenderBaselineSuite {
    #[serde(rename = "frameCases")]
    pub frame_cases: Vec<FrameCase>,
    #[serde(rename = "transparentVideoCases")]
    pub transparent_video_cases: Vec<TransparentVideoCase>,
    #[serde(rename = "compositeVideoCases")]
    pub composite_video_cases: Vec<CompositeVideoCase>,
}

/// One preview-frame baseline case.
#[derive(Clone, Debug, Deserialize)]
pub struct FrameCase {
    pub name: String,
    pub activity: String,
    pub config: String,
    pub second: u32,
    pub baseline: String,
}

/// One transparent-video baseline case.
#[derive(Clone, Debug, Deserialize)]
pub struct TransparentVideoCase {
    pub name: String,
    pub activity: String,
    pub config: String,
    #[serde(rename = "startSeconds")]
    pub start_seconds: f64,
    #[serde(rename = "durationSeconds")]
    pub duration_seconds: f64,
    pub codec: String,
    #[serde(rename = "frameIndices")]
    pub frame_indices: Vec<u32>,
    #[serde(rename = "baselineDir")]
    pub baseline_dir: String,
    #[serde(rename = "expectedCodecName")]
    pub expected_codec_name: String,
    #[serde(rename = "expectedPixFmt")]
    pub expected_pix_fmt: Option<String>,
}

/// One composite-video baseline case.
#[derive(Clone, Debug, Deserialize)]
pub struct CompositeVideoCase {
    pub name: String,
    pub activity: String,
    pub config: String,
    #[serde(rename = "sourceVideo")]
    pub source_video: String,
    #[serde(rename = "syncOffset")]
    pub sync_offset: f64,
    #[serde(rename = "trimStartSeconds")]
    pub trim_start_seconds: f64,
    #[serde(rename = "durationSeconds")]
    pub duration_seconds: f64,
    pub bitrate: String,
    pub codec: String,
    #[serde(rename = "frameIndices")]
    pub frame_indices: Vec<u32>,
    #[serde(rename = "baselineDir")]
    pub baseline_dir: String,
    #[serde(rename = "expectedCodecName")]
    pub expected_codec_name: String,
    #[serde(rename = "expectedPixFmt")]
    pub expected_pix_fmt: Option<String>,
    #[serde(rename = "expectSourceAudioPassthrough")]
    pub expect_source_audio_passthrough: bool,
}

/// Runtime directories used for one rendered baseline case.
///
/// The render code still receives a real `AppPaths`, but tests redirect every
/// write target into `target/render-baseline-suite/...` so no output lands in
/// the user's normal Downloads/runtime folders.
#[derive(Debug)]
struct CaseRuntime {
    app_paths: AppPaths,
    artifacts_dir: PathBuf,
    failure_dir: PathBuf,
}

/// Decoded RGBA pixels plus image dimensions for baseline comparison.
struct DecodedRgbaImage {
    width: u32,
    height: u32,
    bytes: Vec<u8>,
}

/// Human-readable parity summary for one PNG-vs-PNG comparison.
struct ParityStats {
    total_pixels: u64,
    diff_pixels: u64,
}

impl ParityStats {
    /// Returns the percentage of pixels that differ at all.
    fn different_pixel_percent(&self) -> f64 {
        if self.total_pixels == 0 {
            return 0.0;
        }
        (self.diff_pixels as f64 / self.total_pixels as f64) * 100.0
    }
}

/// Returns the lock used by the suite's heavy integration tests.
pub fn suite_lock() -> &'static Mutex<()> {
    SUITE_LOCK.get_or_init(|| Mutex::new(()))
}

/// Loads the JSON manifest that defines the entire baseline suite.
pub fn load_suite() -> Result<RenderBaselineSuite> {
    let json = fs::read_to_string(fixtures_root().join("render-baseline-suite.json"))
        .context("failed to read render baseline suite manifest")?;
    serde_json::from_str(&json).context("failed to parse render baseline suite manifest")
}

/// Executes all preview-frame baseline cases.
pub fn run_frame_cases() -> Result<()> {
    for case in load_suite()?.frame_cases {
        run_frame_case(&case)
            .with_context(|| format!("frame baseline case '{}' failed", case.name))?;
    }
    Ok(())
}

/// Executes all transparent-video baseline cases.
pub fn run_transparent_video_cases() -> Result<()> {
    for case in load_suite()?.transparent_video_cases {
        run_transparent_video_case(&case)
            .with_context(|| format!("transparent baseline case '{}' failed", case.name))?;
    }
    Ok(())
}

/// Executes all composite-video baseline cases.
pub fn run_composite_video_cases() -> Result<()> {
    for case in load_suite()?.composite_video_cases {
        run_composite_video_case(&case)
            .with_context(|| format!("composite baseline case '{}' failed", case.name))?;
    }
    Ok(())
}

/// Runs one preview-frame baseline case from fixture load through pixel compare.
fn run_frame_case(case: &FrameCase) -> Result<()> {
    let runtime = prepare_case_runtime("frame", &case.name)?;
    let activity = load_activity(&case.activity)?;
    let config = load_config(&case.config)?;
    let dense_activity = build_dense_activity_report(&activity, &config)
        .context("failed to build dense activity for frame case")?;
    let actual_path = runtime
        .artifacts_dir
        .join(format!("{}_preview.png", case.name));

    render_preview_to_path(
        &runtime.app_paths,
        &config,
        &activity,
        &dense_activity,
        case.second,
        &actual_path,
    )
    .context("preview render failed")?;

    compare_or_record_png(
        &format!("frame/{}", case.name),
        &actual_path,
        &fixtures_root().join(&case.baseline),
        &runtime.failure_dir,
    )
}

/// Runs one transparent-video baseline case end-to-end.
///
/// Phase/layer flow:
/// 1. Load the shared config fixture and narrow it to a short qtrle window.
/// 2. Render a real transparent video through `render_video`.
/// 3. Probe the container metadata and compare selected decoded frames.
fn run_transparent_video_case(case: &TransparentVideoCase) -> Result<()> {
    let runtime = prepare_case_runtime("transparent", &case.name)?;
    let activity = load_activity(&case.activity)?;
    let mut config = load_config(&case.config)?;
    config.scene.start = case.start_seconds;
    config.scene.end = case.start_seconds + case.duration_seconds;
    config.scene.ffmpeg = json!({ "codec": case.codec });

    let dense_activity = build_dense_activity_report(&activity, &config)
        .context("failed to build dense activity for transparent case")?;
    let total_frames = rendered_frame_count(
        dense_activity.frame_count,
        config.widget_update_rate() as usize,
    ) as u32;
    let controller = RenderController::default();
    controller
        .try_start(total_frames, &format!("transparent baseline {}", case.name))
        .context("failed to start transparent render controller")?;

    let filename = render_video(
        &runtime.app_paths,
        &config,
        &activity,
        &dense_activity,
        &controller,
    )
    .context("transparent render failed")?;
    let output_path = runtime.app_paths.downloads_dir.join(filename);
    assert_nonempty_output(&output_path)?;

    let metadata = probe_video(&runtime.app_paths.repo_root, &output_path.to_string_lossy())
        .context("failed to probe transparent output")?;
    assert_video_metadata(
        &metadata,
        config.scene.width.unwrap_or(1920),
        config.scene.height.unwrap_or(1080),
        config.container_fps().round() as u32,
        1,
        &case.expected_codec_name,
        case.expected_pix_fmt.as_deref(),
        Some(false),
    )?;

    compare_video_frames(
        &format!("transparent/{}", case.name),
        &runtime,
        &output_path,
        &case.baseline_dir,
        &case.frame_indices,
    )
}

/// Runs one composite-video baseline case end-to-end.
///
/// Phase/layer flow:
/// 1. Probe the source MP4 fixture so the test uses its real FPS/duration.
/// 2. Apply a short overlay window to the shared config fixture.
/// 3. Render through the public `render_composite_video` entry point.
/// 4. Validate output metadata and compare selected decoded frames.
fn run_composite_video_case(case: &CompositeVideoCase) -> Result<()> {
    // ── Phase 1: prepare isolated runtime sandbox ────────────────────
    let runtime = prepare_case_runtime("composite", &case.name)?;

    // ── Phase 2: probe source video to get its real FPS/duration ────
    let source_video = fixtures_root().join(&case.source_video);
    ensure_source_video_exists(&source_video)?;
    let source_metadata = probe_video(
        &runtime.app_paths.repo_root,
        &source_video.to_string_lossy(),
    )
    .context("failed to probe composite source video fixture")?;
    let source_fps_num = source_metadata
        .fps_num
        .ok_or_else(|| anyhow!("source video is missing fps_num metadata"))?;
    let source_fps_den = source_metadata
        .fps_den
        .ok_or_else(|| anyhow!("source video is missing fps_den metadata"))?;
    let source_duration = source_metadata
        .duration
        .ok_or_else(|| anyhow!("source video is missing duration metadata"))?;

    // ── Phase 3: load fixtures and apply case-specific scene timing ──
    let activity = load_activity(&case.activity)?;
    let mut config = load_config(&case.config)?;
    config.scene.start = case.sync_offset;
    config.scene.end = case.sync_offset + case.duration_seconds;
    config.scene.ffmpeg = json!({ "codec": case.codec });
    config.scene.composite_video_path = Some(source_video.to_string_lossy().to_string());
    config.scene.composite_bitrate = Some(case.bitrate.clone());
    config.scene.composite_sync_offset = Some(case.sync_offset);
    config.scene.composite_video_fps_num = Some(source_fps_num);
    config.scene.composite_video_fps_den = Some(source_fps_den);
    config.scene.composite_video_duration = Some(source_duration);
    config.scene.composite_render_duration = Some(case.duration_seconds);
    config.scene.composite_video_trim_start = Some(case.trim_start_seconds);
    config.scene.composite_widget_update_rate = Some(1);

    // ── Phase 4: build dense activity and prepare render controller ──
    let dense_activity = build_dense_activity_report(&activity, &config)
        .context("failed to build dense activity for composite case")?;
    let total_frames = (case.duration_seconds * f64::from(source_fps_num)
        / f64::from(source_fps_den))
    .ceil()
    .max(1.0) as u32;
    let controller = RenderController::default();
    controller
        .try_start(total_frames, &format!("composite baseline {}", case.name))
        .context("failed to start composite render controller")?;

    // ── Phase 5: dispatch composite render through public entry point ──
    let filename = render_composite_video(&CompositeRenderRequest {
        paths: &runtime.app_paths,
        config: &config,
        activity: &activity,
        dense_activity: &dense_activity,
        controller: &controller,
        composite_video_path: config
            .scene
            .composite_video_path
            .as_deref()
            .ok_or_else(|| anyhow!("missing composite video path"))?,
        composite_bitrate: config
            .scene
            .composite_bitrate
            .as_deref()
            .ok_or_else(|| anyhow!("missing composite bitrate"))?,
        composite_sync_offset: case.sync_offset,
        composite_video_fps_num: source_fps_num,
        composite_video_fps_den: source_fps_den,
        composite_video_duration: source_duration,
        composite_render_duration: Some(case.duration_seconds),
        composite_video_trim_start: Some(case.trim_start_seconds),
        composite_widget_update_rate: Some(1),
    })
    .context("composite render failed")?;
    let output_path = runtime.app_paths.downloads_dir.join(filename);
    assert_nonempty_output(&output_path)?;

    // ── Phase 6: validate output metadata and compare decoded frames ──
    let output_metadata = probe_video(&runtime.app_paths.repo_root, &output_path.to_string_lossy())
        .context("failed to probe composite output")?;
    let expected_audio = if case.expect_source_audio_passthrough {
        Some(source_metadata.has_audio)
    } else {
        None
    };
    assert_video_metadata(
        &output_metadata,
        config.scene.width.unwrap_or(1920),
        config.scene.height.unwrap_or(1080),
        source_fps_num,
        source_fps_den,
        &case.expected_codec_name,
        case.expected_pix_fmt.as_deref(),
        expected_audio,
    )?;

    compare_video_frames(
        &format!("composite/{}", case.name),
        &runtime,
        &output_path,
        &case.baseline_dir,
        &case.frame_indices,
    )
}

/// Creates a clean repo-local runtime sandbox for one case.
///
/// Each case gets its own subdirectory under
/// `target/render-baseline-suite/{kind}/{case_name}` with isolated
/// downloads, temp, debug, artifacts, and failure directories. Previous
/// output is removed before creating fresh directories so that re-runs
/// are isolated.
fn prepare_case_runtime(kind: &str, case_name: &str) -> Result<CaseRuntime> {
    let case_root = test_config::workspace_root()
        .join("target")
        .join("render-baseline-suite")
        .join(kind)
        .join(case_name);
    if case_root.exists() {
        fs::remove_dir_all(&case_root).with_context(|| {
            format!(
                "failed to clear previous case output at {}",
                case_root.display()
            )
        })?;
    }

    let downloads_dir = case_root.join("downloads");
    let temp_dir = case_root.join("tmp");
    let debug_render_dir = case_root.join("debug_render");
    let artifacts_dir = case_root.join("artifacts");
    let failure_dir = case_root.join("failures");
    let user_templates_dir = case_root.join("templates");

    for dir in [
        &downloads_dir,
        &temp_dir,
        &debug_render_dir,
        &artifacts_dir,
        &failure_dir,
        &user_templates_dir,
    ] {
        fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    }

    let repo_root = test_config::repo_git_root();
    let app_paths = AppPaths {
        repo_root: repo_root.clone(),
        font_dirs: vec![repo_root.join("fonts")]
            .into_iter()
            .filter(|path| path.is_dir())
            .collect(),
        debug_render_dir,
        temp_dir,
        bundled_templates_dirs: vec![repo_root.join("templates")]
            .into_iter()
            .filter(|path| path.is_dir())
            .collect(),
        user_templates_dir,
        downloads_dir,
    };

    Ok(CaseRuntime {
        app_paths,
        artifacts_dir,
        failure_dir,
    })
}

/// Loads a `RenderConfig` from the fixture tree or a real template file.
///
/// Plain config fixtures (the `RenderConfig` shape directly) are parsed as-is.
/// Real template files (wrapping `{ "format": "ovrley-template", "config": {...} }`)
/// have their `config` sub-object extracted first.
fn load_config(relative_path: &str) -> Result<RenderConfig> {
    let path = fixtures_root().join(relative_path);
    let json = fs::read_to_string(&path)
        .with_context(|| format!("failed to read config fixture {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&json)
        .with_context(|| format!("failed to parse JSON from {}", path.display()))?;
    let config_str = match value.get("config") {
        Some(config_obj) => serde_json::to_string(config_obj)
            .context("failed to serialize extracted template config")?,
        None => json,
    };
    parse_config_json(&config_str).context("failed to parse config")
}

/// Loads a `ParsedActivity` from the fixture tree.
fn load_activity(relative_path: &str) -> Result<ParsedActivity> {
    let json = fs::read_to_string(fixtures_root().join(relative_path))
        .with_context(|| format!("failed to read activity fixture {relative_path}"))?;
    parse_activity_json(&json).context("failed to parse activity fixture")
}

/// Compares or records all selected frames for a rendered video output.
fn compare_video_frames(
    case_label: &str,
    runtime: &CaseRuntime,
    video_path: &Path,
    baseline_dir_relative: &str,
    frame_indices: &[u32],
) -> Result<()> {
    for &frame_index in frame_indices {
        let actual_frame_path = runtime
            .artifacts_dir
            .join(format!("frame_{frame_index:04}.png"));
        extract_video_frame_png(
            &runtime.app_paths.repo_root,
            video_path,
            frame_index,
            &actual_frame_path,
        )?;

        let baseline_path = fixtures_root()
            .join(baseline_dir_relative)
            .join(format!("frame_{frame_index:04}.png"));
        compare_or_record_png(
            &format!("{case_label}/frame_{frame_index:04}"),
            &actual_frame_path,
            &baseline_path,
            &runtime.failure_dir,
        )?;
    }
    Ok(())
}

/// Extracts one exact frame index from a rendered video into a PNG artifact.
fn extract_video_frame_png(
    repo_root: &Path,
    video_path: &Path,
    frame_index: u32,
    output_path: &Path,
) -> Result<()> {
    let ffmpeg = resolve_ffmpeg_binary(repo_root).context("failed to resolve ffmpeg")?;
    let filter = format!("select=eq(n\\,{frame_index})");
    let output = Command::new(ffmpeg)
        .args(["-v", "error", "-y", "-i"])
        .arg(video_path)
        .args([
            "-map",
            "0:v:0",
            "-vf",
            &filter,
            "-frames:v",
            "1",
            "-pix_fmt",
            "rgba",
        ])
        .arg(output_path)
        .output()
        .with_context(|| {
            format!(
                "failed to run ffmpeg frame extraction for {}",
                video_path.display()
            )
        })?;

    if !output.status.success() {
        bail!(
            "ffmpeg failed to extract frame {frame_index} from {}: {}",
            video_path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    if !output_path.is_file() {
        bail!(
            "ffmpeg reported success but did not create extracted frame {}",
            output_path.display()
        );
    }
    Ok(())
}

/// Records a new baseline when requested, otherwise compares exact decoded pixels.
fn compare_or_record_png(
    case_label: &str,
    actual_path: &Path,
    baseline_path: &Path,
    failure_dir: &Path,
) -> Result<()> {
    // ── Phase 1: ensure baseline directory exists ────────────────────
    if let Some(parent) = baseline_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    // ── Phase 2: record mode — copy actual over baseline and return ──
    if should_record_baselines() {
        fs::copy(actual_path, baseline_path).with_context(|| {
            format!(
                "failed to record baseline {} from {}",
                baseline_path.display(),
                actual_path.display()
            )
        })?;
        println!(
            "baseline recorded for {case_label}: {}",
            baseline_path.display()
        );
        return Ok(());
    }

    // ── Phase 3: compare mode — ensure baseline exists ──────────────
    if !baseline_path.is_file() {
        bail!(
            "missing baseline for {case_label}: {}. Record baselines with `OVRLEY_RECORD_BASELINES=1 cargo test -p ovrley_core --test render_baseline_suite -- --nocapture`.",
            baseline_path.display()
        );
    }

    // ── Phase 4: decode both images to raw RGBA ─────────────────────
    let repo_root = test_config::repo_git_root();
    let actual = decode_png_to_rgba(&repo_root, actual_path)?;
    let baseline = decode_png_to_rgba(&repo_root, baseline_path)?;

    // ── Phase 5: check dimension equality ───────────────────────────
    if (actual.width, actual.height) != (baseline.width, baseline.height) {
        let mismatch_dir = failure_dir.join(sanitize_case_label(case_label));
        fs::create_dir_all(&mismatch_dir)
            .with_context(|| format!("failed to create {}", mismatch_dir.display()))?;
        fs::copy(actual_path, mismatch_dir.join("actual.png"))
            .with_context(|| format!("failed to copy mismatch artifact for {case_label}"))?;
        bail!(
            "dimension mismatch for {case_label}: actual {:?}, baseline {:?}",
            (actual.width, actual.height),
            (baseline.width, baseline.height)
        );
    }

    // ── Phase 6: per-pixel comparison — build diff image and parity stats ──
    let width = actual.width;
    let height = actual.height;
    let mut stats = ParityStats {
        total_pixels: u64::from(width) * u64::from(height),
        diff_pixels: 0,
    };
    let mut diff_bytes = vec![0u8; actual.bytes.len()];

    for pixel_index in 0..(width as usize * height as usize) {
        let offset = pixel_index * 4;
        let actual_px = &actual.bytes[offset..offset + 4];
        let baseline_px = &baseline.bytes[offset..offset + 4];
        let mut pixel_changed = false;

        for idx in 0..4 {
            let delta = actual_px[idx].abs_diff(baseline_px[idx]);
            diff_bytes[offset + idx] = delta;
            if delta > 0 {
                pixel_changed = true;
            }
        }

        diff_bytes[offset + 3] = 255;
        if pixel_changed {
            stats.diff_pixels += 1;
        }
    }

    if stats.diff_pixels == 0 {
        print_parity_summary(case_label, &stats);
        return Ok(());
    }

    // ── Phase 7: produce mismatch artifacts only on failure ─────────
    let mismatch_dir = failure_dir.join(sanitize_case_label(case_label));
    fs::create_dir_all(&mismatch_dir)
        .with_context(|| format!("failed to create {}", mismatch_dir.display()))?;
    fs::copy(actual_path, mismatch_dir.join("actual.png"))
        .with_context(|| format!("failed to copy actual mismatch artifact for {case_label}"))?;
    fs::copy(baseline_path, mismatch_dir.join("baseline.png"))
        .with_context(|| format!("failed to copy baseline mismatch artifact for {case_label}"))?;
    write_rgba_png(
        &repo_root,
        width,
        height,
        &diff_bytes,
        &mismatch_dir.join("diff.png"),
    )
    .with_context(|| format!("failed to write diff image for {case_label}"))?;

    print_parity_summary(case_label, &stats);
    bail!(
        "baseline mismatch for {case_label}: {} different pixels ({:.6}%). Failure artifacts: {}",
        stats.diff_pixels,
        stats.different_pixel_percent(),
        mismatch_dir.display()
    );
}

/// Prints one parity line for a comparison so successful runs show more than `ok`.
fn print_parity_summary(case_label: &str, stats: &ParityStats) {
    println!(
        "parity {case_label}: {} different pixels ({:.6}%)",
        stats.diff_pixels,
        stats.different_pixel_percent()
    );
}

/// Decodes a PNG file into raw RGBA bytes using the repo's ffmpeg binary.
///
/// This keeps the suite offline-friendly: we do not add any new image-decoding
/// crates, but we still compare rendered pixels instead of PNG file bytes.
fn decode_png_to_rgba(repo_root: &Path, png_path: &Path) -> Result<DecodedRgbaImage> {
    let metadata = probe_video(repo_root, &png_path.to_string_lossy())
        .with_context(|| format!("failed to probe PNG {}", png_path.display()))?;
    let resolution = metadata
        .resolution
        .ok_or_else(|| anyhow!("PNG is missing resolution metadata: {}", png_path.display()))?;
    let width = u32::try_from(resolution.width)
        .with_context(|| format!("PNG width does not fit in u32: {}", resolution.width))?;
    let height = u32::try_from(resolution.height)
        .with_context(|| format!("PNG height does not fit in u32: {}", resolution.height))?;

    let ffmpeg = resolve_ffmpeg_binary(repo_root).context("failed to resolve ffmpeg")?;
    let output = Command::new(ffmpeg)
        .args(["-v", "error", "-i"])
        .arg(png_path)
        .args([
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to decode PNG {}", png_path.display()))?;

    if !output.status.success() {
        bail!(
            "ffmpeg failed to decode PNG {}: {}",
            png_path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let expected_len = width as usize * height as usize * 4;
    if output.stdout.len() != expected_len {
        bail!(
            "decoded PNG {} to {} bytes, expected {} bytes for {}x{} RGBA",
            png_path.display(),
            output.stdout.len(),
            expected_len,
            width,
            height
        );
    }

    Ok(DecodedRgbaImage {
        width,
        height,
        bytes: output.stdout,
    })
}

/// Writes raw RGBA bytes to a PNG file through ffmpeg.
///
/// The suite only uses this for mismatch artifacts, so developers get an
/// immediately viewable diff image without needing any extra image crates.
fn write_rgba_png(
    repo_root: &Path,
    width: u32,
    height: u32,
    bytes: &[u8],
    output_path: &Path,
) -> Result<()> {
    let raw_path = output_path.with_extension("rgba");
    fs::write(&raw_path, bytes)
        .with_context(|| format!("failed to write {}", raw_path.display()))?;

    let ffmpeg = resolve_ffmpeg_binary(repo_root).context("failed to resolve ffmpeg")?;
    let output = Command::new(ffmpeg)
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-video_size",
            &format!("{width}x{height}"),
            "-i",
        ])
        .arg(&raw_path)
        .arg(output_path)
        .output()
        .with_context(|| format!("failed to encode diff PNG {}", output_path.display()))?;

    if !output.status.success() {
        bail!(
            "ffmpeg failed to encode diff PNG {}: {}",
            output_path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let _ = fs::remove_file(raw_path);
    Ok(())
}

/// Validates the key video metadata fields we use as baseline invariants.
fn assert_video_metadata(
    metadata: &VideoMetadata,
    expected_width: u32,
    expected_height: u32,
    expected_fps_num: u32,
    expected_fps_den: u32,
    expected_codec_name: &str,
    expected_pix_fmt: Option<&str>,
    expected_has_audio: Option<bool>,
) -> Result<()> {
    let resolution = metadata
        .resolution
        .as_ref()
        .ok_or_else(|| anyhow!("video metadata is missing resolution"))?;
    if resolution.width != u64::from(expected_width)
        || resolution.height != u64::from(expected_height)
    {
        bail!(
            "unexpected resolution: got {}x{}, expected {}x{}",
            resolution.width,
            resolution.height,
            expected_width,
            expected_height
        );
    }

    if metadata.fps_num != Some(expected_fps_num) || metadata.fps_den != Some(expected_fps_den) {
        bail!(
            "unexpected fps rational: got {:?}/{:?}, expected {}/{}",
            metadata.fps_num,
            metadata.fps_den,
            expected_fps_num,
            expected_fps_den
        );
    }

    if metadata.codec_name.as_deref() != Some(expected_codec_name) {
        bail!(
            "unexpected codec name: got {:?}, expected {}",
            metadata.codec_name,
            expected_codec_name
        );
    }

    if let Some(expected_pix_fmt) = expected_pix_fmt {
        if metadata.pix_fmt.as_deref() != Some(expected_pix_fmt) {
            bail!(
                "unexpected pixel format: got {:?}, expected {}",
                metadata.pix_fmt,
                expected_pix_fmt
            );
        }
    }

    if let Some(expected_has_audio) = expected_has_audio {
        if metadata.has_audio != expected_has_audio {
            bail!(
                "unexpected audio presence: got {}, expected {}",
                metadata.has_audio,
                expected_has_audio
            );
        }
    }

    Ok(())
}

/// Ensures a rendered output exists and is non-empty before deeper assertions.
fn assert_nonempty_output(path: &Path) -> Result<()> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("missing rendered output {}", path.display()))?;
    if metadata.len() == 0 {
        bail!("rendered output is empty: {}", path.display());
    }
    Ok(())
}

/// Ensures the composite source video fixture exists where the suite expects it.
fn ensure_source_video_exists(path: &Path) -> Result<()> {
    if path.is_file() {
        return Ok(());
    }

    bail!(
        "missing composite source fixture {}. Place `test-1080p.mp4` under `src-tauri/ovrley_core/tests/fixtures/video/` and rerun `cargo test -p ovrley_core --test render_baseline_suite -- --nocapture`.",
        path.display()
    );
}

/// Returns true when the suite should overwrite baselines instead of comparing.
fn should_record_baselines() -> bool {
    std::env::var("OVRLEY_RECORD_BASELINES")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

/// Absolute path to the `ovrley_core` integration-test fixture tree.
fn fixtures_root() -> PathBuf {
    test_config::fixtures()
}

/// Converts a logical case label into a file-system-safe directory name.
fn sanitize_case_label(label: &str) -> String {
    label.replace(['/', '\\', ':'], "_")
}
