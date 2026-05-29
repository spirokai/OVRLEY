//! Helper functions for the canvas-parity integration test.
//!
//! This module provides all the machinery to compare a Rust Skia-rendered
//! frame against a Playwright-captured browser SVG frame.  The pipeline:
//!
//! 1. Parse activity + config fixtures
//! 2. Build a dense activity report (interpolated frame data)
//! 3. Render the frame via Skia → `skia.png`
//! 4. Serialize mock data for the frontend (`template.json`, `activity.json`,
//!    `store-state.json`)
//! 5. Start a Vite dev server hosting the React editor app
//! 6. Run a Playwright script that loads the app, injects the mock data,
//!    hides editor chrome, forces native-scale rendering, and screenshots
//!    the widget layer → `canvas.png`
//! 7. Compare the two PNGs via ffmpeg SSIM (cropped to alpha union)
//! 8. Generate a pixel-level diff image + detailed mismatch statistics
//!
//! # Pixel comparison layers
//!
//! The diff analysis categorises every pixel RGBA channel into several
//! concentric buckets so a developer can distinguish genuine rendering bugs
//! from harmless anti-aliasing divergence:
//!
//! | Category | Meaning |
//! |---|---|
//! | **Full-frame mismatch** | Any pixel where at least one RGBA channel differs between Skia and Canvas. Includes transparent / empty pixels. |
//! | **Overlay pixel** | A pixel where *either* image has alpha > `ALPHA_MASK_THRESHOLD` (2). Ignores the vast transparent area. |
//! | **Overlay mismatch** | Overlay pixel with *any* channel difference. |
//! | **Significant mismatch** | Overlay pixel where the max channel delta exceeds `DIFF_CHANNEL_TOLERANCE` (4). Filters out sub-threshold noise. |
//! | **Edge-insensitive mismatch** | Significant mismatch that is *not* on an alpha edge. The cleanest proxy for "real rendering differences." |
//! | **Canvas-only / Skia-only** | Overlay pixel where only one image has alpha > `ONLY_PIXEL_ALPHA_THRESHOLD` (96). Indicates content present in one renderer but absent in the other. |
//!
//! # Threshold constants
//!
//! All five constants at the module level (lines 548-552) control the
//! sensitivity of the diff.  See their individual doc comments for details.
//!
//! All functions in this module are only compiled when the `canvas-parity`
//! Cargo feature is enabled.

use anyhow::{bail, Context, Result};
use ovrley_core::activity::schema::{DenseActivityReport, ParsedActivity};
use ovrley_core::activity::parse_activity_json;
use ovrley_core::config::{parse_template_json, RenderConfig};
use ovrley_core::encode::ffmpeg::resolve_ffmpeg_binary;
use ovrley_core::paths::AppPaths;
use ovrley_core::render::{
    prepare_preview_assets, render_preview_with_prepared_assets, PreviewRenderRequest,
};
use serde_json::Value;
use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::fs;
use std::net::TcpListener;
use std::time::{Duration, Instant};

// ── Fixture parsing ─────────────────────────────────────────────────────────

/// Parses the activity and config fixtures from the given fixture root.
///
/// Returns `(parsed_activity, render_config, raw_activity_json, raw_config_json)`.
pub fn parse_fixtures(fixture_root: &Path) -> Result<(ParsedActivity, RenderConfig, Value, Value)> {
    parse_fixtures_with_config(fixture_root, "test-template-4k.json")
}

/// Parses the activity and config fixtures, using the specified config filename.
pub fn parse_fixtures_with_config(
    fixture_root: &Path,
    config_filename: &str,
) -> Result<(ParsedActivity, RenderConfig, Value, Value)> {
    let activity_path = fixture_root.join("activity").join("gpx-parse-debug.json");
    let config_path = fixture_root.join("config").join(config_filename);

    let activity_raw: Value = read_json(&activity_path)?;
    let config_raw: Value = read_json(&config_path)?;

    let activity = parse_activity_json(&serde_json::to_string(&activity_raw)?)
        .context("failed to parse activity fixture")?;
    let config = parse_template_json(&serde_json::to_string(&config_raw)?)
        .context("failed to parse config fixture")?;

    Ok((activity, config, activity_raw, config_raw))
}

/// Reads a JSON file as a serde Value.
fn read_json(path: &Path) -> Result<Value> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("failed to parse JSON from {}", path.display()))
}

// ── Skia render ─────────────────────────────────────────────────────────────

/// Renders a Skia preview PNG at the given second.
pub fn render_skia_preview(
    paths: &AppPaths,
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    second: u32,
    out_path: &Path,
) -> Result<()> {
    println!(
        "  rendering Skia frame at second {second} → {}",
        out_path.display()
    );
    let (prepared, label_cache_status, prepare_timings, extra_total_ms) =
        prepare_preview_assets(paths, config, activity, dense_activity)
            .context("prepare_preview_assets failed")?;
    render_preview_with_prepared_assets(PreviewRenderRequest {
        paths,
        config,
        dense_activity,
        prepared_preview_assets: &prepared,
        second,
        prepare_timings,
        label_cache_status,
        extra_total_ms,
        out_path,
    })
    .context("render_preview_with_prepared_assets failed")?;
    println!("  Skia render complete: {} kB", file_size_kb(out_path));
    Ok(())
}

/// Returns file size in KB.
fn file_size_kb(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len() / 1024).unwrap_or(0)
}

// ── AppPaths builder ──────────────────────────────────────────────────────

/// Builds an AppPaths targeting a test workspace inside a case directory.
pub fn test_app_paths(git_root: &Path, case_root: &Path) -> Result<AppPaths> {
    let downloads_dir = case_root.join("downloads");
    let temp_dir = case_root.join("tmp");
    let debug_render_dir = case_root.join("debug_render");

    for dir in [&downloads_dir, &temp_dir, &debug_render_dir] {
        fs::create_dir_all(dir)
            .with_context(|| format!("failed to create {}", dir.display()))?;
    }

    let app_paths = AppPaths {
        repo_root: git_root.to_path_buf(),
        font_dirs: vec![git_root.join("fonts")]
            .into_iter()
            .filter(|p| p.is_dir())
            .collect(),
        debug_render_dir,
        temp_dir,
        bundled_templates_dirs: vec![git_root.join("templates")]
            .into_iter()
            .filter(|p| p.is_dir())
            .collect(),
        user_templates_dir: case_root.join("templates"),
        downloads_dir,
    };

    let _ = app_paths.ensure_dirs();
    Ok(app_paths)
}

