//! Ordered parallel CPU frame production for a single FFmpeg process.
//!
//! Prepared render assets are built before this module is entered and remain
//! immutable for the lifetime of the workers. Every worker creates its own
//! Skia surface around an exclusively owned RGBA buffer; surfaces and canvases
//! are never shared between threads.
//!
//! The buffer-before-task invariant is intentional: a worker must acquire its
//! render buffer before claiming a frame index. Reversing that order can
//! deadlock ordered forwarding when an early frame waits for a buffer held by
//! workers that claimed later frames.

use super::frame_pool::{ParallelFrameProgress, ParallelFrameRenderResult, MAX_FRAME_WORKERS};
use super::lifecycle::{PipelineKind, PipelineShutdown};
use crate::debug::{RenderProfiler, TimingBucket};
use crate::encode::pipeline::queue::{merge_timing_maps, queue_frame, FrameBuffer};
use crate::encode::progress::{ProgressEstimator, RenderController};
use crate::error::{CoreError, CoreResult};
use crate::render::VideoFrameRenderer;
use std::collections::BTreeMap;
use std::num::{NonZeroU32, NonZeroUsize};
use std::process::Child;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Supported task count and dense-index stride, checked once during planning.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameProductionPlan {
    count: u32,
    stride: NonZeroU32,
}

impl FrameProductionPlan {
    pub(crate) fn new(count: u64, stride: NonZeroU32) -> CoreResult<Self> {
        let count = u32::try_from(count)
            .ok()
            .filter(|count| *count > 0)
            .ok_or_else(|| CoreError::Encode("Frame count must be positive and fit u32".into()))?;
        let last_index = u64::from(count - 1) * u64::from(stride.get());
        // Task allocation may advance once past the end for each worker.
        usize::try_from(last_index.max(u64::from(count) + MAX_FRAME_WORKERS as u64))
            .map_err(|_| CoreError::Encode("Frame task indices exceed usize".into()))?;
        Ok(Self { count, stride })
    }

    pub(crate) fn decimated(layout_count: u64, stride: NonZeroU32) -> CoreResult<Self> {
        let layout = Self::new(layout_count, NonZeroU32::MIN)?;
        Ok(Self {
            count: (layout.count - 1) / stride.get() + 1,
            stride,
        })
    }

    pub(crate) fn count(self) -> u32 {
        self.count
    }

    pub(crate) fn stride(self) -> NonZeroU32 {
        self.stride
    }
}

struct OrderedFrames<T> {
    total_frames: u64,
    next_index: u64,
    pending: BTreeMap<u64, T>,
}

impl<T> OrderedFrames<T> {
    fn new(total_frames: u64) -> Self {
        Self {
            total_frames,
            next_index: 0,
            pending: BTreeMap::new(),
        }
    }

    fn insert(&mut self, index: u64, frame: T) -> CoreResult<Vec<T>> {
        if index >= self.total_frames {
            return Err(CoreError::Encode(format!(
                "parallel render produced out-of-range frame index {index}; expected 0..{}",
                self.total_frames
            )));
        }
        if index < self.next_index || self.pending.contains_key(&index) {
            return Err(CoreError::Encode(format!(
                "parallel render produced duplicate frame index {index}"
            )));
        }

        self.pending.insert(index, frame);
        let mut ready = Vec::new();
        while let Some(frame) = self.pending.remove(&self.next_index) {
            ready.push(frame);
            self.next_index += 1;
        }
        Ok(ready)
    }
}

struct CompletedFrame {
    dense_frame_index: usize,
    buffer: FrameBuffer,
    completed_at: Instant,
}

/// Wakes the coordinator and other workers if a frame worker unwinds.
struct FrameWorkerFailureGuard<'a>(&'a PipelineShutdown);

impl Drop for FrameWorkerFailureGuard<'_> {
    fn drop(&mut self) {
        if thread::panicking() {
            self.0.signal_failure(CoreError::Render(
                "Parallel frame render worker panicked".into(),
            ));
        }
    }
}

