//! Linear frame reflow and arc/corner intrinsic scaling, with exact content anchoring.
use super::{
    bar_geometry::{
        resolve_arc_track_geometry, resolve_bar_style_geometry, track_corner_radius_max,
    },
    frame::GaugeLayout,
    metric::sample_gauge,
    validation::{ValidatedGauge, ValidatedLinearGaugeOrientation},
};
use crate::{
    activity::ActivitySampler,
    render::preview::PreviewRasterScale,
    error::{WidgetError, WidgetResult},
    model::{
        scene::{validate_scene_presentation, ScenePresentationConfig},
        validation::require_positive_f32,
    },
    render::text::FontResolver,
    widgets::value::config::ValueConfig,
};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
    pub display_variants: BTreeMap<String, ResizeVariant>,
}

#[derive(Serialize)]
pub struct ResizeVariant {
    pub width: f32,
    pub height: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_thickness: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_corner_radius: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_border_thickness: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inner_widget_offset_x: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inner_widget_offset_y: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_max_label_font_size: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bar_gap: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tick_length: Option<f32>,
}

#[allow(clippy::too_many_arguments)]
pub fn resize_gauge(
    widget: ValueConfig,
    scene: ScenePresentationConfig,
    factor: Option<f32>,
    size: Option<[f32; 2]>,
    fixed: [f32; 2],
    point: [f32; 2],
    activity: Option<&ActivitySampler>,
    second: f64,
    fonts: &dyn FontResolver,
) -> WidgetResult<ResizeUpdate> {
    let mut widget = ValidatedGauge::new(widget)?;
    let scene = validate_scene_presentation(scene)?;
    let range = activity.map_or((0.0, 100.0), |activity| {
        activity.gauge_range(widget.metric())
    });
    let offset = activity.map_or(0.0, |activity| {
        activity.gauge_altitude_offset(widget.starting_altitude_m())
    });
    let origin = GaugeLayout::new(&widget, &scene, range, offset, fonts, PreviewRasterScale::NATIVE)?
        .prepare_frame(sample_gauge(&widget, activity, second), fonts)?
        .content_bounds;
    if origin.is_empty() {
        return Err(WidgetError::Render(
            "gauge has no visible content to resize".into(),
        ));
    }
    let (x, y, display_type, font_size, variant) =
        match &mut widget {
            ValidatedGauge::Linear(config) => {
                if factor.is_some() {
                    return Err(WidgetError::Config(
                        "linear gauge resize requires size".into(),
                    ));
                }
                let size = size.ok_or_else(|| {
                    WidgetError::Config("linear gauge resize size is required".into())
                })?;
                if size.iter().any(|value| !value.is_finite() || *value <= 0.0) {
                    return Err(WidgetError::Config("invalid gauge resize size".into()));
                }
                config.width =
                    (config.width + (size[0] - origin.width()) / scene.scale).clamp(8.0, 10_000.0);
                config.height = (config.height + (size[1] - origin.height()) / scene.scale)
                    .clamp(8.0, 10_000.0);
                let (span, cross_extent) = match config.orientation {
                    ValidatedLinearGaugeOrientation::Horizontal => (config.width, config.height),
                    ValidatedLinearGaugeOrientation::Vertical => (config.height, config.width),
                };
                if let Some(bars) = config.bar_geometry {
                    config.bar_geometry = resolve_bar_style_geometry(
                        config.track_fill_style,
                        span,
                        Some(bars.count),
                        Some(bars.gap),
                        "linear resize",
                    )?;
                }
                config.track_corner_radius = config.track_corner_radius.min(
                    track_corner_radius_max(cross_extent, span, config.bar_geometry.as_ref()),
                );
                (
                    config.x,
                    config.y,
                    "linear",
                    None,
                    ResizeVariant {
                        width: config.width,
                        height: config.height,
                        track_thickness: None,
                        track_corner_radius: Some(config.track_corner_radius),
                        track_border_thickness: None,
                        inner_widget_offset_x: None,
                        inner_widget_offset_y: None,
                        min_max_label_font_size: None,
                        bar_gap: config.bar_geometry.map(|bars| bars.gap),
                        tick_length: None,
                    },
                )
            }
            ValidatedGauge::Arc(config) => {
                if size.is_some() {
                    return Err(WidgetError::Config(
                        "arc gauge resize requires factor".into(),
                    ));
                }
                let factor = require_positive_f32(factor, "gauge resize factor")?;
                config.width = (config.width * factor).clamp(8.0, 10_000.0);
                config.height = (config.height * factor).clamp(8.0, 10_000.0);
                config.inner_value.font_size =
                    (config.inner_value.font_size * factor).clamp(8.0, 400.0);
                config.track_thickness = (config.track_thickness * factor).clamp(1.0, 100.0);
                config.track_corner_radius =
                    (config.track_corner_radius * factor).clamp(0.0, config.track_thickness * 0.5);
                config.track_border_thickness =
                    (config.track_border_thickness * factor).clamp(0.0, 24.0);
                config.inner_widget_offset_x =
                    (config.inner_widget_offset_x * factor).clamp(-10_000.0, 10_000.0);
                config.inner_widget_offset_y =
                    (config.inner_widget_offset_y * factor).clamp(-10_000.0, 10_000.0);
                config.min_max_label_font_size =
                    (config.min_max_label_font_size * factor).clamp(6.0, 50.0);
                if let Some(style) = config.gt_style.as_mut() {
                    style.tick_length = (style.tick_length * factor).clamp(0.0, 100.0);
                }
                let bars = config.bar_geometry;
                (config.track_corner_radius, config.bar_geometry) = resolve_arc_track_geometry(
                    config.width.min(config.height),
                    config.track_thickness,
                    config.track_corner_radius,
                    config.track_border_thickness,
                    config.arc_angle,
                    config.corner_orientation.is_some(),
                    config.track_fill_style,
                    bars.map(|bars| bars.count),
                    bars.map(|bars| bars.gap),
                    "arc resize",
                )?;
                (
                    config.x,
                    config.y,
                    config.display_type.as_str(),
                    Some(config.inner_value.font_size),
                    ResizeVariant {
                        width: config.width,
                        height: config.height,
                        track_thickness: Some(config.track_thickness),
                        track_corner_radius: Some(config.track_corner_radius),
                        track_border_thickness: Some(config.track_border_thickness),
                        inner_widget_offset_x: Some(config.inner_widget_offset_x),
                        inner_widget_offset_y: Some(config.inner_widget_offset_y),
                        min_max_label_font_size: Some(config.min_max_label_font_size),
                        bar_gap: config.bar_geometry.map(|bars| bars.gap),
                        tick_length: config.gt_style.as_ref().map(|style| style.tick_length),
                    },
                )
            }
        };
    let display_type = display_type.to_owned();
    let bounds = GaugeLayout::new(&widget, &scene, range, offset, fonts, PreviewRasterScale::NATIVE)?
        .prepare_frame(sample_gauge(&widget, activity, second), fonts)?
        .content_bounds;
    Ok(ResizeUpdate {
        x: x + point[0] - (bounds.left + bounds.width() * fixed[0]),
        y: y + point[1] - (bounds.top + bounds.height() * fixed[1]),
        font_size,
        display_variants: BTreeMap::from([(display_type, variant)]),
    })
}
