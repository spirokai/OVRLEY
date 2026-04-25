use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::commands::AppPaths;
use crate::config::RenderConfig;
use crate::debug::{RenderProfiler, RenderProgress, TimingBucket};
use crate::encode::ffmpeg::{build_ffmpeg_settings, resolve_ffmpeg_binary};
use crate::render::{prepare_preview_assets, render_frame_rgba, LabelCacheStatus, RenderTarget};
use chrono::Local;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const FRAME_QUEUE_SIZE: usize = 12;

#[derive(Clone)]
pub struct RenderController {
    progress: Arc<Mutex<RenderProgress>>,
    cancel_flag: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    next_render_id: Arc<AtomicU32>,
}

impl Default for RenderController {
    fn default() -> Self {
        Self {
            progress: Arc::new(Mutex::new(RenderProgress::default())),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            running: Arc::new(AtomicBool::new(false)),
            next_render_id: Arc::new(AtomicU32::new(0)),
        }
    }
}

impl RenderController {
    pub fn progress(&self) -> RenderProgress {
        self.progress
            .lock()
            .map(|value| value.clone())
            .unwrap_or_default()
    }

    pub fn cancel(&self) -> bool {
        self.cancel_flag.store(true, Ordering::SeqCst);
        if let Ok(mut progress) = self.progress.lock() {
            progress.status = "cancelled".to_string();
            progress.message = "Cancelling render...".to_string();
        }
        self.running.load(Ordering::SeqCst)
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn try_start(&self, total_frames: u32, message: &str) -> Result<u64, String> {
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("A render is already in progress".to_string());
        }
        self.cancel_flag.store(false, Ordering::SeqCst);
        let render_id = self.next_render_id.fetch_add(1, Ordering::SeqCst) as u64 + 1;
        if let Ok(mut progress) = self.progress.lock() {
            *progress = RenderProgress {
                render_id,
                current: 0,
                total: total_frames,
                encoded: 0,
                status: "rendering".to_string(),
                message: message.to_string(),
                estimated_seconds_remaining: None,
                filename: None,
            };
        }
        Ok(render_id)
    }

    pub fn set_frame_progress(
        &self,
        current: u32,
        total: u32,
        encoded: u32,
        estimate: Option<u64>,
    ) {
        if let Ok(mut progress) = self.progress.lock() {
            progress.current = current;
            progress.total = total;
            progress.encoded = encoded;
            progress.estimated_seconds_remaining = estimate;
            progress.message = if current >= total {
                "Encoding output file...".to_string()
            } else {
                "Rendering frames...".to_string()
            };
        }
    }

    pub fn finish_success(&self, filename: String) {
        if let Ok(mut progress) = self.progress.lock() {
            progress.current = progress.total;
            progress.encoded = progress.total;
            progress.status = "complete".to_string();
            progress.message = "Video rendered successfully".to_string();
            progress.estimated_seconds_remaining = Some(0);
            progress.filename = Some(filename);
        }
        self.running.store(false, Ordering::SeqCst);
        self.cancel_flag.store(false, Ordering::SeqCst);
    }

    pub fn finish_error(&self, error: String, cancelled: bool) {
        if let Ok(mut progress) = self.progress.lock() {
            progress.status = if cancelled {
                "cancelled".to_string()
            } else {
                "error".to_string()
            };
            progress.message = if cancelled {
                "Rendering cancelled".to_string()
            } else {
                error
            };
            progress.estimated_seconds_remaining = None;
            progress.filename = None;
        }
        self.running.store(false, Ordering::SeqCst);
        self.cancel_flag.store(false, Ordering::SeqCst);
    }
}

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

struct FrameBuffer {
    pixels: Vec<u8>,
}