enum WorkerEvent {
    Rendered {
        output_frame_index: u64,
        frame: CompletedFrame,
    },
    Failed {
        error: CoreError,
    },
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_frames_parallel(
    renderer: VideoFrameRenderer<'_>,
    plan: FrameProductionPlan,
    workers: NonZeroUsize,
    progress: ParallelFrameProgress<'_>,
    pipeline: PipelineKind,
    controller: &RenderController,
    shutdown: &PipelineShutdown,
    frame_sender: &SyncSender<FrameBuffer>,
    ordered_frame_observer: Option<&dyn Fn(u64, usize, &FrameBuffer) -> CoreResult<()>>,
    free_receiver: Receiver<FrameBuffer>,
    ffmpeg_child: &mut Child,
    render_started: Instant,
) -> CoreResult<ParallelFrameRenderResult> {
    let frame_count = plan.count() as usize;
    let dense_frame_stride = plan.stride();
    let mut prewarm_profiler = RenderProfiler::default();
    let prewarmed_frame =
        prewarm_first_frame(renderer, &free_receiver, shutdown, &mut prewarm_profiler)?;
    let next_task = AtomicUsize::new(1);
    let free_receiver = Arc::new(Mutex::new(free_receiver));
    let (result_sender, result_receiver) = std::sync::mpsc::channel::<WorkerEvent>();

    let timings = thread::scope(|scope| -> CoreResult<BTreeMap<String, TimingBucket>> {
        let mut handles = Vec::with_capacity(workers.get());
        for _ in 0..workers.get() {
            let result_sender = result_sender.clone();
            let free_receiver = Arc::clone(&free_receiver);
            let next_task = &next_task;

            handles.push(scope.spawn(move || {
                let _failure_guard = FrameWorkerFailureGuard(shutdown);
                let mut profiler = RenderProfiler::default();
                loop {
                    if shutdown.is_stopped() {
                        break;
                    }

                    let frame_started = Instant::now();
                    let mut frame_buffer = match acquire_worker_frame_buffer(
                        &free_receiver,
                        shutdown,
                        &mut profiler,
                    ) {
                        Ok(Some(frame_buffer)) => frame_buffer,
                        Ok(None) => break,
                        Err(error) => {
                            shutdown.signal_failure(CoreError::Encode(format!(
                                "Frame buffer acquisition failed: {error}"
                            )));
                            let _ = result_sender.send(WorkerEvent::Failed { error });
                            break;
                        }
                    };
                    let task_index = next_task.fetch_add(1, Ordering::SeqCst);
                    if task_index >= frame_count {
                        break;
                    }
                    let output_frame_index = task_index as u64;
                    let dense_frame_index = task_index * dense_frame_stride.get() as usize;

                    let render_result = renderer
                        .render_rgba(
                            dense_frame_index,
                            frame_buffer.pixels.as_mut_slice(),
                            &mut profiler,
                        )
                        .map(|()| frame_buffer);
                    let worker_frame_ms = frame_started.elapsed().as_secs_f64() * 1000.0;
                    profiler.record_ms("parallel.worker_frame", worker_frame_ms);
                    profiler.record_ms("frame.total", worker_frame_ms);

                    match render_result {
                        Ok(buffer) => {
                            if result_sender
                                .send(WorkerEvent::Rendered {
                                    output_frame_index,
                                    frame: CompletedFrame {
                                        dense_frame_index,
                                        buffer,
                                        completed_at: Instant::now(),
                                    },
                                })
                                .is_err()
                            {
                                break;
                            }
                        }
                        Err(CoreError::Cancelled) if shutdown.is_stopped() => break,
                        Err(error) => {
                            shutdown.signal_failure(CoreError::Encode(format!(
                                "Frame render failed: {error}"
                            )));
                            let _ = result_sender.send(WorkerEvent::Failed { error });
                            break;
                        }
                    }
                }
                profiler.summary()
            }));
        }
        result_sender.send(prewarmed_frame).map_err(|_| {
            CoreError::Encode(
                "Parallel render result channel closed during cache prewarm".to_string(),
            )
        })?;
        drop(result_sender);

        let mut coordinator_profiler = prewarm_profiler;
        let total_frames = u64::from(plan.count());
        let mut ordered_frames = OrderedFrames::new(total_frames);
        let mut written_frames = 0u64;
        let mut estimator = ProgressEstimator::default();
        let mut last_progress_at = Instant::now();
        let mut previous_progress = 0u32;

        let coordinator_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> CoreResult<()> {
            while written_frames < total_frames {
                if let Err(e) = shutdown.check() {
                    return Err(e);
                }

                let wait_started = Instant::now();
                let event = result_receiver.recv_timeout(Duration::from_millis(25));
                coordinator_profiler.record_ms(
                    "parallel.result_wait",
                    wait_started.elapsed().as_secs_f64() * 1000.0,
                );

                match event {
                    Ok(WorkerEvent::Rendered {
                        output_frame_index,
                        frame,
                    }) => {
                        if shutdown.is_stopped() {
                            continue;
                        }
                        let ready_frames = ordered_frames.insert(output_frame_index, frame)?;
                        let ready_frame_count = ready_frames.len();
                        for ready in ready_frames {
                            coordinator_profiler.record_ms(
                                "parallel.reorder_hold",
                                ready.completed_at.elapsed().as_secs_f64() * 1000.0,
                            );
                            if let Some(observer) = ordered_frame_observer {
                                observer(written_frames, ready.dense_frame_index, &ready.buffer)?;
                            }
                            queue_frame(
                                frame_sender,
                                ready.buffer,
                                shutdown,
                                &mut coordinator_profiler,
                            )?;
                            written_frames += 1;
                        }

                        if ready_frame_count > 0 {
                            let (
                                total_progress,
                                current_progress,
                                encoded_progress,
                                fps_multiplier,
                            ) = match &progress {
                                ParallelFrameProgress::Transparent(encoded_frames) => (
                                    plan.count(),
                                    written_frames as u32,
                                    encoded_frames.load(Ordering::SeqCst),
                                    dense_frame_stride,
                                ),
                                ParallelFrameProgress::Composite(plan) => {
                                    let current = plan.output_progress(written_frames);
                                    (
                                        plan.render.output_frame_count,
                                        current,
                                        0,
                                        NonZeroU32::MIN,
                                    )
                                }
                            };
                            let elapsed = last_progress_at.elapsed().as_secs_f64();
                            last_progress_at = Instant::now();
                            let output_progress_added = current_progress - previous_progress;
                            previous_progress = current_progress;
                            let output_equivalent_frame_seconds = if output_progress_added == 0 {
                                0.0
                            } else {
                                elapsed / f64::from(output_progress_added)
                            };
                            let (estimate, rendering_fps) = estimator.record(
                                current_progress,
                                total_progress,
                                output_equivalent_frame_seconds,
                                render_started.elapsed().as_secs_f64(),
                            );
                            let effective_rendering_fps =
                                rendering_fps.map(|fps| fps * f64::from(fps_multiplier.get()));
                            controller.set_frame_progress(
                                current_progress,
                                total_progress,
                                written_frames as u32,
                                encoded_progress,
                                estimate,
                                effective_rendering_fps,
                            );
                        }
                    }
                    Ok(WorkerEvent::Failed { error }) => {
                        return Err(error);
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        if let Err(e) = shutdown.check() {
                            return Err(e);
                        }
                        if let Some(status) = ffmpeg_child.try_wait().map_err(|error| {
                            CoreError::Encode(format!("ffmpeg process error: {error}"))
                        })? {
                            return Err(CoreError::Encode(format!(
                                "{} ffmpeg exited unexpectedly with status {status}",
                                pipeline
                            )));
                        }
                        if handles.iter().all(|handle| handle.is_finished()) {
                            return Err(CoreError::Encode(format!(
                                "parallel render workers ended after producing {written_frames} of {} frames",
                                frame_count
                            )));
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        if let Err(e) = shutdown.check() {
                            return Err(e);
                        }
                        return Err(CoreError::Encode(format!(
                            "parallel render result channel closed after producing {written_frames} of {} frames",
                            frame_count
                        )));
                    }
                }
            }
            Ok(())
        })).unwrap_or_else(|_| Err(CoreError::Render("Frame coordinator panicked".into())));

        if let Err(error) = &coordinator_result {
            if !matches!(error, CoreError::Cancelled) {
                shutdown.signal_failure(CoreError::Render(format!(
                    "Frame coordination stopped: {error}"
                )));
            }
        }
        let mut timings = coordinator_profiler.summary();
        let mut worker_panic = None;
        for handle in handles {
            match handle.join() {
                Ok(worker_timings) => {
                    timings = merge_timing_maps(timings, worker_timings);
                }
                Err(_) if worker_panic.is_none() => {
                    worker_panic = Some(CoreError::Render(
                        "Parallel frame render worker panicked".to_string(),
                    ));
                }
                Err(_) => {}
            }
        }
        if let Some(error) = worker_panic {
            return Err(error);
        }
        coordinator_result?;
        Ok(timings)
    })?;

