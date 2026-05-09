pub mod format;
pub mod surface;
pub mod text;
pub mod widgets;

use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::commands::AppPaths;
use crate::config::RenderConfig;
use crate::debug::{RenderProfiler, TimingBucket};
use crate::render::format::{format_value, frame_index_for_second};
use crate::render::surface::{create_surface, wrap_native_surface, write_surface_png};
use crate::render::text::{draw_text, label_style, value_style};
use crate::render::widgets::{
    draw_elevation_widget, draw_metric_value_widget_with_config, draw_route_widget,
    prepare_render_assets, PreparedRenderAssets, WidgetRenderReport,
};
use serde_json::{json, Value};
use skia_safe::Image;
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

#[derive(Clone, Copy, Debug)]
pub enum LabelCacheStatus {
    None,
    Hit,
    Miss,
}

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

#[derive(Clone)]
pub struct PreparedPreviewAssets {
    pub(crate) labels_image: Option<Image>,
    pub(crate) prepared_assets: PreparedRenderAssets,
}

pub struct RenderTarget<'a> {
    pub pixels: &'a mut [u8],
    pub width: u32,
    pub height: u32,
}

pub fn prepare_preview_assets(
    paths: &AppPaths,
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
) -> Result<
    (
        PreparedPreviewAssets,
        LabelCacheStatus,
        BTreeMap<String, TimingBucket>,
        f64,
    ),
    String,
> {
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

pub fn render_preview_to_path(
    paths: &AppPaths,
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    second: u32,
    out_path: &Path,
) -> Result<(), String> {
    render_preview_with_report(paths, config, activity, dense_activity, second, out_path)
        .map(|report| report.0)
}

pub fn render_preview_with_report(
    paths: &AppPaths,
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    second: u32,
    out_path: &Path,
) -> Result<((), PreviewRenderReport), String> {
    let (prepared_preview_assets, label_cache_status, prepare_timings, prepare_total_ms) =
        prepare_preview_assets(paths, config, activity, dense_activity)?;
    render_preview_with_prepared_assets(
        paths,
        config,
        dense_activity,
        &prepared_preview_assets,
        second,
        prepare_timings,
        label_cache_status,
        prepare_total_ms,
        out_path,
    )
}

pub fn render_preview_with_prepared_assets(
    paths: &AppPaths,
    config: &RenderConfig,
    dense_activity: &DenseActivityReport,
    prepared_preview_assets: &PreparedPreviewAssets,
    second: u32,
    prepare_timings: BTreeMap<String, TimingBucket>,
    label_cache_status: LabelCacheStatus,
    extra_total_ms: f64,
    out_path: &Path,
) -> Result<((), PreviewRenderReport), String> {
    let width = config.scene.width.unwrap_or(1920);
    let height = config.scene.height.unwrap_or(1080);
    let scale = config.scene.scale.unwrap_or(1.0).max(0.1);
    let frame_index = frame_index_for_second(config, dense_activity, second);
    let mut frame_profiler = RenderProfiler::default();
    let mut preview_profiler = RenderProfiler::default();
    let total_started = Instant::now();

    let (mut surface, route_widget, elevation_widget) = render_frame_surface(
        paths,
        config,
        dense_activity,
        &prepared_preview_assets.prepared_assets,
        frame_index,
        scale,
        prepared_preview_assets.labels_image.as_ref(),
        &mut frame_profiler,
        Some(&mut preview_profiler),
    )?;

    preview_profiler.measure("preview.png_write", || {
        write_surface_png(&mut surface, out_path)
            .map_err(|error| format!("Failed to render preview frame: {error}"))
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
        second,
        frame_index,
        width,
        height,
        total_ms: total_started.elapsed().as_secs_f64() * 1000.0 + extra_total_ms,
        surface_ms,
        label_layer_ms,
        value_draw_ms,
        png_write_ms,
        value_count: config.values.len(),
        label_count: config.labels.len(),
        label_cache_status: match label_cache_status {
            LabelCacheStatus::None => "none".to_string(),
            LabelCacheStatus::Hit => "hit".to_string(),
            LabelCacheStatus::Miss => "miss".to_string(),
        },
        route_widget,
        elevation_widget,
        prepare_timings,
        frame_timings,
        preview_only_timings,
    };

    Ok(((), report))
}

pub fn render_frame_rgba(
    paths: &AppPaths,
    config: &RenderConfig,
    dense_activity: &DenseActivityReport,
    prepared_assets: &PreparedRenderAssets,
    frame_index: usize,
    scale: f32,
    labels_image: Option<&Image>,
    target: RenderTarget<'_>,
    frame_profiler: &mut RenderProfiler,
) -> Result<(), String> {
    let width = target.width;
    let height = target.height;
    let mut labels_image = labels_image;
    if let Some(base_rgba) = prepared_assets
        .base_rgba
        .as_ref()
        .filter(|base_rgba| base_rgba.len() == target.pixels.len())
    {
        let started = Instant::now();
        target.pixels.copy_from_slice(base_rgba);
        let restore_ms = started.elapsed().as_secs_f64() * 1000.0;
        frame_profiler.record_ms("base.restore", restore_ms);
        frame_profiler.record_ms("surface.restore", restore_ms);
        labels_image = None;
    } else {
        frame_profiler.measure("surface.clear", || {
            target.pixels.fill(0);
        });
    }

    let mut surface = frame_profiler.measure("surface.create", || {
        wrap_native_surface(width, height, target.pixels)
    })?;
    let _ = render_frame_to_surface(
        surface.canvas(),
        paths,
        config,
        dense_activity,
        prepared_assets,
        frame_index,
        scale,
        labels_image,
        frame_profiler,
    );
    Ok(())
}

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
) -> Result<
    (
        skia_safe::Surface,
        Option<WidgetRenderReport>,
        Option<WidgetRenderReport>,
    ),
    String,
> {
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
        frame_profiler,
    );
    Ok((surface, widgets.0, widgets.1))
}