pub fn render_video(
    paths: &AppPaths,
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    controller: &RenderController,
) -> Result<String, String> {
    let ffmpeg_settings = finalize_ffmpeg_settings(build_ffmpeg_settings(&config.scene.ffmpeg)?);
    let width = make_even(config.scene.width.unwrap_or(1920));
    let height = make_even(config.scene.height.unwrap_or(1080));
    let total_frames = dense_activity.frame_count as u32;
    let debug_dir = create_debug_dir(paths, "phase_6")?;
    let (prepared_preview_assets, label_cache_status, prepare_timings, prepare_total_ms) =
        prepare_preview_assets(paths, config, activity, dense_activity)?;
    write_prepare_summary(
        &debug_dir,
        prepare_total_ms,
        &prepare_timings,
        label_cache_status,
    )?;

    let public_filename = format!(
        "video_{}.{}",
        timestamp_seconds()?,
        ffmpeg_settings.extension
    );
    let output_path = paths.public_dir.join(&public_filename);
    let ffmpeg_bin = resolve_ffmpeg_binary(&paths.repo_root)?;
    let input_pix_fmt = ffmpeg_input_pix_fmt(&ffmpeg_settings.codec);
    let encoded_frames = Arc::new(AtomicU32::new(0));
    let cancel_flag = controller.cancel_flag.clone();
    let mut aggregate_profiler = RenderProfiler::default();
    let render_started = Instant::now();

    let frame_byte_len = (width as usize) * (height as usize) * 4;
    let (sender, receiver) = mpsc::sync_channel::<FrameBuffer>(FRAME_QUEUE_SIZE);
    let (free_sender, free_receiver) = mpsc::sync_channel::<FrameBuffer>(FRAME_QUEUE_SIZE + 1);
    for _ in 0..(FRAME_QUEUE_SIZE + 1) {
        free_sender
            .send(FrameBuffer {
                pixels: vec![0u8; frame_byte_len],
            })
            .map_err(|_| "Failed to initialize frame buffer pool".to_string())?;
    }
    let mut child = spawn_ffmpeg_process(
        &ffmpeg_bin,
        &ffmpeg_settings,
        &output_path,
        width,
        height,
        config.scene.fps,
        &input_pix_fmt,
    )?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Failed to capture ffmpeg stderr".to_string())?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Failed to capture ffmpeg stdin".to_string())?;
    let encoded_frames_for_monitor = encoded_frames.clone();
    let monitor_thread = thread::spawn(move || monitor_ffmpeg(stderr, encoded_frames_for_monitor));
    let cancel_flag_for_writer = cancel_flag.clone();
    let writer_thread =
        thread::spawn(move || writer_worker(stdin, receiver, free_sender, cancel_flag_for_writer));

    let sample_frame_indices = if render_sample_frames_enabled() {
        sample_frame_indices(total_frames as usize)
    } else {
        Vec::new()
    };
    let scale = config.scene.scale.unwrap_or(1.0).max(0.1);
    let mut estimator = ProgressEstimator::default();
    let mut rendered_frames = 0u32;

    for frame_index in 0..(total_frames as usize) {
        if cancel_flag.load(Ordering::SeqCst) {
            break;
        }
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            return Err(format!("ffmpeg exited unexpectedly with status {status}"));
        }

        let frame_started = Instant::now();
        let mut frame_buffer =
            acquire_frame_buffer(&free_receiver, &cancel_flag, &mut aggregate_profiler)?;
        render_frame_rgba(
            paths,
            config,
            dense_activity,
            &prepared_preview_assets.prepared_assets,
            frame_index,
            scale,
            None,
            RenderTarget {
                width,
                height,
                pixels: frame_buffer.pixels.as_mut_slice(),
            },
            &mut aggregate_profiler,
        )?;
        if sample_frame_indices.contains(&frame_index) {
            aggregate_profiler.measure("debug.sample_frame_write", || {
                write_sample_frame(
                    &ffmpeg_bin,
                    &debug_dir,
                    width,
                    height,
                    frame_buffer.pixels.as_slice(),
                    frame_index,
                    &input_pix_fmt,
                )
            })?;
        }
        queue_frame(&sender, frame_buffer, &cancel_flag, &mut aggregate_profiler)?;
        rendered_frames += 1;
        let frame_ms = frame_started.elapsed().as_secs_f64() * 1000.0;
        aggregate_profiler.record_ms("frame.total", frame_ms);
        let estimate = estimator.record(rendered_frames, total_frames, frame_ms / 1000.0);
        controller.set_frame_progress(
            rendered_frames,
            total_frames,
            encoded_frames.load(Ordering::SeqCst),
            estimate,
        );
    }

    drop(sender);
    let writer_result = writer_thread
        .join()
        .map_err(|_| "Encoder writer thread panicked".to_string())??;
    let _monitor_result = monitor_thread
        .join()
        .map_err(|_| "FFmpeg monitor thread panicked".to_string())?;
    let status = child.wait().map_err(|error| error.to_string())?;

    if cancel_flag.load(Ordering::SeqCst) {
        let _ = child.kill();
        let _ = fs::remove_file(&output_path);
        return Err("Rendering cancelled".to_string());
    }
    if !status.success() {
        let _ = fs::remove_file(&output_path);
        return Err(format!("ffmpeg encoding failed ({status})"));
    }
    if writer_result.written_frames != total_frames {
        let _ = fs::remove_file(&output_path);
        return Err(format!(
            "ffmpeg encode pipeline ended early: wrote {} of {} frames",
            writer_result.written_frames, total_frames
        ));
    }

    let total_time_taken = render_started.elapsed().as_secs_f64();
    let merged_timings = merge_timing_maps(aggregate_profiler.summary(), writer_result.timings);
    write_timing_summary_with_phase(
        &debug_dir,
        config,
        &output_path,
        "phase_6",
        total_frames,
        rendered_frames,
        total_time_taken,
        sample_frame_indices,
        merged_timings,
    )?;
    if let Err(error) = copy_output_to_downloads(paths, &output_path, &public_filename) {
        eprintln!("{error}");
    }
    Ok(public_filename)
}

