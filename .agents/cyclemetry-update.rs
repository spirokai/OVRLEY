/// Scene orchestration: builds caches, drives the frame loop, pipes to FFmpeg.
///
/// Optimization layout:
/// - One-time: parse GPX, interpolate, smooth, build chart/font caches, pre-render base frame
/// - Per-frame: blit base + chart marker + dynamic text → pipe raw RGBA to FFmpeg stdin
/// - Frame loop: producer thread renders rayon parallel chunks and sends frames through a
/// bounded channel; consumer (main thread) drains to FFmpeg stdin concurrently so render
/// and encode overlap instead of running back-to-back.
use rayon::prelude::\*; #[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::render::activity::Activity;
use crate::render::frame::{SceneCache, render_frame};
use crate::render::template::Template;

pub struct RenderProgress {
pub frames_rendered: Arc<AtomicU64>,
pub total_frames: Arc<AtomicU64>,
pub cancelled: Arc<AtomicBool>,
}

impl RenderProgress {
pub fn new() -> Self {
RenderProgress {
frames_rendered: Arc::new(AtomicU64::new(0)),
total_frames: Arc::new(AtomicU64::new(0)),
cancelled: Arc::new(AtomicBool::new(false)),
}
}

    pub fn snapshot(&self) -> (u64, u64) {
        (
            self.frames_rendered.load(Ordering::Relaxed),
            self.total_frames.load(Ordering::Relaxed),
        )
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

}

/// Full video render pipeline.
pub fn render_video(
gpx_path: &str,
template: &Template,
output_path: &str,
fonts_dir: &str,
assets_dirs: &[&str],
progress: &RenderProgress,
) -> Result<(), String> {
log::info!(
"render_video: start gpx={gpx_path}, output={output_path}, assets_dirs={assets_dirs:?}"
);

    // --- Load and prepare activity data ---
    log::info!("render_video: loading activity");
    let mut activity = Activity::from_file(gpx_path)?;
    if progress.cancelled.load(Ordering::Relaxed) {
        return Err("Render cancelled".to_string());
    }
    log::info!(
        "render_video: activity loaded ({} samples)",
        activity.data_len()
    );
    activity.interpolate(template.scene.fps);
    if progress.cancelled.load(Ordering::Relaxed) {
        return Err("Render cancelled".to_string());
    }
    log::info!(
        "render_video: activity interpolated to {}fps ({} frames)",
        template.scene.fps,
        activity.data_len()
    );

    // scene.start/end are in seconds; convert to frame indices before trimming.
    let fps = template.scene.fps as usize;
    let start_frame = template.scene.start.unwrap_or(0) * fps;
    if let Some(end_secs) = template.scene.end {
        let end_frame = (end_secs * fps).min(activity.data_len());
        if end_frame > start_frame {
            activity.trim(start_frame, end_frame)?;
        }
    } else if start_frame > 0 {
        activity.trim(start_frame, activity.data_len())?;
    }

    let total_frames = activity.data_len();
    progress
        .total_frames
        .store(total_frames as u64, Ordering::Relaxed);

    // --- Build caches ---
    log::info!("render_video: building scene cache");
    let cache = SceneCache::build(&activity, template, fonts_dir, assets_dirs)
        .map_err(|e| format!("Cache build failed: {e}"))?;
    if progress.cancelled.load(Ordering::Relaxed) {
        return Err("Render cancelled".to_string());
    }
    log::info!("render_video: scene cache built");

    // --- Crop to the union of all visible elements ---
    // The overlay is mostly transparent; rasterising + piping + encoding the
    // full 4K frame is overhead. compute_crop_rect returns None when cropping
    // wouldn't pay off, in which case we keep the full-frame path.
    log::info!("render_video: computing crop rect");
    let crop = crate::render::frame::compute_crop_rect(
        &activity,
        template,
        fonts_dir,
        Some(&progress.cancelled),
    );
    if progress.cancelled.load(Ordering::Relaxed) {
        return Err("Render cancelled".to_string());
    }
    log::info!("render_video: crop rect computed");
    let (w, h) = match &crop {
        Some(c) => {
            log::info!(
                "render_video: cropping to {}x{} at offset ({},{}) — {:.0}% of {}x{} frame",
                c.w,
                c.h,
                c.x,
                c.y,
                100.0 * (c.w as f32 * c.h as f32) / (cache.width as f32 * cache.height as f32),
                cache.width,
                cache.height
            );
            (c.w, c.h)
        }
        // Ensure even dimensions (codec requirement)
        None => ((cache.width + 1) & !1, (cache.height + 1) & !1),
    };

    // Snapshot the full canvas before `cache` is moved into the producer
    // closure; needed for the placement sidecar after the render scope.
    let (full_w, full_h) = (cache.width, cache.height);

    // --- Spawn FFmpeg ---
    let ffmpeg_bin = resolve_ffmpeg();
    log::info!("render_video: spawning FFmpeg from {ffmpeg_bin}");
    let mut cmd = Command::new(&ffmpeg_bin);
    cmd.args([
        "-loglevel",
        "warning",
        "-f",
        "rawvideo",
        // Skia renders BGRA8888 natively; feeding bgra means FFmpeg does
        // zero per-frame swscale conversion before the encoder.
        "-pix_fmt",
        "bgra",
        "-s",
        &format!("{w}x{h}"),
        "-r",
        &template.scene.fps.to_string(),
        "-i",
        "-",
        "-c:v",
        ffmpeg_codec(),
        "-profile:v",
        "4444",
        "-y",
        output_path,
    ])
    .stdin(Stdio::piped())
    .stdout(Stdio::null())
    .stderr(Stdio::piped());
    // Suppress the console window Windows opens for console-subsystem
    // executables when spawned from a GUI process (CREATE_NO_WINDOW).
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    let mut ffmpeg = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn FFmpeg ({ffmpeg_bin}): {e}"))?;

    let mut stdin = ffmpeg.stdin.take().unwrap();
    log::info!("render_video: FFmpeg spawned (PID {})", ffmpeg.id());

    // Drain stderr in a background thread to prevent pipe-buffer deadlock during
    // finalization: FFmpeg flushes encoder + writes moov atom and may emit >65 KB
    // of progress/stats to stderr. If we only read it after ffmpeg.wait(), the pipe
    // fills, FFmpeg blocks, and wait() deadlocks.
    // Log each stderr line as it arrives so finalization issues are visible in real time.
    // Collecting into Vec<String> lets us surface the last lines on failure.
    let stderr_drainer = ffmpeg.stderr.take().map(|stderr| {
        std::thread::spawn(move || -> Vec<String> {
            use std::io::BufRead;
            let mut lines = Vec::new();
            for line in std::io::BufReader::new(stderr)
                .lines()
                .map_while(Result::ok)
            {
                log::warn!("ffmpeg: {line}");
                lines.push(line);
            }
            lines
        })
    });

    // --- Pipelined frame loop ---
    // Layout: producer renders rayon chunks → bounded channel → consumer writes to FFmpeg stdin.
    // Consumer runs in its own thread so the main scope thread can act as a cancel watchdog:
    // when cancelled, the watchdog kills FFmpeg, which breaks the stdin pipe and causes the
    // consumer's write_all to return EPIPE immediately — no more stuck write_all on disk I/O.
    let num_threads = rayon::current_num_threads();
    let chunk_size = (num_threads * 2).max(8);
    log::info!(
        "render_video: {total_frames} frames, {num_threads} rayon threads, chunk_size={chunk_size}"
    );

    let pipeline_start = Instant::now();

    // thread::scope guarantees all spawned threads finish before the scope exits,
    // allowing safe borrows of `cache`, `activity`, `template`, and `progress`.
    let scope_result: Result<(Duration, Duration), String> = std::thread::scope(|s| {
        // tx/rx are created INSIDE the scope so the producer can take ownership of tx.
        // With `move ||`, when producer exits it drops tx, which disconnects the channel
        // and lets consumer's rx.recv() return Err — without this, tx would live in the
        // outer function and rx.recv() would block forever after all frames are queued.
        let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(chunk_size);

        let cancelled = &progress.cancelled;

        let producer = s.spawn(move || -> (bool, Duration) {
            let mut sent = 0usize;
            let mut total_render = Duration::ZERO;
            while sent < total_frames {
                if cancelled.load(Ordering::Relaxed) {
                    return (true, total_render);
                }
                let chunk_end = (sent + chunk_size).min(total_frames);
                let t0 = Instant::now();
                let frames: Vec<Vec<u8>> = (sent..chunk_end)
                    .into_par_iter()
                    .map(|i| render_frame(i, &cache, &activity, template, crop.as_ref()))
                    .collect();
                let render_elapsed = t0.elapsed();
                total_render += render_elapsed;
                log::debug!(
                    "chunk {sent}..{chunk_end}: render={:.0}ms",
                    render_elapsed.as_secs_f64() * 1000.0,
                );
                for frame in frames {
                    if tx.send(frame).is_err() {
                        return (false, total_render); // consumer stopped (error or cancel)
                    }
                    sent += 1;
                }
            }
            (false, total_render)
        });

        // Consumer thread: drain frames from the channel and write to FFmpeg stdin.
        // stdin is moved in so the pipe closes (EOF) when this thread exits.
        //
        // Blocking write_all: the kernel wakes the writer the instant pipe
        // space frees, so feed throughput tracks FFmpeg's encode rate exactly.
        // (The previous O_NONBLOCK + 1ms-sleep poll fed at best one pipe buffer
        // per scheduler quantum — an order-of-magnitude throughput tax that
        // dominated wall time once the frames themselves were cheap.)
        // Cancellation stays prompt: the watchdog below kills FFmpeg on cancel,
        // which breaks the pipe so write_all returns EPIPE immediately; we also
        // check the cancel flag once per frame (≤ one frame of latency).
        let cancelled_consumer = Arc::clone(&progress.cancelled);
        let frames_counter = Arc::clone(&progress.frames_rendered);
        let consumer = s.spawn(move || -> (Option<String>, Duration) {
            use std::io::Write as _;

            let drain_start = Instant::now();
            let mut frame_idx = 0usize;
            let mut consumer_err: Option<String> = None;
            let mut last_write_log = Instant::now();

            while let Ok(data) = rx.recv() {
                if cancelled_consumer.load(Ordering::Relaxed) {
                    break;
                }
                if last_write_log.elapsed() >= Duration::from_secs(5) {
                    log::info!(
                        "consumer: writing frame {frame_idx}/{total_frames} \
                         ({:.0}s elapsed)",
                        drain_start.elapsed().as_secs_f64()
                    );
                    last_write_log = Instant::now();
                }
                if let Err(e) = stdin.write_all(&data) {
                    // EPIPE = FFmpeg exited (cancel watchdog kill, or an encode
                    // error surfaced via stderr); silent for the cancel case.
                    let msg = e.to_string();
                    if !msg.contains("Broken pipe") && !msg.contains("os error 32") {
                        consumer_err = Some(format!("FFmpeg pipe error at frame {frame_idx}: {e}"));
                    }
                    break;
                }
                frame_idx += 1;
                frames_counter.store(frame_idx as u64, Ordering::Relaxed);
            }
            let total_drain = drain_start.elapsed();
            log::info!(
                "consumer: done — {frame_idx} frames in {:.1}s",
                total_drain.as_secs_f64()
            );
            (consumer_err, total_drain)
        });

        // Cancel watchdog: poll the cancel flag every 10ms.
        // When cancelled, kill FFmpeg — this breaks the stdin pipe so the consumer's
        // write_all returns EPIPE immediately instead of blocking until disk I/O drains.
        loop {
            if cancelled.load(Ordering::Relaxed) {
                log::info!("render_video: cancel detected in watchdog — killing FFmpeg");
                let _ = ffmpeg.kill();
                break;
            }
            if consumer.is_finished() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        let (consumer_err, total_drain) = consumer.join().expect("render consumer panicked");
        let (was_cancelled, total_render) = producer.join().expect("render producer panicked");

        if let Some(e) = consumer_err {
            return Err(e);
        }
        if was_cancelled || cancelled.load(Ordering::Relaxed) {
            return Err("Render cancelled".to_string());
        }
        Ok((total_render, total_drain))
    });

    log::info!("render_video: scope exited — stdin EOF sent, awaiting FFmpeg");

    let (total_render_time, total_drain_time) = match scope_result {
        Ok(t) => t,
        Err(e) => {
            let _ = ffmpeg.kill();
            let _ = ffmpeg.wait();
            // Collect FFmpeg stderr now — it explains why the scope failed (e.g. startup crash).
            let stderr_lines = stderr_drainer
                .and_then(|t| t.join().ok())
                .unwrap_or_default();
            if std::path::Path::new(output_path).exists() {
                let _ = std::fs::remove_file(output_path);
            }
            return if stderr_lines.is_empty() {
                Err(e)
            } else {
                Err(format!("{e}\nFFmpeg: {}", stderr_lines.join("\n")))
            };
        }
    };

    let total_elapsed = pipeline_start.elapsed();
    let fps_actual = total_frames as f64 / total_elapsed.as_secs_f64();
    log::info!(
        "render_video done: {total_frames} frames in {:.1}s @ {fps_actual:.1} fps  \
         | render {:.1}s ({:.1}ms/frame)  \
         | drain(encode) {:.1}s ({:.1}ms/frame)  \
         | overlap savings {:.1}s",
        total_elapsed.as_secs_f64(),
        total_render_time.as_secs_f64(),
        total_render_time.as_secs_f64() * 1000.0 / total_frames as f64,
        total_drain_time.as_secs_f64(),
        total_drain_time.as_secs_f64() * 1000.0 / total_frames as f64,
        (total_render_time + total_drain_time)
            .saturating_sub(total_elapsed)
            .as_secs_f64(),
    );

    // Poll FFmpeg exit while respecting mid-finalization cancel requests.
    // ffmpeg.wait() would block until FFmpeg exits with no cancel escape hatch;
    // try_wait() lets us interleave a cancel check every 50 ms.
    let finalize_start = Instant::now();
    log::info!("render_video: finalization started (FFmpeg mux/write)");
    let mut last_heartbeat = Instant::now();
    let status = loop {
        if progress.cancelled.load(Ordering::Relaxed) {
            log::info!("render_video: cancelled during finalization — killing FFmpeg");
            let _ = ffmpeg.kill();
            let _ = ffmpeg.wait();
            let _ = stderr_drainer.map(|t| t.join());
            if std::path::Path::new(output_path).exists() {
                let _ = std::fs::remove_file(output_path);
            }
            return Err("Render cancelled".to_string());
        }
        if last_heartbeat.elapsed() >= Duration::from_secs(5) {
            log::info!(
                "render_video: still finalizing… {:.1}s elapsed",
                finalize_start.elapsed().as_secs_f64()
            );
            last_heartbeat = Instant::now();
        }
        match ffmpeg
            .try_wait()
            .map_err(|e| format!("FFmpeg wait error: {e}"))?
        {
            Some(status) => break status,
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    };
    log::info!(
        "render_video: finalization done in {:.1}s",
        finalize_start.elapsed().as_secs_f64()
    );

    let ffmpeg_stderr_lines = stderr_drainer
        .and_then(|t| t.join().ok())
        .unwrap_or_default();

    if !status.success() {
        let ffmpeg_stderr = ffmpeg_stderr_lines.join("\n");
        if std::path::Path::new(output_path).exists() {
            let _ = std::fs::remove_file(output_path);
        }
        return Err(if ffmpeg_stderr.is_empty() {
            format!("FFmpeg failed ({})", status)
        } else {
            format!("FFmpeg failed ({}): {ffmpeg_stderr}", status)
        });
    }

    let frames_written = progress.frames_rendered.load(Ordering::Relaxed);
    if frames_written == 0 {
        if std::path::Path::new(output_path).exists() {
            let _ = std::fs::remove_file(output_path);
        }
        return Err(format!(
            "Render produced no output ({total_frames} frames expected). \
             FFmpeg may be missing — install it via Homebrew: brew install ffmpeg"
        ));
    }

    // Cropped output is smaller than the footage canvas — record exactly where
    // it must be positioned so it isn't guessed-at in the editor later.
    if let Some(c) = crop {
        write_placement_sidecar(output_path, c, full_w, full_h);
    }

    Ok(())

}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Write a human-readable placement file next to the cropped `.mov`. The crop
/// trims the transparent margin, so the video is smaller than the footage
/// canvas and must be offset when composited. The file travels with the video
/// (it's keyed off the output path) so the offset is available whenever the
/// editor project is opened.
fn write_placement_sidecar(
output_path: &str,
crop: crate::render::frame::CropRect,
full_w: u32,
full_h: u32,
) {
let (x, y, w, h) = (crop.x, crop.y, crop.w as i32, crop.h as i32);
// Final Cut "Transform → Position" is measured from the project centre,
// +X right and +Y up. Screen Y grows downward, hence the inversion.
let fcp_x = (x + w / 2) - (full_w as i32 / 2);
let fcp_y = (full_h as i32 / 2) - (y + h / 2);

    let name = std::path::Path::new(output_path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let body = format!(
        "Cyclemetry overlay placement\n\
         ============================\n\
         Overlay video : {name}\n\
         Overlay size  : {w} x {h} px\n\
         Full canvas   : {full_w} x {full_h} px  (match your footage resolution)\n\
         \n\
         The overlay was cropped to the region that actually contains pixels,\n\
         so it is smaller than the canvas. Position it over your footage as:\n\
         \n\
         Top-left placement (unambiguous):\n\
         \tx = {x} px , y = {y} px   from the top-left of the {full_w} x {full_h} canvas\n\
         \n\
         Final Cut Pro — select the overlay clip, open Transform, set Position\n\
         (assumes the FCP project/timeline is {full_w} x {full_h}):\n\
         \tX = {fcp_x}\n\
         \tY = {fcp_y}\n",
    );

    let sidecar = format!("{output_path}.placement.txt");
    match std::fs::write(&sidecar, body) {
        Ok(()) => log::info!("render_video: wrote placement info to {sidecar}"),
        Err(e) => log::warn!("failed to write placement sidecar {sidecar}: {e}"),
    }

}

fn resolve_ffmpeg() -> String {
// On Windows the binary is ffmpeg.exe; on Unix it's ffmpeg.
let bin_name = if cfg!(windows) {
"ffmpeg.exe"
} else {
"ffmpeg"
};

    if let Ok(exe) = std::env::current_exe() {
        // Dev: {repo}/resources/ffmpeg[.exe] (skip zero-byte build stub)
        if let Some(root) = exe.ancestors().find(|p| p.join("resources").exists()) {
            let dev = root.join("resources").join(bin_name);
            if std::fs::metadata(&dev)
                .map(|m| m.len() > 0)
                .unwrap_or(false)
            {
                ensure_executable(&dev);
                return dev.to_string_lossy().to_string();
            }
        }
        // Production macOS .app: exe is Contents/MacOS/cyclemetry,
        // Tauri resources land at Contents/Resources/
        if let Some(contents) = exe.parent().and_then(|p| p.parent()) {
            let bundled = contents.join("Resources").join(bin_name);
            if std::fs::metadata(&bundled)
                .map(|m| m.len() > 0)
                .unwrap_or(false)
            {
                ensure_executable(&bundled);
                return bundled.to_string_lossy().to_string();
            }
        }
        // Production Windows: Tauri NSIS bundles resources next to the exe
        // (destination ".") or inside a resources/ subdirectory.  Check both.
        #[cfg(windows)]
        if let Some(exe_dir) = exe.parent() {
            for candidate in &[
                exe_dir.join(bin_name),
                exe_dir.join("resources").join(bin_name),
            ] {
                if std::fs::metadata(candidate)
                    .map(|m| m.len() > 0)
                    .unwrap_or(false)
                {
                    log::info!("resolve_ffmpeg: found Windows bundled ffmpeg at {candidate:?}");
                    return candidate.to_string_lossy().to_string();
                }
            }
        }
        // Production Linux: depending on the bundle target, resources may sit
        // next to the executable or under a sibling lib/resources directory.
        #[cfg(target_os = "linux")]
        if let Some(exe_dir) = exe.parent() {
            let app_name = std::env::current_exe()
                .ok()
                .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
                .unwrap_or_else(|| "cyclemetry".to_string());
            for candidate in &[
                exe_dir.join(bin_name),
                exe_dir.join("resources").join(bin_name),
                exe_dir
                    .parent()
                    .unwrap_or(exe_dir)
                    .join("lib")
                    .join(&app_name)
                    .join(bin_name),
                exe_dir
                    .parent()
                    .unwrap_or(exe_dir)
                    .join("lib")
                    .join(&app_name)
                    .join("resources")
                    .join(bin_name),
            ] {
                if std::fs::metadata(candidate)
                    .map(|m| m.len() > 0)
                    .unwrap_or(false)
                {
                    ensure_executable(candidate);
                    log::info!("resolve_ffmpeg: found Linux bundled ffmpeg at {candidate:?}");
                    return candidate.to_string_lossy().to_string();
                }
            }
        }
    }
    // Homebrew on macOS doesn't modify the system PATH visible to .app bundles.
    // Check known install locations before falling back to PATH lookup.
    for candidate in &[
        "/opt/homebrew/bin/ffmpeg", // Apple Silicon Homebrew
        "/usr/local/bin/ffmpeg",    // Intel Homebrew
    ] {
        let p = std::path::Path::new(candidate);
        if std::fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false) {
            log::info!("resolve_ffmpeg: using Homebrew ffmpeg at {candidate}");
            return candidate.to_string();
        }
    }

    log::warn!("resolve_ffmpeg: no ffmpeg found — falling back to PATH lookup");
    // On Windows, just "ffmpeg" works if it's on PATH (cmd resolves .exe automatically)
    "ffmpeg".to_string()

}

/// Select a ProRes encoder appropriate for the current platform.
/// - macOS: prores_videotoolbox — hardware-accelerated via Apple VideoToolbox
/// - Windows/Linux: prores_ks — CPU software ProRes, widely available in ffmpeg builds
fn ffmpeg_codec() -> &'static str {
if cfg!(target_os = "macos") {
"prores_videotoolbox"
} else {
"prores_ks"
}
}

fn ensure*executable(path: &std::path::Path) { #[cfg(unix)]
{
use std::os::unix::fs::PermissionsExt;
if let Ok(meta) = std::fs::metadata(path) {
let mut perms = meta.permissions();
if perms.mode() & 0o111 == 0 {
perms.set_mode(perms.mode() | 0o755);
let * = std::fs::set_permissions(path, perms);
}
}
}
}

/// Per-frame Skia rendering — draws one video frame to a raw RGBA byte buffer.
use resvg;
use serde::Serialize;
use skia_safe::{
    Canvas, Color, Font, FontMgr, FontStyle, ISize, ImageInfo, Paint, RRect, Rect, Typeface,
};
use std::collections::HashMap;

use crate::render::activity::{ATTR_DISTANCE, Activity};
use crate::render::chart::ChartCache;
use crate::render::color::{hex_with_opacity, lerp_gradient};
use crate::render::template::{
    Element, GaugeConfig, ImageConfig, LabelConfig, MeterConfig, PlotConfig, RectConfig,
    SceneConfig, Template, ValueConfig,
};
use crate::render::units;

const ITALIC_SKEW_X: f32 = -0.25;

/// Pixel-perfect bounding box for a single overlay element in overlay coordinates.
#[derive(Debug, Clone, Serialize)]
pub struct ElementBounds {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Everything an element needs to measure or draw itself for a given frame.
/// Shared across all elements so a new graphic just reads what it needs.
pub struct ElementCtx<'a> {
    pub activity: &'a Activity,
    pub scene: &'a SceneConfig,
    pub typefaces: &'a HashMap<String, Typeface>,
    /// One ChartCache per plot element, keyed by element id.
    pub charts: &'a HashMap<String, ChartCache>,
    pub fonts_dir: &'a str,
    /// Pre-decoded images keyed by element id.
    pub images: &'a HashMap<String, skia_safe::Image>,
}

/// One overlay graphic. Every element family implements this; rendering,
/// measuring, cropping, and font/cache prep all dispatch through it with
/// zero per-type branching at the call sites.
pub trait OverlayElement {
    /// Visual bounding box for this frame, or `None` if nothing is drawn.
    fn measure(&self, ctx: &ElementCtx, frame_idx: usize) -> Option<ElementBounds>;

    /// Draw onto the canvas for this frame.
    fn draw(&self, canvas: &Canvas, ctx: &ElementCtx, frame_idx: usize);

    /// `true` if the bounding box varies frame-to-frame (e.g. changing text
    /// width). Static elements are measured once for the crop computation.
    fn is_dynamic(&self) -> bool {
        false
    }

    /// Font filenames this element needs preloaded into the typeface cache.
    fn fonts(&self, _scene: &SceneConfig) -> Vec<String> {
        Vec::new()
    }

    /// Prebuilt per-element chart cache, if any (plots only).
    fn build_chart(&self, _activity: &Activity, _fonts_dir: &str) -> Option<ChartCache> {
        None
    }

    /// Extent used for crop union: `(x0, y0, x1, y1)`. Defaults to the
    /// measured box; plots override to circumscribe their rotation.
    fn crop_extent(&self, ctx: &ElementCtx, frame_idx: usize) -> Option<(f32, f32, f32, f32)> {
        self.measure(ctx, frame_idx)
            .map(|b| (b.x, b.y, b.x + b.w, b.y + b.h))
    }
}

impl Element {
    pub fn as_overlay(&self) -> &dyn OverlayElement {
        match self {
            Element::Label(c) => c,
            Element::Value(c) => c,
            Element::Plot(c) => c,
            Element::Meter(c) => c,
            Element::Gauge(c) => c,
            Element::Rect(c) => c,
            Element::Image(c) => c,
        }
    }
}

// ─── Label ─────────────────────────────────────────────────────────────────

impl OverlayElement for LabelConfig {
    fn fonts(&self, scene: &SceneConfig) -> Vec<String> {
        vec![
            self.font
                .as_deref()
                .or(scene.font.as_deref())
                .unwrap_or("Arial.ttf")
                .to_string(),
        ]
    }

    fn measure(&self, ctx: &ElementCtx, _frame_idx: usize) -> Option<ElementBounds> {
        let font_name = self
            .font
            .as_deref()
            .or(ctx.scene.font.as_deref())
            .unwrap_or("Arial.ttf");
        let font_size = self.font_size.or(ctx.scene.font_size).unwrap_or(32.0);
        let italic = self.italic.unwrap_or(false);
        let font = ctx
            .typefaces
            .get(font_name)
            .map(|tf| font_from_typeface(tf.clone(), font_size, italic))
            .or_else(|| load_font(font_name, font_size, ctx.fonts_dir, italic))?;
        let (text_w, rect) = font.measure_str(&self.text, None);
        let draw_x = align_x(self.x, text_w, self.text_align.as_deref());
        Some(ElementBounds {
            id: self.id.clone(),
            x: draw_x + rect.left,
            y: self.y + rect.top,
            w: rect.width(),
            h: rect.height(),
        })
    }

    fn draw(&self, canvas: &Canvas, ctx: &ElementCtx, _frame_idx: usize) {
        let font_name = self
            .font
            .as_deref()
            .or(ctx.scene.font.as_deref())
            .unwrap_or("Arial.ttf");
        let font_size = self.font_size.or(ctx.scene.font_size).unwrap_or(32.0);
        let italic = self.italic.unwrap_or(false);
        let color_str = self.color.as_deref().unwrap_or("#ffffff");
        let (r, g, b, a) = hex_with_opacity(color_str, self.opacity);
        let color = Color::from_argb(a, r, g, b);

        let font = ctx
            .typefaces
            .get(font_name)
            .map(|tf| font_from_typeface(tf.clone(), font_size, italic))
            .or_else(|| load_font(font_name, font_size, ctx.fonts_dir, italic));
        if let Some(font) = font {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(color);
            let text_w = font.measure_str(&self.text, Some(&paint)).0;
            let draw_x = align_x(self.x, text_w, self.text_align.as_deref());
            canvas.draw_str(&self.text, (draw_x, self.y), &font, &paint);
        }
    }
}

// ─── Value ─────────────────────────────────────────────────────────────────

impl ValueConfig {
    fn sample(&self, activity: &Activity, frame_idx: usize) -> f64 {
        if !activity.valid_attributes.contains(&self.value) {
            return 0.0;
        }
        if self.value == ATTR_DISTANCE {
            let target_m = self
                .distance_target
                .map(|t| units::distance_target_to_m(t, self.unit.as_deref()));
            activity.get_distance(self.distance_reference.as_deref(), target_m, frame_idx)
        } else {
            activity.get_scalar(&self.value, frame_idx)
        }
    }
}

impl OverlayElement for ValueConfig {
    fn is_dynamic(&self) -> bool {
        true
    }

    fn fonts(&self, scene: &SceneConfig) -> Vec<String> {
        vec![
            self.font
                .as_deref()
                .or(scene.font.as_deref())
                .unwrap_or("Arial.ttf")
                .to_string(),
        ]
    }

    fn measure(&self, ctx: &ElementCtx, frame_idx: usize) -> Option<ElementBounds> {
        let raw = self.sample(ctx.activity, frame_idx);
        let text = format_value(raw, self);
        let font_name = self
            .font
            .as_deref()
            .or(ctx.scene.font.as_deref())
            .unwrap_or("Arial.ttf");
        let font_size = self.font_size.or(ctx.scene.font_size).unwrap_or(32.0);
        let italic = self.italic.unwrap_or(false);
        let font = ctx
            .typefaces
            .get(font_name)
            .map(|tf| font_from_typeface(tf.clone(), font_size, italic))
            .or_else(|| load_font(font_name, font_size, ctx.fonts_dir, italic))?;
        let (text_w, rect) = font.measure_str(&text, None);
        let draw_x = align_x(self.x, text_w, self.text_align.as_deref());
        Some(ElementBounds {
            id: self.id.clone(),
            x: draw_x + rect.left,
            y: self.y + rect.top,
            w: rect.width(),
            h: rect.height(),
        })
    }

    fn draw(&self, canvas: &Canvas, ctx: &ElementCtx, frame_idx: usize) {
        if !ctx.activity.valid_attributes.contains(&self.value) {
            return;
        }
        let raw = self.sample(ctx.activity, frame_idx);
        let display = format_value(raw, self);

        let font_name = self
            .font
            .as_deref()
            .or(ctx.scene.font.as_deref())
            .unwrap_or("Arial.ttf");
        let font_size = self.font_size.or(ctx.scene.font_size).unwrap_or(32.0);
        let italic = self.italic.unwrap_or(false);
        let color_str = self.color.as_deref().unwrap_or("#ffffff");
        let (r, g, b, a) = hex_with_opacity(color_str, self.opacity);
        let color = Color::from_argb(a, r, g, b);

        if let Some(tf) = ctx.typefaces.get(font_name) {
            let font = font_from_typeface(tf.clone(), font_size, italic);
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(color);
            let text_w = font.measure_str(&display, Some(&paint)).0;
            let draw_x = align_x(self.x, text_w, self.text_align.as_deref());
            canvas.draw_str(&display, (draw_x, self.y), &font, &paint);
        }
    }
}

// ─── Plot ──────────────────────────────────────────────────────────────────

impl OverlayElement for PlotConfig {
    fn build_chart(&self, activity: &Activity, fonts_dir: &str) -> Option<ChartCache> {
        let (x_data, y_data) = activity.plot_data(&self.value);
        ChartCache::build(self, x_data, y_data, fonts_dir)
    }

    fn measure(&self, _ctx: &ElementCtx, _frame_idx: usize) -> Option<ElementBounds> {
        Some(ElementBounds {
            id: self.id.clone(),
            x: self.x as f32,
            y: self.y as f32,
            w: self.width as f32,
            h: self.height as f32,
        })
    }

    fn crop_extent(&self, _ctx: &ElementCtx, _frame_idx: usize) -> Option<(f32, f32, f32, f32)> {
        // Rotated plots are bounded by their circumscribed circle.
        let rot = self.rotation.unwrap_or(0.0);
        if rot != 0.0 {
            let cx = self.x as f32 + self.width as f32 / 2.0;
            let cy = self.y as f32 + self.height as f32 / 2.0;
            let r = ((self.width as f32).powi(2) + (self.height as f32).powi(2)).sqrt() / 2.0;
            Some((cx - r, cy - r, cx + r, cy + r))
        } else {
            Some((
                self.x as f32,
                self.y as f32,
                self.x as f32 + self.width as f32,
                self.y as f32 + self.height as f32,
            ))
        }
    }

    fn draw(&self, canvas: &Canvas, ctx: &ElementCtx, frame_idx: usize) {
        let Some(chart) = ctx.charts.get(&self.id) else {
            return;
        };
        let rotation = self.rotation.unwrap_or(0.0);
        if rotation != 0.0 {
            let cx = self.x as f32 + self.width as f32 / 2.0;
            let cy = self.y as f32 + self.height as f32 / 2.0;
            canvas.save();
            canvas.rotate(rotation, Some(skia_safe::Point::new(cx, cy)));
        }
        if self.has_position_markers() {
            chart.draw_on_canvas(canvas, frame_idx);
        } else {
            canvas.draw_image(
                &chart.background,
                skia_safe::Point::new(chart.x_offset as f32, chart.y_offset as f32),
                None,
            );
        }
        if rotation != 0.0 {
            canvas.restore();
        }
    }
}

// ─── Meter ─────────────────────────────────────────────────────────────────

impl MeterConfig {
    /// Current fill fraction in [0, 1] for this frame.
    fn fraction(&self, activity: &Activity, frame_idx: usize) -> f32 {
        if !activity.valid_attributes.contains(&self.value) {
            return 0.0;
        }
        let raw = activity.get_scalar(&self.value, frame_idx);
        let (conv, _) = units::resolve(&self.value, self.unit.as_deref());
        let v = conv.apply(raw);
        let span = self.max - self.min;
        if span.abs() < f64::EPSILON {
            return 0.0;
        }
        (((v - self.min) / span) as f32).clamp(0.0, 1.0)
    }

    fn rect(&self) -> Rect {
        Rect::from_xywh(
            self.x as f32,
            self.y as f32,
            self.width as f32,
            self.height as f32,
        )
    }

    /// Sub-rect of the track that is filled, given the direction.
    fn fill_rect(&self, frac: f32) -> Rect {
        let (x, y, w, h) = (
            self.x as f32,
            self.y as f32,
            self.width as f32,
            self.height as f32,
        );
        match self.direction.as_deref().unwrap_or("up") {
            "down" => Rect::from_xywh(x, y, w, h * frac),
            "left" => Rect::from_xywh(x + w * (1.0 - frac), y, w * frac, h),
            "right" => Rect::from_xywh(x, y, w * frac, h),
            // "up" (default): grow from the bottom edge upward.
            _ => Rect::from_xywh(x, y + h * (1.0 - frac), w, h * frac),
        }
    }

    /// Rect for segment `i` of `n`, where segment 0 is anchored at the
    /// meter's `min` end (bottom for "up", top for "down", left for
    /// "right", right for "left").
    fn segment_rect(&self, i: u32, n: u32, gap: f32) -> Rect {
        let (x0, y0, w, h) = (
            self.x as f32,
            self.y as f32,
            self.width as f32,
            self.height as f32,
        );
        let dir = self.direction.as_deref().unwrap_or("up");
        let vertical = matches!(dir, "up" | "down");
        let axis = if vertical { h } else { w };
        let seg = ((axis - gap * (n as f32 - 1.0)) / n as f32).max(0.0);
        let offset = i as f32 * (seg + gap);
        match dir {
            "down" => Rect::from_xywh(x0, y0 + offset, w, seg),
            "right" => Rect::from_xywh(x0 + offset, y0, seg, h),
            "left" => Rect::from_xywh(x0 + w - offset - seg, y0, seg, h),
            _ => Rect::from_xywh(x0, y0 + h - offset - seg, w, seg),
        }
    }

    /// Draw a border around the full meter rect, entirely outside the bounding
    /// box (same outside-stroke technique as RectConfig).
    fn draw_border(&self, canvas: &Canvas, paint: &mut Paint, radius: f32) {
        let Some(bc) = self.border_color.as_deref() else {
            return;
        };
        let bw = self.border_width.unwrap_or(2.0);
        let half = bw / 2.0;
        let outer = Rect::from_xywh(
            self.x as f32 - half,
            self.y as f32 - half,
            self.width as f32 + bw,
            self.height as f32 + bw,
        );
        let border_op = self.border_opacity.or(self.opacity);
        let (r, g, b, a) = hex_with_opacity(bc, border_op);
        paint.set_shader(None);
        paint.set_color(Color::from_argb(a, r, g, b));
        paint.set_style(skia_safe::paint::Style::Stroke);
        paint.set_stroke_width(bw);
        if radius > 0.0 {
            canvas.draw_rrect(
                RRect::new_rect_xy(outer, radius + half, radius + half),
                paint,
            );
        } else {
            canvas.draw_rect(outer, paint);
        }
    }

    /// Two-pass scale rendering.
    /// `under_fill = true`  → draw unlabeled ticks + mid labeled tick lines (called before fill).
    /// `under_fill = false` → draw end tick lines + all labels (called after fill).
    fn draw_scale(&self, canvas: &Canvas, ctx: &ElementCtx, under_fill: bool) {
        let tick_count = self.scale_ticks.unwrap_or(0);
        let has_labels = self.scale_labels.is_some();
        if tick_count == 0 && !has_labels {
            return;
        }

        let color_str = self
            .scale_color
            .as_deref()
            .or(self.color.as_deref())
            .unwrap_or("#ffffff");
        let (r, g, b, a) = hex_with_opacity(color_str, self.opacity);
        let color = Color::from_argb(a, r, g, b);

        let end_ext = self.scale_tick_length.unwrap_or(6.0);
        let tick_w = self.scale_tick_width.unwrap_or(1.0);
        let offset = self.scale_offset.unwrap_or(8.0);
        let suffix = self.scale_suffix.as_deref().unwrap_or("");

        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(color);

        let dir = self.direction.as_deref().unwrap_or("up");
        let x0 = self.x as f32;
        let y0 = self.y as f32;
        let w = self.width as f32;
        let h = self.height as f32;

        if under_fill {
            // ── Unlabeled ticks (all span bar width, go under fill) ──────────
            if tick_count >= 1 && tick_w > 0.0 {
                paint.set_style(skia_safe::paint::Style::Stroke);
                paint.set_stroke_width(tick_w);
                for i in 0..tick_count {
                    let t = if tick_count == 1 {
                        0.5_f32
                    } else {
                        i as f32 / (tick_count - 1) as f32
                    };
                    if matches!(dir, "up" | "down") {
                        let ref_y = if dir == "down" {
                            y0 + h * t
                        } else {
                            y0 + h * (1.0 - t)
                        };
                        canvas.draw_line((x0, ref_y), (x0 + w, ref_y), &paint);
                    } else {
                        let ref_x = if dir == "left" {
                            x0 + w * (1.0 - t)
                        } else {
                            x0 + w * t
                        };
                        canvas.draw_line((ref_x, y0), (ref_x, y0 + h), &paint);
                    }
                }
            }

            // ── Mid labeled tick lines (no extension, go under fill) ─────────
            if has_labels && tick_w > 0.0 {
                let labels = self.scale_labels.as_ref().unwrap();
                let mid = (self.min + self.max) / 2.0;
                let values: Vec<f64> = if labels.is_empty() {
                    vec![self.min, mid, self.max]
                } else {
                    labels.clone()
                };
                let span = (self.max - self.min) as f32;
                let n = values.len();
                paint.set_style(skia_safe::paint::Style::Stroke);
                paint.set_stroke_width(tick_w);
                for (idx, &v) in values.iter().enumerate() {
                    if idx == 0 || idx == n - 1 {
                        continue; // end ticks drawn in the over pass
                    }
                    let t = if span.abs() < f32::EPSILON {
                        0.0_f32
                    } else {
                        ((v - self.min) as f32 / span).clamp(0.0, 1.0)
                    };
                    if matches!(dir, "up" | "down") {
                        let ref_y = if dir == "down" {
                            y0 + h * t
                        } else {
                            y0 + h * (1.0 - t)
                        };
                        canvas.draw_line((x0, ref_y), (x0 + w, ref_y), &paint);
                    } else {
                        let ref_x = if dir == "left" {
                            x0 + w * (1.0 - t)
                        } else {
                            x0 + w * t
                        };
                        canvas.draw_line((ref_x, y0), (ref_x, y0 + h), &paint);
                    }
                }
            }
        } else {
            // ── Over fill: end tick lines + all labels ───────────────────────
            if !has_labels {
                return;
            }
            let labels = self.scale_labels.as_ref().unwrap();
            let mid = (self.min + self.max) / 2.0;
            let values: Vec<f64> = if labels.is_empty() {
                vec![self.min, mid, self.max]
            } else {
                labels.clone()
            };

            let font_name = self.scale_font.as_deref().unwrap_or("Arial.ttf");
            let font_size = self.scale_font_size.unwrap_or(20.0);
            let font = ctx
                .typefaces
                .get(font_name)
                .map(|tf| font_from_typeface(tf.clone(), font_size, false))
                .or_else(|| load_font(font_name, font_size, ctx.fonts_dir, false));
            let font = match font {
                Some(f) => f,
                None => return,
            };

            let span = (self.max - self.min) as f32;
            let (_, metrics) = font.metrics();
            let cap_h = metrics.cap_height.abs();
            let n = values.len();

            for (idx, &v) in values.iter().enumerate() {
                let t = if span.abs() < f32::EPSILON {
                    0.0_f32
                } else {
                    ((v - self.min) as f32 / span).clamp(0.0, 1.0)
                };
                let is_end = idx == 0 || idx == n - 1;
                let ext = if is_end { end_ext } else { 0.0 };
                let label = if (v.fract()).abs() < 1e-9 {
                    format!("{}{}", v as i64, suffix)
                } else {
                    format!("{:.1}{}", v, suffix)
                };

                if matches!(dir, "up" | "down") {
                    let ref_y = if dir == "down" {
                        y0 + h * t
                    } else {
                        y0 + h * (1.0 - t)
                    };
                    if is_end && tick_w > 0.0 {
                        paint.set_style(skia_safe::paint::Style::Stroke);
                        paint.set_stroke_width(tick_w);
                        canvas.draw_line((x0 - ext, ref_y), (x0 + w + ext, ref_y), &paint);
                    }
                    let label_x = x0 + w + end_ext + offset;
                    let label_y = ref_y + cap_h / 2.0;
                    paint.set_style(skia_safe::paint::Style::Fill);
                    canvas.draw_str(&label, (label_x, label_y), &font, &paint);
                } else {
                    let ref_x = if dir == "left" {
                        x0 + w * (1.0 - t)
                    } else {
                        x0 + w * t
                    };
                    if is_end && tick_w > 0.0 {
                        paint.set_style(skia_safe::paint::Style::Stroke);
                        paint.set_stroke_width(tick_w);
                        canvas.draw_line((ref_x, y0 - ext), (ref_x, y0 + h + ext), &paint);
                    }
                    let text_w = font.measure_str(&label, Some(&paint)).0;
                    let label_x = ref_x - text_w / 2.0;
                    let label_y = y0 + h + end_ext + offset + cap_h;
                    paint.set_style(skia_safe::paint::Style::Fill);
                    canvas.draw_str(&label, (label_x, label_y), &font, &paint);
                }
            }
        }
    }

    fn draw_segmented(&self, canvas: &Canvas, paint: &mut Paint, n: u32, frac: f32, radius: f32) {
        let gap = self.gap.unwrap_or(0.0).max(0.0);
        let lit = (frac * n as f32).round() as u32;
        let grad = self.gradient.as_ref().filter(|g| !g.is_empty());
        for i in 0..n {
            let color = if i < lit {
                let fill_op = self.fill_opacity.or(self.opacity);
                let (r, g, b, a) = match grad {
                    Some(stops) => {
                        let t = (i as f32 + 0.5) / n as f32;
                        lerp_gradient(stops, t, fill_op)
                    }
                    None => hex_with_opacity(self.color.as_deref().unwrap_or("#ffffff"), fill_op),
                };
                Some(Color::from_argb(a, r, g, b))
            } else {
                self.background.as_deref().map(|bg| {
                    let bg_opacity = self.background_opacity;
                    let (r, g, b, a) = hex_with_opacity(bg, bg_opacity);
                    Color::from_argb(a, r, g, b)
                })
            };
            if let Some(c) = color {
                paint.set_color(c);
                let rect = self.segment_rect(i, n, gap);
                if radius > 0.0 {
                    canvas.draw_rrect(RRect::new_rect_xy(rect, radius, radius), paint);
                } else {
                    canvas.draw_rect(rect, paint);
                }
            }
        }
    }
}

impl OverlayElement for MeterConfig {
    fn measure(&self, _ctx: &ElementCtx, _frame_idx: usize) -> Option<ElementBounds> {
        Some(ElementBounds {
            id: self.id.clone(),
            x: self.x as f32,
            y: self.y as f32,
            w: self.width as f32,
            h: self.height as f32,
        })
    }

    fn draw(&self, canvas: &Canvas, ctx: &ElementCtx, frame_idx: usize) {
        let rotation = self.rotation.unwrap_or(0.0);
        if rotation != 0.0 {
            let cx = self.x as f32 + self.width as f32 / 2.0;
            let cy = self.y as f32 + self.height as f32 / 2.0;
            canvas.save();
            canvas.rotate(rotation, Some(skia_safe::Point::new(cx, cy)));
        }

        let radius = self.radius.unwrap_or(0.0);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);

        if let Some(n) = self.segments.filter(|n| *n >= 1) {
            let frac = self.fraction(ctx.activity, frame_idx);
            self.draw_segmented(canvas, &mut paint, n, frac, radius);
            self.draw_border(canvas, &mut paint, radius);
            self.draw_scale(canvas, ctx, false);
            if rotation != 0.0 {
                canvas.restore();
            }
            return;
        }

        // Track (empty portion), if a background color is set.
        if let Some(bg) = self.background.as_deref() {
            let bg_opacity = self.background_opacity;
            let (r, g, b, a) = hex_with_opacity(bg, bg_opacity);
            paint.set_color(Color::from_argb(a, r, g, b));
            if radius > 0.0 {
                canvas.draw_rrect(RRect::new_rect_xy(self.rect(), radius, radius), &paint);
            } else {
                canvas.draw_rect(self.rect(), &paint);
            }
        }

        // Mid ticks go under the fill so the fill bar overlaps them.
        self.draw_scale(canvas, ctx, true);

        // Fill.
        let frac = self.fraction(ctx.activity, frame_idx);
        if frac > 0.0 {
            let fr = self.fill_rect(frac);

            if let Some(stops) = self.gradient.as_ref().filter(|g| g.len() >= 2) {
                // Direction-aware gradient anchored to the full element bounds so
                // the gradient position is stable as the fill grows — a half-full
                // "up" meter shows the low-value colors at the bottom.
                let cx = self.x as f32 + self.width as f32 / 2.0;
                let cy = self.y as f32 + self.height as f32 / 2.0;
                let (p0, p1) = match self.direction.as_deref().unwrap_or("up") {
                    "down" => (
                        skia_safe::Point::new(cx, self.y as f32),
                        skia_safe::Point::new(cx, self.y as f32 + self.height as f32),
                    ),
                    "right" => (
                        skia_safe::Point::new(self.x as f32, cy),
                        skia_safe::Point::new(self.x as f32 + self.width as f32, cy),
                    ),
                    "left" => (
                        skia_safe::Point::new(self.x as f32 + self.width as f32, cy),
                        skia_safe::Point::new(self.x as f32, cy),
                    ),
                    // "up": low color at bottom (fill grows upward), high at top.
                    _ => (
                        skia_safe::Point::new(cx, self.y as f32 + self.height as f32),
                        skia_safe::Point::new(cx, self.y as f32),
                    ),
                };
                let fill_op = self.fill_opacity.or(self.opacity);
                let n = stops.len();
                let colors: Vec<Color> = stops
                    .iter()
                    .map(|s| {
                        let (r, g, b, a) = hex_with_opacity(s, fill_op);
                        Color::from_argb(a, r, g, b)
                    })
                    .collect();
                let pos: Vec<f32> = (0..n).map(|i| i as f32 / (n - 1) as f32).collect();
                let shader = skia_safe::gradient_shader::linear(
                    (p0, p1),
                    skia_safe::gradient_shader::GradientShaderColors::Colors(&colors),
                    Some(pos.as_slice()),
                    skia_safe::TileMode::Clamp,
                    None,
                    None,
                );
                paint.set_shader(shader);
                // A shader's output is still modulated by the paint's alpha.
                // The background draw above may have left a low/zero alpha, so
                // reset to opaque — the fill's own alpha is baked into the stops.
                paint.set_alpha(255);
            } else {
                let color_str = self.color.as_deref().unwrap_or("#ffffff");
                let fill_op = self.fill_opacity.or(self.opacity);
                let (r, g, b, a) = hex_with_opacity(color_str, fill_op);
                paint.set_color(Color::from_argb(a, r, g, b));
            }

            if radius > 0.0 {
                canvas.draw_rrect(RRect::new_rect_xy(fr, radius, radius), &paint);
            } else {
                canvas.draw_rect(fr, &paint);
            }
            paint.set_shader(None);
        }

        self.draw_border(canvas, &mut paint, radius);
        // End tick lines + all labels go on top of the fill.
        self.draw_scale(canvas, ctx, false);

        if rotation != 0.0 {
            canvas.restore();
        }
    }
}

// ─── Gauge ─────────────────────────────────────────────────────────────────

impl GaugeConfig {
    fn fraction(&self, activity: &Activity, frame_idx: usize) -> f32 {
        if !activity.valid_attributes.contains(&self.value) {
            return 0.0;
        }
        let raw = activity.get_scalar(&self.value, frame_idx);
        let (conv, _) = units::resolve(&self.value, self.unit.as_deref());
        let v = conv.apply(raw);
        let span = self.max - self.min;
        if span.abs() < f64::EPSILON {
            return 0.0;
        }
        (((v - self.min) / span) as f32).clamp(0.0, 1.0)
    }

