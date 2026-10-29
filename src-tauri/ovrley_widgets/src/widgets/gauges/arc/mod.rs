//! Shared arc/corner track cache, range labels and inner value/unit layout.
mod geometry;
pub(crate) mod inner_widget;
mod layer;
mod path;
mod segment;

use super::{
    bar_geometry::{scale_bar_geometry, ResolvedBarGeometry},
    frame::{prepare_text, raster_bounds, ArcValueLayout, GaugeFrameLayout, TextRun},
    metric::format_gauge_label,
    validation::{ValidatedArcGaugeWidget, MAX_ARC_ANGLE_DEGREES},
};
use crate::{
    error::WidgetResult,
    model::{scene::ValidatedScenePresentation, types::TrackFillStyle},
    render::preview::{PreviewRasterScale, StaticRaster},
    render::text::{
        origin_x_for_centered_text, parse_color, resolve_font, validated_value_style, FontResolver,
        ResolvedTextStyle,
    },
    render::track_fill::TrackFill,
    widgets::value::preview::SHAPE_COLOR_TYPE,
};
pub use geometry::{
    arc_gauge_geometry, arc_point, arc_radius, arc_start_end_angles, corner_gauge_geometry,
    corner_start_end_angles, gt_gauge_geometry, gt_start_end_angles, ArcGaugeGeometry,
    GT_GAUGE_TRACK_ROTATION_DEGREES,
};
use inner_widget::{
    inner_widget_layout, unit_font_size, unit_style, value_unit_gap, ArcInnerWidgetLayout,
    LINE_HEIGHT,
};
#[cfg(test)]
pub(crate) use path::{ArcTrackSpec, RevealClip};
pub(crate) use segment::rounded_segment_path;
#[cfg(test)]
pub(crate) use segment::{rounded_segment_geometry, segment_geometries};
use skia_safe::{Canvas, Font, Point, Rect};

#[derive(Clone, Debug)]
pub struct ArcGaugeCache {
    /// Track, ticks and labels in gauge-local coordinates; the gauge origin is (x, y).
    pub(crate) static_layer: StaticRaster,
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
    pub fill: TrackFill,
    pub track_fill_flat: bool,
    pub track_fill_style: TrackFillStyle,
    pub bar_geometry: Option<ResolvedBarGeometry>,
    pub text_style: ResolvedTextStyle,
    pub value_font: Font,
    pub has_unit: bool,
    pub unit_font_size: f32,
    /// GT fill glow intensity; zero for arc and corner gauges.
    pub glow: f32,
    pub inner_widget_gap: f32,
    pub inner_widget_offset_x: f32,
    pub inner_widget_offset_y: f32,
}

/// Arc gauge layout before its static raster is drawn; frame bounds need nothing more.
pub(crate) struct ArcGaugeMeasure {
    /// Track geometry relative to the gauge origin, as the static raster draws it.
    local_geometry: ArcGaugeGeometry,
    texts: Vec<TextRun>,
    /// Static raster rectangle relative to the gauge origin.
    local_bounds: Rect,
    content_bounds: Rect,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    track_thickness: f32,
    track_corner_radius: f32,
    track_border_thickness: f32,
    track_fill_style: TrackFillStyle,
    bar_geometry: Option<ResolvedBarGeometry>,
    text_style: ResolvedTextStyle,
    value_font: Font,
    has_unit: bool,
    unit_font_size: f32,
    glow: f32,
    inner_widget_gap: f32,
    inner_widget_offset_x: f32,
    inner_widget_offset_y: f32,
}

pub(crate) fn measure_arc_gauge(
    gauge: &ValidatedArcGaugeWidget,
    scene: &ValidatedScenePresentation,
    range: (f64, f64),
    altitude_offset_m: f64,
    unit_text: Option<&str>,
    fonts: &dyn FontResolver,
) -> WidgetResult<ArcGaugeMeasure> {
    let scale = scene.scale;
    let width = gauge.width * scale;
    let height = gauge.height * scale;
    let thickness = gauge.track_thickness * scale;
    let border = gauge.track_border_thickness * scale;
    let corner = gauge.track_corner_radius * scale;
    // GT rotation is resolved into the cached angles once, never applied per frame.
    let geometry = match (gauge.corner_orientation, &gauge.gt_style) {
        (Some(orientation), _) => {
            corner_gauge_geometry(width, height, orientation, thickness, corner, border)
        }
        (None, Some(_)) => gt_gauge_geometry(width, height, gauge.arc_angle, thickness, border),
        (None, None) => arc_gauge_geometry(width, height, gauge.arc_angle, thickness, border),
    };
    let text_style = validated_value_style(&gauge.inner_value, scene, scale);
    let value_font = resolve_font(fonts, text_style.font_name.as_deref(), text_style.font_size)?;
    // GT gauges always render a continuous fill, whatever track style is configured.
    let (track_fill_style, bars) = if gauge.gt_style.is_some() {
        (TrackFillStyle::Fill, None)
    } else {
        (
            gauge.track_fill_style,
            scale_bar_geometry(gauge.bar_geometry, scale),
        )
    };
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
    let glow = gauge.gt_style.as_ref().map_or(0.0, |style| style.glow);
    if glow > 0.0 {
        let mut halo = content;
        let extent = layer::glow_sigma(thickness) * 3.0;
        halo.outset((extent, extent));
        bounds.join(halo);
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
        let style = unit_style(gauge, geometry, scene, text, &font, &text_style);
        texts.push(prepare_text(text, text, style, fonts)?);
    }
    for text in &texts {
        content.join(text.content_bounds);
        bounds.join(text.bounds);
    }
    content.offset((gauge.x, gauge.y));
    let inner_widget_gap = value_unit_gap(gauge, text_style.font_size, scale);
    Ok(ArcGaugeMeasure {
        local_geometry: geometry,
        texts,
        local_bounds: raster_bounds(bounds),
        content_bounds: content,
        x: gauge.x,
        y: gauge.y,
        width,
        height,
        track_thickness: thickness,
        track_corner_radius: corner,
        track_border_thickness: border,
        track_fill_style,
        bar_geometry: bars,
        text_style,
        value_font,
        has_unit: unit_text.is_some(),
        unit_font_size: unit_size,
        glow,
        inner_widget_gap,
        inner_widget_offset_x: gauge.inner_widget_offset_x * scale,
        inner_widget_offset_y: gauge.inner_widget_offset_y * scale,
    })
}

