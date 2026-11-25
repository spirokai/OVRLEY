//! Vertical altitude or depth tape with a live readout and a vertical-speed trend.
use crate::activity::ActivitySampler;
use crate::model::catalog::{altitude_tape_major_tick_intervals, catalog, DisplayType, MetricKind};
use crate::model::scene::{
    validate_scene_presentation, Numerals, ScenePresentationConfig, ValidatedScenePresentation,
};
use crate::model::validation::{
    require_bool, require_f32, require_hex_color, require_non_negative_f32, require_percentage,
    require_positive_f32, require_positive_u32, require_string, require_unit_opacity,
};
use crate::preview::ResizeGesture;
use crate::preview::WidgetHost;
use crate::render::text::{layout_text, parse_color, resolve_font, shadow_paint, FontResolver};
use crate::render::tile::{surface, TileBounds, SHAPE_COLOR_TYPE};
use crate::render::tile::{Presented, Tile};
use crate::render::tile::{PreviewRasterScale, StaticRaster};
use crate::widgets::value::format::{convert_standard_metric_value, format_number};
use crate::widgets::value::{normalize_starting_altitude_m, ValueConfig};
use crate::{WidgetError, WidgetResult};
use serde::Serialize;
use skia_safe::canvas::SaveLayerRec;
use skia_safe::paint::{Cap, Style};
use skia_safe::{
    Canvas, ClipOp, Color, FilterMode, Font, Image, Paint, Path, PathBuilder, Point, Rect, TextBlob,
};

/// The metric the tape scrolls. Altitude grows upwards; depth grows downwards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TapeSource {
    Altitude,
    Depth,
}

impl TapeSource {
    pub fn metric(self) -> MetricKind {
        match self {
            Self::Altitude => MetricKind::Altitude,
            Self::Depth => MetricKind::Depth,
        }
    }

    /// +1 when larger values sit higher on the tape, -1 when they sit lower.
    pub fn direction(self) -> f64 {
        match self {
            Self::Altitude => 1.0,
            Self::Depth => -1.0,
        }
    }
}

/// Direction the readout points; ticks and labels sit on the opposite edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TapeOrientation {
    Left,
    Right,
}

#[derive(Clone, Debug)]
pub struct ValidatedAltitudeTape {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub opacity: f32,
    pub source: TapeSource,
    pub orientation: TapeOrientation,
    /// Tape and readout unit, one of the source metric's display units.
    pub display_unit: String,
    pub starting_altitude_m: Option<f64>,
    pub major_tick_interval: u32,
    pub minor_ticks_per_major: u32,
    pub major_tick_spacing: f32,
    pub major_tick_length_pct: f32,
    pub minor_tick_length_pct: f32,
    pub major_tick_thickness: f32,
    pub minor_tick_thickness: f32,
    pub major_tick_color: String,
    pub minor_tick_color: String,
    pub show_tick_labels: bool,
    pub label_font: String,
    pub label_font_size: f32,
    pub label_color: String,
    pub label_offset: f32,
    pub font: String,
    pub font_size: f32,
    pub color: String,
    pub show_value_decimal: bool,
    pub value_background_color: String,
    pub value_border_color: String,
    pub value_border_thickness: f32,
    pub value_shadow_color: String,
    pub value_shadow_sigma: f32,
    pub value_shadow_offset: f32,
    pub show_vertical_speed: bool,
    pub vertical_speed_unit: String,
    pub show_vertical_speed_decimal: bool,
    pub vertical_speed_font_size: f32,
    pub vertical_speed_bar_thickness: f32,
    pub vertical_speed_positive_color: String,
    pub vertical_speed_negative_color: String,
    pub vertical_speed_trend_seconds: f32,
}