    fn argb(&self, specific: Option<&str>, opacity: Option<f32>) -> Color {
        let s = specific.or(self.color.as_deref()).unwrap_or("#ffffff");
        let (r, g, b, a) = hex_with_opacity(s, opacity);
        Color::from_argb(a, r, g, b)
    }
}

impl OverlayElement for GaugeConfig {
    fn measure(&self, _ctx: &ElementCtx, _frame_idx: usize) -> Option<ElementBounds> {
        Some(ElementBounds {
            id: self.id.clone(),
            x: self.x as f32,
            y: self.y as f32,
            w: self.width as f32,
            h: self.height as f32,
        })
    }

    fn draw(&self, canvas: &Canvas, ctx: &ElementCtx, frame_idx: usize) {
        let cx = self.x as f32 + self.width as f32 / 2.0;
        let cy = self.y as f32 + self.height as f32 / 2.0;

        let rotation = self.rotation.unwrap_or(0.0);
        if rotation != 0.0 {
            canvas.save();
            canvas.rotate(rotation, Some(skia_safe::Point::new(cx, cy)));
        }

        let arc_w = self.arc_width.unwrap_or(8.0);
        let needle_w = self.needle_width.unwrap_or(4.0);
        // Keep the stroked arc fully inside the bounding box.
        let r = (self.width.min(self.height) as f32) / 2.0 - arc_w / 2.0;
        if r <= 0.0 {
            if rotation != 0.0 {
                canvas.restore();
            }
            return;
        }
        let start = self.start_angle.unwrap_or(135.0);
        let sweep = self.sweep_angle.unwrap_or(270.0);
        let frac = self.fraction(ctx.activity, frame_idx);

        // Background circle.
        if let Some(bg_op) = self.background_opacity.filter(|&op| op > 0.0) {
            let bg_str = self.background_color.as_deref().unwrap_or("#000000");
            let (r2, g2, b2, _) = crate::render::color::parse_hex_color(bg_str);
            let alpha = (bg_op.clamp(0.0, 1.0) * 255.0) as u8;
            let mut bg_paint = Paint::default();
            bg_paint.set_anti_alias(true);
            bg_paint.set_color(Color::from_argb(alpha, r2, g2, b2));
            bg_paint.set_style(skia_safe::paint::Style::Fill);
            let full_r =
                self.width.min(self.height) as f32 / 2.0 + self.background_margin.unwrap_or(0.0);
            canvas.draw_circle((cx, cy), full_r, &bg_paint);
        }

        let oval = Rect::from_ltrb(cx - r, cy - r, cx + r, cy + r);

        let mut arc_paint = Paint::default();
        arc_paint.set_anti_alias(true);
        arc_paint.set_style(skia_safe::paint::Style::Stroke);
        arc_paint.set_stroke_cap(skia_safe::paint::Cap::Round);

        arc_paint.set_stroke_width(arc_w);

        // Track arc (unfilled portion, start → max). Hidden when hide_track is set.
        if !self.hide_track.unwrap_or(false) {
            arc_paint.set_color(self.argb(self.arc_color.as_deref(), self.opacity));
            canvas.draw_arc(oval, start, sweep, false, &arc_paint);
        }

        // Progress arc (start → current).
        if frac > 0.0 {
            let progress_sweep = sweep * frac;
            let has_gradient = self
                .gradient
                .as_ref()
                .map(|g| g.len() >= 2)
                .unwrap_or(false);

            if has_gradient {
                let grad = self.gradient.as_ref().unwrap();
                let n = grad.len();
                let colors: Vec<Color> = grad
                    .iter()
                    .map(|c| self.argb(Some(c), self.opacity))
                    .collect();
                let pos: Vec<f32> = (0..n).map(|i| i as f32 / (n - 1) as f32).collect();
                // Rotate the canvas so the arc begins at 0° in local space.
                // The sweep gradient range (0, sweep) then maps directly onto the
                // arc without any local-matrix offset and without the 360°-wrap
                // problem that occurs when the arc crosses the East axis.
                canvas.save();
                canvas.rotate(start, Some(skia_safe::Point::new(cx, cy)));
                let shader = skia_safe::gradient_shader::sweep(
                    skia_safe::Point::new(cx, cy),
                    skia_safe::gradient_shader::GradientShaderColors::Colors(&colors),
                    pos.as_slice(),
                    skia_safe::TileMode::Clamp,
                    Some((0.0, sweep)),
                    None,
                    None::<&skia_safe::Matrix>,
                );
                arc_paint.set_shader(shader);
                arc_paint.set_alpha(255);
                canvas.draw_arc(oval, 0.0, progress_sweep, false, &arc_paint);
                arc_paint.set_shader(None);
                canvas.restore();
            } else if let Some(pc) = self.progress_color.as_deref() {
                arc_paint.set_color(self.argb(Some(pc), self.opacity));
                canvas.draw_arc(oval, start, progress_sweep, false, &arc_paint);
            }
        }

        // Cap dot at the tip of the progress arc.
        if let Some(cap_str) = self.cap_color.as_deref() {
            let theta = (start + sweep * frac).to_radians();
            let dot_x = cx + theta.cos() * r;
            let dot_y = cy + theta.sin() * r;
            let cap_r = self.cap_radius.unwrap_or(arc_w / 2.0);
            let mut cap_paint = Paint::default();
            cap_paint.set_anti_alias(true);
            cap_paint.set_color(self.argb(Some(cap_str), self.opacity));
            cap_paint.set_style(skia_safe::paint::Style::Fill);
            canvas.draw_circle((dot_x, dot_y), cap_r, &cap_paint);
        }

        // Needle (traditional pointer from center). Skipped when needle_width = 0.
        if needle_w > 0.0 {
            let theta = (start + sweep * frac).to_radians();
            let nx = cx + theta.cos() * r;
            let ny = cy + theta.sin() * r;
            let mut needle = Paint::default();
            needle.set_anti_alias(true);
            needle.set_style(skia_safe::paint::Style::Stroke);
            needle.set_stroke_cap(skia_safe::paint::Cap::Round);
            needle.set_stroke_width(needle_w);
            needle.set_color(self.argb(self.needle_color.as_deref(), self.opacity));
            canvas.draw_line((cx, cy), (nx, ny), &needle);

            // Hub.
            needle.set_style(skia_safe::paint::Style::Fill);
            canvas.draw_circle((cx, cy), needle_w * 1.5, &needle);
        }

        if rotation != 0.0 {
            canvas.restore();
        }
    }
}

// ─── Rect ──────────────────────────────────────────────────────────────────

impl OverlayElement for RectConfig {
    fn measure(&self, _ctx: &ElementCtx, _frame_idx: usize) -> Option<ElementBounds> {
        Some(ElementBounds {
            id: self.id.clone(),
            x: self.x as f32,
            y: self.y as f32,
            w: self.width as f32,
            h: self.height as f32,
        })
    }