/// Scene geometry of the track; the static raster keeps the local copy.
fn placed_geometry(mut geometry: ArcGaugeGeometry, x: f32, y: f32) -> ArcGaugeGeometry {
    geometry.center_x += x;
    geometry.center_y += y;
    geometry.inner_widget_center_x += x;
    geometry.inner_widget_center_y += y;
    geometry
}

impl ArcGaugeMeasure {
    pub(crate) fn frame_layout(&self) -> GaugeFrameLayout<'_> {
        GaugeFrameLayout {
            content_bounds: self.content_bounds,
            raster_bounds: self.local_bounds.with_offset((self.x, self.y)),
            content_width: self.width,
            value: Some(ArcValueLayout {
                inner: inner_widget_layout(
                    placed_geometry(self.local_geometry, self.x, self.y),
                    self.inner_widget_offset_x,
                    self.inner_widget_offset_y,
                    &self.text_style,
                    self.has_unit.then_some(self.unit_font_size),
                    self.inner_widget_gap,
                ),
                style: &self.text_style,
                font: &self.value_font,
            }),
        }
    }

    pub(crate) fn into_cache(
        self,
        gauge: &ValidatedArcGaugeWidget,
        scene: &ValidatedScenePresentation,
        raster_scale: PreviewRasterScale,
    ) -> WidgetResult<ArcGaugeCache> {
        let bounds = self.local_bounds;
        let static_layer = StaticRaster::new(
            raster_scale,
            Point::new(self.x, self.y),
            bounds,
            SHAPE_COLOR_TYPE,
            |canvas| {
                layer::draw_static_layer(
                    canvas,
                    gauge,
                    scene,
                    self.local_geometry,
                    self.bar_geometry,
                    scene.scale,
                    &self.text_style,
                );
                for text in &self.texts {
                    text.draw(canvas);
                }
                Ok(())
            },
        )?;
        // The fill raster shares the static raster's frame, so the sweep is gauge-local.
        let local = self.local_geometry;
        let fill_paint = gauge.track_filled_color.sweep_paint(
            gauge.track_filled_opacity * self.text_style.opacity,
            Point::new(local.center_x, local.center_y),
            local.start_angle,
            local.sweep_angle,
        )?;
        let mut cache = ArcGaugeCache {
            static_layer,
            content_bounds: self.content_bounds,
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
            rotation: gauge.rotation,
            geometry: placed_geometry(local, self.x, self.y),
            track_thickness: self.track_thickness,
            track_corner_radius: self.track_corner_radius,
            track_border_thickness: self.track_border_thickness,
            // Replaced below once the full fill can be drawn from this geometry.
            fill: TrackFill::Paint(fill_paint.clone()),
            track_fill_flat: gauge.track_fill_flat,
            track_fill_style: self.track_fill_style,
            bar_geometry: self.bar_geometry,
            text_style: self.text_style,
            value_font: self.value_font,
            has_unit: self.has_unit,
            unit_font_size: self.unit_font_size,
            glow: self.glow,
            inner_widget_gap: self.inner_widget_gap,
            inner_widget_offset_x: self.inner_widget_offset_x,
            inner_widget_offset_y: self.inner_widget_offset_y,
        };
        cache.fill = TrackFill::prepare(
            &gauge.track_filled_color,
            fill_paint,
            raster_scale,
            bounds,
            (self.x, self.y),
            |canvas, paint| layer::draw_full_fill_layer(canvas, &cache, local, paint),
        )?;
        Ok(cache)
    }
}

pub fn prepare_arc_gauge_cache(
    gauge: &ValidatedArcGaugeWidget,
    scene: &ValidatedScenePresentation,
    range: (f64, f64),
    altitude_offset_m: f64,
    unit_text: Option<&str>,
    fonts: &dyn FontResolver,
    raster_scale: PreviewRasterScale,
) -> WidgetResult<ArcGaugeCache> {
    measure_arc_gauge(gauge, scene, range, altitude_offset_m, unit_text, fonts)?.into_cache(
        gauge,
        scene,
        raster_scale,
    )
}

impl ArcGaugeCache {
    pub fn raster_bounds(&self) -> Rect {
        self.static_layer.bounds().with_offset((self.x, self.y))
    }
    pub(crate) fn frame_layout(&self) -> GaugeFrameLayout<'_> {
        GaugeFrameLayout {
            content_bounds: self.content_bounds,
            raster_bounds: self.raster_bounds(),
            content_width: self.width,
            value: Some(ArcValueLayout {
                inner: self.inner_layout(),
                style: &self.text_style,
                font: &self.value_font,
            }),
        }
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
        canvas.save();
        canvas.translate((self.x, self.y));
        self.static_layer.draw(canvas, None);
        canvas.restore();
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