pub fn validate_altitude_tape(
    value: ValueConfig,
    index: usize,
) -> WidgetResult<ValidatedAltitudeTape> {
    let p = |field: &str| format!("values[{index}].{field}");
    let source = match (value.value, value.display_type) {
        (MetricKind::Altitude, DisplayType::AltitudeTape) => TapeSource::Altitude,
        (MetricKind::Depth, DisplayType::AltitudeTape) => TapeSource::Depth,
        _ => {
            return Err(WidgetError::Config(format!(
                "values[{index}]: expected altitude or depth with altitude_tape"
            )))
        }
    };
    let starting_altitude_m = normalize_starting_altitude_m(
        value.value,
        value.starting_altitude,
        value.display_unit.as_deref(),
        &p("starting_altitude"),
        &p("display_unit"),
    )?;
    let display_unit = require_string(value.display_unit, &p("display_unit"))?;
    let supports = |metric: MetricKind, unit: &str| {
        catalog()
            .standard_metric(metric)
            .expect("tape metrics are standard metrics")
            .supported_display_units
            .iter()
            .any(|option| option.value == unit)
    };
    if !supports(source.metric(), &display_unit) {
        return Err(WidgetError::Config(format!(
            "{}: invalid value '{display_unit}'",
            p("display_unit")
        )));
    }
    let vertical_speed_unit = require_string(value.vertical_speed_unit, &p("vertical_speed_unit"))?;
    // The readout offers exactly the vertical_speed metric's display units.
    if !supports(MetricKind::VerticalSpeed, &vertical_speed_unit) {
        return Err(WidgetError::Config(format!(
            "{}: invalid value '{vertical_speed_unit}'",
            p("vertical_speed_unit")
        )));
    }
    let major_tick_interval =
        require_positive_u32(value.major_tick_interval, &p("major_tick_interval"))?;
    if !altitude_tape_major_tick_intervals().contains(&major_tick_interval) {
        return Err(WidgetError::Config(format!(
            "{}: {major_tick_interval} is not an offered interval",
            p("major_tick_interval")
        )));
    }
    let orientation = match require_string(value.tape_orientation, &p("tape_orientation"))?.as_str()
    {
        "left" => TapeOrientation::Left,
        "right" => TapeOrientation::Right,
        other => {
            return Err(WidgetError::Config(format!(
                "{}: invalid value '{other}'",
                p("tape_orientation")
            )))
        }
    };
    let font = |raw: Option<String>, field: &str| {
        let font = require_string(raw, &p(field))?;
        if font.trim().is_empty() {
            return Err(WidgetError::Config(format!("{}: required", p(field))));
        }
        Ok(font)
    };
    let non_negative = |raw, field| require_non_negative_f32(raw, &p(field));
    let percentage = |raw, field| require_percentage(raw, &p(field));
    let color = |raw: Option<String>, field: &str| require_hex_color(raw.as_deref(), &p(field));
    let positive = |raw, field: &str| require_positive_f32(raw, &p(field));
    Ok(ValidatedAltitudeTape {
        x: require_f32(Some(value.x), &p("x"))?,
        y: require_f32(Some(value.y), &p("y"))?,
        width: positive(value.width, "width")?,
        height: positive(value.height, "height")?,
        opacity: require_unit_opacity(value.opacity, &p("opacity"))?,
        source,
        orientation,
        display_unit,
        starting_altitude_m,
        major_tick_interval,
        minor_ticks_per_major: require_positive_u32(
            value.minor_ticks_per_major,
            &p("minor_ticks_per_major"),
        )?,
        major_tick_spacing: positive(value.major_tick_spacing, "major_tick_spacing")?,
        major_tick_length_pct: percentage(value.major_tick_length_pct, "major_tick_length_pct")?,
        minor_tick_length_pct: percentage(value.minor_tick_length_pct, "minor_tick_length_pct")?,
        major_tick_thickness: positive(value.major_tick_thickness, "major_tick_thickness")?,
        minor_tick_thickness: positive(value.minor_tick_thickness, "minor_tick_thickness")?,
        major_tick_color: color(value.major_tick_color, "major_tick_color")?,
        minor_tick_color: color(value.minor_tick_color, "minor_tick_color")?,
        show_tick_labels: require_bool(value.show_tick_labels, &p("show_tick_labels"))?,
        label_font: font(value.label_font, "label_font")?,
        label_font_size: positive(value.label_font_size, "label_font_size")?,
        label_color: color(value.label_color, "label_color")?,
        label_offset: non_negative(value.label_offset, "label_offset")?,
        font: font(value.font, "font")?,
        font_size: positive(value.font_size, "font_size")?,
        color: color(value.color, "color")?,
        show_value_decimal: require_bool(value.show_value_decimal, &p("show_value_decimal"))?,
        value_background_color: color(value.value_background_color, "value_background_color")?,
        value_border_color: color(value.value_border_color, "value_border_color")?,
        value_border_thickness: non_negative(
            value.value_border_thickness,
            "value_border_thickness",
        )?,
        value_shadow_color: color(value.value_shadow_color, "value_shadow_color")?,
        value_shadow_sigma: non_negative(value.value_shadow_sigma, "value_shadow_sigma")?,
        value_shadow_offset: non_negative(value.value_shadow_offset, "value_shadow_offset")?,
        show_vertical_speed: require_bool(value.show_vertical_speed, &p("show_vertical_speed"))?,
        vertical_speed_unit,
        show_vertical_speed_decimal: require_bool(
            value.show_vertical_speed_decimal,
            &p("show_vertical_speed_decimal"),
        )?,
        vertical_speed_font_size: positive(
            value.vertical_speed_font_size,
            "vertical_speed_font_size",
        )?,
        vertical_speed_bar_thickness: positive(
            value.vertical_speed_bar_thickness,
            "vertical_speed_bar_thickness",
        )?,
        vertical_speed_positive_color: color(
            value.vertical_speed_positive_color,
            "vertical_speed_positive_color",
        )?,
        vertical_speed_negative_color: color(
            value.vertical_speed_negative_color,
            "vertical_speed_negative_color",
        )?,
        vertical_speed_trend_seconds: positive(
            value.vertical_speed_trend_seconds,
            "vertical_speed_trend_seconds",
        )?,
    })
}

/// The vertical-speed footer row is this many of its font sizes tall.
pub const TEXT_ROW_RATIO: f32 = 1.8;
/// The readout box is this many value font sizes tall; its pointer is this share of its height deep.
pub const VALUE_BOX_HEIGHT_RATIO: f32 = 1.4;
pub const VALUE_POINTER_RATIO: f32 = 0.35;
/// Readout text keeps this share of the box height clear of the box's flat border.
const VALUE_PADDING_RATIO: f32 = 0.25;
/// The vertical-speed trend reaches at most this share of the tape body from its centre.
pub const TREND_MAX_BODY_SHARE: f32 = 0.5;
/// The vertical-speed box fits this speed's digits in the chosen unit; faster readings extend
/// leftwards without moving the unit.
const READOUT_REFERENCE_SPEED_MPS: f64 = 20.0;
/// Text used to centre digit labels on their tick.
const DIGIT_REFERENCE: &str = "0123456789";

/// Header, scrolling body, footer, and the fixed readout box centred on the body. Ticks sit on
/// the edge opposite the readout's pointer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AltitudeTapeLayout {
    pub orientation: TapeOrientation,
    pub frame: Rect,
    pub body: Rect,
    pub footer: Rect,
    pub value_box: Rect,
    pub pointer_depth: f32,
    pub major_length: f32,
    pub minor_length: f32,
}

impl AltitudeTapeLayout {
    pub fn center_y(&self) -> f32 {
        self.body.center_y()
    }

    /// Scene x of the readout pointer's tip, where the vertical-speed trend starts.
    pub fn tip_x(&self) -> f32 {
        match self.orientation {
            TapeOrientation::Left => self.frame.left,
            TapeOrientation::Right => self.frame.right,
        }
    }

    /// Frame-local x span of a tick of `length` on the tick edge.
    pub fn tick_span(&self, length: f32) -> (f32, f32) {
        match self.orientation {
            TapeOrientation::Left => (self.frame.width() - length, self.frame.width()),
            TapeOrientation::Right => (0.0, length),
        }
    }

    /// Scene x `inset` inwards from the tick edge.
    pub fn from_tick_edge(&self, inset: f32) -> f32 {
        match self.orientation {
            TapeOrientation::Left => self.frame.right - inset,
            TapeOrientation::Right => self.frame.left + inset,
        }
    }

