//! Skia overlay rendering.
//!
//! Rendering is split into preparation and per-frame composition. Preparation
//! resolves static labels, widget geometry, and reusable base pixels. Per-frame
//! rendering restores that base, draws dynamic metric values, and composites
//! route/elevation widgets. The same primitives power preview PNG generation
//! and video frame streaming.

/// Value formatting and metric display helpers.
pub mod format;
/// Shared static label/metric-part caching and base-layer preparation helpers.
mod static_layer;
/// Skia surface allocation and PNG output helpers.
pub mod surface;
/// Font resolution, text measurement, and text drawing helpers.
pub mod text;
/// Route, elevation, and metric widget rendering.
pub mod widgets;

use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::debug::{RenderProfiler, TimingBucket};
use crate::error::{CoreError, CoreResult};
use crate::normalize::ValidatedRenderConfig;
use crate::normalize::ValidatedSceneConfig;
use crate::paths::AppPaths;
use crate::render::format::frame_index_for_second;
use crate::render::static_layer::{cached_labels_image, config_has_static_metric_parts};
use crate::render::surface::{create_surface, wrap_native_surface, write_surface_png};
use crate::render::text::{
    validated_gradient_style, validated_lap_timer_style, validated_time_style,
    validated_value_style,
};
use crate::render::widgets::types::PreparedValue;
use crate::render::widgets::value::MetricWidgetRequest;
use crate::render::widgets::{
    draw_elevation_widget, draw_metric_presentation, draw_metric_value_widget_with_config,
    draw_route_widget, prepare_render_assets, static_metric_parts_for_value,
    MetricPresentationReport, PreparedRenderAssets, StaticMetricParts, WidgetRenderReport,
};
use crate::standard_metrics::{display_type_layout_mode, DisplayTypeLayoutMode};
use skia_safe::Canvas;
use skia_safe::Image;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

pub use self::static_layer::prepare_base_rgba;

/// Indicates whether the static label layer was not needed, reused, or rebuilt.
#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LabelCacheStatus {
    /// No static labels or static metric parts were present.
    None,
    /// A previously rendered static label image was reused.
    Hit,
    /// Static label image was rendered and inserted into the cache.
    Miss,
}

/// Serializable performance and geometry report for one preview render.
#[derive(Clone, Debug, serde::Serialize)]
pub struct PreviewRenderReport {
    pub second: f64,
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
    pub label_cache_status: LabelCacheStatus,
    pub route_widget: Option<WidgetRenderReport>,
    pub elevation_widget: Option<WidgetRenderReport>,
    pub metric_presentations: Vec<MetricPresentationReport>,
    pub prepare_timings: BTreeMap<String, TimingBucket>,
    pub frame_timings: BTreeMap<String, TimingBucket>,
    pub preview_only_timings: BTreeMap<String, TimingBucket>,
}

/// Assets prepared once and reused by preview/video frame rendering.
#[derive(Clone)]
pub struct PreparedPreviewAssets {
    /// Cached static label/metric-part layer for preview surfaces.
    pub(crate) labels_image: Option<Image>,
    /// Widget caches and optional base RGBA bytes.
    pub(crate) prepared_assets: PreparedRenderAssets,
}

impl PreparedPreviewAssets {
    pub fn scene(&self) -> &ValidatedSceneConfig {
        &self.prepared_assets.scene
    }

    /// Returns the elevation geometry as a JSON value for parity tests.
    pub fn elevation_geometry_json(&self) -> Option<serde_json::Value> {
        self.prepared_assets.elevation_geometry_json()
    }

