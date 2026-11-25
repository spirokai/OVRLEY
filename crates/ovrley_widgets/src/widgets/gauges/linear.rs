//! Linear gauges: validation, the track layer and the boundary labels.
use crate::model::catalog::{catalog, widget_font_weight, DisplayType, MetricKind, TrackFillStyle};
use crate::model::scene::{Numerals, ValidatedScenePresentation};
use crate::model::validation::require_non_negative_f32;
use crate::model::validation::{
    require_bool, require_f32, require_hex_color, require_positive_f32, require_string,
};
use crate::render::text::{
    origin_x_for_centered_text, parse_color, resolve_font, shadow_paint, FontResolver,
    ResolvedTextStyle,
};
use crate::render::tile::SHAPE_COLOR_TYPE;
use crate::render::tile::{PreviewRasterScale, StaticRaster};
use crate::render::track_fill::{validate_track_fill_color, FillShape, TrackFill, TrackFillColor};
use crate::widgets::gauges::track::{
    resolve_bar_style_geometry, scale_bar_geometry, track_corner_radius_max,
    translated_track_cap_path, translated_track_cap_reveal, ResolvedBarGeometry, TrackPathFrame,
};
use crate::widgets::gauges::{
    bar_fill_count, format_gauge_label, prepare_text, raster_bounds, GaugeFrameLayout, TextRun,
};
use crate::widgets::value::{normalize_starting_altitude_m, ValueConfig};
use crate::{WidgetError, WidgetResult};
use skia_safe::{BlendMode, Canvas, ClipOp, Paint, PathBuilder, PathFillType, Point, RRect, Rect};

#[derive(Clone, Debug)]
pub struct LinearGaugeCache {
    /// Track and labels in gauge-local coordinates; the gauge origin is (x, y).
    pub(crate) static_layer: StaticRaster,
    pub content_bounds: Rect,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub orientation: LinearOrientation,
    pub track_corner_radius: f32,
    pub track_border_thickness: f32,
    pub fill: TrackFill,
    pub track_fill_flat: bool,
    pub track_fill_style: TrackFillStyle,
    pub bar_geometry: Option<ResolvedBarGeometry>,
}

/// Linear gauge layout before its static raster is drawn; frame bounds need nothing more.
pub(crate) struct LinearGaugeMeasure {
    texts: Vec<TextRun>,
    /// Static raster rectangle relative to the gauge origin.
    local_bounds: Rect,
    content_bounds: Rect,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

pub(crate) fn measure_linear_gauge(
    gauge: &ValidatedLinearGaugeWidget,
    scene: &ValidatedScenePresentation,
    range: (f64, f64),
    altitude_offset_m: f64,
    fonts: &dyn FontResolver,
) -> WidgetResult<LinearGaugeMeasure> {
    let scale = scene.scale;
    let width = gauge.width * scale;
    let height = gauge.height * scale;
    let mut content = Rect::from_xywh(0.0, 0.0, width, height);
    let mut bounds = content;
    if gauge.track_border_thickness > 0.0 && scene.shadow_strength > 0.0 {
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
        let size = gauge.min_max_label_font_size * scale;
        let font = resolve_font(fonts, Some(&gauge.min_max_label_font), size)?;
        let labels = label_layout(
            gauge,
            width,
            height,
            scale,
            &font,
            range.0,
            range.1,
            altitude_offset_m,
        );
        for (text, origin) in [
            (&labels.min_label, labels.min_origin),
            (&labels.max_label, labels.max_origin),
        ] {
            let (_, ink) = font.measure_str(text, None);
            let style = ResolvedTextStyle {
                x: origin.x,
                y: origin.y + ink.top,
                font_name: Some(gauge.min_max_label_font.clone()),
                font_size: size,
                font_weight: widget_font_weight(),
                italic: false,
                letter_spacing: 0.0,
                line_height: ink.height(),
                color: parse_color(&gauge.min_max_label_color, scene.opacity),
                opacity: scene.opacity,
                shadow_color: None,
                shadow_strength: 0.0,
                shadow_distance: 0.0,
                border_color: None,
                border_thickness: 0.0,
                numerals: Numerals::Default,
            };
            let run = prepare_text(text, text, style, fonts)?;
            content.join(run.content_bounds);
            bounds.join(run.bounds);
            texts.push(run);
        }
    }
    content.offset((gauge.x, gauge.y));
    Ok(LinearGaugeMeasure {
        texts,
        local_bounds: raster_bounds(bounds),
        content_bounds: content,
        x: gauge.x,
        y: gauge.y,
        width,
        height,
    })
}

impl LinearGaugeMeasure {
    pub(crate) fn frame_layout(&self) -> GaugeFrameLayout<'_> {
        GaugeFrameLayout {
            content_bounds: self.content_bounds,
            raster_bounds: self.local_bounds.with_offset((self.x, self.y)),
            content_width: self.width,
            value: None,
        }
    }

