//! Skia overlay rendering.
//!
//! Rendering is split into preparation and per-frame composition. Preparation
//! resolves static labels, widget geometry, and reusable base pixels. Per-frame
//! rendering restores that base, draws dynamic metric values, and composites
//! route/elevation widgets. The same primitives power preview PNG generation
//! and video frame streaming.

/// Value formatting and metric display helpers.
pub mod format;
/// Shared static label/icon caching and base-layer preparation helpers.
mod static_layer;
/// Skia surface allocation and PNG output helpers.
pub mod surface;
/// Font resolution, text measurement, and text drawing helpers.
pub mod text;
/// Route, elevation, and metric widget rendering.
pub mod widgets;

use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::commands::AppPaths;
use crate::config::RenderConfig;
use crate::debug::{RenderProfiler, TimingBucket};
use crate::error::{CoreError, CoreResult};
use crate::render::format::{format_value, frame_index_for_second};
use crate::render::static_layer::{cached_labels_image, config_has_static_metric_icons};
use crate::render::surface::{create_surface, wrap_native_surface, write_surface_png};
use crate::render::text::{draw_text, value_style};
use crate::render::widgets::value::MetricWidgetRequest;
use crate::render::widgets::{
    draw_elevation_widget, draw_metric_value_widget_with_config, draw_route_widget,
    has_static_metric_icon, prepare_render_assets, PreparedRenderAssets, WidgetRenderReport,
};
use skia_safe::Image;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

pub use self::static_layer::prepare_base_rgba;

/// Indicates whether the static label layer was not needed, reused, or rebuilt.
#[derive(Clone, Copy, Debug)]
pub enum LabelCacheStatus {
    /// No static labels or static icons were present.
    None,
    /// A previously rendered static label image was reused.
    Hit,
    /// Static label image was rendered and inserted into the cache.
    Miss,
}

/// Serializable performance and geometry report for one preview render.
#[derive(Clone, Debug, serde::Serialize)]
pub struct PreviewRenderReport {
    pub second: u32,
    pub frame_index: usize,
    pub width: u32,
    pub height: u32,
    pub total_ms: f64,
    pub surface_ms: f64,
    pub label_layer_ms: f64,
    pub value_draw_ms: f64,
    pub png_write_ms: f64,
    pub value_count: usize,
    pub label_count: usize,
    pub label_cache_status: String,
    pub route_widget: Option<WidgetRenderReport>,
    pub elevation_widget: Option<WidgetRenderReport>,
    pub prepare_timings: BTreeMap<String, TimingBucket>,
    pub frame_timings: BTreeMap<String, TimingBucket>,
    pub preview_only_timings: BTreeMap<String, TimingBucket>,
}

/// Assets prepared once and reused by preview/video frame rendering.
#[derive(Clone)]
pub struct PreparedPreviewAssets {
    /// Cached static label/icon layer for preview surfaces.
    pub(crate) labels_image: Option<Image>,
    /// Widget caches and optional base RGBA bytes.
    pub(crate) prepared_assets: PreparedRenderAssets,
}

/// Mutable raw-pixel render target used by the video encoder pipeline.
pub struct RenderTarget<'a> {
    /// RGBA pixel buffer that Skia will draw into.
    pub pixels: &'a mut [u8],
    /// Target width in pixels.
    pub width: u32,
    /// Target height in pixels.
    pub height: u32,
}

/// Prepares all reusable assets needed to render preview or video frames.
///
/// The result includes static labels/icons, widget caches, timing buckets, and
/// total preparation time. Video rendering uses the embedded base RGBA buffer to
/// avoid redrawing static content every frame.
pub fn prepare_preview_assets(
    paths: &AppPaths,
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
) -> CoreResult<(
    PreparedPreviewAssets,
    LabelCacheStatus,
    BTreeMap<String, TimingBucket>,
    f64,
)> {
    let width = config.scene.width.unwrap_or(1920);
    let height = config.scene.height.unwrap_or(1080);
    let scale = config.scene.scale.unwrap_or(1.0).max(0.1);
    let mut prepare_profiler = RenderProfiler::default();
    let prepare_started = Instant::now();
    let (labels_image, label_cache_status) =
        cached_labels_image(paths, config, width, height, scale, &mut prepare_profiler)?;
    let mut prepared_assets =
        prepare_render_assets(config, activity, dense_activity, &mut prepare_profiler)?;
    prepared_assets.base_rgba =
        prepare_base_rgba(paths, config, width, height, scale, &mut prepare_profiler)?;
    let prepare_timings = annotate_timing_aliases(
        prepare_profiler.summary(),
        &[("prepare.surface.clear", "surface.clear")],
    );

    Ok((
        PreparedPreviewAssets {
            labels_image,
            prepared_assets,
        },
        label_cache_status,
        prepare_timings,
        prepare_started.elapsed().as_secs_f64() * 1000.0,
    ))
}

