//! Shared arc/corner track cache, range labels and inner value/unit layout.
mod geometry;
pub(crate) mod inner_widget;
mod layer;
mod path;
mod segment;

use super::{
    bar_geometry::{scale_bar_geometry, ResolvedBarGeometry},
    frame::{prepare_text, raster_bounds, TextRun},
    metric::format_gauge_label,
    validation::{ValidatedArcGaugeWidget, MAX_ARC_ANGLE_DEGREES},
};
use crate::{
    error::WidgetResult,
    model::{scene::ValidatedScenePresentation, types::TrackFillStyle},
    render::text::{
        origin_x_for_centered_text, parse_color, resolve_font, validated_value_style, FontResolver,
        ResolvedTextStyle,
    },
    widgets::value::preview::surface,
};
pub use geometry::{
    arc_gauge_geometry, arc_point, arc_radius, arc_start_end_angles, corner_gauge_geometry,
    corner_start_end_angles, ArcGaugeGeometry,
};
use inner_widget::{
    inner_widget_layout, unit_font_size, unit_style, ArcInnerWidgetLayout, DEFAULT_GAP_PX,
    LINE_HEIGHT,
};
#[cfg(test)]
pub(crate) use path::{ArcTrackSpec, RevealClip};
#[cfg(test)]
pub(crate) use segment::{rounded_segment_geometry, rounded_segment_path, segment_geometries};
use skia_safe::{Canvas, Color, Font, Image, Rect};

#[derive(Clone, Debug)]
pub struct ArcGaugeCache {
    pub static_image: Image,
    pub static_image_x: f32,
    pub static_image_y: f32,
    pub content_bounds: Rect,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub geometry: ArcGaugeGeometry,
    pub track_thickness: f32,
    pub track_corner_radius: f32,
    pub track_border_thickness: f32,
    pub track_filled_color: String,
    pub track_filled_opacity: f32,
    pub track_fill_flat: bool,
    pub track_fill_style: TrackFillStyle,
    pub bar_geometry: Option<ResolvedBarGeometry>,
    pub text_style: ResolvedTextStyle,
    pub value_font: Font,
    pub has_unit: bool,
    pub unit_font_size: f32,
    pub inner_widget_gap: f32,
    pub inner_widget_offset_x: f32,
    pub inner_widget_offset_y: f32,
}

pub fn prepare_arc_gauge_cache(
    gauge: &ValidatedArcGaugeWidget,
    scene: &ValidatedScenePresentation,
    range: (f64, f64),
    altitude_offset_m: f64,
    unit_text: Option<&str>,
    fonts: &dyn FontResolver,
) -> WidgetResult<ArcGaugeCache> {
    let scale = scene.scale;
    let width = gauge.width * scale;
    let height = gauge.height * scale;
    let thickness = gauge.track_thickness * scale;
    let border = gauge.track_border_thickness * scale;
    let corner = gauge.track_corner_radius * scale;
    let mut geometry = match gauge.corner_orientation {
        Some(orientation) => {
            corner_gauge_geometry(width, height, orientation, thickness, corner, border)
        }
        None => arc_gauge_geometry(width, height, gauge.arc_angle, thickness, border),
    };
    let text_style = validated_value_style(&gauge.inner_value, scene, scale);
    let value_font = resolve_font(fonts, text_style.font_name.as_deref(), text_style.font_size)?;
    let bars = scale_bar_geometry(gauge.bar_geometry, scale);
    let segments = bars
        .map(|bars| segment::segment_geometries(geometry, bars))
        .unwrap_or_else(|| vec![geometry]);
    let mut content = Rect::default();
    for segment in segments {
        if let Some(path) = segment::rounded_segment_path(segment, thickness, corner) {
            content.join(path.compute_tight_bounds());
        }
    }
    let mut bounds = content;
    if border > 0.0 && scene.shadow_strength > 0.0 {
        let mut shadow = content;
        shadow.outset((
            scene.shadow_strength * scale * 3.0,
            scene.shadow_strength * scale * 3.0,
        ));
        shadow.offset((scene.shadow_distance * scale, scene.shadow_distance * scale));
        bounds.join(shadow);
    }
    let mut texts = Vec::new();
    if gauge.show_min_max_labels {
        texts.extend(arc_labels(
            gauge,
            geometry,
            scale,
            fonts,
            range,
            &text_style,
            altitude_offset_m,
        )?);
    }
    let unit_size = unit_font_size(&text_style, scale);
    if let Some(text) = unit_text {
        let font = resolve_font(fonts, text_style.font_name.as_deref(), unit_size)?;
        let style = unit_style(gauge, geometry, scale, text, &font, &text_style);
        texts.push(prepare_text(text, text, style, fonts)?);
    }
    for text in &texts {
        content.join(text.content_bounds);
        bounds.join(text.bounds);
    }
    let bounds = raster_bounds(bounds);
    let mut surface = surface(bounds.width() as u32, bounds.height() as u32)?;
    let canvas = surface.canvas();
    canvas.clear(Color::TRANSPARENT);
    canvas.translate((-bounds.left, -bounds.top));
    layer::draw_static_layer(canvas, gauge, scene, geometry, scale, &text_style);
    for text in texts {
        text.draw(canvas);
    }
    content.offset((gauge.x, gauge.y));
    geometry.center_x += gauge.x;
    geometry.center_y += gauge.y;
    geometry.inner_widget_center_x += gauge.x;
    geometry.inner_widget_center_y += gauge.y;
    Ok(ArcGaugeCache {
        static_image: surface.image_snapshot(),
        static_image_x: gauge.x + bounds.left,
        static_image_y: gauge.y + bounds.top,
        content_bounds: content,
        x: gauge.x,
        y: gauge.y,
        width,
        height,
        rotation: gauge.rotation,
        geometry,
        track_thickness: thickness,
        track_corner_radius: corner,
        track_border_thickness: border,
        track_filled_color: gauge.track_filled_color.clone(),
        track_filled_opacity: gauge.track_filled_opacity,
        track_fill_flat: gauge.track_fill_flat,
        track_fill_style: gauge.track_fill_style,
        bar_geometry: bars,
        text_style,
        value_font,
        has_unit: unit_text.is_some(),
        unit_font_size: unit_size,
        inner_widget_gap: DEFAULT_GAP_PX * scale,
        inner_widget_offset_x: gauge.inner_widget_offset_x * scale,
        inner_widget_offset_y: gauge.inner_widget_offset_y * scale,
    })
}