    pub(crate) fn into_cache(
        self,
        gauge: &ValidatedLinearGaugeWidget,
        scene: &ValidatedScenePresentation,
        raster_scale: PreviewRasterScale,
    ) -> WidgetResult<LinearGaugeCache> {
        let scale = scene.scale;
        let bounds = self.local_bounds;
        let static_layer = StaticRaster::new(
            raster_scale,
            Point::new(self.x, self.y),
            bounds,
            SHAPE_COLOR_TYPE,
            |canvas| {
                draw_static_layer(canvas, gauge, scene, self.width, self.height, scale);
                for text in &self.texts {
                    text.draw(canvas);
                }
                Ok(())
            },
        )?;
        let track_border_thickness = gauge.track_border_thickness * scale;
        // The fill raster shares the static raster's frame, so the axis is gauge-local.
        let (start, end) = fill_axis(
            self.width,
            self.height,
            gauge.orientation,
            track_border_thickness,
        );
        let fill_paint = gauge.track_filled_color.linear_paint(
            gauge.track_filled_opacity * scene.opacity,
            start,
            end,
        )?;
        let mut cache = LinearGaugeCache {
            static_layer,
            content_bounds: self.content_bounds,
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
            rotation: gauge.rotation,
            orientation: gauge.orientation,
            track_corner_radius: gauge.track_corner_radius * scale,
            track_border_thickness,
            // Replaced below once the full fill can be drawn from this geometry.
            fill: TrackFill::Paint(fill_paint.clone()),
            track_fill_flat: gauge.track_fill_flat,
            track_fill_style: gauge.track_fill_style,
            bar_geometry: scale_bar_geometry(gauge.bar_geometry, scale),
        };
        cache.fill = TrackFill::prepare(
            &gauge.track_filled_color,
            fill_paint,
            raster_scale,
            bounds,
            (self.x, self.y),
            |canvas, paint| draw_full_fill_layer(canvas, &cache, paint),
        )?;
        Ok(cache)
    }
}

pub fn prepare_linear_gauge_cache(
    gauge: &ValidatedLinearGaugeWidget,
    scene: &ValidatedScenePresentation,
    range: (f64, f64),
    altitude_offset_m: f64,
    fonts: &dyn FontResolver,
    raster_scale: PreviewRasterScale,
) -> WidgetResult<LinearGaugeCache> {
    measure_linear_gauge(gauge, scene, range, altitude_offset_m, fonts)?.into_cache(
        gauge,
        scene,
        raster_scale,
    )
}

impl LinearGaugeCache {
    pub(crate) fn frame_layout(&self) -> GaugeFrameLayout<'_> {
        GaugeFrameLayout {
            content_bounds: self.content_bounds,
            raster_bounds: self.raster_bounds(),
            content_width: self.width,
            value: None,
        }
    }
    pub fn raster_bounds(&self) -> Rect {
        self.static_layer.bounds().with_offset((self.x, self.y))
    }
    pub fn draw(&self, canvas: &Canvas, fill01: f32) {
        canvas.save();
        canvas.translate((self.x, self.y));
        self.static_layer.draw(canvas, None);
        canvas.restore();
        match self.track_fill_style {
            TrackFillStyle::Bars => draw_segmented_fill(canvas, self, fill01),
            TrackFillStyle::Fill => draw_continuous_fill(canvas, self, fill01),
        }
    }
}

/// The axis a linear gauge fills along.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinearOrientation {
    Horizontal,
    Vertical,
}

impl LinearOrientation {
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "horizontal" => Some(Self::Horizontal),
            "vertical" => Some(Self::Vertical),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }

    /// The label positions this orientation offers; the first is its default.
    pub fn label_positions(self) -> [LabelPosition; 2] {
        match self {
            Self::Horizontal => [LabelPosition::Bottom, LabelPosition::Top],
            Self::Vertical => [LabelPosition::Left, LabelPosition::Right],
        }
    }

    /// The label position `key` names, when this orientation offers it.
    pub fn label_position(self, key: &str) -> Option<LabelPosition> {
        self.label_positions()
            .into_iter()
            .find(|position| position.as_str() == key)
    }
}

