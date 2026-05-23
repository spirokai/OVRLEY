//! Shared static label and icon rendering for preview and video paths.
//!
//! This module owns the reusable static overlay layer that is drawn before any
//! per-frame metric values or plot widgets. It keeps the label-image cache
//! private, exposes helper functions for preview/video preparation, and ensures
//! the cached-image and base-RGBA paths use the same static drawing loop.

use super::LabelCacheStatus;
use crate::commands::AppPaths;
use crate::config::RenderConfig;
use crate::debug::RenderProfiler;
use crate::error::CoreResult;
use crate::render::surface::{create_surface, wrap_native_surface};
use crate::render::text::{draw_text, label_style, value_style};
use crate::render::widgets::{draw_static_metric_icon_for_value, has_static_metric_icon};
use skia_safe::Image;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Returns a cached static label/icon image or renders and caches a new one.
///
/// The cache key covers every config input that can change static pixels, so
/// different render configurations cannot reuse stale images across renders.
pub(super) fn cached_labels_image(
    paths: &AppPaths,
    config: &RenderConfig,
    width: u32,
    height: u32,
    scale: f32,
    prepare_profiler: &mut RenderProfiler,
) -> CoreResult<(Option<Image>, LabelCacheStatus)> {
    if config.labels.is_empty() && !config_has_static_metric_icons(config) {
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
        draw_static_text_and_icons(surface.canvas(), paths, config, scale);
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

/// Pre-renders static labels and icons into a reusable RGBA base buffer.
///
/// Video rendering restores this buffer into each frame before drawing dynamic
/// values so the hot path does not have to redraw static content repeatedly.
pub fn prepare_base_rgba(
    paths: &AppPaths,
    config: &RenderConfig,
    width: u32,
    height: u32,
    scale: f32,
    prepare_profiler: &mut RenderProfiler,
) -> CoreResult<Option<Vec<u8>>> {
    let row_bytes = (width as usize) * 4;
    let mut pixels = vec![0u8; row_bytes * (height as usize)];
    if config.labels.is_empty() && !config_has_static_metric_icons(config) {
        return Ok(Some(pixels));
    }

    let mut surface = prepare_profiler.measure("create_base_image", || {
        wrap_native_surface(width, height, pixels.as_mut_slice())
    })?;
    prepare_profiler.measure("text.static.cache", || {
        draw_static_text_and_icons(surface.canvas(), paths, config, scale);
    });
    drop(surface);
    Ok(Some(pixels))
}

/// Returns whether any configured metric widget contributes a static icon.
pub(super) fn config_has_static_metric_icons(config: &RenderConfig) -> bool {
    config.values.iter().any(has_static_metric_icon)
}

/// Draws the full static text-and-icon layer shared by preview and video prep.
///
/// This shared loop is the single source of truth for static overlay content so
/// cached preview images and copied RGBA base buffers cannot drift apart.
fn draw_static_text_and_icons(
    canvas: &skia_safe::Canvas,
    paths: &AppPaths,
    config: &RenderConfig,
    scale: f32,
) {
    for label in &config.labels {
        let style = label_style(&config.scene, label, scale);
        draw_text(canvas, &label.text, &style, &paths.font_dirs);
    }
    draw_static_metric_icons(canvas, paths, config, scale);
}

/// Computes the cache key for the shared static label/icon layer.
fn labels_cache_key(config: &RenderConfig, width: u32, height: u32, scale: f32) -> CoreResult<u64> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    scale.to_bits().hash(&mut hasher);
    serde_json::to_string(&config.scene)?.hash(&mut hasher);
    serde_json::to_string(&config.labels)?.hash(&mut hasher);
    serde_json::to_string(&config.values)?.hash(&mut hasher);
    Ok(hasher.finish())
}

/// Draws all metric icons whose pixels do not depend on the current frame.
fn draw_static_metric_icons(
    canvas: &skia_safe::Canvas,
    paths: &AppPaths,
    config: &RenderConfig,
    scale: f32,
) {
    for value in &config.values {
        let style = value_style(&config.scene, value, scale);
        draw_static_metric_icon_for_value(canvas, value, &style, scale, &paths.font_dirs);
    }
}