    /// Returns the route geometry as a JSON value for parity tests.
    pub fn route_geometry_json(&self) -> Option<serde_json::Value> {
        self.prepared_assets.route_geometry_json()
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

/// Prepared, immutable context shared by video frame-render workers.
///
/// Construction resolves the preview-owned asset bundle into the video
/// renderer's strict contract once. Per-frame callers then provide only the
/// activity-frame index, destination pixels, and profiler.
#[derive(Clone, Copy)]
pub struct VideoFrameRenderer<'a> {
    paths: &'a AppPaths,
    dense_activity: &'a DenseActivityReport,
    prepared_assets: &'a PreparedRenderAssets,
    frame_size: FrameSize,
    scale: f32,
    base_rgba: &'a [u8],
    blank_leading_frame_count: usize,
}

impl<'a> VideoFrameRenderer<'a> {
    pub fn new(
        paths: &'a AppPaths,
        dense_activity: &'a DenseActivityReport,
        prepared_preview_assets: &'a PreparedPreviewAssets,
        frame_size: FrameSize,
        blank_leading_frame_count: u64,
    ) -> CoreResult<Self> {
        let expected_len = frame_size.rgba_len()?;
        let base_rgba = prepared_preview_assets
            .prepared_assets
            .base_rgba
            .as_deref()
            .ok_or_else(|| {
                CoreError::Render("Prepared video assets have no RGBA base layer".into())
            })?;
        if base_rgba.len() != expected_len {
            return Err(CoreError::Render(format!(
                "Prepared RGBA base layer has {} bytes; expected {expected_len} for {}x{} video frames",
                base_rgba.len(), frame_size.width, frame_size.height
            )));
        }
        let blank_leading_frame_count = usize::try_from(blank_leading_frame_count)
            .map_err(|_| CoreError::Render("Blank leading frame count exceeds usize".into()))?;

        Ok(Self {
            paths,
            dense_activity,
            prepared_assets: &prepared_preview_assets.prepared_assets,
            frame_size,
            scale: prepared_preview_assets.scene().scale,
            base_rgba,
            blank_leading_frame_count,
        })
    }

    pub fn render_rgba(
        &self,
        frame_index: usize,
        pixels: &mut [u8],
        frame_profiler: &mut RenderProfiler,
    ) -> CoreResult<()> {
        if pixels.len() != self.base_rgba.len() {
            return Err(CoreError::Render(format!(
                "Video frame buffer has {} bytes; expected {}",
                pixels.len(),
                self.base_rgba.len()
            )));
        }

        let started = Instant::now();
        let activity_frame_index = prepare_output_frame(
            frame_index,
            self.blank_leading_frame_count,
            self.dense_activity.frame_count,
            pixels,
        );
        let Some(activity_frame_index) = activity_frame_index else {
            frame_profiler.record_ms(
                "blank.clear",
                Instant::now().duration_since(started).as_secs_f64() * 1000.0,
            );
            return Ok(());
        };

        pixels.copy_from_slice(self.base_rgba);
        let restore_ms = started.elapsed().as_secs_f64() * 1000.0;
        frame_profiler.record_ms("base.restore", restore_ms);
        frame_profiler.record_ms("surface.restore", restore_ms);

        let mut surface = frame_profiler.measure("surface.create", || {
            wrap_native_surface(self.frame_size.width, self.frame_size.height, pixels)
        })?;
        let _ = render_frame_to_surface(
            surface.canvas(),
            self.paths,
            self.dense_activity,
            self.prepared_assets,
            activity_frame_index,
            self.scale,
            None,
            true,
            frame_profiler,
        )?;
        Ok(())
    }
}

fn prepare_output_frame(
    output_frame_index: usize,
    blank_leading_frame_count: usize,
    activity_frame_count: usize,
    pixels: &mut [u8],
) -> Option<usize> {
    let activity_frame_index = output_frame_index.checked_sub(blank_leading_frame_count);
    if activity_frame_index.is_none_or(|index| index >= activity_frame_count) {
        pixels.fill(0);
        return None;
    }
    activity_frame_index
}

#[cfg(test)]
mod video_frame_mapping_tests {
    use super::prepare_output_frame;

