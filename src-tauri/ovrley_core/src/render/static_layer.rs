//! Shared static label and icon rendering for preview and video paths.
//!
//! This module owns the reusable static overlay layer that is drawn before any
//! per-frame metric values or plot widgets. It keeps the label-image cache
//! private, exposes helper functions for preview/video preparation, and ensures
//! the cached-image and base-RGBA paths use the same static drawing loop.

use super::LabelCacheStatus;
use crate::debug::RenderProfiler;
use crate::error::CoreResult;
use crate::normalize::ValidatedSceneConfig;
use crate::paths::AppPaths;
use crate::render::surface::{create_surface, wrap_native_surface};
use crate::render::text::{draw_text, validated_label_style, validated_value_style};
use crate::render::widgets::types::PreparedValue;
use crate::render::widgets::{
    draw_static_metric_icon_for_value_validated, has_static_metric_icon_validated,
};
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
    labels: &[crate::normalize::ValidatedLabel],
    values: &[PreparedValue],
    scene: &ValidatedSceneConfig,
    prepare_profiler: &mut RenderProfiler,
) -> CoreResult<(Option<Image>, LabelCacheStatus)> {
    let width = scene.width;
    let height = scene.height;
    let scale = scene.scale;
    if labels.is_empty() && !config_has_static_metric_icons(values) {
        return Ok((None, LabelCacheStatus::None));
    }

    static CACHE: OnceLock<Mutex<HashMap<u64, Image>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let cache_key = labels_cache_key(labels, values, scene, width, height, scale);

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
        draw_static_text_and_icons(surface.canvas(), paths, labels, values, scene, scale)
    })?;
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
    labels: &[crate::normalize::ValidatedLabel],
    values: &[PreparedValue],
    scene: &ValidatedSceneConfig,
    prepare_profiler: &mut RenderProfiler,
) -> CoreResult<Option<Vec<u8>>> {
    let width = scene.width;
    let height = scene.height;
    let scale = scene.scale;
    let row_bytes = (width as usize) * 4;
    let mut pixels = vec![0u8; row_bytes * (height as usize)];
    if labels.is_empty() && !config_has_static_metric_icons(values) {
        return Ok(Some(pixels));
    }

    let mut surface = prepare_profiler.measure("create_base_image", || {
        wrap_native_surface(width, height, pixels.as_mut_slice())
    })?;
    prepare_profiler.measure("text.static.cache", || {
        draw_static_text_and_icons(surface.canvas(), paths, labels, values, scene, scale)
    })?;
    drop(surface);
    Ok(Some(pixels))
}

/// Returns whether any configured metric widget contributes a static icon.
pub(super) fn config_has_static_metric_icons(values: &[PreparedValue]) -> bool {
    values
        .iter()
        .filter_map(text_value)
        .any(has_static_metric_icon_validated)
}

/// Draws the full static text-and-icon layer shared by preview and video prep.
///
/// This shared loop is the single source of truth for static overlay content so
/// cached preview images and copied RGBA base buffers cannot drift apart.
fn draw_static_text_and_icons(
    canvas: &skia_safe::Canvas,
    paths: &AppPaths,
    labels: &[crate::normalize::ValidatedLabel],
    values: &[PreparedValue],
    scene: &ValidatedSceneConfig,
    scale: f32,
) -> CoreResult<()> {
    for validated in labels {
        let style = validated_label_style(validated, scene, scale);
        draw_text(canvas, &validated.text, &style, &paths.font_dirs)?;
    }
    draw_static_metric_icons(canvas, paths, values, scene, scale)?;
    Ok(())
}

/// Computes the cache key for the shared static label/icon layer.
fn labels_cache_key(
    labels: &[crate::normalize::ValidatedLabel],
    values: &[PreparedValue],
    scene: &ValidatedSceneConfig,
    width: u32,
    height: u32,
    scale: f32,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    scale.to_bits().hash(&mut hasher);
    format!("{scene:?}").hash(&mut hasher);
    format!("{labels:?}").hash(&mut hasher);
    format!("{values:?}").hash(&mut hasher);
    hasher.finish()
}

/// Draws all metric icons whose pixels do not depend on the current frame.
///
/// Validates standard metric text widgets upfront so validated icons use
/// zero backend-owned defaults.
fn draw_static_metric_icons(
    canvas: &skia_safe::Canvas,
    paths: &AppPaths,
    values: &[PreparedValue],
    scene: &ValidatedSceneConfig,
    scale: f32,
) -> CoreResult<()> {
    for validated in values.iter().filter_map(text_value) {
        if !has_static_metric_icon_validated(validated) {
            continue;
        }
        let style = validated_value_style(validated, scene, scale);
        draw_static_metric_icon_for_value_validated(
            canvas,
            validated,
            &style,
            scale,
            &paths.font_dirs,
        )?;
    }
    Ok(())
}

fn text_value(value: &PreparedValue) -> Option<&crate::normalize::ValidatedValueWidget> {
    match value {
        PreparedValue::StandardText(validated) => Some(validated),
        PreparedValue::TimeText(validated) => Some(&validated.base),
        PreparedValue::Gradient(_)
        | PreparedValue::HeadingTape(_)
        | PreparedValue::LinearGauge(_) => None,
    }
}
