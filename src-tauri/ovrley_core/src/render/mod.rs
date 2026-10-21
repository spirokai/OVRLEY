//! Native overlay jobs: prepare shared widgets, compose frames, and write preview output.
pub mod assets;
mod atlas;
mod engine;
mod frame;
pub mod report;
mod static_layer;
pub mod surface;

use crate::{
    activity::schema::{DenseActivityReport, ParsedActivity},
    debug::{RenderProfiler, TimingBucket},
    error::{CoreError, CoreResult},
    fonts::NativeFonts,
    normalize::{ValidatedRenderConfig, ValidatedSceneConfig},
    paths::AppPaths,
};
use assets::{prepare_render_assets, PreparedRenderAssets};
use skia_safe::Image;
use static_layer::prepare_rasters;
use std::{collections::BTreeMap, path::Path, time::Instant};
use surface::{create_surface, write_surface_png};

pub use atlas::{AtlasPlan, AtlasSlot};
pub use engine::{ExportWidgets, PreparedAtlas, WorkerWidgets};
pub use frame::VideoFrameRenderer;
pub use report::{LabelCacheStatus, PreviewRenderReport};
pub use static_layer::{PreparedStaticLayer, StaticLayer};

/// Assets prepared once and reused by preview/video frame rendering.
#[derive(Clone)]
pub struct PreparedPreviewAssets {
    /// Cached static label/metric-part layer for preview surfaces.
    pub(crate) labels_image: Option<Image>,
    /// Shared widget layouts paired with native frame samples.
    pub prepared_assets: PreparedRenderAssets,
    /// Images remain visible on video frames without activity coverage.
    pub(crate) raster_rgba: Vec<u8>,
    /// Required static pixels for video composition.
    pub(crate) base_rgba: Vec<u8>,
}

impl PreparedPreviewAssets {
    pub fn scene(&self) -> &ValidatedSceneConfig {
        &self.prepared_assets.scene
    }
}

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

/// Prepares all reusable assets needed to render preview or video frames.
///
/// The result includes static labels/icons, widget caches, timing buckets, and
/// total preparation time. Video rendering uses the embedded base RGBA buffer to
/// avoid redrawing static content every frame.
pub fn prepare_preview_assets(
    paths: &AppPaths,
    config: &ValidatedRenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
) -> CoreResult<(
    PreparedPreviewAssets,
    LabelCacheStatus,
    BTreeMap<String, TimingBucket>,
    f64,
)> {
    let mut prepare_profiler = RenderProfiler::default();
    let prepare_started = Instant::now();
    let prepared_assets = prepare_render_assets(
        &NativeFonts(&paths.font_dirs),
        config,
        activity,
        dense_activity,
        &mut prepare_profiler,
    )?;
    let rasters = prepare_rasters(&config.rasters, prepared_assets.scene.presentation.scale)?;
    let raster_rgba = StaticLayer {
        backdrops: &[],
        rasters: &rasters,
        labels: &[],
        values: &[],
        scene: &prepared_assets.scene,
    }
    .prepare(paths, &mut prepare_profiler)?
    .base_rgba;
    let static_layer = StaticLayer {
        backdrops: &config.backdrops,
        rasters: &rasters,
        labels: &config.labels,
        values: &config.values,
        scene: &prepared_assets.scene,
    }
    .prepare(paths, &mut prepare_profiler)?;
    let labels_image = static_layer.image;
    let label_cache_status = static_layer.cache_status;
    let prepare_timings = prepare_profiler.summary();

    Ok((
        PreparedPreviewAssets {
            labels_image,
            prepared_assets,
            raster_rgba,
            base_rgba: static_layer.base_rgba,
        },
        label_cache_status,
        prepare_timings,
        prepare_started.elapsed().as_secs_f64() * 1000.0,
    ))
}

/// Renders a preview PNG and returns a performance report.
pub fn render_preview_with_report(
    paths: &AppPaths,
    config: &ValidatedRenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    second: f64,
    out_path: &Path,
) -> CoreResult<PreviewRenderReport> {
    let (prepared_preview_assets, label_cache_status, prepare_timings, prepare_total_ms) =
        prepare_preview_assets(paths, config, activity, dense_activity)?;
    render_preview_with_prepared_assets(PreviewRenderRequest {
        paths,
        dense_activity,
        prepared_preview_assets: &prepared_preview_assets,
        second,
        prepare_timings,
        label_cache_status,
        extra_total_ms: prepare_total_ms,
        out_path,
    })
}

