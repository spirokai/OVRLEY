//! Arc, corner and GT gauges: validation, geometry, the static track layer and the inner value.
use crate::model::catalog::{DisplayType, MetricKind, TrackFillStyle};
use crate::model::scene::ValidatedScenePresentation;
use crate::model::validation::require_non_negative_f32;
use crate::model::validation::require_unit_opacity;
use crate::model::validation::{
    require_bool, require_f32, require_hex_color, require_positive_f32, require_string,
};
use crate::render::text::LINE_HEIGHT_RATIO;
use crate::render::text::{
    origin_x_for_centered_text, parse_color, resolve_font, rgba_color, shadow_paint,
    validated_value_style, FontResolver, ResolvedTextStyle,
};
use crate::render::tile::SHAPE_COLOR_TYPE;
use crate::render::tile::{PreviewRasterScale, StaticRaster};
use crate::render::track_fill::{validate_track_fill_color, FillShape, TrackFill, TrackFillColor};
use crate::widgets::gauges::track::{
    append_circular_arc, arc_track_radius, corner_track_cap_padding, corner_track_radius,
    draw_segment, inner_segment_geometry, resolve_arc_track_geometry, scale_bar_geometry,
    segment_geometries, ArcTrackSpec, ResolvedBarGeometry,
};
use crate::widgets::gauges::{
    format_gauge_label, prepare_text, raster_bounds, ArcValueLayout, GaugeFrameLayout, TextRun,
};
use crate::widgets::value::{validate_arc_inner_value_widget, ValidatedValueWidget, ValueConfig};
use crate::{WidgetError, WidgetResult};
use skia_safe::paint::{Cap, Style};
use skia_safe::{BlendMode, Canvas, Font, Paint, PathBuilder, Point, Rect};

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
        .map(|bars| crate::widgets::gauges::track::segment_geometries(geometry, bars))
        .unwrap_or_else(|| vec![geometry]);
    let mut content = Rect::default();
    for segment in segments {
        if let Some(path) =
            crate::widgets::gauges::track::rounded_segment_path(segment, thickness, corner)
        {
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
        let extent = glow_sigma(thickness) * 3.0;
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
                draw_static_layer(
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
            |canvas, paint| draw_full_fill_layer(canvas, &cache, local, paint),
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
            draw_fill(canvas, self, self.geometry, fill01);
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
    style.line_height = font_size * LINE_HEIGHT_RATIO;
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

pub const MIN_ARC_ANGLE_DEGREES: f32 = 30.0;
pub const MAX_ARC_ANGLE_DEGREES: f32 = 360.0;
pub const CORNER_GAUGE_ANGLE_DEGREES: f32 = 90.0;
pub const MIN_GT_GAUGE_TICK_COUNT: u32 = 2;

/// The supported bottom-corner placements for a fixed 90° gauge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CornerOrientation {
    BottomLeft,
    BottomRight,
}

/// GT gauge decoration drawn around the shared arc track.
#[derive(Clone, Debug, PartialEq)]
pub struct GtStyle {
    /// Evenly spaced value-less ticks, including both track ends.
    pub tick_count: u32,
    pub tick_length: f32,
    pub tick_color: String,
    /// Fill glow intensity in 0..=1; zero disables the glow.
    pub glow: f32,
}

/// Fully validated arc gauge configuration.
#[derive(Clone, Debug)]
pub struct ValidatedArcGaugeWidget {
    pub metric: MetricKind,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub display_type: DisplayType,
    pub arc_angle: f32,
    pub corner_orientation: Option<CornerOrientation>,
    pub gt_style: Option<GtStyle>,
    pub inner_widget_offset_x: f32,
    pub inner_widget_offset_y: f32,
    pub track_thickness: f32,
    pub track_corner_radius: f32,
    pub track_border_thickness: f32,
    pub track_border_color: String,
    pub track_empty_color: String,
    pub track_empty_opacity: f32,
    pub track_filled_color: TrackFillColor,
    pub track_filled_opacity: f32,
    pub track_fill_flat: bool,
    pub track_fill_style: TrackFillStyle,
    pub bar_geometry: Option<ResolvedBarGeometry>,
    pub show_min_max_labels: bool,
    pub min_max_label_font: String,
    pub min_max_label_font_size: f32,
    pub min_max_label_color: String,
    /// Shared text formatting and styling for the inner value/unit stack.
    /// `show_icon` is always false for this presentation.
    pub inner_value: ValidatedValueWidget,
}

/// Validates a raw value config as an arc gauge.
pub fn validate_arc_gauge(
    value: ValueConfig,
    index: usize,
) -> WidgetResult<ValidatedArcGaugeWidget> {
    let p = |field: &str| format!("values[{index}].{field}");

    if value.display_type != DisplayType::Arc {
        return Err(WidgetError::Config(format!(
            "{}: expected arc display_type, got '{}'",
            p("display_type"),
            value.display_type.as_str()
        )));
    }

    let arc_angle = validate_arc_angle(value.arc_angle, &p("arc_angle"))?;
    validate_arc_shaped_gauge(value, index, DisplayType::Arc, arc_angle, None, None)
}

/// Validates a raw value config as a GT gauge: a rotated arc with ticks and fill glow.
pub fn validate_gt_gauge(
    value: ValueConfig,
    index: usize,
) -> WidgetResult<ValidatedArcGaugeWidget> {
    let p = |field: &str| format!("values[{index}].{field}");

    if value.display_type != DisplayType::GtGauge {
        return Err(WidgetError::Config(format!(
            "{}: expected gt_gauge display_type, got '{}'",
            p("display_type"),
            value.display_type.as_str()
        )));
    }

    let arc_angle = validate_arc_angle(value.arc_angle, &p("arc_angle"))?;
    let tick_count = value
        .tick_count
        .ok_or_else(|| WidgetError::Config(format!("{}: required", p("tick_count"))))?;
    if tick_count < MIN_GT_GAUGE_TICK_COUNT {
        return Err(WidgetError::Config(format!(
            "{}: must be >= {MIN_GT_GAUGE_TICK_COUNT}, got {tick_count}",
            p("tick_count")
        )));
    }
    let tick_length = require_non_negative_f32(value.tick_length, &p("tick_length"))?;
    let glow = require_unit_opacity(value.glow, &p("glow"))?;
    let gt_style = GtStyle {
        tick_count,
        tick_length,
        tick_color: require_hex_color(value.tick_color.as_deref(), &p("tick_color"))?,
        glow,
    };

    validate_arc_shaped_gauge(
        value,
        index,
        DisplayType::GtGauge,
        arc_angle,
        None,
        Some(gt_style),
    )
}

fn validate_arc_angle(arc_angle: Option<f32>, field: &str) -> WidgetResult<f32> {
    let arc_angle = require_f32(arc_angle, field)?;
    if !(MIN_ARC_ANGLE_DEGREES..=MAX_ARC_ANGLE_DEGREES).contains(&arc_angle) {
        return Err(WidgetError::Config(format!(
            "{field}: must be {}..={}, got {arc_angle}",
            MIN_ARC_ANGLE_DEGREES as u32, MAX_ARC_ANGLE_DEGREES as u32,
        )));
    }
    Ok(arc_angle)
}

/// Validates a raw value config as a fixed 90° bottom-corner gauge.
pub fn validate_corner_gauge(
    value: ValueConfig,
    index: usize,
) -> WidgetResult<ValidatedArcGaugeWidget> {
    let p = |field: &str| format!("values[{index}].{field}");

    if value.display_type != DisplayType::Corner {
        return Err(WidgetError::Config(format!(
            "{}: expected corner display_type, got '{}'",
            p("display_type"),
            value.display_type.as_str()
        )));
    }

    let corner_orientation = match require_string(
        value.corner_orientation.clone(),
        &p("corner_orientation"),
    )?
    .as_str()
    {
        "bottom-left" => CornerOrientation::BottomLeft,
        "bottom-right" => CornerOrientation::BottomRight,
        orientation => {
            return Err(WidgetError::Config(format!(
                "{}: must be 'bottom-left' or 'bottom-right', got '{orientation}'",
                p("corner_orientation")
            )));
        }
    };

    validate_arc_shaped_gauge(
        value,
        index,
        DisplayType::Corner,
        CORNER_GAUGE_ANGLE_DEGREES,
        Some(corner_orientation),
        None,
    )
}

fn validate_arc_shaped_gauge(
    value: ValueConfig,
    index: usize,
    display_type: DisplayType,
    arc_angle: f32,
    corner_orientation: Option<CornerOrientation>,
    gt_style: Option<GtStyle>,
) -> WidgetResult<ValidatedArcGaugeWidget> {
    let p = |field: &str| format!("values[{index}].{field}");

    crate::widgets::gauges::validate_supported_metric(value.value, display_type)?;
    require_f32(Some(value.x), &p("x"))?;
    require_f32(Some(value.y), &p("y"))?;
    let width = require_positive_f32(value.width, &p("width"))?;
    let height = require_positive_f32(value.height, &p("height"))?;

    let track_thickness = require_positive_f32(value.track_thickness, &p("track_thickness"))?;

    let raw_corner_radius =
        require_non_negative_f32(value.track_corner_radius, &p("track_corner_radius"))?;

    let track_border_thickness =
        require_f32(value.track_border_thickness, &p("track_border_thickness"))?;
    if track_border_thickness < 0.0 {
        return Err(WidgetError::Config(format!(
            "{}: must be >= 0, got {track_border_thickness}",
            p("track_border_thickness")
        )));
    }

    let track_empty_opacity = require_f32(value.track_empty_opacity, &p("track_empty_opacity"))?;
    let track_filled_opacity = require_f32(value.track_filled_opacity, &p("track_filled_opacity"))?;
    for (field, opacity) in [
        ("track_empty_opacity", track_empty_opacity),
        ("track_filled_opacity", track_filled_opacity),
    ] {
        if !(0.0..=1.0).contains(&opacity) {
            return Err(WidgetError::Config(format!(
                "{}: must be 0.0..1.0, got {opacity}",
                p(field)
            )));
        }
    }

    let min_max_label_font_size =
        require_f32(value.min_max_label_font_size, &p("min_max_label_font_size"))?;
    if min_max_label_font_size <= 0.0 {
        return Err(WidgetError::Config(format!(
            "{}: must be > 0, got {min_max_label_font_size}",
            p("min_max_label_font_size")
        )));
    }

    let metric = value.value;
    let x = value.x;
    let y = value.y;
    let rotation = require_f32(Some(value.rotation.unwrap_or(0.0)), &p("rotation"))?;
    let inner_widget_offset_x =
        require_f32(value.inner_widget_offset_x, &p("inner_widget_offset_x"))?;
    let inner_widget_offset_y =
        require_f32(value.inner_widget_offset_y, &p("inner_widget_offset_y"))?;
    let track_border_color = require_hex_color(
        value.track_border_color.as_deref(),
        &p("track_border_color"),
    )?;
    let track_empty_color =
        require_hex_color(value.track_empty_color.as_deref(), &p("track_empty_color"))?;
    // GT gauges keep a solid fill; their glow is tinted with that single color.
    let track_filled_color = if gt_style.is_some() {
        TrackFillColor::Solid(require_hex_color(
            value.track_filled_color.as_deref(),
            &p("track_filled_color"),
        )?)
    } else {
        validate_track_fill_color(
            value.track_filled_color.as_deref(),
            value.track_filled_gradient,
            value.track_filled_gradient_stops.clone(),
            p,
        )?
    };
    let track_fill_flat = require_bool(value.track_fill_flat, &p("track_fill_flat"))?;
    let track_fill_style = value.track_fill_style.unwrap_or_default();
    let (track_corner_radius, bar_geometry) = resolve_arc_track_geometry(
        width.min(height),
        track_thickness,
        raw_corner_radius,
        track_border_thickness,
        arc_angle,
        corner_orientation.is_some(),
        track_fill_style,
        value.bar_count,
        value.bar_gap,
        &p("track_fill_style"),
    )?;
    let show_min_max_labels = require_bool(value.show_min_max_labels, &p("show_min_max_labels"))?;
    let min_max_label_font = crate::widgets::gauges::validate_label_font(
        value.min_max_label_font.clone(),
        &p("min_max_label_font"),
    )?;
    let min_max_label_color = require_hex_color(
        value.min_max_label_color.as_deref(),
        &p("min_max_label_color"),
    )?;

    let inner_value = validate_arc_inner_value_widget(value, index)?;

    Ok(ValidatedArcGaugeWidget {
        metric,
        x,
        y,
        width,
        height,
        rotation,
        display_type,
        arc_angle,
        corner_orientation,
        gt_style,
        inner_widget_offset_x,
        inner_widget_offset_y,
        track_thickness,
        track_corner_radius,
        track_border_thickness,
        track_border_color,
        track_empty_color,
        track_empty_opacity,
        track_filled_color,
        track_filled_opacity,
        track_fill_flat,
        track_fill_style,
        bar_geometry,
        show_min_max_labels,
        min_max_label_font,
        min_max_label_font_size,
        min_max_label_color,
        inner_value,
    })
}

const CORNER_GAUGE_DEFAULT_FRAME_SIZE: f32 = 110.0;
const CORNER_GAUGE_INNER_INSET: f32 = 22.0;

/// Arc geometry shared by static and dynamic drawing. Angles use Skia's
/// screen-space convention: 0° is right and increasing angles move clockwise.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArcGaugeGeometry {
    /// Track-circle center on the widget-local x axis.
    pub center_x: f32,
    /// Track-circle center on the widget-local y axis.
    pub center_y: f32,
    /// Horizontal anchor for the numeric value/unit stack.
    pub inner_widget_center_x: f32,
    /// Vertical anchor for the numeric value/unit stack.
    pub inner_widget_center_y: f32,
    /// Radius measured to the track centerline.
    pub radius: f32,
    /// Start angle in Skia screen-space degrees.
    pub start_angle: f32,
    /// Signed clockwise sweep in Skia screen-space degrees.
    pub sweep_angle: f32,
}

/// Centers a validated arc sweep around the top of its frame.
pub fn arc_start_end_angles(arc_angle: f32) -> (f32, f32) {
    let angle = arc_angle.clamp(MIN_ARC_ANGLE_DEGREES, MAX_ARC_ANGLE_DEGREES);
    (270.0 - angle * 0.5, 270.0 + angle * 0.5)
}

/// GT gauges open their gap on the right instead of the bottom.
pub const GT_GAUGE_TRACK_ROTATION_DEGREES: f32 = -90.0;

/// Centers a validated GT sweep around the left of its frame.
pub fn gt_start_end_angles(arc_angle: f32) -> (f32, f32) {
    let (start, end) = arc_start_end_angles(arc_angle);
    (
        start + GT_GAUGE_TRACK_ROTATION_DEGREES,
        end + GT_GAUGE_TRACK_ROTATION_DEGREES,
    )
}

/// Returns the directed quarter-circle sweep for a corner gauge.
pub fn corner_start_end_angles(orientation: CornerOrientation) -> (f32, f32) {
    match orientation {
        CornerOrientation::BottomLeft => (0.0, -90.0),
        CornerOrientation::BottomRight => (180.0, 270.0),
    }
}

/// Fits the track centerline radius inside the smaller frame dimension.
pub fn arc_radius(width: f32, height: f32, track_thickness: f32, border_thickness: f32) -> f32 {
    arc_track_radius(width.min(height), track_thickness, border_thickness)
}

/// Projects a polar track coordinate into widget-local Cartesian space.
pub fn arc_point(center_x: f32, center_y: f32, radius: f32, angle: f32) -> Point {
    let radians = angle.to_radians();
    Point::new(
        center_x + radius * radians.cos(),
        center_y + radius * radians.sin(),
    )
}

/// Resolves centered arc-gauge geometry from validated dimensions and style.
pub fn arc_gauge_geometry(
    width: f32,
    height: f32,
    arc_angle: f32,
    track_thickness: f32,
    border_thickness: f32,
) -> ArcGaugeGeometry {
    let (start_angle, end_angle) = arc_start_end_angles(arc_angle);
    let center_x = width * 0.5;
    let center_y = height * 0.5;
    ArcGaugeGeometry {
        center_x,
        center_y,
        inner_widget_center_x: center_x,
        inner_widget_center_y: center_y,
        radius: arc_radius(width, height, track_thickness, border_thickness),
        start_angle,
        sweep_angle: end_angle - start_angle,
    }
}

/// Resolves arc geometry rotated into the GT gauge's fixed track orientation.
pub fn gt_gauge_geometry(
    width: f32,
    height: f32,
    arc_angle: f32,
    track_thickness: f32,
    border_thickness: f32,
) -> ArcGaugeGeometry {
    let (start_angle, _) = gt_start_end_angles(arc_angle);
    ArcGaugeGeometry {
        start_angle,
        ..arc_gauge_geometry(width, height, arc_angle, track_thickness, border_thickness)
    }
}

/// Resolves quarter-arc geometry and its inset value anchor for a corner gauge.
pub fn corner_gauge_geometry(
    width: f32,
    height: f32,
    orientation: CornerOrientation,
    track_thickness: f32,
    track_corner_radius: f32,
    border_thickness: f32,
) -> ArcGaugeGeometry {
    let (start_angle, end_angle) = corner_start_end_angles(orientation);
    let frame_size = width.min(height);
    let cap_padding = corner_track_cap_padding(
        frame_size,
        track_thickness,
        track_corner_radius,
        border_thickness,
    );
    let inner_inset = frame_size * CORNER_GAUGE_INNER_INSET / CORNER_GAUGE_DEFAULT_FRAME_SIZE;
    let is_bottom_right = orientation == CornerOrientation::BottomRight;
    let center_x = if is_bottom_right {
        width - cap_padding
    } else {
        cap_padding
    };

    ArcGaugeGeometry {
        center_x,
        center_y: height - cap_padding,
        inner_widget_center_x: if is_bottom_right {
            width - inner_inset
        } else {
            inner_inset
        },
        inner_widget_center_y: height - inner_inset,
        radius: corner_track_radius(
            frame_size,
            track_thickness,
            track_corner_radius,
            border_thickness,
        ),
        start_angle,
        sweep_angle: end_angle - start_angle,
    }
}

const UNIT_RATIO: f32 = 0.28;
const MIN_UNIT_FONT_SIZE: f32 = 12.0;
const DEFAULT_GAP_PX: f32 = 4.0;
const GAP_RATIO: f32 = 0.08;
/// GT gauges separate the unit from the value more generously than arc gauges.
const GT_GAP_RATIO: f32 = 0.30;

#[derive(Clone, Copy, Debug)]
/// Baselines and total bounds for the centered value/unit stack.
pub(crate) struct ArcInnerWidgetLayout {
    pub(crate) center_x: f32,
    pub(crate) value_top: f32,
    pub(crate) unit_top: Option<f32>,
}

/// Resolves the unit font size relative to the validated value style.
pub(crate) fn unit_font_size(text_style: &ResolvedTextStyle, scale: f32) -> f32 {
    (text_style.font_size * UNIT_RATIO).max(MIN_UNIT_FONT_SIZE * scale)
}

/// Resolves the vertical value/unit spacing for the gauge's presentation.
pub(crate) fn value_unit_gap(gauge: &ValidatedArcGaugeWidget, font_size: f32, scale: f32) -> f32 {
    let ratio = if gauge.gt_style.is_some() {
        GT_GAP_RATIO
    } else {
        GAP_RATIO
    };
    (font_size * ratio).max(DEFAULT_GAP_PX * scale)
}

/// Measures and centers the value and optional unit around the inner anchor.
pub(crate) fn inner_widget_layout(
    geometry: ArcGaugeGeometry,
    offset_x: f32,
    offset_y: f32,
    text_style: &ResolvedTextStyle,
    unit_font_size: Option<f32>,
    gap: f32,
) -> ArcInnerWidgetLayout {
    let value_line_height = text_style.font_size * LINE_HEIGHT_RATIO;
    let unit_line_height = unit_font_size.unwrap_or(0.0) * LINE_HEIGHT_RATIO;
    let group_height = if unit_font_size.is_some() {
        value_line_height + gap + unit_line_height
    } else {
        value_line_height
    };
    let top = geometry.inner_widget_center_y + offset_y - group_height * 0.5;

    ArcInnerWidgetLayout {
        center_x: geometry.inner_widget_center_x + offset_x,
        value_top: top,
        unit_top: unit_font_size.map(|_| top + value_line_height + gap),
    }
}

/// Draws the optional unit into the frame-invariant static layer.
pub(super) fn unit_style(
    gauge: &ValidatedArcGaugeWidget,
    geometry: ArcGaugeGeometry,
    scene: &ValidatedScenePresentation,
    unit_text: &str,
    font: &skia_safe::Font,
    text_style: &ResolvedTextStyle,
) -> ResolvedTextStyle {
    let scale = scene.scale;
    let mut unit_style = text_style.clone();
    unit_style.font_size = unit_font_size(text_style, scale);
    unit_style.line_height = unit_style.font_size * LINE_HEIGHT_RATIO;
    unit_style.color = rgba_color(gauge.inner_value.unit_color, scene.opacity);
    let layout = inner_widget_layout(
        geometry,
        gauge.inner_widget_offset_x * scale,
        gauge.inner_widget_offset_y * scale,
        text_style,
        Some(unit_style.font_size),
        value_unit_gap(gauge, text_style.font_size, scale),
    );
    unit_style.x = origin_x_for_centered_text(unit_text, layout.center_x, font);
    unit_style.y = layout
        .unit_top
        .expect("unit layout must include a top when a unit exists");
    unit_style
}

/// GT ticks start this many track thicknesses inside the track's inner edge.
const GT_TICK_GAP_RATIO: f32 = 1.0;
/// GT tick stroke width relative to the track thickness.
const GT_TICK_WIDTH_RATIO: f32 = 0.2;
/// GT glow blur radius relative to the track thickness.
const GT_GLOW_SIGMA_RATIO: f32 = 0.6;

/// Blur sigma of the GT fill glow; also sizes the glow's raster padding.
pub(super) fn glow_sigma(track_thickness: f32) -> f32 {
    track_thickness * GT_GLOW_SIGMA_RATIO
}

/// Builds the padded, cached image containing every frame-invariant element.
pub(super) fn draw_static_layer(
    canvas: &Canvas,
    gauge: &ValidatedArcGaugeWidget,
    scene: &ValidatedScenePresentation,
    geometry: ArcGaugeGeometry,
    bars: Option<ResolvedBarGeometry>,
    scale: f32,
    text_style: &ResolvedTextStyle,
) {
    let track_thickness = gauge.track_thickness * scale;
    let border_thickness = gauge.track_border_thickness * scale;
    let shadow = if border_thickness > 0.0 {
        shadow_paint(
            &track_paint(parse_color(&gauge.track_border_color, text_style.opacity)),
            parse_color(&scene.shadow_color, text_style.opacity),
            scene.shadow_strength * scale,
        )
    } else {
        None
    };

    match bars {
        Some(bar_geometry) => {
            for segment in segment_geometries(geometry, bar_geometry) {
                draw_static_track(
                    canvas,
                    gauge,
                    segment,
                    track_thickness,
                    border_thickness,
                    scale,
                    text_style,
                    shadow
                        .as_ref()
                        .map(|paint| (paint, scene.shadow_distance * scale)),
                );
            }
        }
        None => draw_static_track(
            canvas,
            gauge,
            geometry,
            track_thickness,
            border_thickness,
            scale,
            text_style,
            shadow
                .as_ref()
                .map(|paint| (paint, scene.shadow_distance * scale)),
        ),
    }
    draw_gt_ticks(canvas, gauge, geometry, scale, text_style);
}

/// Value-less ticks evenly spaced along the sweep, inside the track.
fn draw_gt_ticks(
    canvas: &Canvas,
    gauge: &ValidatedArcGaugeWidget,
    geometry: ArcGaugeGeometry,
    scale: f32,
    text_style: &ResolvedTextStyle,
) {
    let Some(style) = &gauge.gt_style else {
        return;
    };
    let length = style.tick_length * scale;
    if length <= 0.0 {
        return;
    }
    let thickness = gauge.track_thickness * scale;
    let outer = geometry.radius
        - thickness * (0.5 + GT_TICK_GAP_RATIO)
        - gauge.track_border_thickness * scale;
    let inner = (outer - length).max(0.0);
    if outer <= inner {
        return;
    }
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(Style::Stroke);
    paint.set_stroke_cap(Cap::Round);
    paint.set_stroke_width((thickness * GT_TICK_WIDTH_RATIO).max(scale));
    paint.set_color(parse_color(&style.tick_color, text_style.opacity));
    let steps = (style.tick_count - 1) as f32;
    for index in 0..style.tick_count {
        let angle = geometry.start_angle + geometry.sweep_angle * index as f32 / steps;
        canvas.draw_line(
            arc_point(geometry.center_x, geometry.center_y, outer, angle),
            arc_point(geometry.center_x, geometry.center_y, inner, angle),
            &paint,
        );
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the track's resolved geometry and scaled thicknesses are computed once by its caller"
)]
fn draw_static_track(
    canvas: &Canvas,
    gauge: &ValidatedArcGaugeWidget,
    geometry: ArcGaugeGeometry,
    track_thickness: f32,
    border_thickness: f32,
    scale: f32,
    text_style: &ResolvedTextStyle,
    shadow: Option<(&Paint, f32)>,
) {
    let inner_corner_radius = gauge.track_corner_radius * scale;
    let border_color = parse_color(&gauge.track_border_color, text_style.opacity);

    if let Some((paint, distance)) = shadow {
        canvas.save();
        canvas.translate((distance, distance));
        draw_segment(
            canvas,
            geometry,
            track_thickness,
            inner_corner_radius,
            paint,
        );
        canvas.restore();
    }
    if border_thickness > 0.0 {
        draw_segment(
            canvas,
            geometry,
            track_thickness,
            inner_corner_radius,
            &track_paint(border_color),
        );
        let inner_geo = inner_segment_geometry(geometry, border_thickness);
        let inner_thickness = (track_thickness - border_thickness * 2.0).max(0.0);
        let inner_cnr = (inner_corner_radius - border_thickness).max(0.0);
        draw_segment(
            canvas,
            inner_geo,
            inner_thickness,
            inner_cnr,
            &clear_track_paint(),
        );
        draw_segment(
            canvas,
            inner_geo,
            inner_thickness,
            inner_cnr,
            &track_paint(parse_color(
                &gauge.track_empty_color,
                gauge.track_empty_opacity * text_style.opacity,
            )),
        );
    } else {
        draw_segment(
            canvas,
            geometry,
            track_thickness,
            inner_corner_radius,
            &track_paint(parse_color(
                &gauge.track_empty_color,
                gauge.track_empty_opacity * text_style.opacity,
            )),
        );
    }
}

/// Dispatches the current fill to the continuous or segmented painting model.
pub(super) fn draw_fill(
    canvas: &Canvas,
    cache: &ArcGaugeCache,
    geometry: ArcGaugeGeometry,
    fill01: f32,
) {
    match cache.track_fill_style {
        TrackFillStyle::Bars => draw_segmented_fill(canvas, cache, geometry, fill01),
        TrackFillStyle::Fill => draw_continuous_fill(canvas, cache, geometry, fill01),
    }
}

/// Paints the first `count` border-inset bars of `geometry`.
fn draw_bars(
    canvas: &Canvas,
    cache: &ArcGaugeCache,
    geometry: ArcGaugeGeometry,
    bars: ResolvedBarGeometry,
    count: usize,
    paint: &Paint,
) {
    let inner_thickness = (cache.track_thickness - cache.track_border_thickness * 2.0).max(0.0);
    let inner_corner = (cache.track_corner_radius - cache.track_border_thickness).max(0.0);
    for segment in segment_geometries(geometry, bars).into_iter().take(count) {
        let inner_geo = inner_segment_geometry(segment, cache.track_border_thickness);
        draw_segment(canvas, inner_geo, inner_thickness, inner_corner, paint);
    }
}

/// Paints the completely filled track in `geometry`'s frame for the cached gradient raster.
pub(super) fn draw_full_fill_layer(
    canvas: &Canvas,
    cache: &ArcGaugeCache,
    geometry: ArcGaugeGeometry,
    paint: &Paint,
) {
    match cache.bar_geometry {
        Some(bars) => draw_bars(canvas, cache, geometry, bars, bars.count as usize, paint),
        None => {
            let bt = cache.track_border_thickness;
            draw_segment(
                canvas,
                inner_segment_geometry(geometry, bt),
                (cache.track_thickness - bt * 2.0).max(0.0),
                (cache.track_corner_radius - bt).max(0.0),
                paint,
            );
        }
    }
}

fn draw_segmented_fill(
    canvas: &Canvas,
    cache: &ArcGaugeCache,
    geometry: ArcGaugeGeometry,
    fill01: f32,
) {
    let bars = cache
        .bar_geometry
        .expect("bars fill style must carry resolved bar geometry");
    let filled_count = crate::widgets::gauges::bar_fill_count(fill01, bars.count);
    if filled_count == 0 {
        return;
    }
    // Gaps are empty in the full-fill raster, so one wedge from the center, ending
    // mid-gap after the last filled bar, reveals whole bars.
    let direction = geometry.sweep_angle.signum();
    let degrees = |length: f32| direction * (length / geometry.radius).to_degrees();
    let start = geometry.start_angle - degrees(bars.gap * 0.5);
    let sweep = degrees(filled_count as f32 * (bars.extent + bars.gap));
    let radius = geometry.radius + cache.track_thickness;
    let mut wedge = PathBuilder::new();
    wedge.move_to((geometry.center_x, geometry.center_y));
    wedge.line_to(arc_point(
        geometry.center_x,
        geometry.center_y,
        radius,
        start,
    ));
    append_circular_arc(
        &mut wedge,
        geometry.center_x,
        geometry.center_y,
        radius,
        start,
        sweep,
    );
    wedge.close();
    cache
        .fill
        .draw(canvas, &FillShape::Path(wedge.detach()), |paint| {
            draw_bars(canvas, cache, geometry, bars, filled_count, paint)
        });
}

fn draw_continuous_fill(
    canvas: &Canvas,
    cache: &ArcGaugeCache,
    geometry: ArcGaugeGeometry,
    fill01: f32,
) {
    let bt = cache.track_border_thickness;
    let inner_thickness = (cache.track_thickness - bt * 2.0).max(0.0);
    let inner_geometry = inner_segment_geometry(geometry, bt);
    let inner_corner = (cache.track_corner_radius - bt).max(0.0);
    let end_corner = if cache.track_fill_flat {
        0.0
    } else {
        inner_corner
    };
    let reveal_spec = ArcTrackSpec::full(inner_geometry, inner_thickness, inner_corner)
        .with_end_corner_radius(end_corner);
    let Some(reveal) = reveal_spec.reveal_clip(fill01) else {
        return;
    };
    let Some(reveal_path) = reveal.path(&reveal_spec) else {
        return;
    };
    let reveal = FillShape::Path(reveal_path);
    cache.fill.draw(canvas, &reveal, |paint| {
        if let Some(glow) = glow_paint(cache, paint) {
            // The blur hides the tip's exact cap shape, so the glow needs no reveal clip.
            let glow_geometry = ArcGaugeGeometry {
                sweep_angle: inner_geometry.sweep_angle * fill01,
                ..inner_geometry
            };
            draw_segment(canvas, glow_geometry, inner_thickness, end_corner, &glow);
        }
        canvas.save();
        reveal.clip(canvas);
        draw_segment(canvas, inner_geometry, inner_thickness, inner_corner, paint);
        canvas.restore();
    });
}

/// Subtle blurred halo in the fill color, drawn beneath the fill.
fn glow_paint(cache: &ArcGaugeCache, fill: &Paint) -> Option<Paint> {
    if cache.glow <= 0.0 {
        return None;
    }
    let color = fill.color().with_a((cache.glow * 255.0).round() as u8);
    shadow_paint(fill, color, glow_sigma(cache.track_thickness))
}

fn track_paint(color: skia_safe::Color) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(Style::Fill);
    paint.set_color(color);
    paint
}

fn clear_track_paint() -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(Style::Fill);
    paint.set_blend_mode(BlendMode::Clear);
    paint
}
