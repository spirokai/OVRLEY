//! Shared frame queues, buffer reuse, and FFmpeg stdin writing.

use std::collections::BTreeMap;
use std::io::Write;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::debug::{RenderProfiler, TimingBucket};
use crate::encode::pipeline::lifecycle::PipelineShutdown;
use crate::error::{CoreError, CoreResult};

/// Reusable raw RGBA frame buffer exchanged through the encode queues.
pub(crate) struct FrameBuffer {
    /// Pixel bytes in row-major RGBA order.
    pub(crate) pixels: Vec<u8>,
}

/// The two intentional FFmpeg writer behaviors.
pub(crate) enum WriterMode {
    Transparent,
    Composite,
}

/// Result returned by the shared ffmpeg stdin writer thread.
pub(crate) struct WriterResult {
    /// Number of complete frames written into ffmpeg stdin.
    pub(crate) written_frames: u64,
    /// Writer-side timing buckets collected while draining the queue.
    pub(crate) timings: BTreeMap<String, TimingBucket>,
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

/// Sends a completed frame to the writer thread while respecting shutdown.
pub(crate) fn queue_frame(
    sender: &SyncSender<FrameBuffer>,
    frame_buffer: FrameBuffer,
    shutdown: &PipelineShutdown,
    profiler: &mut RenderProfiler,
) -> CoreResult<()> {
    // `try_send` lets the render loop poll shutdown while backpressure
    // clears, instead of blocking indefinitely inside `send`.
    let started = Instant::now();
    let mut payload = frame_buffer;
    loop {
        shutdown.check()?;
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
    stdin: std::process::ChildStdin,
    receiver: Receiver<FrameBuffer>,
    free_sender: SyncSender<FrameBuffer>,
    shutdown: Arc<PipelineShutdown>,
    mode: WriterMode,
) -> CoreResult<WriterResult> {
    let result = writer_worker_inner(stdin, receiver, &free_sender, &shutdown, &mode);
    if result.is_err() {
        shutdown.signal_failure(CoreError::Encode("Encoder writer failed".to_string()));
    }
    result
}

fn writer_worker_inner(
    mut stdin: std::process::ChildStdin,
    receiver: Receiver<FrameBuffer>,
    free_sender: &SyncSender<FrameBuffer>,
    shutdown: &PipelineShutdown,
    mode: &WriterMode,
) -> CoreResult<WriterResult> {
    let mut profiler = RenderProfiler::default();
    let mut written_frames = 0u64;
    loop {
        if shutdown.is_stopped() {
            break;
        }
        let queue_started = Instant::now();
        let frame = match receiver.recv_timeout(Duration::from_millis(25)) {
            Ok(frame) => {
                profiler.record_ms(
                    "writer.rendered_frame_wait",
                    queue_started.elapsed().as_secs_f64() * 1000.0,
                );
                frame
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                profiler.record_ms(
                    "writer.rendered_frame_wait",
                    queue_started.elapsed().as_secs_f64() * 1000.0,
                );
                break;
            }
        };
        if shutdown.is_stopped() {
            break;
        }
        let write_started = Instant::now();
        stdin
            .write_all(frame.pixels.as_slice())
            .map_err(|error| match mode {
                WriterMode::Transparent => {
                    CoreError::Encode(format!("Failed writing frame to ffmpeg: {error}"))
                }
                WriterMode::Composite => {
                    CoreError::Encode(format!("Failed writing composite overlay frame: {error}"))
                }
            })?;
        profiler.record_ms(
            "ffmpeg.write",
            write_started.elapsed().as_secs_f64() * 1000.0,
        );
        written_frames += 1;

        let release_started = Instant::now();
        let release_result = free_sender.send(frame);
        if matches!(mode, WriterMode::Transparent) {
            profiler.record_ms(
                "buffer.release_wait",
                release_started.elapsed().as_secs_f64() * 1000.0,
            );
            if release_result.is_err() {
                return Err(CoreError::Encode(
                    "Frame buffer pool disconnected".to_string(),
                ));
            }
        }
    }

    let flush_result = stdin
        .flush()
        .map_err(|error| CoreError::Encode(error.to_string()));
    if matches!(mode, WriterMode::Transparent) {
        flush_result?;
    }

    Ok(WriterResult {
        written_frames,
        timings: profiler.summary(),
    })
}
