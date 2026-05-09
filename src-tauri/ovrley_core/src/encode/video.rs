use crate::activity::build_dense_activity_report;
use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::commands::AppPaths;
use crate::config::RenderConfig;
use crate::debug::RenderProgress;
use crate::encode::ffmpeg::resolve_ffmpeg_binary;
use crate::encode::video_debug::{concat_video_segments, timestamp_nanos, write_stitch_summary};
use crate::encode::video_pipeline::{render_video_single, rendered_frame_count};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

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

    pub(crate) fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancel_flag.clone()
    }
}

pub fn run_parallel_renders(
    paths: &AppPaths,
    configs: Vec<RenderConfig>,
    activity: &ParsedActivity,
    reports: Vec<DenseActivityReport>,
) -> Result<Duration, String> {
    if configs.len() != reports.len() {
        return Err("Configs and reports vectors must have the same length".to_string());
    }

    let start_time = Instant::now();
    let total_jobs = configs.len();
    let worker_count = estimate_parallel_render_worker_count(total_jobs);
    let work_queue = Arc::new(Mutex::new(
        configs
            .into_iter()
            .zip(reports)
            .enumerate()
            .collect::<VecDeque<_>>(),
    ));
    let (result_tx, result_rx) = mpsc::channel::<(usize, Result<String, String>)>();
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

    let mut filenames = vec![None; total_jobs];
    for _ in 0..filenames.len() {
        let (index, result) = result_rx
            .recv()
            .map_err(|_| "Parallel render worker channel disconnected".to_string())?;
        filenames[index] = Some(result?);
    }

    for handle in handles {
        handle
            .join()
            .map_err(|_| "Parallel render thread panicked".to_string())?;
    }

    let ffmpeg_bin = resolve_ffmpeg_binary(&paths.repo_root)?;
    let output_filename = format!("parallel_stitch_{}.mov", timestamp_nanos()?);
    let output_path = paths.downloads_dir.join(&output_filename);
    let filenames = filenames
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            "Parallel render finished without producing all output filenames".to_string()
        })?;
    concat_video_segments(paths, &ffmpeg_bin, &filenames, &output_path)?;

    Ok(start_time.elapsed())
}

fn estimate_parallel_render_worker_count(total_jobs: usize) -> usize {
    let logical_cores = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(4);
    let worker_count = (logical_cores / 4).max(1);
    worker_count.min(total_jobs.max(1))
}

pub fn render_video(
    paths: &AppPaths,
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    controller: &RenderController,
) -> Result<String, String> {
    if should_parallelize_qtrle(config, dense_activity) {
        return render_video_segmented_qtrle(paths, config, activity, dense_activity, controller);
    }
    render_video_single(paths, config, activity, dense_activity, controller)
}

fn should_parallelize_qtrle(config: &RenderConfig, dense_activity: &DenseActivityReport) -> bool {
    config
        .scene
        .ffmpeg
        .as_object()
        .and_then(|map| map.get("codec"))
        .and_then(serde_json::Value::as_str)
        .map(|codec| codec == "qtrle")
        .unwrap_or(false)
        && integer_second_duration(config).unwrap_or(0) >= 2
        && dense_activity.frame_count >= 2
}