    /// Text origin for an advance `width` anchored at `anchor`; labels and the readout align
    /// towards the tick edge.
    pub fn text_x(&self, anchor: f32, width: f32) -> f32 {
        match self.orientation {
            TapeOrientation::Left => anchor - width,
            TapeOrientation::Right => anchor,
        }
    }

    /// Pentagon whose tip sits on the pointer edge at the body centre.
    pub fn value_box_path(&self) -> Path {
        let rect = self.value_box;
        let (tip, shoulder, flat) = match self.orientation {
            TapeOrientation::Left => (rect.left, rect.left + self.pointer_depth, rect.right),
            TapeOrientation::Right => (rect.right, rect.right - self.pointer_depth, rect.left),
        };
        let mut path = PathBuilder::new();
        path.move_to((tip, rect.center_y()));
        path.line_to((shoulder, rect.top));
        path.line_to((flat, rect.top));
        path.line_to((flat, rect.bottom));
        path.line_to((shoulder, rect.bottom));
        path.close();
        path.detach()
    }
}

/// `vertical_speed_font_size` is absent when the vertical-speed readout is hidden; its footer
/// row then collapses.
pub fn altitude_tape_layout(
    frame: Rect,
    orientation: TapeOrientation,
    vertical_speed_font_size: Option<f32>,
    value_font_size: f32,
    major_tick_length_pct: f32,
    minor_tick_length_pct: f32,
) -> AltitudeTapeLayout {
    let footer_height = vertical_speed_font_size.map_or(0.0, |size| size * TEXT_ROW_RATIO);
    let body_bottom = (frame.bottom - footer_height).max(frame.top + 1.0);
    let body = Rect::new(frame.left, frame.top, frame.right, body_bottom);
    let box_height = value_font_size * VALUE_BOX_HEIGHT_RATIO;
    AltitudeTapeLayout {
        orientation,
        frame,
        body,
        footer: Rect::new(
            frame.left,
            body_bottom,
            frame.right,
            body_bottom + footer_height,
        ),
        value_box: Rect::from_xywh(
            frame.left,
            body.center_y() - box_height * 0.5,
            frame.width(),
            box_height,
        ),
        pointer_depth: box_height * VALUE_POINTER_RATIO,
        major_length: frame.width() * major_tick_length_pct / 100.0,
        minor_length: frame.width() * minor_tick_length_pct / 100.0,
    }
}

/// Scene y of tape `value` while `current` sits on the body centre. With `direction` +1 larger
/// values sit higher; with -1 (depth) they sit lower.
pub fn tape_y(
    center_y: f32,
    value: f64,
    current: f64,
    pixels_per_unit: f64,
    direction: f64,
) -> f32 {
    center_y - ((value - current) * pixels_per_unit * direction) as f32
}

/// Major tick values whose labels can intersect the body around `current`.
pub fn visible_major_values(
    current: f64,
    interval: u32,
    pixels_per_unit: f64,
    half_extent: f32,
) -> impl Iterator<Item = f64> {
    let interval = f64::from(interval);
    let reach = f64::from(half_extent) / pixels_per_unit;
    let first = ((current - reach) / interval).ceil() as i64;
    let last = ((current + reach) / interval).floor() as i64;
    (first..=last).map(move |index| index as f64 * interval)
}

/// Signed trend length in scene pixels (positive climbs), capped at a quarter of the body.
pub fn trend_length(
    vertical_speed_mps: f64,
    trend_seconds: f32,
    pixels_per_meter: f64,
    body_height: f32,
) -> f32 {
    let limit = body_height * TREND_MAX_BODY_SHARE;
    ((vertical_speed_mps * f64::from(trend_seconds) * pixels_per_meter) as f32).clamp(-limit, limit)
}

/// Fill plus optional scene shadow for text drawn each frame.
#[derive(Clone, Debug)]
pub(crate) struct TextPaint {
    pub fill: Paint,
    pub shadow: Option<Paint>,
    pub shadow_distance: f32,
}

#[derive(Clone, Debug)]
pub struct AltitudeTapeCache {
    pub(crate) layout: AltitudeTapeLayout,
    /// Whole major periods of ticks covering the body plus one period, a major at the top edge.
    /// Scrolling draws it once at an offset within one period.
    pub(crate) tick_strip: Image,
    pub(crate) raster_scale: f32,
    /// Scene height of one major period.
    pub(crate) period: f32,
    /// Scene pixels per tape unit, snapped so one period is a whole number of image pixels.
    pub(crate) pixels_per_unit: f64,
    /// Tape units per metre: the display unit's scale.
    pub(crate) units_per_meter: f64,
    pub(crate) source: TapeSource,
    /// Right-hand overhang of the tick image for shadows.
    pub(crate) tick_overhang: f32,
    pub(crate) major_tick_interval: u32,
    pub(crate) display_unit: String,
    /// Absent for depth, which has no starting offset.
    pub(crate) starting_altitude_m: Option<f64>,
    pub(crate) show_tick_labels: bool,
    pub(crate) label_font: Font,
    pub(crate) label_paint: TextPaint,
    /// Tick-side edge of tick labels and the baseline offset that centres digits on a tick.
    pub(crate) label_anchor: f32,
    pub(crate) label_baseline_offset: f32,
    pub(crate) value_box: StaticRaster,
    pub(crate) value_font: Font,
    pub(crate) value_paint: Paint,
    pub(crate) value_anchor: f32,
    pub(crate) value_baseline_offset: f32,
    pub(crate) show_value_decimal: bool,
    pub(crate) show_vertical_speed: bool,
    pub(crate) vertical_speed_font: Font,
    pub(crate) vertical_speed_baseline: f32,
    /// Right edge of the fixed number box; the box and unit are centred together.
    pub(crate) vertical_speed_value_right: f32,
    /// The unit label after the box; absent for an empty label.
    pub(crate) vertical_speed_unit_text: Option<(TextBlob, Point)>,
    pub(crate) vertical_speed_unit: String,
    pub(crate) show_vertical_speed_decimal: bool,
    pub(crate) bar_thickness: f32,
    pub(crate) bar_positive: Paint,
    pub(crate) bar_negative: Paint,
    pub(crate) climb_text_paint: TextPaint,
    pub(crate) descent_text_paint: TextPaint,
    pub(crate) trend_seconds: f32,
    pub(crate) numerals: Numerals,
    /// Stable raster bounds: nothing the frame draws leaves them.
    pub(crate) bounds: Rect,
    pub(crate) content_bounds: Rect,
    pub(crate) opacity: f32,
}