    #[test]
    fn clears_frames_outside_activity_and_maps_frames_inside_it() {
        let mut pixels = [255; 4];
        assert_eq!(prepare_output_frame(0, 2, 3, &mut pixels), None);
        assert_eq!(pixels, [0; 4]);

        pixels.fill(255);
        assert_eq!(prepare_output_frame(2, 2, 3, &mut pixels), Some(0));
        assert_eq!(pixels, [255; 4]);

        assert_eq!(prepare_output_frame(4, 2, 3, &mut pixels), Some(2));
        assert_eq!(prepare_output_frame(5, 2, 3, &mut pixels), None);
        assert_eq!(pixels, [0; 4]);
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
    let mut prepared_assets = prepare_render_assets(
        paths,
        config,
        activity,
        dense_activity,
        &mut prepare_profiler,
    )?;
    let (labels_image, label_cache_status) = cached_labels_image(
        paths,
        &prepared_assets.backdrops,
        &prepared_assets.labels,
        &prepared_assets.values,
        &prepared_assets.scene,
        &mut prepare_profiler,
    )?;
    prepared_assets.base_rgba = prepare_base_rgba(
        paths,
        &prepared_assets.backdrops,
        &prepared_assets.labels,
        &prepared_assets.values,
        &prepared_assets.scene,
        &mut prepare_profiler,
    )?;
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
    config: &ValidatedRenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    second: f64,
    out_path: &Path,
) -> CoreResult<()> {
    render_preview_with_report(paths, config, activity, dense_activity, second, out_path)
        .map(|_| ())
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
    let width = request.prepared_preview_assets.scene().width;
    let height = request.prepared_preview_assets.scene().height;
    let scale = request.prepared_preview_assets.scene().scale;
    let frame_index = frame_index_for_second(
        request.prepared_preview_assets.scene(),
        request.dense_activity,
        request.second,
    );
    let mut frame_profiler = RenderProfiler::default();
    let mut preview_profiler = RenderProfiler::default();
    let total_started = Instant::now();

    // Phase 2: draw all overlay layers into a Skia surface.
    let (mut surface, route_widget, elevation_widget, metric_presentations) = render_frame_surface(
        request.paths,
        request.dense_activity,
        &request.prepared_preview_assets.prepared_assets,
        frame_index,
        scale,
        request.prepared_preview_assets.labels_image.as_ref(),
        &mut frame_profiler,
        Some(&mut preview_profiler),
    )?;

    // Phase 3: encode the surface snapshot as PNG to disk.
    preview_profiler.measure("preview.png_write", || {
        write_surface_png(&mut surface, request.out_path)
            .map_err(|error| CoreError::Render(format!("Failed to render preview frame: {error}")))
    })?;

    // Phase 4: collect timing data and build the preview report.
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
        value_count: request.prepared_preview_assets.prepared_assets.values.len(),
        label_count: request.prepared_preview_assets.prepared_assets.labels.len(),
        label_cache_status: request.label_cache_status,
        route_widget,
        elevation_widget,
        metric_presentations,
        prepare_timings: request.prepare_timings,
        frame_timings,
        preview_only_timings,
    };

    Ok(report)
}

// Creates an owned Skia surface and renders one preview frame onto it.
#[allow(clippy::too_many_arguments)]
fn render_frame_surface(
    paths: &AppPaths,
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
    Vec<MetricPresentationReport>,
)> {
    // Preview rendering owns its surface and writes a PNG, while video rendering
    // wraps caller-owned pixels. This helper is the preview-side equivalent of
    // `render_frame_rgba`.
    let width = prepared_assets.scene.width;
    let height = prepared_assets.scene.height;
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
        dense_activity,
        prepared_assets,
        frame_index,
        scale,
        labels_image,
        false,
        frame_profiler,
    )?;
    Ok((surface, widgets.0, widgets.1, widgets.2))
}

