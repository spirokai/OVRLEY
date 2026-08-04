//! Frame-pool sizing and the data shared by parallel render workers.

use crate::debug::TimingBucket;
use crate::encode::pipeline::composite_plan::CompositePipelinePlan;
use crate::encode::pipeline::queue::FrameBuffer;
use crate::error::{CoreError, CoreResult};
use crate::render::FrameSize;
use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::atomic::AtomicU32;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};

const MAX_PARALLEL_FRAME_BUFFERS: usize = 5;
pub const MAX_FRAME_WORKERS: usize = MAX_PARALLEL_FRAME_BUFFERS - 1;

/// Upper limit for RGBA frame pool memory.
///
/// Increased from 192 MiB to 512 MiB to support high-resolution sources,
/// including DJI Action 4 portrait footage (2880×3840), while maintaining
/// a conservative upper memory bound.
///
/// This allows the default 4-worker pipeline to allocate enough RGBA
/// frame buffers without exhausting the frame pool.
const PARALLEL_FRAME_MEMORY_CEILING_BYTES: usize = 512 * 1024 * 1024;

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

    pub(crate) fn create_channels(self) -> CoreResult<ParallelFrameChannels> {
        let (frame_sender, frame_receiver) = sync_channel(self.queue_capacity);
        let (free_sender, free_receiver) = sync_channel(self.buffer_count);
        for _ in 0..self.buffer_count {
            free_sender
                .send(FrameBuffer {
                    pixels: vec![0; self.frame_byte_len],
                })
                .map_err(|_| {
                    CoreError::Encode("Failed to initialize parallel frame buffer pool".into())
                })?;
        }
        Ok(ParallelFrameChannels {
            frame_sender,
            frame_receiver,
            free_sender,
            free_receiver,
        })
    }
}

pub(crate) struct ParallelFrameChannels {
    pub(crate) frame_sender: SyncSender<FrameBuffer>,
    pub(crate) frame_receiver: Receiver<FrameBuffer>,
    pub(crate) free_sender: SyncSender<FrameBuffer>,
    pub(crate) free_receiver: Receiver<FrameBuffer>,
}

pub(crate) enum ParallelFrameProgress<'a> {
    Transparent(&'a AtomicU32),
    Composite(&'a CompositePipelinePlan),
}

pub(crate) struct ParallelFrameRenderResult {
    pub(crate) timings: BTreeMap<String, TimingBucket>,
    pub(crate) rendered_frames: u32,
    pub(crate) free_receiver: Receiver<FrameBuffer>,
}