pub fn prepare_altitude_tape_cache(
    widget: &ValidatedAltitudeTape,
    scene: &ValidatedScenePresentation,
    fonts: &dyn FontResolver,
    raster_scale: PreviewRasterScale,
) -> WidgetResult<AltitudeTapeCache> {
    let scale = scene.scale;
    let frame = Rect::from_xywh(
        widget.x,
        widget.y,
        widget.width * scale,
        widget.height * scale,
    );
    let label_font_size = widget.label_font_size * scale;
    let vertical_speed_font_size = widget.vertical_speed_font_size * scale;
    let layout = altitude_tape_layout(
        frame,
        widget.orientation,
        widget
            .show_vertical_speed
            .then_some(vertical_speed_font_size),
        widget.font_size * scale,
        widget.major_tick_length_pct,
        widget.minor_tick_length_pct,
    );
    let shadow_sigma = scene.shadow_strength * scale;
    let shadow_distance = scene.shadow_distance * scale;
    let shadow_color = parse_color(&scene.shadow_color, 1.0);
    let shadow_margin = if shadow_sigma > 0.0 {
        shadow_sigma * 3.0 + shadow_distance
    } else {
        0.0
    } + 1.0;

    // A whole-pixel period keeps the strip periodic on the host's pixel grid.
    let raster = raster_scale.value();
    let period_pixels = (widget.major_tick_spacing * scale * raster)
        .round()
        .max(1.0);
    let period = period_pixels / raster;
    let pixels_per_unit = f64::from(period) / f64::from(widget.major_tick_interval);
    let tick_overhang = shadow_margin;
    let tick_strip = tick_strip_image(
        widget,
        &layout,
        period,
        raster,
        period_pixels as u32,
        scale,
        tick_overhang,
        shadow_color,
        shadow_sigma,
        shadow_distance,
    )?;

    let text_paint = |color: &str| {
        let mut fill = Paint::default();
        fill.set_anti_alias(true);
        fill.set_color(parse_color(color, 1.0));
        TextPaint {
            shadow: shadow_paint(&fill, shadow_color, shadow_sigma),
            fill,
            shadow_distance,
        }
    };
    let label_paint = text_paint(&widget.label_color);
    let label_font = resolve_font(fonts, Some(&widget.label_font), label_font_size)?;
    let label_baseline_offset = digit_baseline_offset(&label_font, scene.numerals);

    let value_shadow_sigma = widget.value_shadow_sigma * scale;
    let value_shadow_offset = widget.value_shadow_offset * scale;
    let border = widget.value_border_thickness * scale;
    let mut box_bounds = layout.value_box;
    box_bounds.outset((
        value_shadow_sigma * 3.0 + 1.0,
        value_shadow_sigma * 3.0 + 1.0,
    ));
    box_bounds.bottom += value_shadow_offset;
    let value_box = StaticRaster::new(
        raster_scale,
        Point::default(),
        box_bounds,
        SHAPE_COLOR_TYPE,
        |canvas| {
            draw_value_box(
                canvas,
                &layout.value_box_path(),
                widget,
                border,
                value_shadow_sigma,
                value_shadow_offset,
            );
            Ok(())
        },
    )?;
    let value_font = resolve_font(fonts, Some(&widget.font), widget.font_size * scale)?;
    let mut value_paint = Paint::default();
    value_paint.set_anti_alias(true);
    value_paint.set_color(parse_color(&widget.color, 1.0));

    let vertical_speed_font =
        resolve_font(fonts, Some(&widget.label_font), vertical_speed_font_size)?;
    let vertical_speed_baseline =
        layout.footer.center_y() + digit_baseline_offset(&vertical_speed_font, scene.numerals);
    let (vertical_speed_value_right, vertical_speed_unit_text) = vertical_speed_readout_layout(
        &layout,
        &vertical_speed_font,
        scene.numerals,
        &widget.vertical_speed_unit,
        widget.show_vertical_speed_decimal,
        vertical_speed_baseline,
    );
    let bar_paint = |color: &str| {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(parse_color(color, 1.0));
        paint
    };

    let mut bounds = frame;
    bounds.right += tick_overhang;
    bounds.join(value_box.bounds());
    bounds.outset((shadow_margin, shadow_margin));
    Ok(AltitudeTapeCache {
        layout,
        tick_strip,
        raster_scale: raster,
        period,
        pixels_per_unit,
        units_per_meter: convert_standard_metric_value(
            widget.source.metric(),
            Some(&widget.display_unit),
            1.0,
        ),
        source: widget.source,
        tick_overhang,
        major_tick_interval: widget.major_tick_interval,
        display_unit: widget.display_unit.clone(),
        starting_altitude_m: widget.starting_altitude_m,
        show_tick_labels: widget.show_tick_labels,
        label_anchor: layout.from_tick_edge(layout.major_length + widget.label_offset * scale),
        label_baseline_offset,
        label_font,
        label_paint,
        value_box,
        value_baseline_offset: digit_baseline_offset(&value_font, scene.numerals),
        value_anchor: layout
            .from_tick_edge(border + layout.value_box.height() * VALUE_PADDING_RATIO),
        value_font,
        value_paint,
        show_value_decimal: widget.show_value_decimal,
        show_vertical_speed: widget.show_vertical_speed,
        vertical_speed_baseline,
        vertical_speed_value_right,
        vertical_speed_unit_text,
        vertical_speed_font,
        vertical_speed_unit: widget.vertical_speed_unit.clone(),
        show_vertical_speed_decimal: widget.show_vertical_speed_decimal,
        bar_thickness: widget.vertical_speed_bar_thickness * scale,
        bar_positive: bar_paint(&widget.vertical_speed_positive_color),
        bar_negative: bar_paint(&widget.vertical_speed_negative_color),
        climb_text_paint: text_paint(&widget.vertical_speed_positive_color),
        descent_text_paint: text_paint(&widget.vertical_speed_negative_color),
        trend_seconds: widget.vertical_speed_trend_seconds,
        numerals: scene.numerals,
        bounds: Rect::new(
            bounds.left.floor(),
            bounds.top.floor(),
            bounds.right.ceil(),
            bounds.bottom.ceil(),
        ),
        content_bounds: frame,
        opacity: widget.opacity * scene.opacity,
    })
}

