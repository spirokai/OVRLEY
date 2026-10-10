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

use super::diagnostics::EncoderMonitor;
use super::process::{PipelineKind, PipelineShutdown};
use crate::debug::{RenderProfiler, TimingBucket};
use crate::encode::pipeline::buffers::{merge_timing_maps, queue_frame, FrameBuffer};
use crate::encode::plan::CompositeRenderPlan;
use crate::encode::progress::{ProgressEstimator, RenderController};
use crate::error::{CoreError, CoreResult};
use crate::render::VideoFrameRenderer;
use std::collections::BTreeMap;
use std::num::{NonZeroU32, NonZeroUsize};
use std::process::Child;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

use crate::encode::plan::FrameProductionPlan;

pub(crate) enum ParallelFrameProgress<'a> {
    Transparent(&'a EncoderMonitor),
    Composite(&'a CompositeRenderPlan),
}

pub(crate) struct ParallelFrameRenderResult {
    pub(crate) timings: BTreeMap<String, TimingBucket>,
    pub(crate) rendered_frames: u32,
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

/// Distributes each dense frame exactly once. Frame zero is prewarmed by the owner.
struct FrameTasks {
    next: AtomicUsize,
    plan: FrameProductionPlan,
}

impl FrameTasks {
    fn new(plan: FrameProductionPlan) -> Self {
        Self {
            next: AtomicUsize::new(1),
            plan,
        }
    }

    fn claim(&self) -> Option<(u64, usize)> {
        let index = self
            .next
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |index| {
                (index < self.plan.count() as usize).then(|| index + 1)
            })
            .ok()?;
        Some((index as u64, index * self.plan.stride().get() as usize))
    }
}

struct RenderedFrame {
    output_frame_index: u64,
    frame: CompletedFrame,
}

/// Ordered output is owned by the coordinator, never by frame workers.
struct FrameOutput<'a> {
    sender: &'a SyncSender<FrameBuffer>,
    observer: Option<&'a super::OrderedFrameObserver<'a>>,
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
    ordered_frame_observer: Option<&super::OrderedFrameObserver<'_>>,
    buffers: &super::buffers::FrameBufferPool,
    ffmpeg_child: &mut Child,
    render_started: Instant,
) -> CoreResult<ParallelFrameRenderResult> {
    let mut profiler = RenderProfiler::default();
    let prewarmed = prewarm_first_frame(renderer, buffers, shutdown, &mut profiler)?;
    let tasks = FrameTasks::new(plan);
    let (sender, receiver) = std::sync::mpsc::channel();

    thread::scope(|scope| {
        let mut workers_started = Vec::with_capacity(workers.get());
        for index in 0..workers.get() {
            let sender = sender.clone();
            let tasks = &tasks;
            match thread::Builder::new()
                .name(format!("frame-render-{index}"))
                .spawn_scoped(scope, move || {
                    frame_worker(renderer, tasks, buffers, shutdown, sender)
                }) {
                Ok(worker) => workers_started.push(worker),
                Err(error) => {
                    shutdown.signal_failure(CoreError::Render(format!(
                        "Could not start frame worker: {error}"
                    )));
                    break;
                }
            }
        }
        sender
            .send(prewarmed)
            .expect("coordinator owns the result receiver");
        drop(sender);
        let mut reporter = FrameProgressReporter::new(plan, progress, controller, render_started);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            coordinate_frames(
                &receiver,
                shutdown,
                FrameOutput {
                    sender: frame_sender,
                    observer: ordered_frame_observer,
                },
                &mut reporter,
                &mut profiler,
                ffmpeg_child,
                pipeline,
            )
        }))
        .unwrap_or_else(|_| Err(CoreError::Render("Frame coordinator panicked".into())));
        if let Err(error) = result {
            shutdown.signal_failure(error);
        }
        // Shutdown is published before joining so workers blocked on buffers exit.
        let mut timings = profiler.summary();
        for worker in workers_started {
            match worker.join() {
                Ok(worker_timings) => timings = merge_timing_maps(timings, worker_timings),
                Err(_) => shutdown.signal_failure(CoreError::Render(
                    "Parallel frame render worker panicked".into(),
                )),
            }
        }
        shutdown.check()?;
        Ok(ParallelFrameRenderResult {
            timings,
            rendered_frames: plan.count(),
        })
    })
}