    let rendered_frames = plan.count();
    let free_receiver = Arc::try_unwrap(free_receiver)
        .map_err(|_| CoreError::Encode("Parallel buffer receiver is still shared".to_string()))?
        .into_inner()
        .map_err(|_| CoreError::Encode("Frame buffer pool lock poisoned".to_string()))?;
    Ok(ParallelFrameRenderResult {
        timings,
        rendered_frames,
        free_receiver,
    })
}

fn acquire_worker_frame_buffer(
    receiver: &Mutex<Receiver<FrameBuffer>>,
    shutdown: &PipelineShutdown,
    profiler: &mut RenderProfiler,
) -> CoreResult<Option<FrameBuffer>> {
    let started = Instant::now();
    loop {
        if shutdown.is_stopped() {
            return Ok(None);
        }
        let receive_result = receiver
            .lock()
            .map_err(|_| CoreError::Encode("Frame buffer pool lock poisoned".to_string()))?
            .recv_timeout(Duration::from_millis(25));
        match receive_result {
            Ok(buffer) => {
                profiler.record_ms(
                    "buffer.acquire_wait",
                    started.elapsed().as_secs_f64() * 1000.0,
                );
                return Ok(Some(buffer));
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Err(CoreError::Encode(
                    "Frame buffer pool disconnected".to_string(),
                ));
            }
        }
    }
}

