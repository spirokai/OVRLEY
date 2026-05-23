/// Metric icon drawing: kind mapping, SVG cache, paint creation, and primitives
/// rendering.
///
/// Each icon kind is parsed once from bundled SVG markup (`include_str!`-imported
/// at compile time) and cached in a `OnceLock<Option<ParsedSvgIcon>>` for the
/// lifetime of the process.

use crate::render::format::MetricIconKind;
use crate::MetricKind;
use skia_safe::{
    image_filters, paint::Style, Canvas, Color, Paint, PaintCap, PaintJoin, Point,
};
use std::sync::OnceLock;

pub(crate) const ICON_VIEWBOX_SIZE: f32 = 24.0;

/// Parsed SVG icon with enough data for Skia stroke rendering.
#[derive(Clone, Debug)]
pub(crate) struct ParsedSvgIcon {
    pub stroke_width: f32,
    pub primitives: Vec<SvgPrimitive>,
}

/// Primitive SVG elements supported by the local icon parser.
#[derive(Clone, Debug)]
pub(crate) enum SvgPrimitive {
    Path(String),
    Line { x1: f32, y1: f32, x2: f32, y2: f32 },
    Circle { cx: f32, cy: f32, r: f32 },
}

/// Maps a telemetry value key to its built-in icon kind.
pub(crate) fn metric_icon_kind_for_value(kind: MetricKind) -> Option<MetricIconKind> {
    match kind {
        MetricKind::Speed => Some(MetricIconKind::Gauge),
        MetricKind::Heartrate => Some(MetricIconKind::Heart),
        MetricKind::Cadence => Some(MetricIconKind::RefreshCw),
        MetricKind::Power => Some(MetricIconKind::Zap),
        MetricKind::Time => Some(MetricIconKind::Clock3),
        MetricKind::Temperature => Some(MetricIconKind::Thermometer),
        _ => None,
    }
}

/// Returns the cached parsed SVG representation for a metric icon.
///
/// Each icon is parsed once and cached. Six independent `OnceLock` caches
/// exist (one per `MetricIconKind`), lazily initialized on first access. On
/// the hot path (every frame that draws an icon), the lookup hits the
/// pre-initialized `OnceLock` and returns an `Option<&>` without locking.
fn parsed_metric_icon(icon_kind: MetricIconKind) -> Option<&'static ParsedSvgIcon> {
    match icon_kind {
        MetricIconKind::Gauge => {
            static CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
            CACHE
                .get_or_init(|| super::svg::parse_svg_icon(metric_icon_svg_markup(icon_kind)))
                .as_ref()
        }
        MetricIconKind::Heart => {
            static CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
            CACHE
                .get_or_init(|| super::svg::parse_svg_icon(metric_icon_svg_markup(icon_kind)))
                .as_ref()
        }
        MetricIconKind::RefreshCw => {
            static CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
            CACHE
                .get_or_init(|| super::svg::parse_svg_icon(metric_icon_svg_markup(icon_kind)))
                .as_ref()
        }
        MetricIconKind::Zap => {
            static CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
            CACHE
                .get_or_init(|| super::svg::parse_svg_icon(metric_icon_svg_markup(icon_kind)))
                .as_ref()
        }
        MetricIconKind::Clock3 => {
            static CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
            CACHE
                .get_or_init(|| super::svg::parse_svg_icon(metric_icon_svg_markup(icon_kind)))
                .as_ref()
        }
        MetricIconKind::Thermometer => {
            static CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
            CACHE
                .get_or_init(|| super::svg::parse_svg_icon(metric_icon_svg_markup(icon_kind)))
                .as_ref()
        }
    }
}

/// Returns bundled SVG markup for a metric icon kind.
///
/// SVG assets are loaded at compile time from the shared `assets/widget-icons/`
/// directory so the backend and frontend use the same canonical files.
fn metric_icon_svg_markup(icon_kind: MetricIconKind) -> &'static str {
    match icon_kind {
        MetricIconKind::Gauge => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/widget-icons/widget-speed.svg"
        )),
        MetricIconKind::Heart => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/widget-icons/widget-heartrate.svg"
        )),
        MetricIconKind::RefreshCw => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/widget-icons/widget-cadence.svg"
        )),
        MetricIconKind::Zap => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/widget-icons/widget-power.svg"
        )),
        MetricIconKind::Clock3 => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/widget-icons/widget-time.svg"
        )),
        MetricIconKind::Thermometer => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/widget-icons/widget-temperature.svg"
        )),
    }
}

/// Draws a scaled parsed metric icon with optional shadow.
///
/// Scales the 24×24 SVG viewbox to the requested icon size. Shadow distances
/// are transformed back through the inverse scale so visual blur stays in
/// output pixels.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_metric_icon(
    canvas: &Canvas,
    icon_kind: Option<MetricIconKind>,
    icon_color: &str,
    widget_opacity: f32,
    shadow_color: Option<Color>,
    shadow_strength: f32,
    shadow_distance: f32,
    x: f32,
    y: f32,
    size: f32,
) {
    if size <= 0.0 {
        return;
    }
    let Some(icon_kind) = icon_kind else {
        return;
    };
    let Some(icon) = parsed_metric_icon(icon_kind) else {
        return;
    };
    let icon_scale = size / ICON_VIEWBOX_SIZE;

    let paint = metric_icon_paint(
        icon,
        crate::render::text::parse_color(icon_color, widget_opacity),
    );

    canvas.save();
    canvas.translate((x, y));
    canvas.scale((icon_scale, icon_scale));
    if let Some(shadow_color) = shadow_color {
        if shadow_strength > 0.0 || shadow_distance != 0.0 {
            let inverse_scale = if icon_scale.abs() <= f32::EPSILON {
                1.0
            } else {
                1.0 / icon_scale
            };
            if let Some(shadow_filter) = image_filters::drop_shadow_only(
                (
                    shadow_distance * inverse_scale,
                    shadow_distance * inverse_scale,
                ),
                (
                    shadow_strength * inverse_scale,
                    shadow_strength * inverse_scale,
                ),
                shadow_color,
                None,
                None,
            ) {
                let mut shadow_paint = metric_icon_paint(
                    icon,
                    crate::render::text::parse_color(icon_color, widget_opacity),
                );
                shadow_paint.set_image_filter(shadow_filter);
                draw_metric_icon_primitives(canvas, icon, &shadow_paint);
            }
        }
    }
    draw_metric_icon_primitives(canvas, icon, &paint);
    canvas.restore();
}

/// Creates Skia stroke paint for a parsed metric icon.
fn metric_icon_paint(icon: &ParsedSvgIcon, color: Color) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(Style::Stroke);
    paint.set_stroke_width(icon.stroke_width.max(1.0));
    paint.set_stroke_cap(PaintCap::Round);
    paint.set_stroke_join(PaintJoin::Round);
    paint.set_color(color);
    paint
}

/// Draws parsed SVG icon primitives onto the current canvas.
pub(crate) fn draw_metric_icon_primitives(canvas: &Canvas, icon: &ParsedSvgIcon, paint: &Paint) {
    for primitive in &icon.primitives {
        match primitive {
            SvgPrimitive::Path(data) => {
                if let Some(path) = super::svg::svg_path_to_skia_path(data) {
                    canvas.draw_path(&path, paint);
                }
            }
            SvgPrimitive::Line { x1, y1, x2, y2 } => {
                canvas.draw_line(Point::new(*x1, *y1), Point::new(*x2, *y2), paint);
            }
            SvgPrimitive::Circle { cx, cy, r } => {
                canvas.draw_circle(Point::new(*cx, *cy), *r, paint);
            }
        }
    }
}