fn finalize_ffmpeg_settings(
    mut ffmpeg_settings: crate::encode::ffmpeg::FfmpegSettings,
) -> crate::encode::ffmpeg::FfmpegSettings {
    if ffmpeg_settings.codec == "prores_ks_vulkan"
        && !ffmpeg_settings.output_args.iter().any(|value| value == "-async_depth")
    {
        ffmpeg_settings.output_args.push("-async_depth".to_string());
        ffmpeg_settings.output_args.push("4".to_string());
    }
    ffmpeg_settings
}

fn queue_frame(
    sender: &SyncSender<FrameBuffer>,
    frame_buffer: FrameBuffer,
    cancel_flag: &AtomicBool,
    profiler: &mut RenderProfiler,
) -> Result<(), String> {
    let started = Instant::now();
    let mut payload = frame_buffer;
    loop {
        if cancel_flag.load(Ordering::SeqCst) {
            return Err("Rendering cancelled".to_string());
        }
        match sender.try_send(payload) {
            Ok(()) => {
                profiler.record_ms("queue.put_wait", started.elapsed().as_secs_f64() * 1000.0);
                return Ok(());
            }
            Err(TrySendError::Full(returned_payload)) => {
                payload = returned_payload;
                thread::sleep(Duration::from_millis(10));
            }
            Err(TrySendError::Disconnected(_)) => {
                return Err("Encoder queue disconnected".to_string());
            }
        }
    }
}

fn spawn_ffmpeg_process(
    ffmpeg_bin: &Path,
    ffmpeg_settings: &crate::encode::ffmpeg::FfmpegSettings,
    output_path: &Path,
    width: u32,
    height: u32,
    fps: f64,
    input_pix_fmt: &str,
) -> Result<std::process::Child, String> {
    let mut command = Command::new(ffmpeg_bin);
    command.arg("-loglevel").arg(&ffmpeg_settings.loglevel);

    if !ffmpeg_settings.hw_init_args.is_empty() {
        command.args(&ffmpeg_settings.hw_init_args);
    }

    command
        .arg("-f")
        .arg("rawvideo")
        .arg("-s")
        .arg(format!("{width}x{height}"))
        .arg("-pix_fmt")
        .arg(input_pix_fmt)
        .arg("-r")
        .arg(fps.to_string())
        .arg("-i")
        .arg("-");

    if let Some(filters) = &ffmpeg_settings.filters {
        command.arg("-vf").arg(filters);
    }

    command
        .args(&ffmpeg_settings.output_args)
        .args(
            ffmpeg_settings
                .muxer
                .iter()
                .flat_map(|muxer| ["-f".to_string(), muxer.clone()]),
        )
        .arg("-pix_fmt")
        .arg(&ffmpeg_settings.pix_fmt)
        .arg("-y")
        .arg(output_path)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .stdout(Stdio::null());

    command
        .spawn()
        .map_err(|error| format!("Could not start ffmpeg: {error}"))
}

struct WriterResult {
    written_frames: u32,
    timings: BTreeMap<String, TimingBucket>,
}

fn writer_worker(
    mut stdin: std::process::ChildStdin,
    receiver: Receiver<FrameBuffer>,
    free_sender: SyncSender<FrameBuffer>,
    cancel_flag: Arc<AtomicBool>,
) -> Result<WriterResult, String> {
    let mut profiler = RenderProfiler::default();
    let mut written_frames = 0u32;
    loop {
        let queue_started = Instant::now();
        let frame = match receiver.recv() {
            Ok(frame) => {
                profiler.record_ms(
                    "encoder.queue_wait",
                    queue_started.elapsed().as_secs_f64() * 1000.0,
                );
                frame
            }
            Err(_) => {
                profiler.record_ms(
                    "encoder.queue_wait",
                    queue_started.elapsed().as_secs_f64() * 1000.0,
                );
                break;
            }
        };
        if cancel_flag.load(Ordering::SeqCst) {
            break;
        }
        let write_started = Instant::now();
        stdin
            .write_all(frame.pixels.as_slice())
            .map_err(|error| format!("Failed writing frame to ffmpeg: {error}"))?;
        profiler.record_ms(
            "ffmpeg.write",
            write_started.elapsed().as_secs_f64() * 1000.0,
        );
        written_frames += 1;
        let release_started = Instant::now();
        free_sender
            .send(frame)
            .map_err(|_| "Frame buffer pool disconnected".to_string())?;
        profiler.record_ms(
            "buffer.release_wait",
            release_started.elapsed().as_secs_f64() * 1000.0,
        );
    }
    stdin.flush().map_err(|error| error.to_string())?;
    Ok(WriterResult {
        written_frames,
        timings: profiler.summary(),
    })
}