// ── Mock data generation ─────────────────────────────────────────────────────

/// Writes `template.json`, `activity.json`, and `store-state.json` to the given
/// mock directory, mimicking what the frontend receives from the backend.
pub fn write_mock_data(
    mock_dir: &Path,
    config: &RenderConfig,
    config_raw: &Value,
    activity: &ParsedActivity,
    activity_raw: &Value,
    selected_second: u32,
) -> Result<()> {
    fs::create_dir_all(mock_dir)
        .with_context(|| format!("failed to create mock dir {}", mock_dir.display()))?;
    let global_defaults = resolve_global_defaults(config, config_raw);

    // 1. template.json — ovrley-template envelope wrapping the config
    let config_obj = config_raw
        .get("config")
        .cloned()
        .unwrap_or_else(|| config_raw.clone());
    let template = serde_json::json!({
        "format": "ovrley-template",
        "version": 2,
        "name": "test",
        "savedAt": "2026-05-10T00:00:00.000Z",
        "config": config_obj,
        "settings": {
            "globalDefaults": global_defaults.clone()
        }
    });
    write_json(&mock_dir.join("template.json"), &template)?;
    println!("  wrote template.json");

    // 2. activity.json — raw parsed activity data (same as the fixture)
    write_json(&mock_dir.join("activity.json"), activity_raw)?;
    println!("  wrote activity.json");

    // 3. store-state.json — Zustand store snapshot
    let config_value = serde_json::to_value(config)
        .context("failed to serialize config for store state")?;
    let store_state = serde_json::json!({
        "config": config_value,
        "globalDefaults": global_defaults,
        "selectedSecond": selected_second,
        "startSecond": config.scene.start,
        "endSecond": config.scene.end,
        "activitySummary": derive_activity_summary(activity),
        "backgroundMode": "none",
        "gridVisible": false,
        "selectedWidgetId": null,
        "editorZoomLevel": 1.0,
        "widgetDrawerOpen": false
    });
    write_json(&mock_dir.join("store-state.json"), &store_state)?;
    println!("  wrote store-state.json");

    Ok(())
}

fn resolve_global_defaults(config: &RenderConfig, config_raw: &Value) -> Value {
    config_raw
        .get("settings")
        .and_then(|settings| settings.get("globalDefaults"))
        .cloned()
        .unwrap_or_else(|| derive_global_defaults(config))
}

/// Derives global default values from a RenderConfig.
fn derive_global_defaults(config: &RenderConfig) -> Value {
    serde_json::json!({
        "font_values": config.scene.font.as_deref().unwrap_or("Furore.otf"),
        "font_text": config.scene.font.as_deref().unwrap_or("Arial.ttf"),
        "color_values": config.scene.color.as_deref().unwrap_or("#ffffff"),
        "color_text": config.scene.color.as_deref().unwrap_or("#ffffff"),
        "color_icons": config.scene.color.as_deref().unwrap_or("#ffffff"),
        "border_color": config.scene.border_color.as_deref().unwrap_or("#ff0000"),
        "border_thickness": config.scene.border_thickness.unwrap_or(0.0) as i32,
        "border_strength": config.scene.border_strength.unwrap_or(0.0) as i32,
        "border_distance": config.scene.border_distance.unwrap_or(0.0) as i32,
        "shadow_color": config.scene.shadow_color.as_deref().unwrap_or("#0059ff"),
        "shadow_strength": config.scene.shadow_strength.unwrap_or(0.0) as i32,
        "shadow_distance": config.scene.shadow_distance.unwrap_or(0.0) as i32,
        "opacity": (config.scene.opacity.unwrap_or(1.0) * 100.0).round() as i32,
        "scale": config.scene.scale.unwrap_or(2.0) as i32,
    })
}

fn derive_activity_summary(activity: &ParsedActivity) -> Value {
    let sample_count = activity.sample_elapsed_seconds.len();
    let duration_seconds = activity
        .trim_end_seconds
        .max(activity.sample_elapsed_seconds.last().copied().unwrap_or(0.0));
    let total_distance = estimate_total_distance(activity);
    serde_json::json!({
        "durationSeconds": duration_seconds as u64,
        "endTime": activity.source_start_time.as_deref().unwrap_or(""),
        "fileFormat": activity.file_format.as_deref().unwrap_or("gpx"),
        "fileName": activity.file_name.as_deref().unwrap_or("activity"),
        "sampleCount": sample_count,
        "startTime": activity.source_start_time.as_deref().unwrap_or(""),
        "totalDistanceMeters": total_distance,
    })
}