    fn crop_extent(&self, _ctx: &ElementCtx, _frame_idx: usize) -> Option<(f32, f32, f32, f32)> {
        let rot = self.rotation.unwrap_or(0.0);
        if rot != 0.0 {
            let cx = self.x as f32 + self.width as f32 / 2.0;
            let cy = self.y as f32 + self.height as f32 / 2.0;
            let r = ((self.width as f32).powi(2) + (self.height as f32).powi(2)).sqrt() / 2.0;
            Some((cx - r, cy - r, cx + r, cy + r))
        } else {
            Some((
                self.x as f32,
                self.y as f32,
                self.x as f32 + self.width as f32,
                self.y as f32 + self.height as f32,
            ))
        }
    }

    fn draw(&self, canvas: &Canvas, _ctx: &ElementCtx, _frame_idx: usize) {
        let rotation = self.rotation.unwrap_or(0.0);
        let rect = Rect::from_xywh(
            self.x as f32,
            self.y as f32,
            self.width as f32,
            self.height as f32,
        );
        if rotation != 0.0 {
            let cx = self.x as f32 + self.width as f32 / 2.0;
            let cy = self.y as f32 + self.height as f32 / 2.0;
            canvas.save();
            canvas.rotate(rotation, Some(skia_safe::Point::new(cx, cy)));
        }

        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        let radius = self.radius.unwrap_or(0.0);

        // fill_opacity is the primary fill control; opacity is the fallback
        // and also gates the border. They are NOT multiplied — set fill_opacity
        // to 0 for a transparent fill while keeping opacity:1 for a visible border.
        let fill_op = self.fill_opacity.or(self.opacity);
        let color_str = self.color.as_deref().unwrap_or("#ffffff");
        let (r, g, b, a) = hex_with_opacity(color_str, fill_op);
        paint.set_color(Color::from_argb(a, r, g, b));
        paint.set_style(skia_safe::paint::Style::Fill);
        if radius > 0.0 {
            canvas.draw_rrect(RRect::new_rect_xy(rect, radius, radius), &paint);
        } else {
            canvas.draw_rect(rect, &paint);
        }

        // Border stroke — drawn outside the fill rect so the element's
        // bounding box is the inner edge of the stroke, not the center.
        // Skia strokes are centered by default, so we expand the rect by
        // half the stroke width on every side to achieve an outside stroke.
        if let Some(bc) = self.border_color.as_deref() {
            let bw = self.border_width.unwrap_or(2.0);
            let half = bw / 2.0;
            let outer = Rect::from_xywh(
                self.x as f32 - half,
                self.y as f32 - half,
                self.width as f32 + bw,
                self.height as f32 + bw,
            );
            let border_op = self.border_opacity.or(self.opacity);
            let (r, g, b, a) = hex_with_opacity(bc, border_op);
            paint.set_color(Color::from_argb(a, r, g, b));
            paint.set_style(skia_safe::paint::Style::Stroke);
            paint.set_stroke_width(bw);
            if radius > 0.0 {
                canvas.draw_rrect(
                    RRect::new_rect_xy(outer, radius + half, radius + half),
                    &paint,
                );
            } else {
                canvas.draw_rect(outer, &paint);
            }
        }

        if rotation != 0.0 {
            canvas.restore();
        }
    }
}

// ─── Measurement ───────────────────────────────────────────────────────────

/// Measure every element in the template for the given frame, returning
/// pixel-perfect bounding boxes using the same Skia font metrics used to render.
pub fn measure_elements(
    frame_idx: usize,
    activity: &Activity,
    template: &Template,
    fonts_dir: &str,
) -> Vec<ElementBounds> {
    let charts: HashMap<String, ChartCache> = HashMap::new();
    let typefaces: HashMap<String, Typeface> = HashMap::new();
    let images: HashMap<String, skia_safe::Image> = HashMap::new();
    let ctx = ElementCtx {
        activity,
        scene: &template.scene,
        typefaces: &typefaces,
        charts: &charts,
        fonts_dir,
        images: &images,
    };
    template
        .elements
        .iter()
        .filter_map(|e| e.as_overlay().measure(&ctx, frame_idx))
        .collect()
}

/// All pre-computed data that stays constant across frames.
pub struct SceneCache {
    /// Pre-rendered base frame as an immutable Skia Image.
    /// Stored as Image (not raw bytes) to avoid a heap allocation + 8 MB copy on every frame.
    pub base_image: skia_safe::Image,
    /// One ChartCache per plot element, keyed by element id.
    pub charts: HashMap<String, ChartCache>,
    pub width: u32,
    pub height: u32,
    /// Pre-loaded typefaces keyed by filename. Eliminates disk I/O inside the per-frame
    /// hot path — Font::new(typeface.clone(), size) is trivially cheap.
    pub typefaces: HashMap<String, Typeface>,
    /// Pre-decoded images keyed by element id.
    pub images: HashMap<String, skia_safe::Image>,
}

impl SceneCache {
    pub fn build(
        activity: &Activity,
        template: &Template,
        fonts_dir: &str,
        assets_dirs: &[&str],
    ) -> Result<Self, String> {
        let w = template.scene.width;
        let h = template.scene.height;

        // --- Pre-load all typefaces referenced by elements ---
        let mut typefaces: HashMap<String, Typeface> = HashMap::new();
        for font_name in template
            .elements
            .iter()
            .flat_map(|e| e.as_overlay().fonts(&template.scene))
        {
            if let std::collections::hash_map::Entry::Vacant(e) = typefaces.entry(font_name.clone())
            {
                if let Some(tf) = load_typeface(&font_name, fonts_dir) {
                    e.insert(tf);
                } else {
                    eprintln!(
                        "Warning: could not load font '{font_name}'; text elements using it will not render"
                    );
                }
            }
        }

        // --- Build chart caches (keyed by element id) ---
        let mut charts: HashMap<String, ChartCache> = HashMap::new();
        for el in &template.elements {
            if let Some(cache) = el.as_overlay().build_chart(activity, fonts_dir) {
                charts.insert(el.id().to_string(), cache);
            }
        }

        // --- Pre-decode images (static assets, loaded once per render session) ---
        let mut images: HashMap<String, skia_safe::Image> = HashMap::new();
        for el in &template.elements {
            if let Element::Image(cfg) = el {
                if cfg.file.trim().is_empty() {
                    log::warn!(
                        "SceneCache: image element '{}' has no asset selected; skipping",
                        cfg.id
                    );
                    continue;
                }
                log::info!(
                    "SceneCache: loading image asset '{}' for element '{}'",
                    cfg.file,
                    cfg.id
                );
                match load_asset_image(&cfg.file, assets_dirs) {
                    Some(img) => {
                        images.insert(cfg.id.clone(), img);
                    }
                    None => log::warn!(
                        "SceneCache: could not load image asset '{}' for element '{}'; skipping",
                        cfg.file,
                        cfg.id
                    ),
                }
            }
        }

        // --- Pre-render transparent base frame as a Skia Image ---
        let base_image = render_base_frame(w, h)?;

        Ok(SceneCache {
            base_image,
            charts,
            width: w,
            height: h,
            typefaces,
            images,
        })
    }
}

/// Rectangular sub-region of the full overlay frame, in overlay pixel coords.
/// When a render is cropped to the union of all visible elements, only this
/// window is rasterised + piped + encoded — the rest is fully transparent and
/// pure overhead. `x`/`y` is the placement offset the compositor needs.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct CropRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// Render a single video frame and return raw BGRA bytes.
///
/// `crop`: when `Some`, the surface is sized to the crop window and the canvas
/// is translated so all absolute-coordinate draws (base image, charts, text)
/// land correctly while only the window is captured. `None` = full frame
/// (preview path).
pub fn render_frame(
    frame_idx: usize,
    cache: &SceneCache,
    activity: &Activity,
    template: &Template,
    crop: Option<&CropRect>,
) -> Vec<u8> {
    let (w, h, ox, oy) = match crop {
        Some(c) => (c.w as i32, c.h as i32, c.x, c.y),
        None => (cache.width as i32, cache.height as i32, 0, 0),
    };

    let info = ImageInfo::new(
        ISize::new(w, h),
        skia_safe::ColorType::BGRA8888,
        skia_safe::AlphaType::Premul,
        None,
    );
    let row_bytes = (w * 4) as usize;

    // Composite straight into the output buffer: wrap a raster surface around
    // `pixels` so Skia draws in place. This drops both the surface's separate
    // backing-store allocation and the full-frame read_pixels copy the old
    // raster()+read_pixels path paid every frame. Render is fully hidden behind
    // encode, so this is a memory-traffic / allocator-churn win, not a speedup.
    // A fresh vec is zeroed = transparent BGRA; the base image covers the whole
    // crop window, so no explicit clear is needed.
    let mut pixels = vec![0u8; (h as usize) * row_bytes];
    {
        let mut surface =
            skia_safe::surfaces::wrap_pixels(&info, &mut pixels, Some(row_bytes), None)
                .expect("Skia surface");
        let canvas = surface.canvas();

        // Shift the world so the crop window maps to (0,0); all draw calls
        // below keep using absolute overlay coordinates unchanged.
        if ox != 0 || oy != 0 {
            canvas.translate((-ox as f32, -oy as f32));
        }

        // 1. Blit transparent base frame.
        //    Drawing an Image reference — no extra allocation or byte copy.
        canvas.draw_image(&cache.base_image, (0, 0), None);

        // 2. Draw all elements back-to-front according to scene.layers.
        let ctx = ElementCtx {
            activity,
            scene: &template.scene,
            typefaces: &cache.typefaces,
            charts: &cache.charts,
            fonts_dir: "",
            images: &cache.images,
        };
        for idx in template.layer_order() {
            if let Some(el) = template.elements.get(idx) {
                el.as_overlay().draw(canvas, &ctx, frame_idx);
            }
        }
    } // surface dropped here → releases the &mut pixels borrow

    pixels
}

/// Union bounding box of every element that is ever drawn, across all frames.
///
/// Static elements (plots/labels) are measured once; dynamic elements (value
/// text changes width frame-to-frame) are measured at every frame (cheap: one
/// cached `Font` per config via load_font). The box is padded, clamped to the
/// frame, and rounded to even dimensions. Returns `None` when the box covers
/// ≥95% of the frame (cropping wouldn't pay off) or there is nothing to draw —
/// callers fall back to the full-frame path.
pub fn compute_crop_rect(
    activity: &Activity,
    template: &Template,
    fonts_dir: &str,
    cancelled: Option<&std::sync::atomic::AtomicBool>,
) -> Option<CropRect> {
    let fw = template.scene.width as f32;
    let fh = template.scene.height as f32;

    let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
    let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    let mut acc = |x0: f32, y0: f32, x1: f32, y1: f32| {
        min_x = min_x.min(x0);
        min_y = min_y.min(y0);
        max_x = max_x.max(x1);
        max_y = max_y.max(y1);
    };

    let charts: HashMap<String, ChartCache> = HashMap::new();
    let mut typefaces: HashMap<String, Typeface> = HashMap::new();
    for font_name in template
        .elements
        .iter()
        .flat_map(|e| e.as_overlay().fonts(&template.scene))
    {
        if let std::collections::hash_map::Entry::Vacant(e) = typefaces.entry(font_name.clone())
            && let Some(tf) = load_typeface(&font_name, fonts_dir)
        {
            e.insert(tf);
        }
    }
    let images: HashMap<String, skia_safe::Image> = HashMap::new();
    let ctx = ElementCtx {
        activity,
        scene: &template.scene,
        typefaces: &typefaces,
        charts: &charts,
        fonts_dir,
        images: &images,
    };

    let n = activity.data_len();
    for el in &template.elements {
        let ov = el.as_overlay();
        if ov.is_dynamic() {
            for i in 0..n {
                if i % 1024 == 0
                    && cancelled
                        .map(|c| c.load(std::sync::atomic::Ordering::Relaxed))
                        .unwrap_or(false)
                {
                    return None;
                }
                if let Some((x0, y0, x1, y1)) = ov.crop_extent(&ctx, i) {
                    acc(x0, y0, x1, y1);
                }
            }
        } else if let Some((x0, y0, x1, y1)) = ov.crop_extent(&ctx, 0) {
            acc(x0, y0, x1, y1);
        }
    }

    if !min_x.is_finite() || !max_x.is_finite() || max_x <= min_x || max_y <= min_y {
        return None;
    }

    // Pad, clamp to frame, round origin down / extent up to even dimensions.
    const PAD: f32 = 16.0;
    let x0 = (min_x - PAD).floor().max(0.0);
    let y0 = (min_y - PAD).floor().max(0.0);
    let x1 = (max_x + PAD).ceil().min(fw);
    let y1 = (max_y + PAD).ceil().min(fh);

    let x = x0 as i32 & !1;
    let y = y0 as i32 & !1;
    let w = (((x1 as i32) - x).max(2) as u32 + 1) & !1;
    let h = (((y1 as i32) - y).max(2) as u32 + 1) & !1;
    let w = w.min(template.scene.width - x as u32);
    let h = h.min(template.scene.height - y as u32);

    // Not worth the contract change if the box is essentially the whole frame.
    if (w as f32 * h as f32) >= 0.95 * fw * fh {
        return None;
    }

    Some(CropRect { x, y, w, h })
}

// ─── Base frame pre-renderer ───────────────────────────────────────────────

fn render_base_frame(w: u32, h: u32) -> Result<skia_safe::Image, String> {
    let info = ImageInfo::new(
        ISize::new(w as i32, h as i32),
        skia_safe::ColorType::BGRA8888,
        skia_safe::AlphaType::Premul,
        None,
    );
    let mut surface = skia_safe::surfaces::raster(&info, None, None)
        .ok_or("Failed to create base frame surface")?;
    let canvas = surface.canvas();
    canvas.clear(Color::TRANSPARENT);

    Ok(surface.image_snapshot())
}

// ─── Font loading ──────────────────────────────────────────────────────────

pub(crate) fn load_typeface(font_name: &str, fonts_dir: &str) -> Option<Typeface> {
    let mgr = FontMgr::default();
    // Bundled fonts first, then user-installed custom fonts.
    let candidates = [
        std::path::PathBuf::from(format!("{fonts_dir}/{font_name}")),
        crate::fonts_user_dir().join(font_name),
    ];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(&path) {
            let data = skia_safe::Data::new_copy(&bytes);
            if let Some(tf) = mgr.new_from_data(&data, None) {
                return Some(tf);
            }
        }
    }
    // System font fallback: try the font name as a family, then common Linux
    // families (Arial is not typically installed on Linux).
    let primary = font_name
        .trim_end_matches(".ttf")
        .trim_end_matches(".TTF")
        .trim_end_matches(".otf")
        .trim_end_matches(".OTF");
    let fallbacks: &[&str] = if cfg!(target_os = "linux") {
        &[
            primary,
            "DejaVu Sans",
            "Liberation Sans",
            "Noto Sans",
            "Ubuntu",
            "FreeSans",
            "sans-serif",
        ]
    } else {
        &[primary]
    };
    for family in fallbacks {
        if let Some(tf) = mgr.match_family_style(family, FontStyle::normal()) {
            return Some(tf);
        }
    }
    None
}