// Draws all overlay layers for one frame onto an existing Skia canvas.
#[allow(clippy::too_many_arguments)]
fn render_frame_to_surface(
    canvas: &Canvas,
    paths: &AppPaths,
    dense_activity: &DenseActivityReport,
    prepared_assets: &PreparedRenderAssets,
    frame_index: usize,
    scale: f32,
    labels_image: Option<&Image>,
    base_layer_restored: bool,
    frame_profiler: &mut RenderProfiler,
) -> CoreResult<(
    Option<WidgetRenderReport>,
    Option<WidgetRenderReport>,
    Vec<MetricPresentationReport>,
)> {
    let frame_started = Instant::now();
    if let Some(labels_image) = labels_image {
        frame_profiler.measure("base.restore", || {
            canvas.draw_image(labels_image, (0, 0), None);
        });
    }

    let static_metric_parts_rendered = config_has_static_metric_parts(&prepared_assets.values)
        && (labels_image.is_some() || base_layer_restored);

    // Phase 1: Intrinsic text rendering (gradient + text display types).
    // This phase is timed as "text.dynamic" for report compatibility.
    let mut boxed_values: Vec<(usize, &PreparedValue)> = Vec::new();
    frame_profiler.measure("text.dynamic", || -> CoreResult<()> {
        for (idx, value) in prepared_assets.values.iter().enumerate() {
            // Boxed display types (heading_tape, etc.) are handled in Phase 2
            // via metric_presentation dispatch, not the text widget path.
            if display_type_layout_mode(value.display_type()) == DisplayTypeLayoutMode::Boxed {
                boxed_values.push((idx, value));
                continue;
            }

            match value {
                PreparedValue::StandardText(prepared) => {
                    let validated = &prepared.validated;
                    let style = validated_value_style(validated, &prepared_assets.scene, scale);
                    let static_parts = if static_metric_parts_rendered {
                        static_metric_parts_for_value(validated)
                    } else {
                        StaticMetricParts::default()
                    };
                    draw_metric_value_widget_with_config(MetricWidgetRequest {
                        canvas,
                        metric_kind: validated.metric,
                        display_type: validated.display_type,
                        base_style: &style,
                        dense_activity,
                        frame_index,
                        scale,
                        font_dirs: &paths.font_dirs,
                        static_parts,
                        validated: Some(validated),
                        validated_gradient: None,
                        validated_time: None,
                        altitude_offset_m: prepared.altitude_offset_m,
                        timezone: None,
                    })?;
                }
                PreparedValue::TimeText(validated) => {
                    let style = validated_time_style(validated, &prepared_assets.scene, scale);
                    let static_parts = if static_metric_parts_rendered {
                        static_metric_parts_for_value(&validated.base)
                    } else {
                        StaticMetricParts::default()
                    };
                    draw_metric_value_widget_with_config(MetricWidgetRequest {
                        canvas,
                        metric_kind: crate::MetricKind::Time,
                        display_type: crate::DisplayType::Text,
                        base_style: &style,
                        dense_activity,
                        frame_index,
                        scale,
                        font_dirs: &paths.font_dirs,
                        static_parts,
                        validated: None,
                        validated_gradient: None,
                        validated_time: Some(validated),
                        altitude_offset_m: 0.0,
                        timezone: prepared_assets.timezone,
                    })?;
                }
                PreparedValue::Gradient(validated) => {
                    let style = validated_gradient_style(validated, &prepared_assets.scene, scale);
                    draw_metric_value_widget_with_config(MetricWidgetRequest {
                        canvas,
                        metric_kind: crate::MetricKind::Gradient,
                        display_type: crate::DisplayType::Text,
                        base_style: &style,
                        dense_activity,
                        frame_index,
                        scale,
                        font_dirs: &paths.font_dirs,
                        static_parts: StaticMetricParts::default(),
                        validated: None,
                        validated_gradient: Some(validated),
                        validated_time: None,
                        altitude_offset_m: 0.0,
                        timezone: None,
                    })?;
                }
                PreparedValue::LapTimer(widget) => {
                    let validated = &widget.validated;
                    let style = validated_lap_timer_style(validated, &prepared_assets.scene, scale);
                    let label_style = crate::render::text::validated_lap_timer_label_style(
                        validated,
                        &prepared_assets.scene,
                        scale,
                    );
                    let cache = widget.cache.as_ref().ok_or_else(|| {
                        CoreError::Render(format!("lap timer cache is missing for value {idx}"))
                    })?;
                    crate::render::widgets::lap_timer::draw_lap_timer(
                        canvas,
                        validated,
                        cache,
                        dense_activity,
                        frame_index,
                        &style,
                        &label_style,
                        &paths.font_dirs,
                    )?;
                }
                PreparedValue::HeadingTape(_)
                | PreparedValue::LeanAngle(_)
                | PreparedValue::LinearGauge(_)
                | PreparedValue::ArcGauge(_)
                | PreparedValue::GForce(_) => {}
            }
        }
        Ok(())
    })?;

    // Phase 2: boxed values draw through the typed cache they own.
    // A missing cache means preparation did not produce a drawable presentation.
    let mut metric_presentations = Vec::new();
    for (idx, value) in &boxed_values {
        // Dispatch reads the cache directly from this prepared value.
        let report = draw_metric_presentation(
            canvas,
            value,
            dense_activity,
            frame_index,
            scale,
            &paths.font_dirs,
            frame_profiler,
        );
        if let Some(report) = report {
            metric_presentations.push(MetricPresentationReport {
                value_idx: *idx,
                metric_kind: value.metric_kind(),
                display_type: value.display_type(),
                widget: report,
            });
        }
    }

    let route_widget = prepared_assets
        .route_cache
        .as_ref()
        .and_then(|cache| draw_route_widget(canvas, cache, frame_index, frame_profiler));
    let elevation_widget = if let Some(cache) = &prepared_assets.elevation_cache {
        draw_elevation_widget(
            canvas,
            paths,
            cache,
            frame_index,
            &prepared_assets.scene,
            frame_profiler,
        )?
    } else {
        None
    };
    frame_profiler.record_ms("frame.draw", frame_started.elapsed().as_secs_f64() * 1000.0);
    Ok((route_widget, elevation_widget, metric_presentations))
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
