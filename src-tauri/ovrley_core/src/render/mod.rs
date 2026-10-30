//! Native overlay rendering: every widget draws through the export host of the shared
//! widget engine, for video frames and for preview PNGs alike.
mod atlas;
mod engine;
mod map_export;
pub mod surface;

use crate::{
    activity::schema::{DenseActivityReport, ParsedActivity},
    debug::{RenderProfiler, TimingBucket},
    encode::progress::RenderController,
    error::{CoreError, CoreResult},
    normalize::{ValidatedRenderConfig, ValidatedSceneConfig},
    paths::AppPaths,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Instant,
};
use surface::{wrap_native_surface, write_surface_png};

pub use atlas::{AtlasPlan, AtlasSlot, SlotPlacement};
pub use engine::{ExportWidgets, PreparedAtlas, WorkerWidgets};
pub use map_export::{download_preview_tiles, ExportMapJob};

/// Canonical dimensions for a rendered video frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameSize {
    pub width: u32,
    pub height: u32,
}

impl FrameSize {
    pub fn rgba_len(self) -> CoreResult<usize> {
        let pixel_count = usize::try_from(self.width)
            .ok()
            .and_then(|width| {
                usize::try_from(self.height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .ok_or_else(|| {
                CoreError::Render(format!(
                    "RGBA frame dimensions overflow: {}x{}",
                    self.width, self.height
                ))
            })?;
        pixel_count.checked_mul(4).ok_or_else(|| {
            CoreError::Render(format!(
                "RGBA frame byte length overflow: {}x{}",
                self.width, self.height
            ))
        })
    }
}

/// Renders the full scene frames of dense activity frames `dense_indices` as straight-alpha RGBA,
/// through the full-frame atlas plan export uses for transparent video.
pub fn render_full_frames(
    font_dirs: &[PathBuf],
    config: &ValidatedRenderConfig,
    activity: &ParsedActivity,
    dense: &DenseActivityReport,
    dense_indices: &[usize],
    profiler: &mut RenderProfiler,
) -> CoreResult<Vec<Vec<u8>>> {
    let size = FrameSize {
        width: config.scene.presentation.width,
        height: config.scene.presentation.height,
    };
    let controller = RenderController::default();
    let maps = ExportMapJob {
        source: None,
        frame_indices: dense_indices,
        controller: &controller,
    };
    let export = ExportWidgets::prepare(font_dirs, config, activity, dense, size, 0, maps, profiler)?;
    let atlas = export.prepare_atlas(export.full_frame_atlas(), profiler)?;
    let mut worker = export.worker()?;
    dense_indices
        .iter()
        .map(|index| {
            let mut pixels = vec![0; size.rgba_len()?];
            worker.render_atlas(*index, &atlas, &mut pixels, profiler)?;
            Ok(pixels)
        })
        .collect()
}

/// One preview PNG: the activity second it shows and where it is written.
pub struct PreviewFrame {
    pub second: f64,
    pub out_path: PathBuf,
}

/// Serializable timing report for a batch of preview PNGs.
#[derive(Clone, Debug, serde::Serialize)]
pub struct PreviewRenderReport {
    pub seconds: Vec<f64>,
    pub frame_indices: Vec<usize>,
    pub width: u32,
    pub height: u32,
    pub total_ms: f64,
    pub timings: BTreeMap<String, TimingBucket>,
}

/// Renders preview PNGs of full scene frames and returns their timing report.
pub fn render_preview_frames(
    paths: &AppPaths,
    config: &ValidatedRenderConfig,
    activity: &ParsedActivity,
    dense: &DenseActivityReport,
    frames: &[PreviewFrame],
) -> CoreResult<PreviewRenderReport> {
    let started = Instant::now();
    let mut profiler = RenderProfiler::default();
    let scene = &config.scene;
    let frame_indices: Vec<usize> = frames
        .iter()
        .map(|frame| frame_index_for_second(scene, dense, frame.second))
        .collect();
    let pixels = render_full_frames(&paths.font_dirs, config, activity, dense, &frame_indices, &mut profiler)?;
    let (width, height) = (scene.presentation.width, scene.presentation.height);
    for (frame, mut pixels) in frames.iter().zip(pixels) {
        profiler.measure("preview.png_write", || -> CoreResult<()> {
            let mut surface = wrap_native_surface(width, height, &mut pixels)?;
            write_surface_png(&mut surface, &frame.out_path)
                .map_err(|error| CoreError::Render(format!("Failed to render preview frame: {error}")))
        })?;
    }
    Ok(PreviewRenderReport {
        seconds: frames.iter().map(|frame| frame.second).collect(),
        frame_indices,
        width,
        height,
        total_ms: started.elapsed().as_secs_f64() * 1000.0,
        timings: profiler.summary(),
    })
}

/// Renders one preview PNG for activity `second`.
pub fn render_preview_frame(
    paths: &AppPaths,
    config: &ValidatedRenderConfig,
    activity: &ParsedActivity,
    dense: &DenseActivityReport,
    second: f64,
    out_path: &Path,
) -> CoreResult<PreviewRenderReport> {
    render_preview_frames(
        paths,
        config,
        activity,
        dense,
        &[PreviewFrame {
            second,
            out_path: out_path.to_path_buf(),
        }],
    )
}

fn frame_index_for_second(
    scene: &ValidatedSceneConfig,
    activity: &DenseActivityReport,
    second: f64,
) -> usize {
    if activity.frame_count == 0 {
        return 0;
    }
    let relative_second = (second - scene.start).clamp(0.0, scene.end - scene.start);
    let index = (relative_second * scene.fps).round() as isize;
    index.clamp(0, activity.frame_count.saturating_sub(1) as isize) as usize
}