pub(crate) fn font_from_typeface(typeface: Typeface, size: f32, italic: bool) -> Font {
    let mut font = Font::new(typeface, size);
    if italic {
        font.set_skew_x(ITALIC_SKEW_X);
    }
    font
}

fn load_font(font_name: &str, size: f32, fonts_dir: &str, italic: bool) -> Option<Font> {
    load_typeface(font_name, fonts_dir).map(|tf| font_from_typeface(tf, size, italic))
}

/// Adjust a base x coordinate for text alignment.
/// `base_x` is the authored position; returns the draw-origin x for Skia (left edge of glyphs).
fn align_x(base_x: f32, text_width: f32, align: Option<&str>) -> f32 {
    match align.unwrap_or("left") {
        "right" => base_x - text_width,
        "center" => base_x - text_width / 2.0,
        _ => base_x, // "left" default
    }
}

// ─── Value formatting ──────────────────────────────────────────────────────

fn format_value(raw: f64, cfg: &ValueConfig) -> String {
    // Convert from the GPX-native unit. Value elements do not auto-append a
    // unit suffix; the optional manual `suffix` field is applied below.
    let (conv, _) = units::resolve(&cfg.value, cfg.unit.as_deref());
    let v = conv.apply(raw);

    // Decimal rounding
    let text = match cfg.decimal_rounding {
        Some(0) => format!("{}", v.round() as i64),
        Some(n) if n > 0 => format!("{:.prec$}", v, prec = n as usize),
        _ => format!("{}", v.round() as i64),
    };

    // Suffix
    match &cfg.suffix {
        Some(s) => format!("{text}{s}"),
        None => text,
    }
}

