//! Native overlay rendering: every widget draws through the export host of the shared
//! widget engine, for video frames and for preview PNGs alike.

use crate::{
    activity::build_dense_report,
    activity::schema::{DenseActivityReport, ParsedActivity},
    diagnostics::{RenderProfiler, TimingBucket},
    encode::progress::RenderController,
    error::{CoreError, CoreResult},
    config::{ValidatedRenderConfig, ValidatedSceneConfig},
    paths::AppPaths,
};
use ovrley_map::gpu::MapSource;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
pub use atlas::{AtlasPlan, AtlasSlot, SlotPlacement};
pub use engine::{ExportWidgets, PreparedAtlas, WorkerWidgets};
pub use maps::{download_preview_tiles, ExportMapJob};
use skia_safe::{surfaces, Borrows, EncodedImageFormat, ImageInfo, Surface};
use std::path::Path;

mod atlas;
mod engine;
mod maps;



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
/// through the full-frame atlas plan export uses for transparent video. Maps draw from
/// `map_source`, which tools without one leave out: they fail on any map.
pub fn render_full_frames(
    font_dirs: &[PathBuf],
    config: &ValidatedRenderConfig,
    activity: &ParsedActivity,
    dense: &DenseActivityReport,
    dense_indices: &[usize],
    map_source: Option<&MapSource>,
    profiler: &mut RenderProfiler,
) -> CoreResult<Vec<Vec<u8>>> {
    let size = FrameSize {
        width: config.scene.presentation.width,
        height: config.scene.presentation.height,
    };
    let controller = RenderController::default();
    let maps = ExportMapJob {
        source: map_source,
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
    map_source: Option<&MapSource>,
) -> CoreResult<PreviewRenderReport> {
    let started = Instant::now();
    let mut profiler = RenderProfiler::default();
    let scene = &config.scene;
    let frame_indices: Vec<usize> = frames
        .iter()
        .map(|frame| frame_index_for_second(scene, dense, frame.second))
        .collect();
    let pixels = render_full_frames(
        &paths.font_dirs,
        config,
        activity,
        dense,
        &frame_indices,
        map_source,
        &mut profiler,
    )?;
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

/// Renders the frame at `second` of a render config to a transparent PNG in Downloads and returns
/// its path.
pub fn render_preview_frame(
    paths: &AppPaths,
    config: crate::config::RenderConfig,
    parsed_activity: &crate::activity::schema::ParsedActivity,
    second: f64,
    raster_resources: Option<&dyn crate::raster::RasterResourceResolver>,
    map_source: Option<&ovrley_map::gpu::MapSource>,
) -> CoreResult<PathBuf> {
    let mut validated =
        crate::config::validate_render_config(config, raster_resources)?;
    validated.scene.narrow_to_frame(second);
    let dense_activity = build_dense_report(parsed_activity, &validated)?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| CoreError::Encode(format!("Failed to read system time: {error}")))?
        .as_nanos();
    let second_label = format!("{second:.3}").replace('.', "_");
    let out_path = paths
        .downloads_dir
        .join(format!("preview_frame_{timestamp}_t{second_label}.png"));
    let frame = PreviewFrame { second, out_path };
    render_preview_frames(
        paths,
        &validated,
        parsed_activity,
        &dense_activity,
        std::slice::from_ref(&frame),
        map_source,
    )?;
    Ok(frame.out_path)
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

/// Creates an owned Skia raster surface using Skia's native 32-bit format.
pub fn create_surface(width: u32, height: u32) -> CoreResult<Surface> {
    surfaces::raster_n32_premul((width as i32, height as i32)).ok_or_else(|| {
        CoreError::Render(format!("Failed to create raster surface {width}x{height}"))
    })
}

/// Builds image metadata for the RGBA buffers used by the video pipeline.
pub fn native_n32_image_info(width: u32, height: u32) -> ImageInfo {
    ImageInfo::new(
        (width as i32, height as i32),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Unpremul,
        None,
    )
}

/// Wraps caller-owned RGBA pixels in a Skia surface.
///
/// The returned surface borrows `pixels`, so drawing writes directly into the
/// encoder buffer without an intermediate copy.
pub fn wrap_native_surface<'pixels>(
    width: u32,
    height: u32,
    pixels: &'pixels mut [u8],
) -> CoreResult<Borrows<'pixels, Surface>> {
    let info = native_n32_image_info(width, height);
    let row_bytes = (width as usize) * 4;
    surfaces::wrap_pixels(&info, pixels, row_bytes, None).ok_or_else(|| {
        CoreError::Render(format!(
            "Failed to wrap native n32 surface {width}x{height}"
        ))
    })
}

/// Encodes a Skia surface snapshot as PNG and writes it to `path`.
pub fn write_surface_png(surface: &mut Surface, path: &Path) -> CoreResult<()> {
    let image = surface.image_snapshot();
    let data = image
        .encode(None, EncodedImageFormat::PNG, 100)
        .ok_or_else(|| CoreError::Render("Failed to encode preview PNG".into()))?;
    std::fs::write(path, data.as_bytes()).map_err(|source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}