/// Bundled parameters for a preview frame render.
pub struct PreviewRenderRequest<'a> {
    pub paths: &'a AppPaths,
    pub dense_activity: &'a DenseActivityReport,
    pub prepared_preview_assets: &'a PreparedPreviewAssets,
    pub second: f64,
    pub prepare_timings: BTreeMap<String, TimingBucket>,
    pub label_cache_status: LabelCacheStatus,
    pub extra_total_ms: f64,
    pub out_path: &'a Path,
}

/// Renders a preview using already-prepared assets.
///
/// # Phases
///
/// 1. **Setup** — resolve dimensions, frame index, and initialize profilers.
/// 2. **Surface render** — draw all overlay layers into a Skia surface.
/// 3. **PNG export** — encode the surface snapshot to disk.
/// 4. **Report** — collect timing data and build the preview report.
///
/// This is useful for repeated preview generation where static labels and widget
/// geometry should be prepared once and reused.
pub fn render_preview_with_prepared_assets(
    request: PreviewRenderRequest<'_>,
) -> CoreResult<PreviewRenderReport> {
    // Phase 1: resolve dimensions, frame index, and create profilers.
    let width = request.prepared_preview_assets.scene().presentation.width;
    let height = request.prepared_preview_assets.scene().presentation.height;
    let frame_index = frame_index_for_second(
        request.prepared_preview_assets.scene(),
        request.dense_activity,
        request.second,
    );
    let mut frame_profiler = RenderProfiler::default();
    let mut preview_profiler = RenderProfiler::default();
    let total_started = Instant::now();

    // Phase 2: draw all overlay layers into a Skia surface.
    let mut surface = create_surface(width, height)?;
    preview_profiler.measure("preview.surface.create_clear", || {
        surface.canvas().clear(skia_safe::Color::TRANSPARENT);
    });
    if let Some(image) = &request.prepared_preview_assets.labels_image {
        frame_profiler.measure("base.restore", || {
            surface.canvas().draw_image(image, (0, 0), None);
        });
    }
    let frame_report = request.prepared_preview_assets.prepared_assets.draw(
        surface.canvas(),
        request.dense_activity,
        frame_index,
        &NativeFonts(&request.paths.font_dirs),
        true,
        &mut frame_profiler,
    )?;

    // Phase 3: encode the surface snapshot as PNG to disk.
    preview_profiler.measure("preview.png_write", || {
        write_surface_png(&mut surface, request.out_path)
            .map_err(|error| CoreError::Render(format!("Failed to render preview frame: {error}")))
    })?;

    // Phase 4: collect timing data and build the preview report.
    let frame_timings = frame_profiler.summary();
    let preview_only_timings = preview_profiler.summary();
    let surface_ms = preview_only_timings
        .get("preview.surface.create_clear")
        .map(|bucket| bucket.total_ms)
        .unwrap_or(0.0);
    let label_layer_ms = frame_timings
        .get("base.restore")
        .map(|bucket| bucket.total_ms)
        .unwrap_or(0.0);
    let value_draw_ms = frame_timings
        .get("text.dynamic")
        .map(|bucket| bucket.total_ms)
        .unwrap_or(0.0);
    let png_write_ms = preview_only_timings
        .get("preview.png_write")
        .map(|bucket| bucket.total_ms)
        .unwrap_or(0.0);

    let report = PreviewRenderReport {
        second: request.second,
        frame_index,
        width,
        height,
        total_ms: total_started.elapsed().as_secs_f64() * 1000.0 + request.extra_total_ms,
        surface_ms,
        label_layer_ms,
        value_draw_ms,
        png_write_ms,
        value_count: request.prepared_preview_assets.prepared_assets.values.len(),
        label_count: request.prepared_preview_assets.prepared_assets.label_count,
        label_cache_status: request.label_cache_status,
        route_widgets: frame_report.route_widgets,
        elevation_widgets: frame_report.elevation_widgets,
        diving_profiles: frame_report.diving_profiles,
        metric_presentations: frame_report.metric_presentations,
        prepare_timings: request.prepare_timings,
        frame_timings,
        preview_only_timings,
    };

    Ok(report)
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