/// Places the fixed-width number box and the unit after it, centred together in the footer.
fn vertical_speed_readout_layout(
    layout: &AltitudeTapeLayout,
    font: &Font,
    numerals: Numerals,
    unit: &str,
    decimal: bool,
    baseline: f32,
) -> (f32, Option<(TextBlob, Point)>) {
    let reference = convert_standard_metric_value(
        MetricKind::VerticalSpeed,
        Some(unit),
        READOUT_REFERENCE_SPEED_MPS,
    );
    let digits = "0".repeat(format!("{}", reference.trunc() as i64).len());
    let widest = if decimal {
        format!("-{digits}.0")
    } else {
        format!("-{digits}")
    };
    let box_width = layout_text(&widest, font, 0.0, numerals).measurement.width;
    let gap = font.measure_str(" ", None).0;
    let label = catalog().unit_label(MetricKind::VerticalSpeed, Some(unit));
    let unit_text = layout_text(label, font, 0.0, numerals);
    let left = layout.footer.center_x() - (box_width + gap + unit_text.measurement.width) * 0.5;
    let value_right = left + box_width;
    let unit = unit_text
        .blob
        .map(|blob| (blob, Point::new(value_right + gap, baseline)));
    (value_right, unit)
}

/// Offset from a vertical centre to the baseline that centres digit ink on it.
fn digit_baseline_offset(font: &Font, numerals: Numerals) -> f32 {
    let ink = layout_text(DIGIT_REFERENCE, font, 0.0, numerals).measurement;
    -(ink.bounds_top + ink.bounds_bottom) * 0.5
}

/// Rasterizes enough whole major periods at the host density to cover the body at any offset
/// within one period. Ticks one period beyond either edge keep edge strokes and shadows whole.
#[expect(
    clippy::too_many_arguments,
    reason = "the strip's period, density and shadow are independent inputs of one rasterization"
)]
fn tick_strip_image(
    widget: &ValidatedAltitudeTape,
    layout: &AltitudeTapeLayout,
    period: f32,
    raster: f32,
    period_pixels: u32,
    scale: f32,
    overhang: f32,
    shadow_color: Color,
    shadow_sigma: f32,
    shadow_distance: f32,
) -> WidgetResult<Image> {
    let width = layout.frame.width();
    // Shadows offset to the right, so the image overhangs the frame's right edge.
    let image_width = ((width + overhang) * raster).ceil() as u32;
    let periods = (layout.body.height() * raster / period_pixels as f32).ceil() as u32 + 1;
    let mut tape = surface(
        image_width.max(1),
        period_pixels * periods,
        SHAPE_COLOR_TYPE,
    )?;
    let canvas = tape.canvas();
    canvas.clear(Color::TRANSPARENT);
    canvas.scale((raster, raster));
    let minor_count = widget.minor_ticks_per_major;
    let ticks: Vec<(f32, bool)> = (-1..=periods as i32)
        .flat_map(|copy| {
            (0..minor_count).map(move |index| {
                let y = period * (copy as f32 + index as f32 / minor_count as f32);
                (y, index == 0)
            })
        })
        .collect();
    for shadow_pass in [true, false] {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(Style::Stroke);
        paint.set_stroke_cap(Cap::Butt);
        for (y, is_major) in &ticks {
            let (length, thickness, color) = if *is_major {
                (
                    layout.major_length,
                    widget.major_tick_thickness,
                    &widget.major_tick_color,
                )
            } else {
                (
                    layout.minor_length,
                    widget.minor_tick_thickness,
                    &widget.minor_tick_color,
                )
            };
            paint.set_color(parse_color(color, 1.0));
            paint.set_stroke_width(thickness * scale);
            let shadow = if shadow_pass {
                match shadow_paint(&paint, shadow_color, shadow_sigma) {
                    Some(shadow) => Some(shadow),
                    None => continue,
                }
            } else {
                None
            };
            let offset = if shadow_pass { shadow_distance } else { 0.0 };
            let (start, end) = layout.tick_span(length);
            canvas.draw_line(
                Point::new(start + offset, y + offset),
                Point::new(end + offset, y + offset),
                shadow.as_ref().unwrap_or(&paint),
            );
        }
    }
    Ok(tape.image_snapshot())
}

fn draw_value_box(
    canvas: &Canvas,
    path: &Path,
    widget: &ValidatedAltitudeTape,
    border: f32,
    shadow_sigma: f32,
    shadow_offset: f32,
) {
    let mut fill = Paint::default();
    fill.set_anti_alias(true);
    fill.set_color(parse_color(&widget.value_background_color, 1.0));
    // An opaque source keeps the shadow at its own colour alpha under a translucent background.
    let mut opaque = Paint::default();
    opaque.set_anti_alias(true);
    if let Some(shadow) = shadow_paint(
        &opaque,
        parse_color(&widget.value_shadow_color, 1.0),
        shadow_sigma,
    ) {
        canvas.draw_path(&path.with_offset((0.0, shadow_offset)), &shadow);
    }
    canvas.draw_path(path, &fill);
    if border > 0.0 {
        // A doubled stroke clipped to the outline keeps the border inside the box.
        let mut stroke = Paint::default();
        stroke.set_anti_alias(true);
        stroke.set_style(Style::Stroke);
        stroke.set_stroke_width(border * 2.0);
        stroke.set_color(parse_color(&widget.value_border_color, 1.0));
        canvas.save();
        canvas.clip_path(path, ClipOp::Intersect, true);
        canvas.draw_path(path, &stroke);
        canvas.restore();
    }
}

