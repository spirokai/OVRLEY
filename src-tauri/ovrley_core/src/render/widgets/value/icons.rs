/// Metric icon drawing: kind mapping, SVG cache, paint creation, and primitives
/// rendering.
///
/// Each icon kind is parsed once from bundled SVG markup (`include_str!`-imported
/// at compile time) and cached in a `OnceLock<Option<ParsedSvgIcon>>` for the
/// lifetime of the process.
use crate::render::format::MetricIconKind;
use crate::standard_metrics::metric_icon_asset_key;
use crate::MetricKind;
use skia_safe::{image_filters, paint::Style, Canvas, Color, Paint, PaintCap, PaintJoin, Point};
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
    match metric_icon_asset_key(kind)? {
        crate::standard_metrics::MetricIconAssetKey::Speed => Some(MetricIconKind::Gauge),
        crate::standard_metrics::MetricIconAssetKey::Heartrate => Some(MetricIconKind::Heart),
        crate::standard_metrics::MetricIconAssetKey::Cadence => Some(MetricIconKind::RefreshCw),
        crate::standard_metrics::MetricIconAssetKey::Power => Some(MetricIconKind::Zap),
        crate::standard_metrics::MetricIconAssetKey::Time => Some(MetricIconKind::Clock3),
        crate::standard_metrics::MetricIconAssetKey::Temperature => {
            Some(MetricIconKind::Thermometer)
        }
        crate::standard_metrics::MetricIconAssetKey::CoreTemperature => {
            Some(MetricIconKind::CoreTemperature)
        }
        crate::standard_metrics::MetricIconAssetKey::Pace => Some(MetricIconKind::Footprints),
        crate::standard_metrics::MetricIconAssetKey::AirPressure => Some(MetricIconKind::Wind),
        crate::standard_metrics::MetricIconAssetKey::LeftRightBalance => {
            Some(MetricIconKind::Scale)
        }
        crate::standard_metrics::MetricIconAssetKey::StrideLength => Some(MetricIconKind::Ruler),
        crate::standard_metrics::MetricIconAssetKey::StrokeRate => Some(MetricIconKind::Waves),
        crate::standard_metrics::MetricIconAssetKey::VerticalSpeed => {
            Some(MetricIconKind::TrendingUp)
        }
        crate::standard_metrics::MetricIconAssetKey::VerticalRatio => Some(MetricIconKind::Percent),
        crate::standard_metrics::MetricIconAssetKey::GForce => Some(MetricIconKind::GForce),
        crate::standard_metrics::MetricIconAssetKey::GroundContactTime => {
            Some(MetricIconKind::GroundContactTime)
        }
        crate::standard_metrics::MetricIconAssetKey::Torque => Some(MetricIconKind::Torque),
        crate::standard_metrics::MetricIconAssetKey::GearPosition => {
            Some(MetricIconKind::GearPosition)
        }
        crate::standard_metrics::MetricIconAssetKey::VerticalOscillation => {
            Some(MetricIconKind::ArrowUpDown)
        }
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
        MetricIconKind::Gauge => parsed_metric_icon_cached(icon_kind, &GAUGE_ICON_CACHE),
        MetricIconKind::Heart => parsed_metric_icon_cached(icon_kind, &HEART_ICON_CACHE),
        MetricIconKind::RefreshCw => parsed_metric_icon_cached(icon_kind, &REFRESH_CW_ICON_CACHE),
        MetricIconKind::Zap => parsed_metric_icon_cached(icon_kind, &ZAP_ICON_CACHE),
        MetricIconKind::Clock3 => parsed_metric_icon_cached(icon_kind, &CLOCK3_ICON_CACHE),
        MetricIconKind::Thermometer => {
            parsed_metric_icon_cached(icon_kind, &THERMOMETER_ICON_CACHE)
        }
        MetricIconKind::CoreTemperature => {
            parsed_metric_icon_cached(icon_kind, &CORE_TEMPERATURE_ICON_CACHE)
        }
        MetricIconKind::Footprints => parsed_metric_icon_cached(icon_kind, &FOOTPRINTS_ICON_CACHE),
        MetricIconKind::Wind => parsed_metric_icon_cached(icon_kind, &WIND_ICON_CACHE),
        MetricIconKind::Scale => parsed_metric_icon_cached(icon_kind, &SCALE_ICON_CACHE),
        MetricIconKind::Ruler => parsed_metric_icon_cached(icon_kind, &RULER_ICON_CACHE),
        MetricIconKind::Waves => parsed_metric_icon_cached(icon_kind, &WAVES_ICON_CACHE),
        MetricIconKind::TrendingUp => parsed_metric_icon_cached(icon_kind, &TRENDING_UP_ICON_CACHE),
        MetricIconKind::Percent => parsed_metric_icon_cached(icon_kind, &PERCENT_ICON_CACHE),
        MetricIconKind::GForce => parsed_metric_icon_cached(icon_kind, &G_FORCE_ICON_CACHE),
        MetricIconKind::GroundContactTime => {
            parsed_metric_icon_cached(icon_kind, &GROUND_CONTACT_TIME_ICON_CACHE)
        }
        MetricIconKind::Torque => parsed_metric_icon_cached(icon_kind, &TORQUE_ICON_CACHE),
        MetricIconKind::GearPosition => {
            parsed_metric_icon_cached(icon_kind, &GEAR_POSITION_ICON_CACHE)
        }
        MetricIconKind::ArrowUpDown => {
            parsed_metric_icon_cached(icon_kind, &ARROW_UP_DOWN_ICON_CACHE)
        }
    }
}

