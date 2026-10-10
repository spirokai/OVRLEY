//! Fixed render contracts shared by job planning and encoding.

use std::num::NonZeroU32;
use std::path::PathBuf;

use crate::encode::ffmpeg::catalog::CompositeCodecId;
use crate::encode::ffmpeg::settings::FfmpegSettings;
use crate::encode::fps::Fps;
use crate::encode::video_timing::ActivityCoverage;
use crate::error::{CoreError, CoreResult};

/// Supported task count and dense-index stride, checked once during planning.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FrameProductionPlan {
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
        usize::try_from(last_index.max(u64::from(count)))
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

/// Fixed input to the transparent pipeline; timing is supplied by the job owner.
#[derive(Clone, Debug)]
pub struct TransparentRenderPlan {
    pub(crate) frames: FrameProductionPlan,
    pub(crate) ffmpeg: FfmpegSettings,
    pub layout_frame_count: u32,
    pub output_frame_count: u32,
    pub update_rate: NonZeroU32,
    pub container_fps: String,
    pub coverage: ActivityCoverage,
}

/// Validated inputs and derived timing for one composite render.
#[derive(Clone, Debug, PartialEq)]
pub struct CompositeRenderPlan {
    pub(crate) frames: FrameProductionPlan,
    pub(crate) video_path: PathBuf,
    pub quality: crate::encode::quality::EncodingQuality,
    pub(crate) sync_offset: f64,
    pub(crate) trim_start: f64,
    pub(crate) render_duration: f64,
    pub(crate) update_rate: NonZeroU32,
    pub(crate) source_fps: Fps,
    pub(crate) overlay_pipe_fps: Fps,
    pub overlay_frame_count: u64,
    pub output_frame_count: u32,
    pub coverage: crate::encode::video_timing::ActivityCoverage,
    pub(crate) requested_codec_id: CompositeCodecId,
    pub(crate) qsv_full_init_args: Vec<String>,
}

impl CompositeRenderPlan {
    /// Maps a completed overlay tick onto the validated source frame grid.
    pub fn output_progress(&self, written_overlay_frames: u64) -> u32 {
        ((written_overlay_frames - 1) * u64::from(self.update_rate.get())) as u32
    }
}