/// Missing external data shows this placeholder instead of a number.
pub const MISSING_TEXT: &str = "--";

pub struct AltitudeTapeFrame {
    /// Tape position in display units; a missing sample rests the tape on zero.
    altitude: f64,
    value: Option<(TextBlob, Point)>,
    vertical_speed: Option<(TextBlob, Point, Option<bool>)>,
    trend: Option<(Rect, bool)>,
}

/// Altitude or depth readout in the tape's unit.
pub fn format_tape_value(
    metric: MetricKind,
    value_m: Option<f64>,
    unit: &str,
    decimal: bool,
) -> String {
    value_m.map_or_else(
        || MISSING_TEXT.into(),
        |value| {
            let value = convert_standard_metric_value(metric, Some(unit), value);
            format_number(value, usize::from(decimal))
        },
    )
}

/// Vertical-speed number in its unit, and whether it climbs (`Some(true)`) or descends. A value
/// that reads as zero or a missing sample has no direction.
pub fn format_vertical_speed_value(
    speed_mps: Option<f64>,
    unit: &str,
    decimal: bool,
) -> (String, Option<bool>) {
    let Some(speed) = speed_mps else {
        return (MISSING_TEXT.into(), None);
    };
    let value = convert_standard_metric_value(MetricKind::VerticalSpeed, Some(unit), speed);
    let text = format_number(value, usize::from(decimal));
    // The sign follows the rounded text, so "0.0" is never coloured as a climb or descent.
    let climbing = match text.parse::<f64>().expect("formatted readout is numeric") {
        shown if shown > 0.0 => Some(true),
        shown if shown < 0.0 => Some(false),
        _ => None,
    };
    (text, climbing)
}

impl AltitudeTapeCache {
    /// `value_m` is the source's altitude (with its starting offset) or depth in metres;
    /// `vertical_speed_mps` is positive while ascending for either source.
    pub fn prepare_frame(
        &self,
        value_m: Option<f64>,
        vertical_speed_mps: Option<f64>,
    ) -> AltitudeTapeFrame {
        let layout = &self.layout;
        let metric = self.source.metric();
        let value_text =
            format_tape_value(metric, value_m, &self.display_unit, self.show_value_decimal);
        let value = layout_text(&value_text, &self.value_font, 0.0, self.numerals);
        let value = value.blob.map(|blob| {
            (
                blob,
                Point::new(
                    layout.text_x(self.value_anchor, value.measurement.width),
                    layout.center_y() + self.value_baseline_offset,
                ),
            )
        });
        let vertical_speed = self.show_vertical_speed.then(|| {
            let (text, climbing) = format_vertical_speed_value(
                vertical_speed_mps,
                &self.vertical_speed_unit,
                self.show_vertical_speed_decimal,
            );
            let speed = layout_text(&text, &self.vertical_speed_font, 0.0, self.numerals);
            // Numbers right-align in the readout's fixed box, so the unit never moves.
            speed.blob.map(|blob| {
                (
                    blob,
                    Point::new(
                        self.vertical_speed_value_right - speed.measurement.width,
                        self.vertical_speed_baseline,
                    ),
                    climbing,
                )
            })
        });
        let trend = vertical_speed_mps
            .filter(|_| self.show_vertical_speed)
            .map(|speed| {
                trend_length(
                    speed,
                    self.trend_seconds,
                    self.pixels_per_unit * self.units_per_meter,
                    layout.body.height(),
                )
            })
            .filter(|length| *length != 0.0)
            .map(|length| {
                let center = layout.center_y();
                // The bar starts at the pointer tip and stays inside the frame.
                let tip = layout.tip_x();
                let (left, right) = match layout.orientation {
                    TapeOrientation::Left => (tip, tip + self.bar_thickness),
                    TapeOrientation::Right => (tip - self.bar_thickness, tip),
                };
                (
                    Rect::new(
                        left,
                        center.min(center - length),
                        right,
                        center.max(center - length),
                    ),
                    length > 0.0,
                )
            });
        AltitudeTapeFrame {
            altitude: value_m.map_or(0.0, |value| {
                convert_standard_metric_value(metric, Some(&self.display_unit), value)
            }),
            value,
            vertical_speed: vertical_speed.flatten(),
            trend,
        }
    }

    /// Draws the strip on whole host pixels and returns the shift that snapping applied, so the
    /// labels can step on the same pixel grid as their ticks.
    fn draw_ticks(&self, canvas: &Canvas, altitude: f64) -> f32 {
        let layout = &self.layout;
        let interval = f64::from(self.major_tick_interval);
        let major = (altitude / interval).floor() * interval;
        let major_y = tape_y(
            layout.center_y(),
            major,
            altitude,
            self.pixels_per_unit,
            self.source.direction(),
        );
        // The strip starts on the last major at or above the body; it then covers the body.
        let top = major_y - ((major_y - layout.body.top) / self.period).ceil() * self.period;
        // Whole host pixels let the strip copy without resampling and keep thin ticks crisp.
        let snap = |value: f32| (value * self.raster_scale).round() / self.raster_scale;
        let (left, snapped_top) = (snap(layout.frame.left), snap(top));
        let scale = self.raster_scale.recip();
        canvas.draw_image_rect_with_sampling_options(
            &self.tick_strip,
            None,
            Rect::from_xywh(
                left,
                snapped_top,
                self.tick_strip.width() as f32 * scale,
                self.tick_strip.height() as f32 * scale,
            ),
            FilterMode::Nearest,
            &Paint::default(),
        );
        snapped_top - top
    }

    /// `tick_shift` is the strip's snap shift; every label takes it so it moves with its tick.
    fn draw_labels(&self, canvas: &Canvas, altitude: f64, tick_shift: f32) {
        let layout = &self.layout;
        let half_extent = layout.body.height() * 0.5 + self.label_font.size();
        for value in visible_major_values(
            altitude,
            self.major_tick_interval,
            self.pixels_per_unit,
            half_extent,
        ) {
            let text = layout_text(
                &format_number(value, 0),
                &self.label_font,
                0.0,
                self.numerals,
            );
            let Some(blob) = text.blob else {
                continue;
            };
            let origin = Point::new(
                layout.text_x(self.label_anchor, text.measurement.width),
                tape_y(
                    layout.center_y(),
                    value,
                    altitude,
                    self.pixels_per_unit,
                    self.source.direction(),
                ) + tick_shift
                    + self.label_baseline_offset,
            );
            draw_text(canvas, &blob, origin, &self.label_paint);
        }
    }
}

