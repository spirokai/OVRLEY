//! Helper functions for the canvas-parity end-to-end integration test.
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
    let activity_path = fixture_root.join("activity").join("gpx-parse-debug.json");
    let config_path = fixture_root.join("config").join("test-template-4k.json");

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

/// Builds an AppPaths targeting a test workspace inside `target/canvas-parity/`.
pub fn test_app_paths(git_root: &Path, case_name: &str) -> Result<AppPaths> {
    let case_root = git_root
        .join("target")
        .join("canvas-parity")
        .join(case_name);
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
            "globalDefaults": derive_global_defaults(config)
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
        "globalDefaults": derive_global_defaults(config),
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

/// Creates a Command that works on both Windows and Unix.
/// On Windows, wraps via `cmd.exe /c` to ensure PATH resolution for `.cmd` files.
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
            let _ = process.kill();
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
        println!("  canvas dimensions: {w}x{h}  bg: {b}");
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

/// SSIM comparison scores.
pub struct SsimResult {
    pub combined: f64,
    pub y: f64,
    pub u: f64,
    pub v: f64,
}

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

/// Runs ffmpeg SSIM filter comparing two PNG images.
///
/// Both inputs must have the same pixel dimensions.  The Skia PNG is first
/// scaled to match the canvas dimensions if needed.
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

    let output = Command::new(&ffmpeg)
        .args(["-hide_banner", "-nostats", "-v", "info", "-y", "-i"])
        .arg(&input_a)
        .arg("-i")
        .arg(canvas_path)
        .args(["-lavfi", "ssim", "-f", "null", "-"])
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

// ── Diff image generation ────────────────────────────────────────────────────

/// Decodes both PNGs to raw RGBA, compares pixel-by-pixel, and writes a diff PNG
/// where differing pixels are highlighted in red and matching pixels are dimmed.
///
/// Both images must already have the same pixel dimensions.
///
/// Returns the pixel mismatch count.
pub fn generate_diff_png(
    skia_path: &Path,
    canvas_path: &Path,
    diff_path: &Path,
    repo_root: &Path,
) -> Result<u64> {
    let ffmpeg =
        resolve_ffmpeg_binary(repo_root).context("failed to resolve ffmpeg for diff")?;
    let ffprobe = resolve_ffprobe(&ffmpeg);

    let (canvas_width, canvas_height) = probe_png_dimensions(&ffprobe, canvas_path)?;
    let (skia_width, skia_height) = probe_png_dimensions(&ffprobe, skia_path)?;

    if skia_width != canvas_width || skia_height != canvas_height {
        bail!(
            "dimension mismatch: Skia {skia_width}x{skia_height}, Canvas {canvas_width}x{canvas_height}"
        );
    }

    let (width, height, skia_bytes) = decode_png_to_rgba(&ffmpeg, skia_path)?;
    let (_, _, canvas_bytes) = decode_png_to_rgba(&ffmpeg, canvas_path)?;
    let mut diff_bytes = Vec::with_capacity(skia_bytes.len());
    let mut mismatch_count: u64 = 0;

    for (i, chunk) in skia_bytes.chunks_exact(4).enumerate() {
        let offset = i * 4;
        let canvas_chunk = &canvas_bytes[offset..offset + 4];

        if chunk[0] != canvas_chunk[0]
            || chunk[1] != canvas_chunk[1]
            || chunk[2] != canvas_chunk[2]
            || chunk[3] != canvas_chunk[3]
        {
            mismatch_count += 1;
            diff_bytes.extend_from_slice(&[255, 0, 0, 255]);
        } else {
            diff_bytes.extend_from_slice(&[chunk[0] / 2, chunk[1] / 2, chunk[2] / 2, 255]);
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
    Ok(mismatch_count)
}

/// Decodes a PNG to raw RGBA bytes via ffmpeg.
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