// ─── Image element ─────────────────────────────────────────────────────────

/// Beat-pulse envelope: quick attack, smooth quadratic decay. Returns 0..=1.
fn beat_curve(phase: f32) -> f32 {
    const ATTACK: f32 = 0.12;
    if phase < ATTACK {
        phase / ATTACK
    } else {
        ((1.0 - phase) / (1.0 - ATTACK)).powf(2.0)
    }
}

impl ImageConfig {
    fn pulse_scale(&self, ctx: &ElementCtx, frame_idx: usize) -> f32 {
        let amplitude = self.pulse_amplitude.unwrap_or(0.15);
        if amplitude <= 0.0 {
            return 1.0;
        }
        let bpm = if let Some(fixed) = self.pulse_bpm {
            fixed
        } else if let Some(metric) = self.pulse_metric.as_deref() {
            if ctx.activity.valid_attributes.contains(&metric.to_string()) {
                ctx.activity.get_scalar(metric, frame_idx)
            } else {
                return 1.0;
            }
        } else {
            return 1.0;
        };
        if bpm <= 0.0 {
            return 1.0;
        }
        let fps = ctx.scene.fps as f64;
        let time_sec = frame_idx as f64 / fps;
        let phase = ((time_sec * bpm / 60.0).fract() as f32).clamp(0.0, 1.0);
        1.0 + amplitude * beat_curve(phase)
    }
}