fn draw_text(canvas: &Canvas, blob: &TextBlob, origin: Point, paint: &TextPaint) {
    if let Some(shadow) = &paint.shadow {
        canvas.draw_text_blob(
            blob,
            origin + Point::new(paint.shadow_distance, paint.shadow_distance),
            shadow,
        );
    }
    canvas.draw_text_blob(blob, origin, &paint.fill);
}

impl AltitudeTapeFrame {
    pub fn draw(&self, canvas: &Canvas, cache: &AltitudeTapeCache) {
        let layout = &cache.layout;
        // Translucent widgets composite as one layer so the trend never shows through the box.
        let layer = cache.opacity < 1.0;
        if layer {
            let mut opacity = Paint::default();
            opacity.set_alpha_f(cache.opacity);
            canvas.save_layer(
                &SaveLayerRec::default()
                    .bounds(&cache.bounds)
                    .paint(&opacity),
            );
        }
        canvas.save();
        let mut body = layout.body;
        body.right += cache.tick_overhang;
        // A hard clip rounds to whole pixels; an anti-aliased one at a fractional edge would
        // force every tick and label draw inside it through a coverage mask.
        canvas.clip_rect(body, ClipOp::Intersect, false);
        let tick_shift = cache.draw_ticks(canvas, self.altitude);
        if cache.show_tick_labels {
            cache.draw_labels(canvas, self.altitude, tick_shift);
        }
        canvas.restore();
        if let Some((rect, climbing)) = &self.trend {
            let paint = if *climbing {
                &cache.bar_positive
            } else {
                &cache.bar_negative
            };
            canvas.draw_rect(rect, paint);
        }
        cache.value_box.draw(canvas, None);
        if let Some((blob, origin)) = &self.value {
            canvas.draw_text_blob(blob, *origin, &cache.value_paint);
        }
        if let Some((blob, origin, climbing)) = &self.vertical_speed {
            // The readout shares the trend's colours; a level reading keeps the label colour.
            let paint = match climbing {
                Some(true) => &cache.climb_text_paint,
                Some(false) => &cache.descent_text_paint,
                None => &cache.label_paint,
            };
            draw_text(canvas, blob, *origin, paint);
            if let Some((unit, unit_origin)) = &cache.vertical_speed_unit_text {
                draw_text(canvas, unit, *unit_origin, paint);
            }
        }
        if layer {
            canvas.restore();
        }
    }
}

#[derive(Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub display_variants: ResizeVariants,
}

#[derive(Serialize)]
pub struct ResizeVariants {
    pub altitude_tape: ResizeVariant,
}

#[derive(Serialize)]
pub struct ResizeVariant {
    pub width: f32,
    pub height: f32,
}

/// `size` is the requested scene-pixel frame size; the content rectangle is the frame itself.
pub fn resize_altitude_tape(
    widget: ValueConfig,
    scene: ScenePresentationConfig,
    size: [f32; 2],
    gesture: ResizeGesture,
) -> WidgetResult<ResizeUpdate> {
    let ResizeGesture { fixed, point, .. } = gesture;
    if size.iter().any(|value| !value.is_finite() || *value <= 0.0) {
        return Err(WidgetError::Config(
            "invalid altitude tape resize size".into(),
        ));
    }
    let config = validate_altitude_tape(widget, 0)?;
    let scale = validate_scene_presentation(scene)?.scale;
    // The body always keeps room for the readout box above the footer.
    let footer = if config.show_vertical_speed {
        config.vertical_speed_font_size
    } else {
        0.0
    };
    let min_height = footer * TEXT_ROW_RATIO + config.font_size * VALUE_BOX_HEIGHT_RATIO;
    let width = (size[0] / scale).clamp(8.0, 10_000.0);
    let height = (size[1] / scale).clamp(min_height, 10_000.0);
    Ok(ResizeUpdate {
        x: point[0] - width * scale * fixed[0],
        y: point[1] - height * scale * fixed[1],
        display_variants: ResizeVariants {
            altitude_tape: ResizeVariant { width, height },
        },
    })
}

/// Offset altitude or depth in metres, and ascent-positive vertical speed in m/s, drawn by one frame.
pub type AltitudeTapeSample = (Option<f64>, Option<f64>);

pub struct PreviewAltitudeTape {
    cache: AltitudeTapeCache,
    preview_raster_scale: PreviewRasterScale,
    last_sample: Option<AltitudeTapeSample>,
    pub tile: Tile,
}

impl PreviewAltitudeTape {
    pub fn new(
        widget: ValueConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let widget = validate_altitude_tape(widget, 0)?;
        let host = WidgetHost::from_scene(&scene, raster_scale)?;
        Self::from_validated(&widget, &validate_scene_presentation(scene)?, &host, fonts)
    }

    pub fn from_validated(
        widget: &ValidatedAltitudeTape,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        Ok(Self {
            cache: prepare_altitude_tape_cache(widget, scene, fonts, host.raster_scale)?,
            preview_raster_scale: host.raster_scale,
            last_sample: None,
            tile: Tile::default(),
        })
    }

    /// The samples the frame at `second` draws; equal samples draw identical pixels.
    pub fn frame_key(&self, activity: Option<&ActivitySampler>, second: f64) -> AltitudeTapeSample {
        activity.map_or((None, None), |activity| {
            let show_rate = self.cache.show_vertical_speed;
            match self.cache.source {
                TapeSource::Altitude => (
                    activity.altitude_at(second, self.cache.starting_altitude_m),
                    show_rate
                        .then(|| activity.vertical_speed_at(second))
                        .flatten(),
                ),
                TapeSource::Depth => (
                    activity.depth_at(second),
                    show_rate
                        .then(|| activity.depth_vertical_speed_at(second))
                        .flatten(),
                ),
            }
        })
    }