/// Where a linear gauge draws its min/max labels; each orientation offers two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelPosition {
    Left,
    Right,
    Bottom,
    Top,
}

impl LabelPosition {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Bottom => "bottom",
            Self::Top => "top",
        }
    }
}

/// Fully validated linear gauge widget configuration.
#[derive(Clone, Debug)]
pub struct ValidatedLinearGaugeWidget {
    pub metric: MetricKind,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub display_type: DisplayType,
    pub orientation: LinearOrientation,
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
    pub min_max_label_position: LabelPosition,
    pub min_max_label_color: String,
    pub display_unit: Option<String>,
    pub starting_altitude_m: Option<f64>,
}

/// Validates a raw value config as a linear gauge widget.
///
/// All gauge-specific fields are required (not optional) and are checked
/// for valid ranges (opacity 0-1, orientation horizontal/vertical, etc.).
pub fn validate_linear_gauge(
    value: ValueConfig,
    index: usize,
) -> WidgetResult<ValidatedLinearGaugeWidget> {
    let p = |f: &str| format!("values[{index}].{f}");

    crate::widgets::gauges::validate_supported_metric(value.value, DisplayType::Linear)?;

    if value.display_type != DisplayType::Linear {
        return Err(WidgetError::Config(format!(
            "{}: expected linear display_type, got '{}'",
            p("display_type"),
            value.display_type.as_str()
        )));
    }

    require_f32(Some(value.x), &p("x"))?;
    require_f32(Some(value.y), &p("y"))?;
    let width = require_positive_f32(value.width, &p("width"))?;
    let height = require_positive_f32(value.height, &p("height"))?;

    let orientation = require_string(value.orientation, &p("orientation"))?;
    let orientation = LinearOrientation::parse(&orientation).ok_or_else(|| {
        WidgetError::Config(format!(
            "{}: expected horizontal or vertical, got {orientation}",
            p("orientation")
        ))
    })?;
    let position = require_string(value.min_max_label_position, &p("min_max_label_position"))?;
    let min_max_label_position = orientation.label_position(&position).ok_or_else(|| {
        let [first, second] = orientation.label_positions().map(LabelPosition::as_str);
        WidgetError::Config(format!(
            "{}: expected {first} or {second} for {} gauge, got {position}",
            p("min_max_label_position"),
            orientation.as_str()
        ))
    })?;

    let track_empty_opacity = require_f32(value.track_empty_opacity, &p("track_empty_opacity"))?;
    let track_filled_opacity = require_f32(value.track_filled_opacity, &p("track_filled_opacity"))?;
    let raw_corner_radius =
        require_non_negative_f32(value.track_corner_radius, &p("track_corner_radius"))?;
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

    let track_border_thickness =
        require_f32(value.track_border_thickness, &p("track_border_thickness"))?;
    if track_border_thickness < 0.0 {
        return Err(WidgetError::Config(format!(
            "{}: must be >= 0, got {track_border_thickness}",
            p("track_border_thickness")
        )));
    }
    let track_fill_style = value.track_fill_style.unwrap_or_default();
    let bar_span = match orientation {
        LinearOrientation::Horizontal => width as f32,
        LinearOrientation::Vertical => height as f32,
    };
    let bar_geometry = resolve_bar_style_geometry(
        track_fill_style,
        bar_span,
        value.bar_count,
        value.bar_gap,
        &p("track_fill_style"),
    )?;
    let cross_extent = match orientation {
        LinearOrientation::Horizontal => height as f32,
        LinearOrientation::Vertical => width as f32,
    };
    let track_corner_radius = raw_corner_radius.clamp(
        0.0,
        track_corner_radius_max(cross_extent, bar_span, bar_geometry.as_ref()),
    );

    let display_unit = if value.value == MetricKind::Altitude {
        Some(require_string(value.display_unit, &p("display_unit"))?)
    } else {
        value.display_unit
    };
    let starting_altitude_m = normalize_starting_altitude_m(
        value.value,
        value.starting_altitude,
        display_unit.as_deref(),
        &p("starting_altitude"),
        &p("display_unit"),
    )?;
    // Absence retains canonical-unit labels; a supplied unit must be valid for this metric.
    if let Some(unit) = &display_unit {
        let definition = catalog()
            .standard_metric(value.value)
            .expect("supported standard gauge metric");
        if Some(unit.as_str()) != definition.default_display_unit()
            && !definition
                .supported_display_units
                .iter()
                .any(|option| &option.value == unit)
        {
            return Err(WidgetError::Config(format!(
                "{}: unsupported display unit '{unit}'",
                p("display_unit")
            )));
        }
    }

    Ok(ValidatedLinearGaugeWidget {
        metric: value.value,
        x: value.x,
        y: value.y,
        width,
        height,
        rotation: require_f32(Some(value.rotation.unwrap_or(0.0)), &p("rotation"))?,
        display_type: value.display_type,
        orientation,
        track_corner_radius,
        track_border_thickness,
        track_border_color: require_hex_color(
            value.track_border_color.as_deref(),
            &p("track_border_color"),
        )?,
        track_empty_color: require_hex_color(
            value.track_empty_color.as_deref(),
            &p("track_empty_color"),
        )?,
        track_empty_opacity,
        track_filled_color: validate_track_fill_color(
            value.track_filled_color.as_deref(),
            value.track_filled_gradient,
            value.track_filled_gradient_stops,
            p,
        )?,
        track_filled_opacity,
        track_fill_flat: require_bool(value.track_fill_flat, &p("track_fill_flat"))?,
        track_fill_style,
        bar_geometry,
        show_min_max_labels: require_bool(value.show_min_max_labels, &p("show_min_max_labels"))?,
        min_max_label_font: crate::widgets::gauges::validate_label_font(
            value.min_max_label_font,
            &p("min_max_label_font"),
        )?,
        min_max_label_font_size: require_positive_f32(
            value.min_max_label_font_size,
            &p("min_max_label_font_size"),
        )?,
        min_max_label_position,
        min_max_label_color: require_hex_color(
            value.min_max_label_color.as_deref(),
            &p("min_max_label_color"),
        )?,
        display_unit,
        starting_altitude_m,
    })
}

