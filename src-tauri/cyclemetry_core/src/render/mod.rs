pub mod format;
pub mod surface;
pub mod text;

use crate::activity::schema::DenseActivityReport;
use crate::commands::AppPaths;
use crate::config::RenderConfig;
use crate::render::format::{format_value, frame_index_for_second};
use crate::render::surface::{create_surface, write_surface_png};
use crate::render::text::{draw_text, label_style, value_style};
use serde_json::{json, Value};
use skia_safe::{Data, Image};
use std::collections::HashMap;
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
}

pub fn render_preview_to_path(
    paths: &AppPaths,
    config: &RenderConfig,
    dense_activity: &DenseActivityReport,
    second: u32,
    out_path: &Path,
) -> Result<(), String> {
    render_preview_with_report(paths, config, dense_activity, second, out_path).map(|report| report.0)
}

pub fn render_preview_with_report(
    paths: &AppPaths,
    config: &RenderConfig,
    dense_activity: &DenseActivityReport,
    second: u32,
    out_path: &Path,
) -> Result<((), PreviewRenderReport), String> {
    let total_started = Instant::now();
    let width = config.scene.width.unwrap_or(1920);
    let height = config.scene.height.unwrap_or(1080);
    let scale = config.scene.scale.unwrap_or(1.0).max(0.1);
    let frame_index = frame_index_for_second(config, dense_activity, second);

    let surface_started = Instant::now();
    let mut surface = create_surface(width, height)?;
    surface.canvas().clear(skia_safe::Color::TRANSPARENT);
    let surface_ms = surface_started.elapsed().as_secs_f64() * 1000.0;

    let label_started = Instant::now();
    let (labels_image, label_cache_status) =
        cached_labels_image(paths, config, width, height, scale)?;
    if let Some(labels_image) = labels_image {
        surface.canvas().draw_image(&labels_image, (0, 0), None);
    }
    let label_layer_ms = label_started.elapsed().as_secs_f64() * 1000.0;

    let value_started = Instant::now();
    for value in &config.values {
        let text = format_value(config, value, dense_activity, frame_index);
        let style = value_style(&config.scene, value, scale);
        draw_text(surface.canvas(), &text, &style, &paths.font_dirs);
    }
    let value_draw_ms = value_started.elapsed().as_secs_f64() * 1000.0;

    let png_started = Instant::now();
    write_surface_png(&mut surface, out_path)
        .map_err(|error| format!("Failed to render preview frame: {error}"))?;
    let png_write_ms = png_started.elapsed().as_secs_f64() * 1000.0;

    let report = PreviewRenderReport {
        second,
        frame_index,
        width,
        height,
        total_ms: total_started.elapsed().as_secs_f64() * 1000.0,
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
    };

    Ok(((), report))
}

fn cached_labels_image(
    paths: &AppPaths,
    config: &RenderConfig,
    width: u32,
    height: u32,
    scale: f32,
) -> Result<(Option<Image>, LabelCacheStatus), String> {
    if config.labels.is_empty() {
        return Ok((None, LabelCacheStatus::None));
    }

    static CACHE: OnceLock<Mutex<HashMap<u64, Vec<u8>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let cache_key = labels_cache_key(config, width, height, scale)?;

    if let Ok(cache) = cache.lock() {
        if let Some(bytes) = cache.get(&cache_key) {
            return Ok((
                Image::from_encoded(Data::new_copy(bytes)),
                LabelCacheStatus::Hit,
            ));
        }
    }

    let mut surface = create_surface(width, height)?;
    surface.canvas().clear(skia_safe::Color::TRANSPARENT);
    for label in &config.labels {
        let style = label_style(&config.scene, label, scale);
        draw_text(surface.canvas(), &label.text, &style, &paths.font_dirs);
    }
    let image = surface.image_snapshot();
    let encoded = image
        .encode(None, skia_safe::EncodedImageFormat::PNG, 100)
        .ok_or_else(|| "Failed to encode cached label layer".to_string())?;
    let bytes = encoded.as_bytes().to_vec();

    if let Ok(mut cache) = cache.lock() {
        cache.insert(cache_key, bytes.clone());
    }

    Ok((
        Image::from_encoded(Data::new_copy(&bytes)),
        LabelCacheStatus::Miss,
    ))
}

fn labels_cache_key(config: &RenderConfig, width: u32, height: u32, scale: f32) -> Result<u64, String> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    scale.to_bits().hash(&mut hasher);
    serde_json::to_string(&config.scene).map_err(|error| error.to_string())?.hash(&mut hasher);
    serde_json::to_string(&config.labels).map_err(|error| error.to_string())?.hash(&mut hasher);
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