/// Renders a preview PNG at `second`.
pub fn render_preview_to_path(
    paths: &AppPaths,
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    second: u32,
    out_path: &Path,
) -> CoreResult<()> {
    render_preview_with_report(paths, config, activity, dense_activity, second, out_path)
        .map(|report| report.0)
}

/// Renders a preview PNG and returns a performance report.
pub fn render_preview_with_report(
    paths: &AppPaths,
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    second: u32,
    out_path: &Path,
) -> CoreResult<((), PreviewRenderReport)> {
    let (prepared_preview_assets, label_cache_status, prepare_timings, prepare_total_ms) =
        prepare_preview_assets(paths, config, activity, dense_activity)?;
    render_preview_with_prepared_assets(PreviewRenderRequest {
        paths,
        config,
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
    pub config: &'a RenderConfig,
    pub dense_activity: &'a DenseActivityReport,
    pub prepared_preview_assets: &'a PreparedPreviewAssets,
    pub second: u32,
    pub prepare_timings: BTreeMap<String, TimingBucket>,
    pub label_cache_status: LabelCacheStatus,
    pub extra_total_ms: f64,
    pub out_path: &'a Path,
}

/// Bundled parameters for rendering a single frame to RGBA.
pub struct FrameRenderRequest<'a> {
    pub paths: &'a AppPaths,
    pub config: &'a RenderConfig,
    pub dense_activity: &'a DenseActivityReport,
    pub prepared_assets: &'a PreparedRenderAssets,
    pub frame_index: usize,
    pub scale: f32,
    pub labels_image: Option<&'a Image>,
    pub target: RenderTarget<'a>,
    pub frame_profiler: &'a mut RenderProfiler,
}

/// Renders a preview using already-prepared assets.
///
/// This is useful for repeated preview generation where static labels and widget
/// geometry should be prepared once and reused.
pub fn render_preview_with_prepared_assets(
    request: PreviewRenderRequest<'_>,
) -> CoreResult<((), PreviewRenderReport)> {
    let width = request.config.scene.width.unwrap_or(1920);
    let height = request.config.scene.height.unwrap_or(1080);
    let scale = request.config.scene.scale.unwrap_or(1.0).max(0.1);
    let frame_index =
        frame_index_for_second(request.config, request.dense_activity, request.second);
    let mut frame_profiler = RenderProfiler::default();
    let mut preview_profiler = RenderProfiler::default();
    let total_started = Instant::now();

    let (mut surface, route_widget, elevation_widget) = render_frame_surface(
        request.paths,
        request.config,
        request.dense_activity,
        &request.prepared_preview_assets.prepared_assets,
        frame_index,
        scale,
        request.prepared_preview_assets.labels_image.as_ref(),
        &mut frame_profiler,
        Some(&mut preview_profiler),
    )?;

    preview_profiler.measure("preview.png_write", || {
        write_surface_png(&mut surface, request.out_path)
            .map_err(|error| CoreError::Render(format!("Failed to render preview frame: {error}")))
    })?;

    let frame_timings =
        annotate_timing_aliases(frame_profiler.summary(), &[("base.restore", "base.copy")]);
    let preview_only_timings = annotate_timing_aliases(
        preview_profiler.summary(),
        &[
            ("preview.surface.create_clear", "surface.clear"),
            ("preview.png_write", "png.write"),
        ],
    );
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
        value_count: request.config.values.len(),
        label_count: request.config.labels.len(),
        label_cache_status: match request.label_cache_status {
            LabelCacheStatus::None => "none".to_string(),
            LabelCacheStatus::Hit => "hit".to_string(),
            LabelCacheStatus::Miss => "miss".to_string(),
        },
        route_widget,
        elevation_widget,
        prepare_timings: request.prepare_timings,
        frame_timings,
        preview_only_timings,
    };

    Ok(((), report))
}

/// Renders one frame directly into an existing RGBA buffer.
///
/// This is the hot path used by video encoding. If prepared base pixels match
/// the target buffer length, they are copied before dynamic content is drawn.
pub fn render_frame_rgba(request: FrameRenderRequest<'_>) -> CoreResult<()> {
    let width = request.target.width;
    let height = request.target.height;
    let mut labels_image = request.labels_image;
    let mut base_layer_restored = false;
    if let Some(base_rgba) = request
        .prepared_assets
        .base_rgba
        .as_ref()
        .filter(|base_rgba| base_rgba.len() == request.target.pixels.len())
    {
        let started = Instant::now();
        request.target.pixels.copy_from_slice(base_rgba);
        let restore_ms = started.elapsed().as_secs_f64() * 1000.0;
        request.frame_profiler.record_ms("base.restore", restore_ms);
        request
            .frame_profiler
            .record_ms("surface.restore", restore_ms);
        labels_image = None;
        base_layer_restored = true;
    } else {
        request.frame_profiler.measure("surface.clear", || {
            request.target.pixels.fill(0);
        });
    }

    let mut surface = request.frame_profiler.measure("surface.create", || {
        wrap_native_surface(width, height, request.target.pixels)
    })?;
    let _ = render_frame_to_surface(
        surface.canvas(),
        request.paths,
        request.config,
        request.dense_activity,
        request.prepared_assets,
        request.frame_index,
        request.scale,
        labels_image,
        base_layer_restored,
        request.frame_profiler,
    );
    Ok(())
}