/// Computes the filled-bar rect without border insetting.
pub fn bar_fill_rect(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    fill01: f32,
    orientation: LinearOrientation,
) -> (f32, f32, f32, f32) {
    let fill01 = fill01.clamp(0.0, 1.0);
    match orientation {
        LinearOrientation::Horizontal => (x, y, width * fill01, height),
        LinearOrientation::Vertical => {
            let filled_height = height * fill01;
            (x, y + height - filled_height, width, filled_height)
        }
    }
}

/// Computes the filled-bar rect, inset by the border thickness.
pub fn bordered_bar_fill_rect(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    fill01: f32,
    orientation: LinearOrientation,
    border_thickness: f32,
) -> (f32, f32, f32, f32) {
    let inner_width = (width - border_thickness * 2.0).max(0.0);
    let inner_height = (height - border_thickness * 2.0).max(0.0);
    bar_fill_rect(
        x + border_thickness,
        y + border_thickness,
        inner_width,
        inner_height,
        fill01,
        orientation,
    )
}

/// The gauge-local bordered track's fill direction, from empty to full.
pub(super) fn fill_axis(
    width: f32,
    height: f32,
    orientation: LinearOrientation,
    border_thickness: f32,
) -> (Point, Point) {
    let (x, y, width, height) =
        bordered_bar_fill_rect(0.0, 0.0, width, height, 1.0, orientation, border_thickness);
    match orientation {
        LinearOrientation::Horizontal => (
            Point::new(x, y + height * 0.5),
            Point::new(x + width, y + height * 0.5),
        ),
        LinearOrientation::Vertical => (
            Point::new(x + width * 0.5, y + height),
            Point::new(x + width * 0.5, y),
        ),
    }
}

pub(super) fn segment_rect(
    index: u32,
    extent: f32,
    gap: f32,
    width: f32,
    height: f32,
    orientation: LinearOrientation,
) -> Rect {
    match orientation {
        LinearOrientation::Horizontal => {
            Rect::from_xywh(index as f32 * (extent + gap), 0.0, extent, height)
        }
        LinearOrientation::Vertical => Rect::from_xywh(
            0.0,
            height - extent - index as f32 * (extent + gap),
            width,
            extent,
        ),
    }
}

