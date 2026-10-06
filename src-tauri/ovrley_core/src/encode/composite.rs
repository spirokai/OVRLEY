//! Canonical composite-render contract shared by planning and FFmpeg encoding.

use std::num::NonZeroU32;
use std::path::PathBuf;

use crate::encode::ffmpeg::catalog::CompositeCodecId;
use crate::encode::fps::Fps;

/// Validated inputs and derived timing for one composite render.
#[derive(Clone, Debug, PartialEq)]
pub struct CompositeRenderPlan {
    pub(crate) frames: crate::encode::pipeline::frames::FrameProductionPlan,
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