// Creates an owned Skia surface and renders one preview frame onto it.
#[allow(clippy::too_many_arguments)]
fn render_frame_surface(
    paths: &AppPaths,
    config: &RenderConfig,
    dense_activity: &DenseActivityReport,
    prepared_assets: &PreparedRenderAssets,
    frame_index: usize,
    scale: f32,
    labels_image: Option<&Image>,
    frame_profiler: &mut RenderProfiler,
    mut preview_profiler: Option<&mut RenderProfiler>,
) -> CoreResult<(
    skia_safe::Surface,
    Option<WidgetRenderReport>,
    Option<WidgetRenderReport>,
)> {
    // Preview rendering owns its surface and writes a PNG, while video rendering
    // wraps caller-owned pixels. This helper is the preview-side equivalent of
    // `render_frame_rgba`.
    let width = config.scene.width.unwrap_or(1920);
    let height = config.scene.height.unwrap_or(1080);
    let mut surface = if preview_profiler.is_some() {
        create_surface(width, height)?
    } else {
        frame_profiler.measure("surface.create", || create_surface(width, height))?
    };
    if let Some(profiler) = preview_profiler.as_mut() {
        profiler.measure("preview.surface.create_clear", || {
            surface.canvas().clear(skia_safe::Color::TRANSPARENT);
        });
    } else {
        frame_profiler.measure("surface.clear", || {
            surface.canvas().clear(skia_safe::Color::TRANSPARENT);
        });
    }

    let widgets = render_frame_to_surface(
        surface.canvas(),
        paths,
        config,
        dense_activity,
        prepared_assets,
        frame_index,
        scale,
        labels_image,
        false,
        frame_profiler,
    );
    Ok((surface, widgets.0, widgets.1))
}

// Draws all overlay layers for one frame onto an existing Skia canvas.
#[allow(clippy::too_many_arguments)]
fn render_frame_to_surface(
    canvas: &skia_safe::Canvas,
    paths: &AppPaths,
    config: &RenderConfig,
    dense_activity: &DenseActivityReport,
    prepared_assets: &PreparedRenderAssets,
    frame_index: usize,
    scale: f32,
    labels_image: Option<&Image>,
    base_layer_restored: bool,
    frame_profiler: &mut RenderProfiler,
) -> (Option<WidgetRenderReport>, Option<WidgetRenderReport>) {
    // Draw order is important: static labels/icons first, dynamic metric text,
    // then plot widgets. Static metric icons can be skipped here when they were
    // already included in the restored base layer.
    let frame_started = Instant::now();
    if let Some(labels_image) = labels_image {
        frame_profiler.measure("base.restore", || {
            canvas.draw_image(labels_image, (0, 0), None);
        });
    }
    let static_metric_icons_rendered =
        config_has_static_metric_icons(config) && (labels_image.is_some() || base_layer_restored);

    frame_profiler.measure("text.dynamic", || {
        for value in &config.values {
            let style = value_style(&config.scene, value, scale);
            let static_icon_rendered_for_value =
                static_metric_icons_rendered && has_static_metric_icon(value);
            if draw_metric_value_widget_with_config(MetricWidgetRequest {
                canvas,
                config,
                value,
                base_style: &style,
                dense_activity,
                frame_index,
                scale,
                font_dirs: &paths.font_dirs,
                static_icon_rendered: static_icon_rendered_for_value,
            }) {
                continue;
            }
            let text = format_value(config, value, dense_activity, frame_index);
            draw_text(canvas, &text, &style, &paths.font_dirs);
        }
    });

    let route_widget = prepared_assets
        .route_cache
        .as_ref()
        .and_then(|cache| draw_route_widget(canvas, cache, frame_index, frame_profiler));
    let elevation_widget = prepared_assets.elevation_cache.as_ref().and_then(|cache| {
        draw_elevation_widget(canvas, paths, config, cache, frame_index, frame_profiler)
    });
    frame_profiler.record_ms("frame.draw", frame_started.elapsed().as_secs_f64() * 1000.0);
    (route_widget, elevation_widget)
}

// Adds legacy alternate names to timing buckets for compatibility with reports.
fn annotate_timing_aliases(
    mut timings: BTreeMap<String, TimingBucket>,
    aliases: &[(&str, &str)],
) -> BTreeMap<String, TimingBucket> {
    for (bucket_name, alt_name) in aliases {
        if let Some(bucket) = timings.get_mut(*bucket_name) {
            bucket.alt_name = Some((*alt_name).to_string());
        }
    }
    timings
}