fn frame_worker(
    renderer: VideoFrameRenderer<'_>,
    tasks: &FrameTasks,
    buffers: &super::buffers::FrameBufferPool,
    shutdown: &PipelineShutdown,
    sender: std::sync::mpsc::Sender<RenderedFrame>,
) -> BTreeMap<String, TimingBucket> {
    let mut profiler = RenderProfiler::default();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> CoreResult<()> {
        while !shutdown.is_stopped() {
            let started = Instant::now();
            let Some(mut buffer) = buffers.acquire(shutdown, &mut profiler) else {
                break;
            };
            let Some((output_frame_index, dense_frame_index)) = tasks.claim() else {
                buffers.release(buffer);
                break;
            };
            renderer.render_rgba(dense_frame_index, &mut buffer.pixels, &mut profiler)?;
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            profiler.record_ms("parallel.worker_frame", elapsed);
            profiler.record_ms("frame.total", elapsed);
            if sender
                .send(RenderedFrame {
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
        Ok(())
    }))
    .unwrap_or_else(|_| {
        Err(CoreError::Render(
            "Parallel frame render worker panicked".into(),
        ))
    });
    if let Err(error) = result {
        shutdown.signal_failure(error);
    }
    profiler.summary()
}

#[allow(clippy::too_many_arguments)]
fn coordinate_frames(
    receiver: &Receiver<RenderedFrame>,
    shutdown: &PipelineShutdown,
    output: FrameOutput<'_>,
    progress: &mut FrameProgressReporter<'_>,
    profiler: &mut RenderProfiler,
    child: &mut Child,
    pipeline: PipelineKind,
) -> CoreResult<()> {
    let total = u64::from(progress.plan.count());
    let mut ordered = OrderedFrames::new(total);
    let mut forwarded = 0;
    while forwarded < total {
        shutdown.check()?;
        let started = Instant::now();
        let event = receiver.recv_timeout(Duration::from_millis(25));
        profiler.record_ms(
            "parallel.result_wait",
            started.elapsed().as_secs_f64() * 1000.0,
        );
        match event {
            Ok(RenderedFrame {
                output_frame_index,
                frame,
            }) => {
                let ready = ordered.insert(output_frame_index, frame)?;
                let advanced = !ready.is_empty();
                for frame in ready {
                    profiler.record_ms(
                        "parallel.reorder_hold",
                        frame.completed_at.elapsed().as_secs_f64() * 1000.0,
                    );
                    if let Some(observer) = output.observer {
                        observer(forwarded, frame.dense_frame_index, &frame.buffer)?;
                    }
                    queue_frame(output.sender, frame.buffer, shutdown, profiler)?;
                    forwarded += 1;
                }
                if advanced {
                    progress.report(forwarded);
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                shutdown.check()?;
                if let Some(status) = child
                    .try_wait()
                    .map_err(|error| CoreError::Encode(format!("ffmpeg process error: {error}")))?
                {
                    return Err(CoreError::Encode(format!(
                        "{pipeline} ffmpeg exited unexpectedly with status {status}"
                    )));
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                shutdown.check()?;
                return Err(CoreError::Encode(format!(
                    "parallel render workers ended after producing {forwarded} of {total} frames"
                )));
            }
        }
    }
    Ok(())
}

/// Coordinator-only progress state; frame workers never see the controller.
struct FrameProgressReporter<'a> {
    plan: FrameProductionPlan,
    mode: ParallelFrameProgress<'a>,
    controller: &'a RenderController,
    estimator: ProgressEstimator,
    started: Instant,
    last: Instant,
    previous: u32,
}

impl<'a> FrameProgressReporter<'a> {
    fn new(
        plan: FrameProductionPlan,
        mode: ParallelFrameProgress<'a>,
        controller: &'a RenderController,
        started: Instant,
    ) -> Self {
        Self {
            plan,
            mode,
            controller,
            estimator: ProgressEstimator::default(),
            started,
            last: Instant::now(),
            previous: 0,
        }
    }

    fn report(&mut self, forwarded: u64) {
        let (total, current, encoded, fps_multiplier) = match &self.mode {
            ParallelFrameProgress::Transparent(monitor) => (
                self.plan.count(),
                forwarded as u32,
                monitor.encoded_frames(),
                self.plan.stride(),
            ),
            ParallelFrameProgress::Composite(plan) => (
                plan.output_frame_count,
                plan.output_progress(forwarded),
                0,
                NonZeroU32::MIN,
            ),
        };
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        let added = current - self.previous;
        self.previous = current;
        let seconds = if added == 0 {
            0.0
        } else {
            elapsed / f64::from(added)
        };
        let (eta, fps) = self.estimator.record(
            current,
            total,
            seconds,
            now.duration_since(self.started).as_secs_f64(),
        );
        self.controller.set_frame_progress(
            current,
            total,
            forwarded as u32,
            encoded,
            eta,
            fps.map(|fps| fps * f64::from(fps_multiplier.get())),
        );
    }
}

fn prewarm_first_frame(
    renderer: VideoFrameRenderer<'_>,
    buffers: &super::buffers::FrameBufferPool,
    shutdown: &PipelineShutdown,
    profiler: &mut RenderProfiler,
) -> CoreResult<RenderedFrame> {
    shutdown.check()?;
    let mut buffer = buffers
        .acquire(shutdown, profiler)
        .ok_or(CoreError::Cancelled)?;
    let started = Instant::now();
    renderer.render_rgba(0, &mut buffer.pixels, profiler)?;
    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    profiler.record_ms("parallel.worker_frame", elapsed);
    profiler.record_ms("parallel.prewarm_frame", elapsed);
    profiler.record_ms("frame.total", elapsed);
    Ok(RenderedFrame {
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
    use crate::encode::pipeline::buffers::{
        diagnose_frame_worker_count_for_resources, ParallelFramePoolPlan, MAX_FRAME_WORKERS,
    };
    use std::num::NonZeroUsize;

    #[test]
    fn concurrent_workers_claim_every_task_once_and_stop_at_the_end() {
        let tasks =
            FrameTasks::new(FrameProductionPlan::new(1000, NonZeroU32::new(3).unwrap()).unwrap());
        let mut claimed = thread::scope(|scope| {
            let workers = (0..4)
                .map(|_| {
                    scope.spawn(|| {
                        let mut claimed = Vec::new();
                        while let Some(task) = tasks.claim() {
                            claimed.push(task);
                        }
                        claimed
                    })
                })
                .collect::<Vec<_>>();
            workers
                .into_iter()
                .flat_map(|worker| worker.join().unwrap())
                .collect::<Vec<_>>()
        });
        claimed.sort_unstable();
        assert_eq!(
            claimed,
            (1..1000)
                .map(|index| (index as u64, index * 3))
                .collect::<Vec<_>>()
        );
        assert!(tasks.claim().is_none());
        assert_eq!(tasks.next.load(Ordering::Relaxed), 1000);
    }

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