/// Returns the local frame and thickness used by the translated low-fill cap.
pub(super) fn track_cap_frame(
    track: Rect,
    corner_radius: f32,
    orientation: LinearOrientation,
) -> (TrackPathFrame, f32) {
    match orientation {
        LinearOrientation::Horizontal => (
            TrackPathFrame {
                origin: Point::new(track.left + corner_radius, track.top + track.height() * 0.5),
                tangent: Point::new(1.0, 0.0),
                normal: Point::new(0.0, 1.0),
            },
            track.height(),
        ),
        LinearOrientation::Vertical => (
            TrackPathFrame {
                origin: Point::new(
                    track.left + track.width() * 0.5,
                    track.bottom - corner_radius,
                ),
                tangent: Point::new(0.0, -1.0),
                normal: Point::new(1.0, 0.0),
            },
            track.width(),
        ),
    }
}

const LABEL_GAP_PX: f32 = 8.0;

#[derive(Clone, Debug)]
/// Resolved text and baselines for both range endpoints.
pub(super) struct LinearGaugeLabelLayout {
    pub min_label: String,
    pub max_label: String,
    pub min_origin: Point,
    pub max_origin: Point,
}

fn label_gap(font_size: f32) -> f32 {
    (font_size * 0.35).max(LABEL_GAP_PX)
}

/// Places range labels according to the validated orientation/side pairing.
#[expect(
    clippy::too_many_arguments,
    reason = "the labels' frame, font and sampled range are independent inputs"
)]
pub(super) fn label_layout(
    gauge: &ValidatedLinearGaugeWidget,
    width: f32,
    height: f32,
    scale: f32,
    font: &skia_safe::Font,
    min_value: f64,
    max_value: f64,
    altitude_offset_m: f64,
) -> LinearGaugeLabelLayout {
    let gap = label_gap(gauge.min_max_label_font_size * scale);
    let min_label = format_gauge_label(
        gauge.metric,
        gauge.display_unit.as_deref(),
        min_value + altitude_offset_m,
    );
    let max_label = format_gauge_label(
        gauge.metric,
        gauge.display_unit.as_deref(),
        max_value + altitude_offset_m,
    );
    let (_, metrics) = font.metrics();
    let (_, min_bounds) = font.measure_str(&min_label, None);
    let (_, max_bounds) = font.measure_str(&max_label, None);

    match (gauge.orientation, gauge.min_max_label_position) {
        (LinearOrientation::Horizontal, LabelPosition::Top) => {
            let baseline = -gap - metrics.descent;
            LinearGaugeLabelLayout {
                min_origin: Point::new(origin_x_for_centered_text(&min_label, 0.0, font), baseline),
                max_origin: Point::new(
                    origin_x_for_centered_text(&max_label, width, font),
                    baseline,
                ),
                min_label,
                max_label,
            }
        }
        (LinearOrientation::Horizontal, LabelPosition::Bottom) => {
            let baseline = height + gap - metrics.ascent;
            LinearGaugeLabelLayout {
                min_origin: Point::new(origin_x_for_centered_text(&min_label, 0.0, font), baseline),
                max_origin: Point::new(
                    origin_x_for_centered_text(&max_label, width, font),
                    baseline,
                ),
                min_label,
                max_label,
            }
        }
        (LinearOrientation::Vertical, LabelPosition::Left) => LinearGaugeLabelLayout {
            min_origin: Point::new(
                -gap - min_bounds.right,
                height - (min_bounds.top + min_bounds.bottom) * 0.5,
            ),
            max_origin: Point::new(
                -gap - max_bounds.right,
                -(max_bounds.top + max_bounds.bottom) * 0.5,
            ),
            min_label,
            max_label,
        },
        (LinearOrientation::Vertical, LabelPosition::Right) => LinearGaugeLabelLayout {
            min_origin: Point::new(
                width + gap - min_bounds.left,
                height - (min_bounds.top + min_bounds.bottom) * 0.5,
            ),
            max_origin: Point::new(
                width + gap - max_bounds.left,
                -(max_bounds.top + max_bounds.bottom) * 0.5,
            ),
            min_label,
            max_label,
        },
        _ => unreachable!("linear gauge label position must match validated orientation"),
    }
}