fn estimate_total_distance(activity: &ParsedActivity) -> f64 {
    activity
        .sample_distance_progress
        .last()
        .copied()
        .unwrap_or(0.0)
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    let json = serde_json::to_string_pretty(value)
        .with_context(|| format!("failed to serialize JSON for {}", path.display()))?;
    fs::write(path, &json)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

// ── Cross-platform command helpers ───────────────────────────────────────────

/// Creates a [`Command`] that works on both Windows and Unix.
///
/// On Windows, wraps the program via `cmd.exe /c <program>` so that
/// `.cmd` batch files (e.g. `pnpm.cmd`, `node.cmd`) are found on PATH.
/// On Unix, directly spawns the program.
fn platform_command(program: &str) -> Command {
    if cfg!(windows) {
        let mut cmd = Command::new("cmd.exe");
        cmd.arg("/c").arg(program);
        cmd
    } else {
        Command::new(program)
    }
}

// ── Vite server manager ──────────────────────────────────────────────────────

/// Manages a Vite dev server process with automatic cleanup via Drop.
pub struct ViteServer {
    process: Option<Child>,
    port: u16,
}

impl ViteServer {
    /// Starts a Vite dev server on a random available port.
    pub fn start(git_root: &Path) -> Result<Self> {
        let port = find_free_port().context("failed to find a free port for Vite")?;

        println!("  starting Vite dev server on port {port}...");

        let mut cmd = platform_command("pnpm");
        cmd.args(["exec", "vite", "--port", &port.to_string(), "--strictPort"]);
        // Vite must be run from the `app/` subdirectory where it's installed
        cmd.current_dir(git_root.join("app"));
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let process = cmd.spawn().with_context(|| {
            format!(
                "failed to spawn `pnpm exec vite` on port {port}. \
                 Is Node.js / pnpm / Vite installed? \
                 Try `pnpm install` from the repo root."
            )
        })?;

        let mut server = ViteServer {
            process: Some(process),
            port,
        };

        server
            .wait_for_ready(Duration::from_secs(60))
            .context("Vite dev server failed to start within 60s")?;

        println!("  Vite dev server ready at http://localhost:{port}");
        Ok(server)
    }

    /// Returns the base URL of the Vite server.
    pub fn url(&self) -> String {
        format!("http://localhost:{}", self.port)
    }

    /// Waits for Vite to print its "Local:" URL line on stdout.
    ///
    /// Uses a background thread to read stdout so we don't block forever
    /// on Windows pipes (which don't support read timeouts). The thread
    /// owns the stdout handle and exits when the process is killed.
    fn wait_for_ready(&mut self, timeout: Duration) -> Result<()> {
        let start = Instant::now();

        let process = self
            .process
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Vite process not started"))?;

        let mut stdout = process
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Vite process stdout was not captured"))?;

        // Spawn a thread that reads stdout, sends lines via channel
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(&mut stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        let trimmed = line.trim().to_string();
                        if !trimmed.is_empty() {
                            let _ = tx.send(trimmed);
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // Poll for lines with timeout and process-exit checks
        loop {
            let elapsed = start.elapsed();

            if elapsed > timeout {
                let stderr_log = drain_stderr(process);
                if !stderr_log.is_empty() {
                    eprintln!("Vite stderr (timeout): {stderr_log}");
                }
                bail!(
                    "Vite dev server did not print 'Local:' within {timeout:?}. \
                     Check that `pnpm exec vite` works from the repo root."
                );
            }

            // Check if process has exited
            if let Some(status) = process.try_wait()? {
                let stderr_log = drain_stderr(process);
                if !stderr_log.is_empty() {
                    eprintln!("Vite stderr: {stderr_log}");
                }
                bail!("Vite dev server exited prematurely with status {status}");
            }

            // Try to receive a line from the reader thread (non-blocking)
            match rx.try_recv() {
                Ok(line) => {
                    // Vite output — keep silent in test
                    // ANSI codes may appear between "Local" and ":", so
                    // check for the presence of "Local" + "localhost" instead.
                    if line.contains("Local") && line.contains("localhost") {
                        return Ok(());
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Reader thread ended — stdout closed
                    match process.try_wait() {
                        Ok(Some(status)) => {
                            bail!("Vite stdout reader ended; process exited with status {status}");
                        }
                        Ok(None) => {
                            std::thread::sleep(Duration::from_millis(100));
                        }
                        Err(e) => bail!("error checking Vite process: {e}"),
                    }
                }
            }
        }
    }
}

impl Drop for ViteServer {
    fn drop(&mut self) {
        if let Some(mut process) = self.process.take() {
            #[cfg(windows)]
            {
                // On Windows, process.kill() only kills cmd.exe, not the
                // actual Node.js child.  Use taskkill /T to kill the tree.
                let pid = process.id();
                let _ = Command::new("taskkill")
                    .args(["/f", "/t", "/pid", &pid.to_string()])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            #[cfg(not(windows))]
            {
                let _ = process.kill();
            }
            let _ = process.wait();
        }
    }
}

/// Drains stderr from a child process into a String for diagnostics.
fn drain_stderr(process: &mut Child) -> String {
    if let Some(ref mut stderr) = process.stderr {
        let mut buf = String::new();
        let _ = stderr.read_to_string(&mut buf);
        buf
    } else {
        String::new()
    }
}

/// Binds to port 0 and returns the assigned port number.
fn find_free_port() -> Result<u16> {
    let listener =
        TcpListener::bind("127.0.0.1:0").context("failed to bind 127.0.0.1:0")?;
    let port = listener
        .local_addr()
        .context("failed to get local address")?
        .port();
    drop(listener);
    // Give the OS a moment to release the socket
    std::thread::sleep(Duration::from_millis(50));
    Ok(port)
}

// ── Playwright runner ────────────────────────────────────────────────────────

/// Result from the Playwright screenshot step.
pub struct CanvasScreenshotInfo {
    pub width: u32,
    pub height: u32,
}

/// Runs the Playwright screenshot script, waiting for it to complete.
/// Returns the captured dimensions.
pub fn run_playwright_screenshot(
    script_path: &Path,
    mock_dir: &Path,
    vite_url: &str,
    out_path: &Path,
) -> Result<CanvasScreenshotInfo> {
    println!("  running Playwright screenshot script...");

    let mut cmd = platform_command("node");
    cmd.arg(script_path)
        .arg("--mock-dir")
        .arg(mock_dir)
        .arg("--vite-url")
        .arg(vite_url)
        .arg("--out")
        .arg(out_path);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let output = cmd.output().with_context(|| {
            format!(
                "failed to spawn Node.js script at {}. Is Node.js installed?",
                script_path.display()
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.trim().is_empty() {
            println!("  Playwright stdout: {stdout}");
        }
        if !stderr.trim().is_empty() {
            eprintln!("  Playwright stderr: {stderr}");
        }
        bail!(
            "Playwright script exited with code {}: {}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();

    if !out_path.is_file() {
        bail!(
            "Playwright script reported success but did not create {}",
            out_path.display()
        );
    }

    println!("  Playwright screenshot saved: {} kB", file_size_kb(out_path));

    // Parse JSON from stdout: {"width": 3840, "height": 2160, "bg": "transparent"}
    let (canvas_w, canvas_h) = if let Ok(dims) = serde_json::from_str::<serde_json::Value>(trimmed) {
        let w = dims.get("width").and_then(|v| v.as_u64()).unwrap_or(3840) as u32;
        let h = dims.get("height").and_then(|v| v.as_u64()).unwrap_or(2160) as u32;
        let b = dims.get("bg").and_then(|v| v.as_str()).unwrap_or("transparent").to_string();
        let editor_chrome = dims
            .get("editorChrome")
            .and_then(|v| v.as_str())
            .unwrap_or("hidden");
        println!("  canvas dimensions: {w}x{h}  bg: {b}  editor chrome: {editor_chrome}");
        (w, h)
    } else {
        if !trimmed.is_empty() {
            println!("  Playwright output: {trimmed}");
        }
        (3840, 2160)
    };

    Ok(CanvasScreenshotInfo {
        width: canvas_w,
        height: canvas_h,
    })
}

// ── SSIM runner ──────────────────────────────────────────────────────────────

/// SSIM (Structural Similarity Index) comparison scores from ffmpeg.
///
/// Fields are named `y`/`u`/`v` for YUV pixel formats or `r`/`g`/`b` for
/// RGB formats — the parser tries YUV first, then falls back to RGB.
/// `combined` is the aggregate (All:) score.
pub struct SsimResult {
    /// Aggregate SSIM score (All:).
    pub combined: f64,
    /// Channel 1 — either Y (luma) or R.
    pub y: f64,
    /// Channel 2 — either U (chroma blue) or G.
    pub u: f64,
    /// Channel 3 — either V (chroma red) or B.
    pub v: f64,
}

#[derive(Clone, Copy)]
struct AlphaBounds {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

/// Minimum alpha value (0-255) for a pixel to be considered part of the overlay.
///
/// Pixels with alpha ≤ this threshold in *both* images are treated as
/// empty/transparent background and excluded from overlay-level statistics.
/// Set to 2 so that nearly-transparent anti-aliased fringe pixels are still
/// included in the overlay comparison.
const ALPHA_MASK_THRESHOLD: u8 = 2;

/// Alpha threshold for classifying a pixel as "only present in one renderer."
///
/// If the Skia image has alpha > this value while the Canvas image has
/// alpha ≤ this value, the pixel is counted as **Skia-only** (and vice versa
/// for Canvas-only).  Set to 96 (roughly 38 % opacity) so that very faint
/// translucent artifacts do not pollute the orphaned-pixel counts.
const ONLY_PIXEL_ALPHA_THRESHOLD: u8 = 96;

/// Maximum per-channel difference below which a pixel is *not* considered a
/// significant mismatch.
///
/// Tiny channel deltas arise from different FreeType glyph rasterisation
/// (sub-pixel hinting, gamma correction, ClearType vs. greyscale AA).
/// Pixels whose R, G, B, and A all differ by ≤ this value are treated as
/// matching.  Set to 4, which is barely perceptible and filters out
/// harmless sub-pixel positioning noise while catching real colour errors.
const DIFF_CHANNEL_TOLERANCE: u8 = 4;

/// Minimum absolute difference in alpha between a pixel and any of its
/// 8 neighbours for that pixel to be flagged as an "alpha edge."
///
/// A value of 0 means *any* alpha gradient qualifies.  Because uniformly
/// translucent fills (e.g. a semi-transparent shape) should *not* be
/// treated as edges, the edge detector also requires that the pixel's
/// alpha is neither fully transparent nor fully opaque (see
/// [`is_alpha_edge_pixel`]).
const EDGE_ALPHA_DELTA_THRESHOLD: u8 = 0;

/// Radius (in pixels) around each alpha-edge seed pixel that is excluded
/// from "edge-insensitive" mismatch counting.
///
/// When > 0, a band of pixels around every alpha transition is ignored
/// for the clean mismatch metric.  Currently set to 0 so every
/// non-edge pixel is compared strictly.
const EDGE_IGNORE_RADIUS: i32 = 0;

/// Queries (width, height) of a PNG file via ffprobe.
pub fn probe_png_dimensions(ffprobe: &Path, path: &Path) -> Result<(u32, u32)> {
    let output = Command::new(ffprobe)
        .args([
            "-v", "error",
            "-show_entries", "stream=width,height",
            "-of", "csv=p=0",
            &path.to_string_lossy().to_string(),
        ])
        .output()
        .with_context(|| format!("failed to probe {}", path.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let dims: Vec<u32> = stdout
        .trim()
        .split(',')
        .filter_map(|s| s.trim().parse::<u32>().ok())
        .collect();
    if dims.len() >= 2 {
        Ok((dims[0], dims[1]))
    } else {
        bail!("failed to parse PNG dimensions from ffprobe: {stdout}")
    }
}

/// Derives the ffprobe path from the resolved ffmpeg path.
pub fn resolve_ffprobe(ffmpeg: &Path) -> PathBuf {
    let mut p = ffmpeg.to_path_buf();
    let name = if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" };
    p.set_file_name(name);
    if p.is_file() {
        return p;
    }
    let fallback = ffmpeg.parent().unwrap().join(name);
    if fallback.is_file() { fallback } else { ffmpeg.to_path_buf() }
}

/// Runs ffmpeg's SSIM filter comparing two same-sized PNG images.
///
/// The comparison is **cropped to the union of non-transparent pixels**
/// (alpha > `ALPHA_MASK_THRESHOLD`).  This prevents the large empty
/// transparent canvas area from dominating the score.  The crop is
/// computed via [`alpha_union_bounds`] on the raw RGBA data of both images.
///
/// If the Skia PNG has different dimensions from the Canvas PNG, it is
/// scaled via Lanczos to match before the comparison.
///
/// Returns an [`SsimResult`] with per-channel + aggregate scores.
///
/// # Requirements
///
/// - ffmpeg must be available (resolved via [`resolve_ffmpeg_binary`]).
pub fn run_ssim(
    skia_path: &Path,
    canvas_path: &Path,
    repo_root: &Path,
) -> Result<SsimResult> {
    println!("  running ffmpeg SSIM comparison...");

    let ffmpeg =
        resolve_ffmpeg_binary(repo_root).context("failed to resolve ffmpeg binary")?;
    let ffprobe = resolve_ffprobe(&ffmpeg);

    // Detect exact canvas PNG dimensions via ffprobe
    let (canvas_w, canvas_h) = probe_png_dimensions(&ffprobe, canvas_path)?;

    println!(
        "  canvas PNG: {canvas_w}x{canvas_h}"
    );

    // Scale the Skia PNG to match canvas dimensions if needed
    let (skia_w, skia_h) = probe_png_dimensions(&ffprobe, skia_path)?;
    let input_a = if skia_w != canvas_w || skia_h != canvas_h {
        let scaled = skia_path.with_extension("ssim-scaled.png");
        let scale_filter = format!("scale={canvas_w}:{canvas_h}:flags=lanczos");
        let scale_output = Command::new(&ffmpeg)
            .args(["-v", "error", "-y", "-i"])
            .arg(skia_path)
            .args(["-vf", &scale_filter, &scaled.to_string_lossy()])
            .output()?;
        if !scale_output.status.success() {
            bail!("ffmpeg scale failed: {}", String::from_utf8_lossy(&scale_output.stderr).trim());
        }
        scaled
    } else {
        skia_path.to_path_buf()
    };

    let (_, _, skia_bytes) = decode_png_to_rgba(&ffmpeg, &input_a)?;
    let (_, _, canvas_bytes) = decode_png_to_rgba(&ffmpeg, canvas_path)?;
    let bounds = alpha_union_bounds(
        &skia_bytes,
        &canvas_bytes,
        canvas_w,
        canvas_h,
        ALPHA_MASK_THRESHOLD,
    )
    .unwrap_or(AlphaBounds {
        x: 0,
        y: 0,
        width: canvas_w,
        height: canvas_h,
    });
    println!(
        "  SSIM crop from alpha union (alpha > {ALPHA_MASK_THRESHOLD}): {}x{} at {},{}",
        bounds.width, bounds.height, bounds.x, bounds.y
    );

    let filter = format!(
        "[0:v]crop={}:{}:{}:{}[skia];[1:v]crop={}:{}:{}:{}[canvas];[skia][canvas]ssim",
        bounds.width,
        bounds.height,
        bounds.x,
        bounds.y,
        bounds.width,
        bounds.height,
        bounds.x,
        bounds.y
    );

    let output = Command::new(&ffmpeg)
        .args(["-hide_banner", "-nostats", "-v", "info", "-y", "-i"])
        .arg(&input_a)
        .arg("-i")
        .arg(canvas_path)
        .args(["-filter_complex", &filter, "-f", "null", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| {
            "failed to spawn ffmpeg SSIM filter. Is ffmpeg installed? \
             Check vendor/ffmpeg/ or PATH."
        })?;

    if input_a != skia_path {
        let _ = fs::remove_file(&input_a);
    }

    if !output.status.success() {
        bail!(
            "ffmpeg SSIM failed (exit code {}): {}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_ssim_output(&stderr)
        .with_context(|| {
            let ssim_line = stderr.lines().find(|l| l.contains("All:")).unwrap_or("(not found)");
            format!("failed to parse SSIM output — line: {ssim_line}")
        })
}

/// Parses the ffmpeg SSIM stderr output for scores.
///
/// Supports both YUV and RGB channel names (ffmpeg names channels after the input
/// pixel format). Typical lines:
///   "SSIM Y:0.999  U:0.999  V:0.999  All:0.999 (9.999)"
///   "SSIM R:0.947  G:0.057  B:0.058  All:0.354 (1.898)"
fn parse_ssim_output(stderr: &str) -> Result<SsimResult> {
    let ssim_line = stderr
        .lines()
        .find(|line| line.contains("All:"))
        .ok_or_else(|| {
            // Print the final few lines for debugging
            let tail: String = stderr
                .lines()
                .rev()
                .take(5)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
            let line_count = stderr.lines().count();
            anyhow::anyhow!(
                "no SSIM 'All:' score found in ffmpeg output ({line_count} lines).\nLast 5 lines:\n{tail}"
            )
        })?;

    let extract = |prefix: &str| -> Result<f64> {
        let after = ssim_line
            .find(prefix)
            .ok_or_else(|| anyhow::anyhow!("missing {prefix} in SSIM line: {ssim_line}"))?;
        let rest = &ssim_line[after + prefix.len()..];
        let end = rest.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(rest.len());
        rest[..end]
            .parse::<f64>()
            .with_context(|| format!("failed to parse {prefix} score from '{rest}'"))
    };

    // Try YUV first, fall back to RGB
    let c1 = extract("Y:").or_else(|_| extract("R:"))?;
    let c2 = extract("U:").or_else(|_| extract("G:"))?;
    let c3 = extract("V:").or_else(|_| extract("B:"))?;
    let combined = extract("All:")?;

    Ok(SsimResult { combined, y: c1, u: c2, v: c3 })
}

/// Scans both RGBA byte buffers and returns the bounding rectangle of all
/// pixels where *either* image has alpha > `alpha_threshold`.
///
/// This union bounds is used by [`run_ssim`] to crop the SSIM comparison
/// to only the non-empty overlay area, preventing the transparent
/// background from inflating the score.
fn alpha_union_bounds(
    left: &[u8],
    right: &[u8],
    width: u32,
    height: u32,
    alpha_threshold: u8,
) -> Option<AlphaBounds> {
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    let mut found = false;

    for y in 0..height {
        for x in 0..width {
            let offset = ((y * width + x) as usize) * 4;
            let left_alpha = left.get(offset + 3).copied().unwrap_or(0);
            let right_alpha = right.get(offset + 3).copied().unwrap_or(0);
            if left_alpha > alpha_threshold || right_alpha > alpha_threshold {
                found = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }

    found.then_some(AlphaBounds {
        x: min_x,
        y: min_y,
        width: max_x - min_x + 1,
        height: max_y - min_y + 1,
    })
}

// ── Diff image generation ────────────────────────────────────────────────────

/// Pixel-level diff statistics produced by [`generate_diff_png`].
///
/// Fields are ordered from coarsest to finest granularity:
///
/// 1. **`mismatch_pixels`** — every pixel with any channel difference (incl. background).
/// 2. **`overlay_pixels`** / **`overlay_mismatch_pixels`** — restricted to the overlay mask.
/// 3. **`overlay_significant_mismatch_pixels`** — mismatches exceeding
///    `DIFF_CHANNEL_TOLERANCE`.
/// 4. **`edge_insensitive_mismatch_pixels`** / **`edge_compared_pixels`** —
///    significant mismatches on non-edge pixels (the "clean" metric).
/// 5. **`canvas_only_pixels`** / **`skia_only_pixels`** — content exclusive to one renderer.
///
/// See the module-level documentation for a full explanation of each category.
pub struct DiffStats {
    /// Total pixels with any channel difference (full frame, including transparent area).
    pub mismatch_pixels: u64,
    /// Pixels where either image has alpha > `ALPHA_MASK_THRESHOLD`.
    pub overlay_pixels: u64,
    /// Overlay pixels with any channel difference.
    pub overlay_mismatch_pixels: u64,
    /// Overlay pixels where Canvas has alpha > `ONLY_PIXEL_ALPHA_THRESHOLD` but Skia does not.
    pub canvas_only_pixels: u64,
    /// Overlay pixels where Skia has alpha > `ONLY_PIXEL_ALPHA_THRESHOLD` but Canvas does not.
    pub skia_only_pixels: u64,
    /// Overlay pixels whose max channel delta exceeds `DIFF_CHANNEL_TOLERANCE`.
    pub overlay_significant_mismatch_pixels: u64,
    /// Overlay non-edge pixels (used as denominator for edge-insensitive %).
    pub edge_compared_pixels: u64,
    /// Significant mismatches on non-edge pixels — the cleanest rendering-diff metric.
    pub edge_insensitive_mismatch_pixels: u64,
    /// Overlay pixels that fall on an alpha edge and are excluded from the clean count.
    pub edge_ignored_pixels: u64,
    /// Value of [`ALPHA_MASK_THRESHOLD`] used for this comparison.
    pub alpha_threshold: u8,
    /// Value of [`ONLY_PIXEL_ALPHA_THRESHOLD`] used for this comparison.
    pub only_pixel_alpha_threshold: u8,
    /// Value of [`DIFF_CHANNEL_TOLERANCE`] used for this comparison.
    pub channel_tolerance: u8,
    /// Value of [`EDGE_ALPHA_DELTA_THRESHOLD`] used for this comparison.
    pub edge_alpha_delta_threshold: u8,
    /// Value of [`EDGE_IGNORE_RADIUS`] used for this comparison.
    pub edge_ignore_radius: i32,
}

/// Decodes both PNGs to raw RGBA via ffmpeg, compares every pixel, writes a
/// visual diff PNG, and returns detailed [`DiffStats`].
///
/// # Visual encoding of the diff PNG
///
/// | Pixel status | Colour |
/// |---|---|
/// | **Significant mismatch** on a non-edge overlay pixel | Bright red `(255,0,0)` |
/// | All other pixels (matching, sub-threshold, or edge-ignored) | Dimmed copy of the Skia pixel `(R/2, G/2, B/2)` |
///
/// # Pixel classification logic (per pixel)
///
/// For every pixel index `i` in the RGBA byte arrays:
///
/// 1. Read the 4-byte chunks from Skia and Canvas images.
/// 2. Compute `max_delta` — the largest absolute difference across all four
///    channels (R/G/B/A).
/// 3. Determine overlay membership: a pixel is "overlay" if *either* image
///    has alpha > [`ALPHA_MASK_THRESHOLD`].
/// 4. Determine edge membership: a pixel on an alpha gradient is "edge"
///    (see [`build_alpha_edge_ignore_mask`]).
/// 5. Classify mismatches:
///    - `max_delta > 0` → **exact mismatch** (counted in `mismatch_pixels`)
///    - `max_delta > DIFF_CHANNEL_TOLERANCE` → **significant mismatch**
///    - Significant + overlay + non-edge → **edge-insensitive mismatch**
///      (the most useful "real diff" metric)
/// 6. Classify orphaned content:
///    - Skia alpha > `ONLY_PIXEL_ALPHA_THRESHOLD` && Canvas alpha ≤ that
///      → **Skia-only**
///    - Canvas alpha > threshold && Skia alpha ≤ → **Canvas-only**
///
/// # Requirements
///
/// - Both PNGs must have identical pixel dimensions (checked at the top).
/// - ffmpeg must be available (resolved via [`resolve_ffmpeg_binary`]).
pub fn generate_diff_png(
    skia_path: &Path,
    canvas_path: &Path,
    diff_path: &Path,
    repo_root: &Path,
) -> Result<DiffStats> {
    let ffmpeg =
        resolve_ffmpeg_binary(repo_root).context("failed to resolve ffmpeg for diff")?;
    let ffprobe = resolve_ffprobe(&ffmpeg);

    let (canvas_width, canvas_height) = probe_png_dimensions(&ffprobe, canvas_path)?;
    let (skia_width, skia_height) = probe_png_dimensions(&ffprobe, skia_path)?;

    // Scale Skia to match canvas dimensions if needed (same approach as run_ssim)
    let input_a = if skia_width != canvas_width || skia_height != canvas_height {
        let scaled = skia_path.with_extension("diff-scaled.png");
        let scale_filter = format!("scale={canvas_width}:{canvas_height}:flags=lanczos");
        let scale_output = Command::new(&ffmpeg)
            .args(["-v", "error", "-y", "-i"])
            .arg(skia_path)
            .args(["-vf", &scale_filter, &scaled.to_string_lossy()])
            .output()?;
        if !scale_output.status.success() {
            bail!("ffmpeg scale failed for diff: {}", String::from_utf8_lossy(&scale_output.stderr).trim());
        }
        scaled
    } else {
        skia_path.to_path_buf()
    };

    let (width, height, skia_bytes) = decode_png_to_rgba(&ffmpeg, &input_a)?;
    let (_, _, canvas_bytes) = decode_png_to_rgba(&ffmpeg, canvas_path)?;
    let edge_ignore_mask = build_alpha_edge_ignore_mask(
        &skia_bytes,
        &canvas_bytes,
        width,
        height,
        ALPHA_MASK_THRESHOLD,
        EDGE_ALPHA_DELTA_THRESHOLD,
        EDGE_IGNORE_RADIUS,
    );
    let mut diff_bytes = Vec::with_capacity(skia_bytes.len());
    let mut mismatch_count: u64 = 0;
    let mut overlay_pixels: u64 = 0;
    let mut overlay_mismatch_pixels: u64 = 0;
    let mut canvas_only_pixels: u64 = 0;
    let mut skia_only_pixels: u64 = 0;
    let mut overlay_significant_mismatch_pixels: u64 = 0;
    let mut edge_compared_pixels: u64 = 0;
    let mut edge_insensitive_mismatch_pixels: u64 = 0;
    let mut edge_ignored_pixels: u64 = 0;
    let alpha_threshold = ALPHA_MASK_THRESHOLD;
    let only_pixel_alpha_threshold = ONLY_PIXEL_ALPHA_THRESHOLD;
    let channel_tolerance = DIFF_CHANNEL_TOLERANCE;

    for (i, chunk) in skia_bytes.chunks_exact(4).enumerate() {
        let offset = i * 4;
        let canvas_chunk = &canvas_bytes[offset..offset + 4];
        let has_skia_alpha = chunk[3] > alpha_threshold;
        let has_canvas_alpha = canvas_chunk[3] > alpha_threshold;
        let has_skia_only_alpha = chunk[3] > only_pixel_alpha_threshold;
        let has_canvas_only_alpha = canvas_chunk[3] > only_pixel_alpha_threshold;
        let is_overlay_pixel = has_skia_alpha || has_canvas_alpha;
        let is_edge_ignored = is_overlay_pixel && edge_ignore_mask.get(i).copied().unwrap_or(false);
        let max_delta = chunk
            .iter()
            .zip(canvas_chunk.iter())
            .map(|(left, right)| left.abs_diff(*right))
            .max()
            .unwrap_or(0);
        let is_exact_mismatch = max_delta > 0;
        let is_significant_mismatch = max_delta > channel_tolerance;

        if is_exact_mismatch {
            mismatch_count += 1;
            if is_overlay_pixel {
                overlay_mismatch_pixels += 1;
            }
        }

        if is_significant_mismatch {
            if is_overlay_pixel {
                overlay_significant_mismatch_pixels += 1;
            }
            if is_overlay_pixel && !is_edge_ignored {
                edge_insensitive_mismatch_pixels += 1;
                diff_bytes.extend_from_slice(&[255, 0, 0, 255]);
            } else {
                diff_bytes.extend_from_slice(&[chunk[0] / 2, chunk[1] / 2, chunk[2] / 2, 255]);
            }
        } else {
            diff_bytes.extend_from_slice(&[chunk[0] / 2, chunk[1] / 2, chunk[2] / 2, 255]);
        }

        if is_overlay_pixel {
            if has_canvas_only_alpha && !has_skia_only_alpha {
                canvas_only_pixels += 1;
            } else if has_skia_only_alpha && !has_canvas_only_alpha {
                skia_only_pixels += 1;
            }
            overlay_pixels += 1;
            if is_edge_ignored {
                edge_ignored_pixels += 1;
            } else {
                edge_compared_pixels += 1;
            }
        }
    }

    if let Some(parent) = diff_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create diff dir {}", parent.display()))?;
    }

    let raw_path = diff_path.with_extension("rgba");
    fs::write(&raw_path, &diff_bytes)
        .with_context(|| format!("failed to write raw diff to {}", raw_path.display()))?;

    let encode_output = Command::new(&ffmpeg)
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
        .args(["-frames:v", "1"])
        .arg(diff_path)
        .output()
        .with_context(|| format!("failed to encode diff PNG {}", diff_path.display()))?;

    if !encode_output.status.success() {
        bail!(
            "ffmpeg failed to encode diff PNG {}: {}",
            diff_path.display(),
            String::from_utf8_lossy(&encode_output.stderr).trim()
        );
    }

    let _ = fs::remove_file(&raw_path);

    println!("  {mismatch_count} differing pixels");
    Ok(DiffStats {
        mismatch_pixels: mismatch_count,
        overlay_pixels,
        overlay_mismatch_pixels,
        canvas_only_pixels,
        skia_only_pixels,
        overlay_significant_mismatch_pixels,
        edge_compared_pixels,
        edge_insensitive_mismatch_pixels,
        edge_ignored_pixels,
        alpha_threshold,
        only_pixel_alpha_threshold,
        channel_tolerance,
        edge_alpha_delta_threshold: EDGE_ALPHA_DELTA_THRESHOLD,
        edge_ignore_radius: EDGE_IGNORE_RADIUS,
    })
}

/// Builds a boolean mask indicating which overlay pixels lie on an "alpha edge."
///
/// An alpha edge pixel is one whose alpha value differs from at least one
/// of its 8 neighbours by more than `edge_alpha_delta_threshold`, *and*
/// whose alpha is neither fully transparent (≤ `alpha_threshold`) nor fully
/// opaque (`== u8::MAX`).  This excludes uniformly translucent fills from
/// being flagged as edges.
///
/// Every seed pixel then expands into a radius of `edge_ignore_radius`
/// pixels around it.
///
/// Edge pixels are excluded from the "edge-insensitive" mismatch count so
/// that anti-aliased boundaries between transparent and opaque regions do
/// not pollute the clean comparison metric.
fn build_alpha_edge_ignore_mask(
    left: &[u8],
    right: &[u8],
    width: u32,
    height: u32,
    alpha_threshold: u8,
    edge_alpha_delta_threshold: u8,
    edge_ignore_radius: i32,
) -> Vec<bool> {
    let pixel_count = (width as usize).saturating_mul(height as usize);
    let mut seeds = vec![false; pixel_count];
    let mut mask = vec![false; pixel_count];

    for y in 0..height {
        for x in 0..width {
            let index = (y * width + x) as usize;
            // Seed from alpha transitions, not simply alpha < 255, so uniformly
            // translucent fills remain part of the strict comparison.
            if is_alpha_edge_pixel(
                left,
                width,
                height,
                x,
                y,
                alpha_threshold,
                edge_alpha_delta_threshold,
            )
                || is_alpha_edge_pixel(
                    right,
                    width,
                    height,
                    x,
                    y,
                    alpha_threshold,
                    edge_alpha_delta_threshold,
                )
            {
                seeds[index] = true;
            }
        }
    }

    for y in 0..height {
        for x in 0..width {
            let index = (y * width + x) as usize;
            if !seeds[index] {
                continue;
            }

            let min_y = (y as i32 - edge_ignore_radius).max(0) as u32;
            let max_y = (y as i32 + edge_ignore_radius).min(height as i32 - 1) as u32;
            let min_x = (x as i32 - edge_ignore_radius).max(0) as u32;
            let max_x = (x as i32 + edge_ignore_radius).min(width as i32 - 1) as u32;

            for mask_y in min_y..=max_y {
                for mask_x in min_x..=max_x {
                    mask[(mask_y * width + mask_x) as usize] = true;
                }
            }
        }
    }

    mask
}

/// Returns `true` if the pixel at `(x, y)` sits on a visible alpha transition.
///
/// A pixel is an "alpha edge" when:
/// - Its alpha is between `alpha_threshold + 1` and `254` (i.e. neither
///   transparent nor fully opaque).
/// - At least one of its 8 neighbours has an alpha differing by more than
///   `edge_alpha_delta_threshold`.
///
/// With `EDGE_ALPHA_DELTA_THRESHOLD = 0`, *any* gradient qualifies, so
/// only uniformly translucent or fully opaque/transparent regions are
/// excluded.
fn is_alpha_edge_pixel(
    bytes: &[u8],
    width: u32,
    height: u32,
    x: u32,
    y: u32,
    alpha_threshold: u8,
    edge_alpha_delta_threshold: u8,
) -> bool {
    let alpha = alpha_at(bytes, width, x, y);
    if alpha <= alpha_threshold || alpha == u8::MAX {
        return false;
    }

    let min_y = y.saturating_sub(1);
    let max_y = (y + 1).min(height.saturating_sub(1));
    let min_x = x.saturating_sub(1);
    let max_x = (x + 1).min(width.saturating_sub(1));

    for neighbor_y in min_y..=max_y {
        for neighbor_x in min_x..=max_x {
            if neighbor_x == x && neighbor_y == y {
                continue;
            }
            let neighbor_alpha = alpha_at(bytes, width, neighbor_x, neighbor_y);
            if alpha.abs_diff(neighbor_alpha) > edge_alpha_delta_threshold {
                return true;
            }
        }
    }

    false
}

fn alpha_at(bytes: &[u8], width: u32, x: u32, y: u32) -> u8 {
    let offset = ((y * width + x) as usize) * 4 + 3;
    bytes.get(offset).copied().unwrap_or(0)
}

/// Decodes a PNG to raw RGBA bytes via ffmpeg `rawvideo` pipe.
///
/// Steps:
/// 1. Probes dimensions from ffmpeg stderr.
/// 2. Decodes to raw RGBA via `ffmpeg -f rawvideo -pix_fmt rgba pipe:1`.
/// 3. Validates that the output byte count matches `width * height * 4`.
///
/// Returns `(width, height, rgba_bytes)`.
fn decode_png_to_rgba(ffmpeg: &Path, png_path: &Path) -> Result<(u32, u32, Vec<u8>)> {
    // Probe for dimensions
    let probe = Command::new(ffmpeg)
        .args(["-hide_banner", "-v", "info", "-i", png_path.to_str().unwrap(), "-f", "null", "-"])
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to probe PNG {}", png_path.display()))?;

    let stderr = String::from_utf8_lossy(&probe.stderr);

    // Parse "Stream ... 3840x2160 ..." from stderr
    let dims = parse_dimensions_from_ffmpeg_stderr(&stderr)
        .with_context(|| format!("could not parse dimensions for {}:\n{stderr}", png_path.display()))?;

    let (width, height) = dims;

    // Decode raw RGBA
    let output = Command::new(ffmpeg)
        .args(["-v", "error", "-i", png_path.to_str().unwrap()])
        .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
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
            "decoded PNG '{}' to {} bytes, expected {} for {width}x{height} RGBA",
            png_path.display(),
            output.stdout.len(),
            expected_len,
        );
    }

    Ok((width, height, output.stdout))
}

/// Parses video dimensions from ffmpeg stderr output.
///
/// Looks for a pattern like "Stream #0:0: Video: ... 3840x2160 ..."
/// The dimension token may have trailing punctuation (e.g. "3840x2160,").
fn parse_dimensions_from_ffmpeg_stderr(stderr: &str) -> Result<(u32, u32)> {
    for line in stderr.lines() {
        for word in line.split_whitespace() {
            let trimmed: &str = word.trim_end_matches(|c: char| !c.is_ascii_digit() && c != 'x');
            if trimmed.len() >= 3
                && trimmed.contains('x')
                && trimmed.chars().all(|c| c.is_ascii_digit() || c == 'x')
            {
                let parts: Vec<&str> = trimmed.split('x').collect();
                if parts.len() == 2 {
                    if let (Ok(w), Ok(h)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                        return Ok((w, h));
                    }
                }
            }
        }
    }
    bail!("no dimension pattern found in ffmpeg output")
}

// ── Temp directory cleanup ───────────────────────────────────────────────────

/// Thin wrapper around a temp directory path that removes it on Drop.
pub struct TempDir {
    path: Option<PathBuf>,
}

impl TempDir {
    #[allow(dead_code)]
    pub fn path(&self) -> &Path {
        self.path.as_ref().unwrap()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if let Some(ref path) = self.path {
            if path.exists() {
                let _ = fs::remove_dir_all(path);
            }
        }
    }
}
