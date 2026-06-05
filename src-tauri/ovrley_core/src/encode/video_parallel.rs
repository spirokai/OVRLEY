//! Parallel render orchestration helpers.
//!
//! This module owns the benchmark-oriented multi-render path. It launches
//! independent full renders for multiple configs, collects their outputs in
//! order, and stitches them afterward. Normal single-render dispatch still
//! lives behind the public facade in `encode::video`.

use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::encode::ffmpeg::resolve_ffmpeg_binary;
use crate::encode::video::render_video;
use crate::encode::video_debug::{concat_video_segments, timestamp_nanos};
use crate::encode::video_pipeline::rendered_frame_count;
use crate::error::{CoreError, CoreResult};
use crate::normalize::ValidatedRenderConfig;
use crate::paths::AppPaths;
use std::collections::VecDeque;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub use crate::encode::progress::RenderController;

/// Renders multiple configs concurrently and stitches their outputs.
///
/// This is primarily a diagnostic and benchmark helper. Each job receives an
/// independent controller, then ffmpeg concatenates the produced files.
///
/// # Phases
/// 1. Validate config/report count match
/// 2. Distribute work across worker threads via a shared queue
/// 3. Collect results in order while workers produce independently
/// 4. Join all worker threads
/// 5. Stitch produced output files with ffmpeg concat demuxer (stream copy)
pub fn run_parallel_renders(
    paths: &AppPaths,
    configs: Vec<ValidatedRenderConfig>,
    activity: &ParsedActivity,
    reports: Vec<DenseActivityReport>,
) -> CoreResult<Duration> {
    // ── PHASE 1: VALIDATE INPUTS ──
    if configs.len() != reports.len() {
        return Err(CoreError::Encode(
            "Configs and reports vectors must have the same length".to_string(),
        ));
    }

    let start_time = Instant::now();
    let total_jobs = configs.len();
    let worker_count = estimate_parallel_render_worker_count(total_jobs);

    // ── PHASE 2: DISTRIBUTE WORK — shared queue across worker threads ──
    let work_queue = Arc::new(Mutex::new(
        configs
            .into_iter()
            .zip(reports)
            .enumerate()
            .collect::<VecDeque<_>>(),
    ));
    let (result_tx, result_rx) = mpsc::channel::<(usize, CoreResult<String>)>();
    let mut handles = Vec::new();

    for _ in 0..worker_count {
        let paths_clone = paths.clone();
        let activity_clone = activity.clone();
        let work_queue_clone = work_queue.clone();
        let result_tx_clone = result_tx.clone();
        let handle = thread::spawn(move || loop {
            let next_job = {
                let mut queue = match work_queue_clone.lock() {
                    Ok(queue) => queue,
                    Err(_) => return,
                };
                queue.pop_front()
            };
            let Some((index, (config, report))) = next_job else {
                return;
            };

            let controller = RenderController::default();
            let start_result = controller.try_start(
                rendered_frame_count(report.frame_count, config.widget_update_rate() as usize)
                    as u32,
                &format!("Parallel Render {}", index + 1),
            );
            let result = match start_result {
                Ok(_) => render_video(&paths_clone, &config, &activity_clone, &report, &controller),
                Err(error) => Err(error),
            };
            let _ = result_tx_clone.send((index, result));
        });
        handles.push(handle);
    }
    drop(result_tx);

    // ── PHASE 3: COLLECT RESULTS IN ORDER ──
    let mut filenames = vec![None; total_jobs];
    for _ in 0..filenames.len() {
        let (index, result) = result_rx.recv().map_err(|_| {
            CoreError::Encode("Parallel render worker channel disconnected".to_string())
        })?;
        filenames[index] = Some(result?);
    }

    // ── PHASE 4: JOIN ALL WORKER THREADS ──
    for handle in handles {
        handle
            .join()
            .map_err(|_| CoreError::Encode("Parallel render thread panicked".to_string()))?;
    }

    // ── PHASE 5: STITCH OUTPUTS WITH FFMPEG CONCAT DEMUXER ──
    let ffmpeg_bin = resolve_ffmpeg_binary(&paths.repo_root)?;
    let output_filename = format!("parallel_stitch_{}.mov", timestamp_nanos()?);
    let output_path = paths.downloads_dir.join(&output_filename);
    let filenames = filenames
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            CoreError::Encode(
                "Parallel render finished without producing all output filenames".to_string(),
            )
        })?;
    concat_video_segments(paths, &ffmpeg_bin, &filenames, &output_path)?;

    Ok(start_time.elapsed())
}

/// Estimates a conservative number of parallel render workers for the machine.
pub(crate) fn estimate_parallel_render_worker_count(total_jobs: usize) -> usize {
    // Rendering is CPU and memory heavy, so use a conservative fraction of
    // available logical cores instead of saturating the machine.
    let logical_cores = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(4);
    let worker_count = (logical_cores / 4).clamp(1, 4);
    worker_count.min(total_jobs.max(1))
}

/// Estimates how many composite segments are safe for the selected codec.
pub(crate) fn estimate_composite_segment_count(total_jobs: usize, codec: &str) -> usize {
    let worker_count = estimate_parallel_render_worker_count(total_jobs);
    if matches!(codec, "h264_amf" | "hevc_amf") {
        worker_count.min(2)
    } else {
        worker_count
    }
}