/// Builds the padded, cached image containing every frame-invariant element.
pub(super) fn draw_static_layer(
    canvas: &Canvas,
    gauge: &ValidatedLinearGaugeWidget,
    scene: &ValidatedScenePresentation,
    width: f32,
    height: f32,
    scale: f32,
) {
    let width_px = width;
    let height_px = height;
    let radius = gauge.track_corner_radius * scale;
    let border = gauge.track_border_thickness * scale;
    let mut source = Paint::default();
    source.set_anti_alias(true);
    let shadow = if border > 0.0 {
        shadow_paint(
            &source,
            parse_color(&scene.shadow_color, scene.opacity),
            scene.shadow_strength * scale,
        )
    } else {
        None
    };

    if let Some(bar_geometry) = gauge.bar_geometry {
        for index in 0..bar_geometry.count {
            let rect = segment_rect(
                index,
                bar_geometry.extent * scale,
                bar_geometry.gap * scale,
                width_px,
                height_px,
                gauge.orientation,
            );
            draw_static_track(
                canvas,
                gauge,
                rect,
                radius,
                border,
                scene.opacity,
                shadow
                    .as_ref()
                    .map(|paint| (paint, scene.shadow_distance * scale)),
            );
        }
    } else {
        draw_static_track(
            canvas,
            gauge,
            Rect::from_xywh(0.0, 0.0, width_px, height_px),
            radius,
            border,
            scene.opacity,
            shadow
                .as_ref()
                .map(|paint| (paint, scene.shadow_distance * scale)),
        );
    }
}

fn draw_static_track(
    canvas: &Canvas,
    gauge: &ValidatedLinearGaugeWidget,
    rect: Rect,
    configured_radius: f32,
    border: f32,
    opacity: f32,
    shadow: Option<(&Paint, f32)>,
) {
    let radius = configured_radius
        .min(rect.width() * 0.5)
        .min(rect.height() * 0.5);
    let outer_rrect = RRect::new_rect_xy(rect, radius, radius);
    let inner_rect = Rect::from_xywh(
        rect.left + border,
        rect.top + border,
        (rect.width() - border * 2.0).max(0.0),
        (rect.height() - border * 2.0).max(0.0),
    );
    let inner_radius = (radius - border).max(0.0);
    let inner_rrect = RRect::new_rect_xy(inner_rect, inner_radius, inner_radius);

    if let Some((paint, distance)) = shadow {
        canvas.save();
        canvas.translate((distance, distance));
        if border > 0.0 {
            let mut ring_path = PathBuilder::new_with_fill_type(PathFillType::EvenOdd);
            ring_path.add_rrect(outer_rrect, None, None);
            ring_path.add_rrect(inner_rrect, None, None);
            canvas.draw_path(&ring_path.detach(), paint);
        } else {
            canvas.draw_rrect(outer_rrect, paint);
        }
        canvas.restore();
    }

    if border > 0.0 {
        let mut border_paint = Paint::default();
        border_paint.set_anti_alias(true);
        border_paint.set_color(parse_color(&gauge.track_border_color, opacity));
        canvas.draw_rrect(outer_rrect, &border_paint);

        let mut clear_paint = Paint::default();
        clear_paint.set_anti_alias(true);
        clear_paint.set_blend_mode(BlendMode::Clear);
        canvas.draw_rrect(inner_rrect, &clear_paint);
    }

    let mut empty_paint = Paint::default();
    empty_paint.set_anti_alias(true);
    empty_paint.set_color(parse_color(
        &gauge.track_empty_color,
        gauge.track_empty_opacity * opacity,
    ));
    canvas.draw_rrect(inner_rrect, &empty_paint);
}

/// Inner rounded rect of one bar, inset by the border, relative to `origin`.
fn bar_fill_rrect(
    index: u32,
    bars: ResolvedBarGeometry,
    cache: &LinearGaugeCache,
    origin: (f32, f32),
) -> RRect {
    let outer = segment_rect(
        index,
        bars.extent,
        bars.gap,
        cache.width,
        cache.height,
        cache.orientation,
    );
    let border = cache.track_border_thickness;
    let inner = Rect::from_xywh(
        origin.0 + outer.left + border,
        origin.1 + outer.top + border,
        (outer.width() - border * 2.0).max(0.0),
        (outer.height() - border * 2.0).max(0.0),
    );
    let radius = (cache.track_corner_radius - border)
        .max(0.0)
        .min(inner.width() * 0.5)
        .min(inner.height() * 0.5);
    RRect::new_rect_xy(inner, radius, radius)
}