impl ArcGaugeCache {
    pub fn raster_bounds(&self) -> Rect {
        Rect::from_xywh(
            self.static_image_x,
            self.static_image_y,
            self.static_image.width() as f32,
            self.static_image.height() as f32,
        )
    }
    pub(crate) fn inner_layout(&self) -> ArcInnerWidgetLayout {
        inner_widget_layout(
            self.geometry,
            self.inner_widget_offset_x,
            self.inner_widget_offset_y,
            &self.text_style,
            self.has_unit.then_some(self.unit_font_size),
            self.inner_widget_gap,
        )
    }
    pub fn draw(&self, canvas: &Canvas, fill01: f32) {
        canvas.draw_image(
            &self.static_image,
            (self.static_image_x, self.static_image_y),
            None,
        );
        if fill01 > 0.0 && self.geometry.radius > 0.0 {
            layer::draw_fill(canvas, self, self.geometry, fill01);
        }
    }
}

fn arc_labels(
    gauge: &ValidatedArcGaugeWidget,
    geometry: ArcGaugeGeometry,
    scale: f32,
    fonts: &dyn FontResolver,
    range: (f64, f64),
    text_style: &ResolvedTextStyle,
    altitude_offset_m: f64,
) -> WidgetResult<Vec<TextRun>> {
    let font_size = gauge.min_max_label_font_size * scale;
    let font = resolve_font(fonts, Some(&gauge.min_max_label_font), font_size)?;
    let mut style = text_style.clone();
    style.font_name = Some(gauge.min_max_label_font.clone());
    style.font_size = font_size;
    style.line_height = font_size * LINE_HEIGHT;
    style.color = parse_color(&gauge.min_max_label_color, text_style.opacity);
    let unit = Some(gauge.inner_value.display_unit.as_str());
    let min_label = format_gauge_label(gauge.metric, unit, range.0 + altitude_offset_m);
    let max_label = format_gauge_label(gauge.metric, unit, range.1 + altitude_offset_m);
    let angles = if geometry.sweep_angle.abs() >= MAX_ARC_ANGLE_DEGREES - f32::EPSILON {
        (180.0, 0.0)
    } else {
        (
            geometry.start_angle,
            geometry.start_angle + geometry.sweep_angle,
        )
    };
    let gap = (font_size * 0.35).max(8.0 * scale);
    let radius = geometry.radius
        + gauge.track_thickness * scale * 0.5
        + gauge.track_border_thickness * scale
        + gap;
    [(&min_label, angles.0), (&max_label, angles.1)]
        .into_iter()
        .map(|(text, angle)| {
            let anchor = arc_point(geometry.center_x, geometry.center_y, radius, angle);
            let mut style = style.clone();
            style.x = origin_x_for_centered_text(text, anchor.x, &font);
            style.y = anchor.y - style.line_height * 0.5;
            prepare_text(text, text, style, fonts)
        })
        .collect()
}
