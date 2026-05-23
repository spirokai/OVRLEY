//! Render lifecycle orchestration facade.
//!
//! This module keeps the public encode entry points stable while delegating the
//! heavy lifting to focused helpers. Single-pass frame production lives in the
//! pipeline modules, benchmark-only multi-render work lives in
//! `video_parallel`, and segmented transparent/composite orchestration lives in
//! `video_segmented`.

use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::config::RenderConfig;
use crate::encode::video_composite_pipeline::render_composite_video_single;
use crate::encode::video_pipeline::render_video_single;
use crate::encode::video_segmented::{
    render_composite_video_segmented, render_video_segmented, should_parallelize_composite,
    should_parallelize_segmented,
};
use crate::error::CoreResult;
use crate::paths::AppPaths;

pub use crate::encode::progress::RenderController;
pub use crate::encode::video_parallel::run_parallel_renders;
pub use crate::encode::video_pipeline::rendered_frame_count;
pub use crate::encode::video_windows::{composite_output_frame_windows, CompositeSegmentWindow};

/// Renders a video, using segmentation when the selected codec benefits from it.
pub fn render_video(
    paths: &AppPaths,
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    controller: &RenderController,
) -> CoreResult<String> {
    if should_parallelize_segmented(config, dense_activity) {
        return render_video_segmented(paths, config, activity, dense_activity, controller);
    }
    render_video_single(paths, config, activity, dense_activity, controller)
}

/// Bundled parameters for composite MP4 rendering.
///
/// The request shape stays public and stable while the facade can delegate the
/// same bundle to either single-pass or segmented composite paths.
///
/// Fields such as `composite_render_duration` and `composite_video_trim_start`
/// are optional because callers that have already computed them from the render
/// plan can pass them directly, while the facade falls back to defaults derived
/// from `composite_video_duration`.
pub struct CompositeRenderRequest<'a> {
    pub paths: &'a AppPaths,
    pub config: &'a RenderConfig,
    pub activity: &'a ParsedActivity,
    pub dense_activity: &'a DenseActivityReport,
    pub controller: &'a RenderController,
    pub composite_video_path: &'a str,
    pub composite_bitrate: &'a str,
    pub composite_sync_offset: f64,
    pub composite_video_fps_num: u32,
    pub composite_video_fps_den: u32,
    pub composite_video_duration: f64,
    pub composite_render_duration: Option<f64>,
    pub composite_video_trim_start: Option<f64>,
    pub composite_widget_update_rate: Option<u32>,
}

/// Renders an imported video with the Skia overlay composited into an MP4 output.
///
/// Longer renders are automatically split into parallel segments for better CPU
/// utilization and then stitched with FFmpeg stream copy.
pub fn render_composite_video(request: &CompositeRenderRequest<'_>) -> CoreResult<String> {
    let render_duration = request.composite_render_duration.unwrap_or(
        request.composite_video_duration - request.composite_video_trim_start.unwrap_or(0.0),
    );
    let trim_start = request.composite_video_trim_start.unwrap_or(0.0);
    let update_rate = request.composite_widget_update_rate.unwrap_or(1).max(1);

    let codec = request
        .config
        .scene
        .ffmpeg
        .as_object()
        .and_then(|map| map.get("codec"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("libx264");
    if should_parallelize_composite(
        render_duration,
        request.composite_video_fps_num,
        update_rate,
        codec,
    ) {
        return render_composite_video_segmented(request, render_duration, trim_start, update_rate);
    }

    render_composite_video_single(
        request.paths,
        request.config,
        request.activity,
        request.dense_activity,
        request.controller,
        request.composite_video_path,
        request.composite_bitrate,
        request.composite_sync_offset,
        request.composite_video_fps_num,
        request.composite_video_fps_den,
        request.composite_video_duration,
        Some(render_duration),
        Some(trim_start),
        Some(update_rate),
    )
}