fn render_frame_to_surface(
    canvas: &skia_safe::Canvas,
    paths: &AppPaths,
    config: &RenderConfig,
    dense_activity: &DenseActivityReport,
    prepared_assets: &PreparedRenderAssets,
    frame_index: usize,
    scale: f32,
    labels_image: Option<&Image>,
    frame_profiler: &mut RenderProfiler,
) -> (Option<WidgetRenderReport>, Option<WidgetRenderReport>) {
    let frame_started = Instant::now();
    if let Some(labels_image) = labels_image {
        frame_profiler.measure("base.restore", || {
            canvas.draw_image(labels_image, (0, 0), None);
        });
    }

    frame_profiler.measure("text.dynamic", || {
        for value in &config.values {
            let style = value_style(&config.scene, value, scale);
            if draw_metric_value_widget_with_config(
                canvas,
                config,
                value,
                &style,
                dense_activity,
                frame_index,
                scale,
                &paths.font_dirs,
            ) {
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
        draw_elevation_widget(
            canvas,
            paths,
            config.scene.font.as_deref(),
            cache,
            frame_index,
            frame_profiler,
        )
    });
    frame_profiler.record_ms("frame.draw", frame_started.elapsed().as_secs_f64() * 1000.0);
    (route_widget, elevation_widget)
}

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

fn cached_labels_image(
    paths: &AppPaths,
    config: &RenderConfig,
    width: u32,
    height: u32,
    scale: f32,
    prepare_profiler: &mut RenderProfiler,
) -> Result<(Option<Image>, LabelCacheStatus), String> {
    if config.labels.is_empty() {
        return Ok((None, LabelCacheStatus::None));
    }

    static CACHE: OnceLock<Mutex<HashMap<u64, Image>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let cache_key = labels_cache_key(config, width, height, scale)?;

    if let Ok(cache) = cache.lock() {
        if let Some(image) = cache.get(&cache_key) {
            return Ok((Some(image.clone()), LabelCacheStatus::Hit));
        }
    }

    let prepare_started = Instant::now();
    let mut surface =
        prepare_profiler.measure("create_base_image", || create_surface(width, height))?;
    prepare_profiler.measure("prepare.surface.clear", || {
        surface.canvas().clear(skia_safe::Color::TRANSPARENT);
    });
    prepare_profiler.measure("text.static.cache", || {
        for label in &config.labels {
            let style = label_style(&config.scene, label, scale);
            draw_text(surface.canvas(), &label.text, &style, &paths.font_dirs);
        }
    });
    let image = surface.image_snapshot();
    prepare_profiler.record_ms(
        "prepare_render_assets.total",
        prepare_started.elapsed().as_secs_f64() * 1000.0,
    );

    if let Ok(mut cache) = cache.lock() {
        cache.insert(cache_key, image.clone());
    }

    Ok((Some(image), LabelCacheStatus::Miss))
}

pub fn prepare_base_rgba(
    paths: &AppPaths,
    config: &RenderConfig,
    width: u32,
    height: u32,
    scale: f32,
    prepare_profiler: &mut RenderProfiler,
) -> Result<Option<Vec<u8>>, String> {
    let row_bytes = (width as usize) * 4;
    let mut pixels = vec![0u8; row_bytes * (height as usize)];
    if config.labels.is_empty() {
        return Ok(Some(pixels));
    }

    let mut surface = prepare_profiler.measure("create_base_image", || {
        wrap_native_surface(width, height, pixels.as_mut_slice())
    })?;
    prepare_profiler.measure("text.static.cache", || {
        for label in &config.labels {
            let style = label_style(&config.scene, label, scale);
            draw_text(surface.canvas(), &label.text, &style, &paths.font_dirs);
        }
    });
    drop(surface);
    Ok(Some(pixels))
}

fn labels_cache_key(
    config: &RenderConfig,
    width: u32,
    height: u32,
    scale: f32,
) -> Result<u64, String> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    scale.to_bits().hash(&mut hasher);
    serde_json::to_string(&config.scene)
        .map_err(|error| error.to_string())?
        .hash(&mut hasher);
    serde_json::to_string(&config.labels)
        .map_err(|error| error.to_string())?
        .hash(&mut hasher);
    Ok(hasher.finish())
}

pub fn stub_demo_response(filename: &str) -> Value {
    json!({ "filename": filename })
}

pub fn stub_render_response(config: &RenderConfig, dense_activity: &DenseActivityReport) -> Value {
    json!({
        "error": "Phase 3 partial: preview rendering is implemented, but video rendering is not implemented yet.",
        "error_code": "UNIMPLEMENTED",
        "validated": true,
        "frame_count": dense_activity.frame_count,
        "fps": config.scene.fps
    })
}