fn render_video_segmented_qtrle(
    paths: &AppPaths,
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    controller: &RenderController,
) -> Result<String, String> {
    let Some(total_seconds) = integer_second_duration(config) else {
        return render_video_single(paths, config, activity, dense_activity, controller);
    };
    let segment_count = estimate_parallel_render_worker_count(total_seconds as usize);
    if segment_count < 2 {
        return render_video_single(paths, config, activity, dense_activity, controller);
    }

    let mut segment_configs = Vec::with_capacity(segment_count);
    let mut segment_reports = Vec::with_capacity(segment_count);
    for (segment_start, segment_end) in integer_second_windows(config, total_seconds, segment_count)
    {
        let mut segment_config = config.clone();
        segment_config.scene.start = segment_start;
        segment_config.scene.end = segment_end;
        let segment_dense = build_dense_activity_report(activity, &segment_config)?;
        if segment_dense.frame_count == 0 {
            continue;
        }
        segment_configs.push(segment_config);
        segment_reports.push(segment_dense);
    }

    let actual_segment_count = segment_configs.len();
    if actual_segment_count < 2 {
        return render_video_single(paths, config, activity, dense_activity, controller);
    }
    let combined_frames = segment_reports
        .iter()
        .map(|report| {
            rendered_frame_count(report.frame_count, config.widget_update_rate() as usize) as u32
        })
        .sum::<u32>();

    let child_cancel_flag = controller.cancel_flag();
    let segment_controllers = segment_reports
        .iter()
        .map(|report| {
            child_render_controller(
                rendered_frame_count(report.frame_count, config.widget_update_rate() as usize)
                    as u32,
                &child_cancel_flag,
            )
        })
        .collect::<Vec<_>>();

    enum SegmentEvent {
        Completed(usize, Result<String, String>),
    }

    let (tx, rx) = mpsc::channel::<SegmentEvent>();
    let mut handles = Vec::with_capacity(actual_segment_count);
    for index in 0..actual_segment_count {
        let tx = tx.clone();
        let segment_controller = segment_controllers[index].clone();
        let segment_paths = paths.clone();
        let segment_config = segment_configs[index].clone();
        let segment_activity = activity.clone();
        let segment_dense = segment_reports[index].clone();
        let handle = thread::spawn(move || {
            let result = render_video_single(
                &segment_paths,
                &segment_config,
                &segment_activity,
                &segment_dense,
                &segment_controller,
            );
            let _ = tx.send(SegmentEvent::Completed(index, result));
        });
        handles.push(handle);
    }
    drop(tx);

    let mut results = vec![None; actual_segment_count];
    let mut completed = 0usize;
    let mut first_error: Option<String> = None;

    while completed < actual_segment_count {
        let progress_snapshots = segment_controllers
            .iter()
            .map(RenderController::progress)
            .collect::<Vec<_>>();
        let current = progress_snapshots
            .iter()
            .map(|progress| progress.current)
            .sum::<u32>();
        let encoded = progress_snapshots
            .iter()
            .map(|progress| progress.encoded)
            .sum::<u32>();
        let estimate = progress_snapshots
            .iter()
            .filter_map(|progress| progress.estimated_seconds_remaining)
            .max();
        controller.set_frame_progress(current, combined_frames, encoded, estimate);

        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(SegmentEvent::Completed(index, Ok(filename))) => {
                results[index] = Some(filename);
                completed += 1;
            }
            Ok(SegmentEvent::Completed(_, Err(error))) => {
                if first_error.is_none() {
                    first_error = Some(error);
                    controller.cancel_flag().store(true, Ordering::SeqCst);
                }
                completed += 1;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    for handle in handles {
        handle
            .join()
            .map_err(|_| "Segmented qtrle render thread panicked".to_string())?;
    }

    if let Some(error) = first_error {
        cleanup_segment_outputs(paths, &results);
        return Err(error);
    }

    if controller.cancel_flag().load(Ordering::SeqCst) {
        cleanup_segment_outputs(paths, &results);
        return Err("Rendering cancelled".to_string());
    }

    let segment_filenames = results
        .iter()
        .cloned()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            cleanup_segment_outputs(paths, &results);
            "Segmented qtrle render did not produce all output files".to_string()
        })?;

    controller.set_frame_progress(combined_frames, combined_frames, combined_frames, Some(0));

    let ffmpeg_bin = resolve_ffmpeg_binary(&paths.repo_root)?;
    let public_filename = format!("video_{}.mov", timestamp_nanos()?);
    let output_path = paths.downloads_dir.join(&public_filename);
    let concat_started = Instant::now();
    if let Err(error) = concat_video_segments(paths, &ffmpeg_bin, &segment_filenames, &output_path)
    {
        cleanup_segment_outputs(paths, &results);
        return Err(error);
    }
    let concat_duration_ms = concat_started.elapsed().as_secs_f64() * 1000.0;
    if let Err(error) = write_stitch_summary(
        paths,
        config,
        &public_filename,
        concat_duration_ms,
        &segment_configs,
        &segment_reports,
        &segment_filenames,
    ) {
        cleanup_segment_outputs(paths, &results);
        return Err(error);
    }
    cleanup_segment_outputs(paths, &results);
    Ok(public_filename)
}

fn integer_second_duration(config: &RenderConfig) -> Option<u32> {
    let start = config.scene.start.round();
    let end = config.scene.end.round();
    if (config.scene.start - start).abs() > 1e-9 || (config.scene.end - end).abs() > 1e-9 {
        return None;
    }
    if end <= start {
        return None;
    }
    Some((end - start) as u32)
}

fn integer_second_windows(
    config: &RenderConfig,
    total_seconds: u32,
    segment_count: usize,
) -> Vec<(f64, f64)> {
    let actual_segment_count = segment_count.min(total_seconds as usize).max(1);
    let base_seconds = total_seconds / actual_segment_count as u32;
    let extra_segments = total_seconds % actual_segment_count as u32;
    let mut cursor = config.scene.start.round();
    let mut windows = Vec::with_capacity(actual_segment_count);

    for index in 0..actual_segment_count {
        let segment_seconds = base_seconds + u32::from((index as u32) < extra_segments);
        let next_cursor = cursor + f64::from(segment_seconds);
        windows.push((cursor, next_cursor));
        cursor = next_cursor;
    }

    windows
}

fn child_render_controller(total_frames: u32, cancel_flag: &Arc<AtomicBool>) -> RenderController {
    let controller = RenderController {
        progress: Arc::new(Mutex::new(RenderProgress::default())),
        cancel_flag: cancel_flag.clone(),
        running: Arc::new(AtomicBool::new(false)),
        next_render_id: Arc::new(AtomicU32::new(0)),
    };
    let _ = controller.try_start(total_frames, "Segment render");
    controller
}

fn cleanup_segment_outputs(paths: &AppPaths, results: &[Option<String>]) {
    for filename in results.iter().flatten() {
        let _ = std::fs::remove_file(paths.downloads_dir.join(filename));
    }
}