fn monitor_ffmpeg(stderr: std::process::ChildStderr, encoded_frames: Arc<AtomicU32>) {
    let reader = BufReader::new(stderr);
    for line in reader.lines().map_while(Result::ok) {
        if let Some(frame_index) = parse_ffmpeg_frame(&line) {
            encoded_frames.store(frame_index, Ordering::SeqCst);
        }
    }
}

fn parse_ffmpeg_frame(line: &str) -> Option<u32> {
    let marker = "frame=";
    let start = line.find(marker)? + marker.len();
    let digits = line[start..]
        .chars()
        .skip_while(|ch| ch.is_whitespace())
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    digits.parse::<u32>().ok()
}

fn write_prepare_summary(
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

fn write_timing_summary_with_phase(
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

fn merge_timing_maps(
    mut left: BTreeMap<String, TimingBucket>,
    right: BTreeMap<String, TimingBucket>,
) -> BTreeMap<String, TimingBucket> {
    for (name, bucket) in right {
        let entry = left.entry(name).or_default();
        entry.count += bucket.count;
        entry.total_ms += bucket.total_ms;
        entry.avg_ms = if entry.count == 0 {
            0.0
        } else {
            entry.total_ms / f64::from(entry.count)
        };
        entry.max_ms = entry.max_ms.max(bucket.max_ms);
    }
    left
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

fn copy_output_to_downloads(
    paths: &AppPaths,
    output_path: &Path,
    filename: &str,
) -> Result<(), String> {
    let destination = paths.downloads_dir.join(filename);
    fs::copy(output_path, &destination)
        .map_err(|error| format!("Failed to copy {}: {error}", destination.display()))?;
    Ok(())
}

fn create_debug_dir(paths: &AppPaths, phase: &str) -> Result<PathBuf, String> {
    let dir = paths
        .repo_root
        .join("backend")
        .join("debug_render")
        .join(phase)
        .join(timestamp_slug()?);
    fs::create_dir_all(&dir)
        .map_err(|error| format!("Failed to create {}: {error}", dir.display()))?;
    Ok(dir)
}

fn timestamp_slug() -> Result<String, String> {
    Ok(timestamp_seconds()?.to_string())
}

fn timestamp_seconds() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| error.to_string())
}

fn sample_frame_indices(total_frames: usize) -> Vec<usize> {
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

fn render_sample_frames_enabled() -> bool {
    matches!(
        std::env::var("CYCLEMETRY_SAMPLE_FRAMES").ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "YES")
    )
}

fn acquire_frame_buffer(
    receiver: &Receiver<FrameBuffer>,
    cancel_flag: &AtomicBool,
    profiler: &mut RenderProfiler,
) -> Result<FrameBuffer, String> {
    let started = Instant::now();
    loop {
        if cancel_flag.load(Ordering::SeqCst) {
            return Err("Rendering cancelled".to_string());
        }
        match receiver.recv_timeout(Duration::from_millis(25)) {
            Ok(buffer) => {
                profiler.record_ms(
                    "buffer.acquire_wait",
                    started.elapsed().as_secs_f64() * 1000.0,
                );
                return Ok(buffer);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Err("Frame buffer pool disconnected".to_string());
            }
        }
    }
}

fn write_sample_frame(
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

fn make_even(value: u32) -> u32 {
    if value % 2 == 0 {
        value
    } else {
        value + 1
    }
}

fn ffmpeg_input_pix_fmt(codec: &str) -> String {
    std::env::var("CYCLEMETRY_INPUT_PIX_FMT").unwrap_or_else(|_| {
        let _ = codec;
        "rgba".to_string()
    })
}

#[derive(Default)]
struct ProgressEstimator {
    ema_seconds_per_frame: Option<f64>,
}

impl ProgressEstimator {
    fn record(&mut self, current: u32, total: u32, frame_seconds: f64) -> Option<u64> {
        self.ema_seconds_per_frame = Some(match self.ema_seconds_per_frame {
            Some(previous) => previous * 0.85 + frame_seconds * 0.15,
            None => frame_seconds,
        });
        let remaining = total.saturating_sub(current);
        self.ema_seconds_per_frame
            .map(|avg| (avg * remaining as f64).max(0.0).round() as u64)
    }
}
