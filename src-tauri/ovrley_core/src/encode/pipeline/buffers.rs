//! Bounded RGBA buffer ownership, queue backpressure, and timing aggregation.

use super::process::PipelineShutdown;
use crate::debug::{RenderProfiler, TimingBucket};
use crate::error::{CoreError, CoreResult};
use crate::render::FrameSize;
use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Reusable raw RGBA frame buffer exchanged through the encode queues.
pub(crate) struct FrameBuffer {
    /// Pixel bytes in row-major RGBA order.
    pub(crate) pixels: Vec<u8>,
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

const MAX_PARALLEL_FRAME_BUFFERS: usize = 5;
pub const MAX_FRAME_WORKERS: usize = MAX_PARALLEL_FRAME_BUFFERS - 1;
const PARALLEL_FRAME_MEMORY_CEILING_BYTES: usize = 768 * 1024 * 1024;

/// Diagnoses the canonical frame-worker count for one codec profile and render.
pub fn diagnose_frame_worker_count(
    total_frames: usize,
    cpu_cores_per_frame_worker: usize,
) -> CoreResult<NonZeroUsize> {
    let logical_cores = std::thread::available_parallelism()
        .map_err(|error| {
            CoreError::Encode(format!(
                "Could not determine available CPU capacity: {error}"
            ))
        })?
        .get();
    Ok(diagnose_frame_worker_count_for_resources(
        total_frames,
        cpu_cores_per_frame_worker,
        logical_cores,
    ))
}

pub(super) fn diagnose_frame_worker_count_for_resources(
    total_frames: usize,
    cpu_cores_per_frame_worker: usize,
    logical_cores: usize,
) -> NonZeroUsize {
    let workers = if cpu_cores_per_frame_worker == 0 {
        1
    } else {
        (logical_cores / cpu_cores_per_frame_worker)
            .clamp(1, MAX_FRAME_WORKERS)
            .min(total_frames.max(1))
    };
    NonZeroUsize::new(workers).expect("diagnosed frame worker count is non-zero")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ParallelFramePoolPlan {
    pub(crate) frame_byte_len: usize,
    pub(crate) buffer_count: usize,
    pub(crate) queue_capacity: usize,
}

impl ParallelFramePoolPlan {
    pub(crate) fn for_frame_size(frame_size: FrameSize, workers: NonZeroUsize) -> CoreResult<Self> {
        let available_parallelism = std::thread::available_parallelism()
            .map_err(|error| {
                CoreError::Encode(format!(
                    "Could not determine available CPU capacity: {error}"
                ))
            })?
            .get();
        Self::for_resources(frame_size, workers, available_parallelism)
    }

    pub(super) fn for_resources(
        frame_size: FrameSize,
        workers: NonZeroUsize,
        available_parallelism: usize,
    ) -> CoreResult<Self> {
        if workers.get() > MAX_FRAME_WORKERS {
            return Err(CoreError::Encode(format!(
                "parallel frame worker count must be in 1..={MAX_FRAME_WORKERS}; received {}",
                workers.get()
            )));
        }
        let worker_capacity = available_parallelism.saturating_sub(1).max(1);
        if workers.get() > worker_capacity {
            return Err(CoreError::Encode(format!(
                "parallel rendering requested {} workers, but CPU capacity permits {worker_capacity} while reserving one logical processor for FFmpeg",
                workers,
            )));
        }
        let frame_byte_len = frame_size.rgba_len()?;
        let memory_limited_buffers = PARALLEL_FRAME_MEMORY_CEILING_BYTES / frame_byte_len;
        let buffer_count = memory_limited_buffers.min(MAX_PARALLEL_FRAME_BUFFERS);
        let required_buffers = workers
            .get()
            .checked_add(1)
            .ok_or_else(|| CoreError::Encode("Parallel frame buffer count overflow".to_string()))?;
        if buffer_count < required_buffers {
            return Err(CoreError::Encode(format!(
                "{}x{} parallel rendering with {} workers requires at least {required_buffers} RGBA buffers ({} MiB each), exceeding the {} MiB frame-pool ceiling",
                frame_size.width,
                frame_size.height,
                workers,
                frame_byte_len / (1024 * 1024),
                PARALLEL_FRAME_MEMORY_CEILING_BYTES / (1024 * 1024),
            )));
        }

        Ok(Self {
            frame_byte_len,
            buffer_count,
            queue_capacity: buffer_count - 1,
        })
    }

    pub(crate) fn create_channels(self) -> ParallelFrameChannels {
        let (frame_sender, frame_receiver) = sync_channel(self.queue_capacity);
        ParallelFrameChannels {
            frame_sender,
            frame_receiver,
            buffers: Arc::new(FrameBufferPool {
                available: Mutex::new(
                    (0..self.buffer_count)
                        .map(|_| FrameBuffer {
                            pixels: vec![0; self.frame_byte_len],
                        })
                        .collect(),
                ),
                returned: Condvar::new(),
            }),
        }
    }
}

pub(crate) struct ParallelFrameChannels {
    pub(crate) frame_sender: SyncSender<FrameBuffer>,
    pub(crate) frame_receiver: Receiver<FrameBuffer>,
    pub(crate) buffers: Arc<FrameBufferPool>,
}

/// Owned by the pipeline through writer teardown; frame workers only borrow it.
/// Buffer acquisition precedes task allocation to keep ordered forwarding live.
pub(crate) struct FrameBufferPool {
    available: Mutex<Vec<FrameBuffer>>,
    returned: Condvar,
}

impl FrameBufferPool {
    pub(crate) fn acquire(
        &self,
        shutdown: &PipelineShutdown,
        profiler: &mut RenderProfiler,
    ) -> Option<FrameBuffer> {
        let started = Instant::now();
        let mut available = self.available.lock().expect("frame pool mutex poisoned");
        loop {
            if shutdown.is_stopped() {
                return None;
            }
            if let Some(buffer) = available.pop() {
                profiler.record_ms(
                    "buffer.acquire_wait",
                    started.elapsed().as_secs_f64() * 1000.0,
                );
                return Some(buffer);
            }
            available = self
                .returned
                .wait_timeout(available, Duration::from_millis(25))
                .expect("frame pool mutex poisoned")
                .0;
        }
    }

    pub(crate) fn release(&self, buffer: FrameBuffer) {
        self.available
            .lock()
            .expect("frame pool mutex poisoned")
            .push(buffer);
        self.returned.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn exhausted_pool_wakes_for_returned_buffers_and_cancellation() {
        let pool = FrameBufferPool {
            available: Mutex::new(Vec::new()),
            returned: Condvar::new(),
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let shutdown = PipelineShutdown::shared(Arc::clone(&cancel));
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| pool.acquire(&shutdown, &mut RenderProfiler::default()));
            pool.release(FrameBuffer { pixels: vec![17] });
            assert_eq!(worker.join().unwrap().unwrap().pixels, vec![17]);
            let worker = scope.spawn(|| pool.acquire(&shutdown, &mut RenderProfiler::default()));
            cancel.store(true, Ordering::Relaxed);
            assert!(worker.join().unwrap().is_none());
        });
    }
}