static GAUGE_ICON_CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
static HEART_ICON_CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
static REFRESH_CW_ICON_CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
static ZAP_ICON_CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
static CLOCK3_ICON_CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
static THERMOMETER_ICON_CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
static CORE_TEMPERATURE_ICON_CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
static FOOTPRINTS_ICON_CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
static WIND_ICON_CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
static SCALE_ICON_CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
static RULER_ICON_CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
static WAVES_ICON_CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
static TRENDING_UP_ICON_CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
static PERCENT_ICON_CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
static G_FORCE_ICON_CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
static GROUND_CONTACT_TIME_ICON_CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
static TORQUE_ICON_CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
static GEAR_POSITION_ICON_CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
static ARROW_UP_DOWN_ICON_CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();

fn parsed_metric_icon_cached(
    icon_kind: MetricIconKind,
    cache: &'static OnceLock<Option<ParsedSvgIcon>>,
) -> Option<&'static ParsedSvgIcon> {
    cache
        .get_or_init(|| super::svg::parse_svg_icon(metric_icon_svg_markup(icon_kind)))
        .as_ref()
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
        MetricIconKind::CoreTemperature => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/widget-icons/widget-core-temperature.svg"
        )),
        MetricIconKind::Footprints => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/widget-icons/widget-pace.svg"
        )),
        MetricIconKind::Wind => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/widget-icons/widget-air-pressure.svg"
        )),
        MetricIconKind::Scale => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/widget-icons/widget-left-right-balance.svg"
        )),
        MetricIconKind::Ruler => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/widget-icons/widget-stride-length.svg"
        )),
        MetricIconKind::Waves => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/widget-icons/widget-stroke-rate.svg"
        )),
        MetricIconKind::TrendingUp => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/widget-icons/widget-vertical-speed.svg"
        )),
        MetricIconKind::Percent => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/widget-icons/widget-vertical-ratio.svg"
        )),
        MetricIconKind::GForce => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/widget-icons/widget-g-force.svg"
        )),
        MetricIconKind::GroundContactTime => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/widget-icons/widget-ground-contact-time.svg"
        )),
        MetricIconKind::Torque => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/widget-icons/widget-torque.svg"
        )),
        MetricIconKind::GearPosition => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/widget-icons/widget-gear-position.svg"
        )),
        MetricIconKind::ArrowUpDown => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/widget-icons/widget-vertical-oscillation.svg"
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