    /// Scene rectangle a render draws into; the tape scrolls inside fixed bounds.
    pub fn layout_bounds(&self) -> TileBounds {
        self.cache.bounds.into()
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(
        &self,
        canvas: &Canvas,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> TileBounds {
        let (altitude, speed) = self.frame_key(activity, second);
        self.cache
            .prepare_frame(altitude, speed)
            .draw(canvas, &self.cache);
        self.cache.bounds.into()
    }

    pub fn render(&mut self, activity: Option<&ActivitySampler>, second: f64) -> WidgetResult<()> {
        let sample = self.frame_key(activity, second);
        // The tape scrolls continuously, even between whole-unit readouts.
        if self.last_sample == Some(sample) {
            return Ok(());
        }
        let frame = self.cache.prepare_frame(sample.0, sample.1);
        self.tile.render(
            self.preview_raster_scale,
            SHAPE_COLOR_TYPE,
            self.cache.bounds,
            Presented::content(self.cache.content_bounds),
            |canvas| {
                frame.draw(canvas, &self.cache);
                Ok(())
            },
        )?;
        self.last_sample = Some(sample);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        altitude_tape_layout, format_tape_value, format_vertical_speed_value, tape_y, trend_length,
        visible_major_values, TapeOrientation, MISSING_TEXT,
    };
    use crate::model::catalog::MetricKind;
    use skia_safe::Rect;

    #[test]
    fn layout_centres_the_readout_box_on_the_body_above_the_footer() {
        let frame = Rect::from_xywh(10.0, 20.0, 100.0, 400.0);
        let layout =
            altitude_tape_layout(frame, TapeOrientation::Left, Some(20.0), 30.0, 40.0, 20.0);
        assert_eq!(layout.footer, Rect::new(10.0, 384.0, 110.0, 420.0));
        assert_eq!(layout.body, Rect::new(10.0, 20.0, 110.0, 384.0));
        assert_eq!(layout.value_box.height(), 42.0);
        assert_eq!(layout.value_box.center_y(), layout.body.center_y());
        // The box spans the tape and its pointer tip sits on the left edge.
        assert_eq!(
            (layout.value_box.left, layout.value_box.right),
            (10.0, 110.0)
        );
        assert_eq!(layout.major_length, 40.0);
        assert_eq!(layout.minor_length, 20.0);
        assert_eq!(layout.tip_x(), 10.0);
        assert_eq!(layout.tick_span(40.0), (60.0, 100.0));
        // Right-pointing tapes mirror ticks, text alignment and the tip; a hidden readout drops the footer.
        let mirrored = altitude_tape_layout(frame, TapeOrientation::Right, None, 30.0, 40.0, 20.0);
        assert_eq!(mirrored.tip_x(), 110.0);
        assert_eq!(mirrored.tick_span(40.0), (0.0, 40.0));
        assert_eq!(mirrored.from_tick_edge(5.0), 15.0);
        assert_eq!(mirrored.text_x(15.0, 30.0), 15.0);
        assert_eq!(layout.text_x(105.0, 30.0), 75.0);
        assert_eq!(mirrored.body.bottom, 420.0);
        assert_eq!(mirrored.footer.height(), 0.0);
    }

    #[test]
    fn higher_tape_values_sit_above_the_centre() {
        assert_eq!(tape_y(200.0, 40.0, 33.0, 2.0, 1.0), 186.0);
        assert_eq!(tape_y(200.0, 20.0, 33.0, 2.0, 1.0), 226.0);
        // Depth runs the other way: deeper values sit below the centre.
        assert_eq!(tape_y(200.0, 40.0, 33.0, 2.0, -1.0), 214.0);
        assert_eq!(tape_y(200.0, 20.0, 33.0, 2.0, -1.0), 174.0);
    }

    #[test]
    fn visible_majors_cover_the_body_around_the_current_value() {
        let values: Vec<f64> = visible_major_values(33.7, 20, 2.0, 100.0).collect();
        assert_eq!(values, vec![0.0, 20.0, 40.0, 60.0, 80.0]);
        let negative: Vec<f64> = visible_major_values(-5.0, 10, 1.0, 20.0).collect();
        assert_eq!(negative, vec![-20.0, -10.0, 0.0, 10.0]);
    }

    #[test]
    fn trend_projects_vertical_speed_and_caps_at_half_the_body() {
        // 1 m/s for 6 s at 2 px/m is 12 px upward.
        assert_eq!(trend_length(1.0, 6.0, 2.0, 400.0), 12.0);
        assert_eq!(trend_length(-1.0, 6.0, 2.0, 400.0), -12.0);
        assert_eq!(trend_length(50.0, 6.0, 2.0, 400.0), 200.0);
        assert_eq!(trend_length(-50.0, 6.0, 2.0, 400.0), -200.0);
    }

    #[test]
    fn readouts_convert_units_round_and_mark_missing_data() {
        assert_eq!(
            format_tape_value(MetricKind::Altitude, Some(33.74), "m", true),
            "33.7"
        );
        assert_eq!(
            format_tape_value(MetricKind::Altitude, Some(100.0), "ft", false),
            "328"
        );
        assert_eq!(
            format_tape_value(MetricKind::Depth, Some(30.48), "ft", false),
            "100"
        );
        assert_eq!(
            format_tape_value(MetricKind::Altitude, None, "m", true),
            MISSING_TEXT
        );
        let speed = |mps, unit, decimal| format_vertical_speed_value(mps, unit, decimal);
        assert_eq!(speed(Some(-2.0), "mps", true), ("-2.0".into(), Some(false)));
        assert_eq!(speed(Some(1.0), "ftmin", false), ("197".into(), Some(true)));
        assert_eq!(speed(Some(2.0), "ftps", true), ("6.6".into(), Some(true)));
        // Readings that round to zero, and missing samples, have no climb/descent colour.
        assert_eq!(speed(Some(-0.01), "mps", true), ("0.0".into(), None));
        assert_eq!(speed(Some(0.4), "mps", false), ("0".into(), None));
        assert_eq!(speed(None, "mps", false), (MISSING_TEXT.into(), None));
    }
}
