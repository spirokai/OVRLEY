//! Prepare a periodic tick strip and the static readout box once; playback only copies
//! the strip at a whole-pixel scroll offset, places the few visible labels and paints the
//! changing text.
use super::validation::{TapeOrientation, TapeSource, ValidatedAltitudeTape};
use crate::{
    error::WidgetResult,
    model::scene::{Numerals, ValidatedScenePresentation},
    render::{
        preview::{PreviewRasterScale, StaticRaster},
        shadow::shadow_paint,
        text::{layout_text, parse_color, resolve_font, FontResolver},
    },
    model::{standard_metrics::standard_metric_unit_label, types::MetricKind},
    widgets::value::{
        format::convert_standard_metric_value,
        preview::{surface, SHAPE_COLOR_TYPE},
    },
};
use skia_safe::{
    paint::{Cap, Style},
    Canvas, ClipOp, Color, Font, Image, Paint, Path, PathBuilder, Point, Rect, TextBlob,
};

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
        footer: Rect::new(frame.left, body_bottom, frame.right, body_bottom + footer_height),
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
pub fn tape_y(center_y: f32, value: f64, current: f64, pixels_per_unit: f64, direction: f64) -> f32 {
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
        widget.show_vertical_speed.then_some(vertical_speed_font_size),
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
    let period_pixels = (widget.major_tick_spacing * scale * raster).round().max(1.0);
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
    box_bounds.outset((value_shadow_sigma * 3.0 + 1.0, value_shadow_sigma * 3.0 + 1.0));
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
    let widest = if decimal { format!("-{digits}.0") } else { format!("-{digits}") };
    let box_width = layout_text(&widest, font, 0.0, numerals).measurement.width;
    let gap = font.measure_str(" ", None).0;
    let label = standard_metric_unit_label(MetricKind::VerticalSpeed, Some(unit));
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
#[allow(clippy::too_many_arguments)]
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
    let mut tape = surface(image_width.max(1), period_pixels * periods, SHAPE_COLOR_TYPE)?;
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
    if let Some(shadow) = shadow_paint(&opaque, parse_color(&widget.value_shadow_color, 1.0), shadow_sigma) {
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
