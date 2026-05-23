//! Shared internal runtime helpers for encode pipelines.
//!
//! This module owns only the pieces that are common across transparent and
//! composite encoding at the runtime-infrastructure layer: queue buffer
//! payloads, buffer reuse, writer lifecycle helpers, and timing-map
//! aggregation. Pipeline-specific ffmpeg spawning, stderr monitoring, render
//! loops, and progress math stay in their owning pipeline modules.
//!
//! The shared writer helpers are deliberately compatibility-oriented rather
//! than "line-for-line duplicate" moves. Phase 1 first aligned two important
//! differences before sharing more code:
//!
//! - Transparent and composite writers now both count written frames as `u64`.
//! - Writer cancellation policy is explicit: transparent stops on cancel,
//!   while composite keeps draining until the sender closes.
//! - This module stays `pub(crate)` so the transparent pipeline does not gain
//!   wider visibility during the extraction.

use std::collections::BTreeMap;
use std::io::Write;
use std::process::ChildStdin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::debug::{RenderProfiler, TimingBucket};
use crate::error::{CoreError, CoreResult};

/// Reusable raw RGBA frame buffer exchanged through the encode queues.
pub(crate) struct FrameBuffer {
    /// Pixel bytes in row-major RGBA order.
    pub(crate) pixels: Vec<u8>,
}

/// Explicit writer-thread cancellation behavior for shared encode helpers.
pub(crate) enum WriterCancellation {
    /// Stop consuming queued frames once the shared cancel flag is raised.
    StopWhenCancelled(Arc<AtomicBool>),
    /// Continue draining queued frames until the frame sender is dropped.
    DrainUntilQueueCloses,
}

impl WriterCancellation {
    /// Returns whether the writer should stop before writing another frame.
    pub(crate) fn should_stop(&self) -> bool {
        match self {
            Self::StopWhenCancelled(cancel_flag) => cancel_flag.load(Ordering::SeqCst),
            Self::DrainUntilQueueCloses => false,
        }
    }
}

/// Result returned by the shared ffmpeg stdin writer thread.
pub(crate) struct WriterResult {
    /// Number of complete frames written into ffmpeg stdin.
    pub(crate) written_frames: u64,
    /// Writer-side timing buckets collected while draining the queue.
    pub(crate) timings: BTreeMap<String, TimingBucket>,
}

/// Pipeline-specific writer behavior that stays configurable after extraction.
pub(crate) struct WriterWorkerConfig<'a> {
    /// Whether the writer stops early on cancellation or drains until EOF.
    pub(crate) cancellation: WriterCancellation,
    /// Prefix used when turning `write_all` failures into `CoreError::Encode`.
    pub(crate) write_error_context: &'a str,
    /// Optional timing bucket for queue receive wait time.
    pub(crate) queue_wait_metric: Option<&'a str>,
    /// Optional timing bucket for free-buffer return wait time.
    pub(crate) release_wait_metric: Option<&'a str>,
    /// Optional encode error raised when the free-buffer pool disconnects.
    pub(crate) release_error_message: Option<&'a str>,
    /// Whether a final stdin flush failure should fail the render.
    pub(crate) flush_error_is_fatal: bool,
}

/// Merges timing buckets recorded on separate render and writer threads.
pub(crate) fn merge_timing_maps(
    mut left: BTreeMap<String, TimingBucket>,
    right: BTreeMap<String, TimingBucket>,
) -> BTreeMap<String, TimingBucket> {
    // Combine render-thread and writer-thread buckets for one summary file.
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

/// Waits for a reusable frame buffer from the free-buffer pool.
pub(crate) fn acquire_frame_buffer(
    receiver: &Receiver<FrameBuffer>,
    cancel_flag: &AtomicBool,
    profiler: &mut RenderProfiler,
) -> CoreResult<FrameBuffer> {
    // Timeout polling gives cancellation a chance to interrupt even when all
    // buffers are currently held by the writer or encoder.
    let started = Instant::now();
    loop {
        if cancel_flag.load(Ordering::SeqCst) {
            return Err(CoreError::Cancelled);
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
                return Err(CoreError::Encode(
                    "Frame buffer pool disconnected".to_string(),
                ));
            }
        }
    }
}

/// Sends a completed frame to the writer thread while respecting cancellation.
pub(crate) fn queue_frame(
    sender: &SyncSender<FrameBuffer>,
    frame_buffer: FrameBuffer,
    cancel_flag: &AtomicBool,
    profiler: &mut RenderProfiler,
) -> CoreResult<()> {
    // `try_send` lets the render loop poll cancellation while backpressure
    // clears, instead of blocking indefinitely inside `send`.
    let started = Instant::now();
    let mut payload = frame_buffer;
    loop {
        if cancel_flag.load(Ordering::SeqCst) {
            return Err(CoreError::Cancelled);
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
                return Err(CoreError::Encode("Encoder queue disconnected".to_string()));
            }
        }
    }
}

/// Writes queued frame buffers into ffmpeg stdin and returns buffers to the pool.
pub(crate) fn writer_worker(
    mut stdin: ChildStdin,
    receiver: Receiver<FrameBuffer>,
    free_sender: SyncSender<FrameBuffer>,
    config: WriterWorkerConfig<'_>,
) -> CoreResult<WriterResult> {
    // The writer owns ffmpeg stdin. It returns buffers to the free pool after a
    // successful write so the renderer can reuse allocations across frames.
    let mut profiler = RenderProfiler::default();
    let mut written_frames = 0u64;
    loop {
        let queue_started = Instant::now();
        let frame = match receiver.recv() {
            Ok(frame) => {
                record_optional_metric(&mut profiler, config.queue_wait_metric, queue_started);
                frame
            }
            Err(_) => {
                record_optional_metric(&mut profiler, config.queue_wait_metric, queue_started);
                break;
            }
        };
        if config.cancellation.should_stop() {
            break;
        }
        let write_started = Instant::now();
        stdin.write_all(frame.pixels.as_slice()).map_err(|error| {
            CoreError::Encode(format!("{}: {error}", config.write_error_context))
        })?;
        profiler.record_ms(
            "ffmpeg.write",
            write_started.elapsed().as_secs_f64() * 1000.0,
        );
        written_frames += 1;

        let release_started = Instant::now();
        let release_result = free_sender.send(frame);
        record_optional_metric(&mut profiler, config.release_wait_metric, release_started);
        match (release_result, config.release_error_message) {
            (Ok(()), _) => {}
            (Err(_), Some(message)) => {
                return Err(CoreError::Encode(message.to_string()));
            }
            (Err(_), None) => {}
        }
    }

    let flush_result = stdin
        .flush()
        .map_err(|error| CoreError::Encode(error.to_string()));
    if config.flush_error_is_fatal {
        flush_result?;
    }

    Ok(WriterResult {
        written_frames,
        timings: profiler.summary(),
    })
}

/// Records a timing bucket only when the owning pipeline requested it.
fn record_optional_metric(
    profiler: &mut RenderProfiler,
    metric_name: Option<&str>,
    started: Instant,
) {
    if let Some(metric_name) = metric_name {
        profiler.record_ms(metric_name, started.elapsed().as_secs_f64() * 1000.0);
    }
}
