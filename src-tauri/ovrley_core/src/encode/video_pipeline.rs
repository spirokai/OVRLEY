use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::commands::AppPaths;
use crate::config::RenderConfig;
use crate::debug::{RenderProfiler, TimingBucket};
use crate::encode::ffmpeg::{build_ffmpeg_settings, resolve_ffmpeg_binary, suppress_child_console};
use crate::encode::video::RenderController;
use crate::encode::video_debug::{
    create_debug_dir, render_sample_frames_enabled, sample_frame_indices, write_prepare_summary,
    write_sample_frame, write_timing_summary_with_phase,
};
use crate::render::{prepare_preview_assets, render_frame_rgba, RenderTarget};
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const FRAME_QUEUE_SIZE: usize = 12;

struct FrameBuffer {
    pixels: Vec<u8>,
}

struct WriterResult {
    written_frames: u32,
    timings: BTreeMap<String, TimingBucket>,
}

#[derive(Default)]
struct ProgressEstimator {
    ema_seconds_per_frame: Option<f64>,
}

pub(crate) fn render_video_single(
    paths: &AppPaths,
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    controller: &RenderController,
) -> Result<String, String> {
    let ffmpeg_settings = finalize_ffmpeg_settings(build_ffmpeg_settings(&config.scene.ffmpeg)?);
    let width = make_even(config.scene.width.unwrap_or(1920));
    let height = make_even(config.scene.height.unwrap_or(1080));
    let layout_total_frames = dense_activity.frame_count as u32;
    let update_rate = config.widget_update_rate() as usize;
    let total_frames = rendered_frame_count(dense_activity.frame_count, update_rate) as u32;
    let container_fps = config.container_fps();
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
        crate::encode::video_debug::timestamp_nanos()?,
        ffmpeg_settings.extension
    );
    let output_path = paths.downloads_dir.join(&public_filename);
    let ffmpeg_bin = resolve_ffmpeg_binary(&paths.repo_root)?;
    let input_pix_fmt = ffmpeg_input_pix_fmt();
    let encoded_frames = Arc::new(AtomicU32::new(0));
    let cancel_flag = controller.cancel_flag();
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
        container_fps,
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

    let sample_frames = if render_sample_frames_enabled() {
        sample_frame_indices(total_frames as usize)
    } else {
        Vec::new()
    };
    let scale = config.scene.scale.unwrap_or(1.0).max(0.1);
    let mut estimator = ProgressEstimator::default();
    let mut rendered_frames = 0u32;
    let render_result = (|| -> Result<(), String> {
        for output_frame_index in 0..(total_frames as usize) {
            if cancel_flag.load(Ordering::SeqCst) {
                break;
            }
            if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                return Err(format!("ffmpeg exited unexpectedly with status {status}"));
            }

            let frame_started = Instant::now();
            let frame_index = source_frame_index(output_frame_index, update_rate, dense_activity);
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
            if sample_frames.contains(&output_frame_index) {
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
        Ok(())
    })();

    drop(sender);
    let writer_result = writer_thread
        .join()
        .map_err(|_| "Encoder writer thread panicked".to_string())??;
    let _monitor_result = monitor_thread
        .join()
        .map_err(|_| "FFmpeg monitor thread panicked".to_string())?;
    let status = child.wait().map_err(|error| error.to_string())?;

    if let Err(error) = render_result {
        let _ = fs::remove_file(&output_path);
        return Err(error);
    }
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
        layout_total_frames,
        rendered_frames,
        total_time_taken,
        sample_frames,
        merged_timings,
    )?;
    Ok(public_filename)
}

fn finalize_ffmpeg_settings(
    mut ffmpeg_settings: crate::encode::ffmpeg::FfmpegSettings,
) -> crate::encode::ffmpeg::FfmpegSettings {
    if ffmpeg_settings.codec == "prores_ks_vulkan"
        && !ffmpeg_settings
            .output_args
            .iter()
            .any(|value| value == "-async_depth")
    {
        ffmpeg_settings.output_args.push("-async_depth".to_string());
        ffmpeg_settings.output_args.push("4".to_string());
    }
    ffmpeg_settings
}

pub(crate) fn rendered_frame_count(layout_frame_count: usize, update_rate: usize) -> usize {
    if layout_frame_count == 0 {
        return 0;
    }
    let safe_update_rate = update_rate.max(1);
    ((layout_frame_count - 1) / safe_update_rate) + 1
}

fn source_frame_index(
    output_frame_index: usize,
    update_rate: usize,
    dense_activity: &DenseActivityReport,
) -> usize {
    let max_frame_index = dense_activity.frame_count.saturating_sub(1);
    output_frame_index
        .saturating_mul(update_rate.max(1))
        .min(max_frame_index)
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
    suppress_child_console(&mut command);
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

fn make_even(value: u32) -> u32 {
    if value % 2 == 0 {
        value
    } else {
        value + 1
    }
}

fn ffmpeg_input_pix_fmt() -> String {
    std::env::var("OVRLEY_INPUT_PIX_FMT").unwrap_or_else(|_| "rgba".to_string())
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
