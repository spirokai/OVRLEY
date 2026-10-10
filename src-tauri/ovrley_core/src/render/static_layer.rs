//! Shared static backdrop, raster, label and metric rendering for preview and video.
//!
//! This module owns the reusable static overlay layer that is drawn before any
//! per-frame metric values or plot widgets. It keeps the label-image cache
//! private and derives preview images and video base pixels from one static draw.

use super::LabelCacheStatus;
use crate::debug::RenderProfiler;
use crate::error::{CoreError, CoreResult};
use crate::normalize::{ValidatedBackdrop, ValidatedLabel, ValidatedSceneConfig};
use crate::paths::AppPaths;
use crate::render::raster::{draw_rasters, PreparedRaster};
use crate::render::surface::{create_surface, native_n32_image_info};
use crate::render::text::{draw_text, validated_label_style, validated_value_style};
use crate::render::widgets::types::PreparedValue;
use crate::render::widgets::{
    draw_backdrops_static_layer, draw_static_metric_parts_for_value, static_metric_parts_for_value,
};
use skia_safe::Image;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Immutable inputs for the static layer, borrowed only during preparation.
pub struct StaticLayer<'a> {
    pub backdrops: &'a [ValidatedBackdrop],
    pub rasters: &'a [PreparedRaster],
    pub labels: &'a [ValidatedLabel],
    pub values: &'a [PreparedValue],
    pub scene: &'a ValidatedSceneConfig,
}

/// Preview and video consume the same pixels produced by one static draw.
pub struct PreparedStaticLayer {
    pub image: Option<Image>,
    pub base_rgba: Vec<u8>,
    pub cache_status: LabelCacheStatus,
}

impl StaticLayer<'_> {
    pub fn prepare(
        &self,
        paths: &AppPaths,
        profiler: &mut RenderProfiler,
    ) -> CoreResult<PreparedStaticLayer> {
        let (image, cache_status) = self.cached_image(paths, profiler)?;
        let frame_size = super::FrameSize {
            width: self.scene.presentation.width,
            height: self.scene.presentation.height,
        };
        let mut base_rgba = vec![0; frame_size.rgba_len()?];
        if let Some(image) = &image {
            let info = native_n32_image_info(frame_size.width, frame_size.height);
            let copied = profiler.measure("prepare.base.read_pixels", || {
                image.read_pixels(
                    &info,
                    &mut base_rgba,
                    frame_size.width as usize * 4,
                    (0, 0),
                    skia_safe::image::CachingHint::Allow,
                )
            });
            if !copied {
                return Err(CoreError::Render(
                    "Failed to read static layer pixels".into(),
                ));
            }
        }
        Ok(PreparedStaticLayer {
            image,
            base_rgba,
            cache_status,
        })
    }

    fn cached_image(
        &self,
        paths: &AppPaths,
        profiler: &mut RenderProfiler,
    ) -> CoreResult<(Option<Image>, LabelCacheStatus)> {
        if self.backdrops.is_empty()
            && self.rasters.is_empty()
            && self.labels.is_empty()
            && !config_has_static_metric_parts(self.values)
        {
            return Ok((None, LabelCacheStatus::None));
        }
        static CACHE: OnceLock<Mutex<HashMap<u64, Image>>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        let key = self.cache_key(&paths.font_dirs);
        if let Some(image) = cache.lock().unwrap().get(&key) {
            return Ok((Some(image.clone()), LabelCacheStatus::Hit));
        }
        let started = Instant::now();
        let mut surface = profiler.measure("create_base_image", || {
            create_surface(
                self.scene.presentation.width,
                self.scene.presentation.height,
            )
        })?;
        profiler.measure("prepare.surface.clear", || {
            surface.canvas().clear(skia_safe::Color::TRANSPARENT);
        });
        profiler.measure("text.static.cache", || {
            self.draw(surface.canvas(), &paths.font_dirs)
        })?;
        let image = surface.image_snapshot();
        profiler.record_ms(
            "prepare_render_assets.total",
            started.elapsed().as_secs_f64() * 1000.0,
        );
        cache.lock().unwrap().insert(key, image.clone());
        Ok((Some(image), LabelCacheStatus::Miss))
    }

    fn draw(&self, canvas: &skia_safe::Canvas, font_dirs: &[std::path::PathBuf]) -> CoreResult<()> {
        let scene = self.scene;
        let scale = scene.presentation.scale;
        draw_backdrops_static_layer(canvas, self.backdrops, scale);
        draw_rasters(canvas, self.rasters, scale, scene.presentation.opacity);
        for label in self.labels {
            let style = validated_label_style(label, scene, scale);
            draw_text(canvas, &label.text, &style, font_dirs)?;
        }
        for value in self.values.iter().filter_map(text_value) {
            let parts = static_metric_parts_for_value(value);
            if parts.icon || parts.unit {
                let style = validated_value_style(value, scene, scale);
                draw_static_metric_parts_for_value(canvas, value, &style, scale, font_dirs)?;
            }
        }
        Ok(())
    }

    fn cache_key(&self, font_dirs: &[std::path::PathBuf]) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        font_dirs.hash(&mut hasher);
        format!("{:?}", self.scene).hash(&mut hasher);
        format!("{:?}", self.backdrops).hash(&mut hasher);
        for raster in self.rasters {
            format!("{:?}", raster.geometry).hash(&mut hasher);
            raster.content_hash.hash(&mut hasher);
        }
        format!("{:?}", self.labels).hash(&mut hasher);
        format!("{:?}", self.values).hash(&mut hasher);
        hasher.finish()
    }
}

/// Returns whether any configured metric widget contributes a static part.
pub(super) fn config_has_static_metric_parts(values: &[PreparedValue]) -> bool {
    values
        .iter()
        .filter_map(text_value)
        .map(static_metric_parts_for_value)
        .any(|parts| parts.icon || parts.unit)
}

fn text_value(value: &PreparedValue) -> Option<&crate::normalize::ValidatedValueWidget> {
    match value {
        PreparedValue::StandardText(prepared) => Some(&prepared.validated),
        PreparedValue::TimeText(validated) => Some(&validated.base),
        PreparedValue::Gradient(_)
        | PreparedValue::HeadingTape(_)
        | PreparedValue::LeanAngle(_)
        | PreparedValue::LinearGauge(_)
        | PreparedValue::ArcGauge(_)
        | PreparedValue::GForce(_)
        | PreparedValue::LapTimer(_) => None,
    }
}