impl OverlayElement for ImageConfig {
    fn measure(&self, ctx: &ElementCtx, frame_idx: usize) -> Option<ElementBounds> {
        let max_scale = 1.0 + self.pulse_amplitude.unwrap_or(0.0).max(0.0);
        let w = self.width as f32 * max_scale;
        let h = self.height as f32 * max_scale;
        let cx = self.x as f32 + self.width as f32 / 2.0;
        let cy = self.y as f32 + self.height as f32 / 2.0;
        // Suppress unused warning — frame_idx not needed for measure but
        // signature must match the trait.
        let _ = (ctx, frame_idx);
        Some(ElementBounds {
            id: self.id.clone(),
            x: cx - w / 2.0,
            y: cy - h / 2.0,
            w,
            h,
        })
    }

    fn draw(&self, canvas: &Canvas, ctx: &ElementCtx, frame_idx: usize) {
        let Some(img) = ctx.images.get(&self.id) else {
            return;
        };
        let opacity = self.opacity.unwrap_or(1.0);
        let rotation = self.rotation.unwrap_or(0.0);
        let w = self.width as f32;
        let h = self.height as f32;
        let cx = self.x as f32 + w / 2.0;
        let cy = self.y as f32 + h / 2.0;
        let scale = self.pulse_scale(ctx, frame_idx);

        let needs_transform = rotation != 0.0 || scale != 1.0;
        if needs_transform {
            canvas.save();
            if scale != 1.0 {
                canvas.translate((cx, cy));
                canvas.scale((scale, scale));
                canvas.translate((-cx, -cy));
            }
            if rotation != 0.0 {
                canvas.rotate(rotation, Some(skia_safe::Point::new(cx, cy)));
            }
        }

        let dst = Rect::from_xywh(self.x as f32, self.y as f32, w, h);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_alpha_f(opacity);
        let sampling = skia_safe::SamplingOptions::new(
            skia_safe::FilterMode::Linear,
            skia_safe::MipmapMode::None,
        );
        canvas.draw_image_rect_with_sampling_options(img, None, dst, sampling, &paint);

        if needs_transform {
            canvas.restore();
        }
    }
}