fn prewarm_first_frame(
    renderer: VideoFrameRenderer<'_>,
    free_receiver: &Receiver<FrameBuffer>,
    shutdown: &PipelineShutdown,
    profiler: &mut RenderProfiler,
) -> CoreResult<WorkerEvent> {
    shutdown.check()?;

    let acquire_started = Instant::now();
    let mut buffer = free_receiver
        .recv_timeout(Duration::from_millis(250))
        .map_err(|error| {
            CoreError::Encode(format!(
                "Could not acquire parallel prewarm buffer: {error}"
            ))
        })?;
    profiler.record_ms(
        "buffer.acquire_wait",
        acquire_started.elapsed().as_secs_f64() * 1000.0,
    );
    let render_started = Instant::now();
    renderer.render_rgba(0, buffer.pixels.as_mut_slice(), profiler)?;
    let render_ms = render_started.elapsed().as_secs_f64() * 1000.0;
    profiler.record_ms("parallel.worker_frame", render_ms);
    profiler.record_ms("parallel.prewarm_frame", render_ms);
    profiler.record_ms("frame.total", render_ms);

    Ok(WorkerEvent::Rendered {
        output_frame_index: 0,
        frame: CompletedFrame {
            dense_frame_index: 0,
            buffer,
            completed_at: Instant::now(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::pipeline::frame_pool::{
        diagnose_frame_worker_count_for_resources, ParallelFramePoolPlan,
    };
    use std::num::NonZeroUsize;

    #[test]
    fn frame_planning_rejects_unsupported_counts_before_workers_start() {
        for count in [0, u64::from(u32::MAX) + 1] {
            assert!(FrameProductionPlan::new(count, NonZeroU32::MIN).is_err());
            assert!(FrameProductionPlan::decimated(count, NonZeroU32::new(6).unwrap()).is_err());
        }
    }

    #[test]
    fn decimation_keeps_the_last_dense_index_inside_the_layout() {
        for layout in [
            1,
            5,
            6,
            7,
            61,
            u64::from(u32::MAX) - MAX_FRAME_WORKERS as u64,
        ] {
            let plan = FrameProductionPlan::decimated(layout, NonZeroU32::new(6).unwrap()).unwrap();
            let last_index = u64::from(plan.count() - 1) * u64::from(plan.stride().get());
            assert!(last_index < layout);
            assert!(last_index + 6 >= layout);
        }
    }

    #[test]
    fn diagnoses_workers_from_profile_cpu_cost_and_frame_count() {
        assert_eq!(
            diagnose_frame_worker_count_for_resources(1_000, 4, 16).get(),
            4
        );
        assert_eq!(diagnose_frame_worker_count_for_resources(2, 4, 16).get(), 2);
        assert_eq!(
            diagnose_frame_worker_count_for_resources(1_000, 3, 8).get(),
            2
        );
        assert_eq!(
            diagnose_frame_worker_count_for_resources(1_000, 0, 64).get(),
            1
        );
    }

    #[test]
    fn sizes_parallel_pool_from_resolution_and_worker_count() {
        let plan = ParallelFramePoolPlan::for_resources(
            crate::render::FrameSize {
                width: 3840,
                height: 2160,
            },
            NonZeroUsize::new(3).unwrap(),
            8,
        )
        .unwrap();
        assert_eq!(plan.frame_byte_len, 3840 * 2160 * 4);
        assert_eq!(plan.buffer_count, 5);
        assert_eq!(plan.queue_capacity, 4);

        let error = ParallelFramePoolPlan::for_resources(
            crate::render::FrameSize {
                width: 11520,
                height: 6480,
            },
            NonZeroUsize::new(2).unwrap(),
            8,
        )
        .unwrap_err();
        assert!(error.to_string().contains("frame-pool ceiling"));

        let error = ParallelFramePoolPlan::for_resources(
            crate::render::FrameSize {
                width: 1920,
                height: 1080,
            },
            NonZeroUsize::new(4).unwrap(),
            4,
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("reserving one logical processor"));
    }

    #[test]
    fn ordered_frames_wait_for_missing_indices_and_reject_duplicates() {
        let mut frames = OrderedFrames::new(3);

        assert!(frames.insert(2, 'c').unwrap().is_empty());
        assert_eq!(frames.insert(0, 'a').unwrap(), vec!['a']);
        assert_eq!(frames.insert(1, 'b').unwrap(), vec!['b', 'c']);

        let duplicate = frames.insert(2, 'x').unwrap_err();
        assert!(duplicate.to_string().contains("duplicate frame index 2"));
    }
}