/// The border-inset continuous track relative to `origin`.
fn inner_track_rrect(cache: &LinearGaugeCache, origin: (f32, f32)) -> RRect {
    let inset = cache.track_border_thickness;
    let inner_rect = Rect::from_xywh(
        origin.0 + inset,
        origin.1 + inset,
        (cache.width - inset * 2.0).max(0.0),
        (cache.height - inset * 2.0).max(0.0),
    );
    let radius = (cache.track_corner_radius - inset).max(0.0);
    RRect::new_rect_xy(inner_rect, radius, radius)
}

/// Paints the completely filled track relative to the gauge origin; per-frame clips
/// reveal this cached gradient raster instead of shading the gradient every frame.
pub(super) fn draw_full_fill_layer(canvas: &Canvas, cache: &LinearGaugeCache, paint: &Paint) {
    match (cache.track_fill_style, cache.bar_geometry) {
        (TrackFillStyle::Bars, Some(bars)) => {
            for index in 0..bars.count {
                canvas.draw_rrect(bar_fill_rrect(index, bars, cache, (0.0, 0.0)), paint);
            }
        }
        _ => {
            canvas.draw_rrect(inner_track_rrect(cache, (0.0, 0.0)), paint);
        }
    }
}

/// Draws only complete bars whose discrete buckets are active at `fill01`.
pub(super) fn draw_segmented_fill(canvas: &Canvas, cache: &LinearGaugeCache, fill01: f32) {
    let bars = cache
        .bar_geometry
        .expect("bars fill style must carry resolved bar geometry");
    let filled_count = bar_fill_count(fill01, bars.count) as u32;
    if filled_count == 0 {
        return;
    }
    // Gaps are empty in the full-fill raster, so one rect ending mid-gap reveals whole bars.
    let segment = |index| {
        segment_rect(
            index,
            bars.extent,
            bars.gap,
            cache.width,
            cache.height,
            cache.orientation,
        )
    };
    let mut reveal = segment(0);
    reveal.join(segment(filled_count - 1));
    reveal.outset((bars.gap * 0.5, bars.gap * 0.5));
    reveal.offset((cache.x, cache.y));
    cache.fill.draw(canvas, &FillShape::Rect(reveal), |paint| {
        for index in 0..filled_count {
            canvas.draw_rrect(
                bar_fill_rrect(index, bars, cache, (cache.x, cache.y)),
                paint,
            );
        }
    });
}

/// Reveals the continuous source track while preserving its configured caps.
pub(super) fn draw_continuous_fill(canvas: &Canvas, cache: &LinearGaugeCache, fill01: f32) {
    let (x, y, width, height) = bordered_bar_fill_rect(
        cache.x,
        cache.y,
        cache.width,
        cache.height,
        fill01,
        cache.orientation,
        cache.track_border_thickness,
    );
    if width <= 0.0 || height <= 0.0 {
        return;
    }

    let fill_rect = Rect::from_xywh(x, y, width, height);
    let radius = (cache.track_corner_radius - cache.track_border_thickness).max(0.0);
    let inner_rrect = inner_track_rrect(cache, (cache.x, cache.y));
    if radius == 0.0 {
        let reveal = FillShape::Rect(fill_rect);
        cache
            .fill
            .draw(canvas, &reveal, |paint| reveal.draw(canvas, paint));
        return;
    }
    if cache.track_fill_flat {
        cache
            .fill
            .draw(canvas, &FillShape::Rect(fill_rect), |paint| {
                canvas.save();
                canvas.clip_rect(fill_rect, ClipOp::Intersect, true);
                canvas.draw_rrect(inner_rrect, paint);
                canvas.restore();
            });
        return;
    }
    let revealed_length = match cache.orientation {
        LinearOrientation::Horizontal => width,
        LinearOrientation::Vertical => height,
    };
    let reveal = match translated_track_cap_reveal(revealed_length, radius) {
        Some(cap) => {
            let (frame, track_thickness) =
                track_cap_frame(*inner_rrect.rect(), radius, cache.orientation);
            FillShape::Path(translated_track_cap_path(frame, track_thickness, cap))
        }
        None => FillShape::RRect(RRect::new_rect_xy(fill_rect, radius, radius)),
    };
    cache.fill.draw(canvas, &reveal, |paint| {
        canvas.save();
        canvas.clip_rrect(inner_rrect, ClipOp::Intersect, true);
        reveal.draw(canvas, paint);
        canvas.restore();
    });
}