/// Resolve an asset filename against an ordered list of search directories.
/// Checks absolute path first, then each directory in order.
pub(crate) fn resolve_asset_path(file: &str, search_dirs: &[&str]) -> Option<std::path::PathBuf> {
    let p = std::path::Path::new(file);
    if p.is_absolute() && p.exists() {
        return Some(p.to_path_buf());
    }
    let basename = p.file_name().unwrap_or(p.as_os_str());
    for dir in search_dirs {
        let candidate = std::path::Path::new(dir).join(basename);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Load a PNG/WebP/SVG asset into a Skia Image, searching the given directories.
pub(crate) fn load_asset_image(file: &str, search_dirs: &[&str]) -> Option<skia_safe::Image> {
    if file.trim().is_empty() {
        return None;
    }
    let path = resolve_asset_path(file, search_dirs)?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if ext == "svg" {
        load_svg_image(&path)
    } else if ext == "webp" {
        load_webp_image(&path)
    } else {
        let bytes = std::fs::read(&path).ok()?;
        let data = skia_safe::Data::new_copy(&bytes);
        skia_safe::Image::from_encoded(data)
    }
}

fn load_webp_image(path: &std::path::Path) -> Option<skia_safe::Image> {
    let bytes = std::fs::read(path).ok()?;
    let mut decoder = image_webp::WebPDecoder::new(std::io::Cursor::new(bytes)).ok()?;
    let (w, h) = decoder.dimensions();
    if w == 0 || h == 0 {
        return None;
    }

    let mut decoded = vec![0; decoder.output_buffer_size()?];
    decoder.read_image(&mut decoded).ok()?;

    let rgba = if decoder.has_alpha() {
        decoded
    } else {
        let mut out = Vec::with_capacity(w as usize * h as usize * 4);
        for px in decoded.chunks_exact(3) {
            out.extend_from_slice(&[px[0], px[1], px[2], 255]);
        }
        out
    };

    let info = ImageInfo::new(
        ISize::new(w as i32, h as i32),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Unpremul,
        None,
    );
    let data = skia_safe::Data::new_copy(&rgba);
    skia_safe::images::raster_from_data(&info, data, (w * 4) as usize)
}

fn load_svg_image(path: &std::path::Path) -> Option<skia_safe::Image> {
    let content = std::fs::read_to_string(path).ok()?;
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(&content, &opt).ok()?;
    let size = tree.size().to_int_size();
    let (w, h) = (size.width(), size.height());
    if w == 0 || h == 0 {
        return None;
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    let png_bytes = pixmap.encode_png().ok()?;
    let data = skia_safe::Data::new_copy(&png_bytes);
    skia_safe::Image::from_encoded(data)
}

#[cfg(test)]
mod tests {
    use crate::render::template::Template;

    /// Unified model: explicit `scene.layers` ids drive draw order; elements
    /// not listed fall back to array order after the listed ones.
    #[test]
    fn layer_order_honors_explicit_ids_then_array_order() {
        let raw = serde_json::json!({
            "scene": { "width": 100, "height": 100, "layers": ["plot-0", "label-0"] },
            "elements": [
                { "type": "label", "id": "label-0", "text": "A", "x": 0.0, "y": 0.0 },
                { "type": "value", "id": "value-0", "value": "speed", "x": 0.0, "y": 0.0 },
                { "type": "plot", "id": "plot-0", "value": "elevation",
                  "x": 0, "y": 0, "width": 10, "height": 10 }
            ]
        });
        let t = Template::from_value(raw).unwrap();
        // plot-0 (idx 2), label-0 (idx 0) listed first; value-0 (idx 1) trails.
        assert_eq!(t.layer_order(), vec![2, 0, 1]);
    }
}
