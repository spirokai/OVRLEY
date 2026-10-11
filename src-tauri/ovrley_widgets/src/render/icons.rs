//! Manifest-backed icon resources. SVG geometry is parsed once per asset.
use super::svg::{parse_svg_icon, ParsedSvgIcon, SvgPrimitive};
use crate::model::{
    standard_metrics::standard_metric_definition, standard_widgets::time_icon_asset_file,
    types::MetricKind,
};
use skia_safe::{
    image_filters, paint::Style, Canvas, Color, Paint, PaintCap, PaintJoin, Point, Rect,
};
use std::sync::{Mutex, MutexGuard, OnceLock};

const ICON_VIEWBOX_SIZE: f32 = 24.0;

struct IconAsset {
    markup: &'static str,
    // Skia paths are Send but not Sync; each immutable resource is borrowed under its lock.
    parsed: OnceLock<Mutex<ParsedSvgIcon>>,
}

include!(concat!(env!("OUT_DIR"), "/icon_assets.rs"));

pub fn metric_icon_asset_file(kind: MetricKind) -> Option<&'static str> {
    if kind == MetricKind::Time {
        Some(time_icon_asset_file())
    } else {
        standard_metric_definition(kind).map(|definition| definition.icon.asset_file.as_str())
    }
}

fn parsed_metric_icon(asset_file: &str) -> MutexGuard<'static, ParsedSvgIcon> {
    let asset = icon_asset(asset_file);
    asset
        .parsed
        .get_or_init(|| {
            Mutex::new(
                parse_svg_icon(asset.markup)
                    .unwrap_or_else(|| panic!("invalid bundled SVG icon: {asset_file}")),
            )
        })
        .lock()
        .expect("icon geometry lock poisoned")
}

/// Tight vector bounds, including round strokes but excluding the SVG viewbox whitespace.
pub fn metric_icon_bounds(asset_file: &str, placement: Rect) -> Rect {
    let icon = parsed_metric_icon(asset_file);
    let mut bounds = Rect::new_empty();
    for primitive in &icon.primitives {
        let (mut rect, style) = match primitive {
            SvgPrimitive::Path { path, style } => (path.compute_tight_bounds(), style),
            SvgPrimitive::Line {
                x1,
                y1,
                x2,
                y2,
                style,
            } => (
                Rect::new(x1.min(*x2), y1.min(*y2), x1.max(*x2), y1.max(*y2)),
                style,
            ),
            SvgPrimitive::Circle { cx, cy, r, style } => {
                (Rect::new(cx - r, cy - r, cx + r, cy + r), style)
            }
        };
        if style.stroke {
            let width = style.stroke_width.unwrap_or(icon.stroke_width.max(1.0));
            rect.outset((width * 0.5, width * 0.5));
        }
        if style.fill || style.stroke {
            bounds.join(rect);
        }
    }
    let scale = placement.width() / ICON_VIEWBOX_SIZE;
    Rect::new(
        placement.left + bounds.left * scale,
        placement.top + bounds.top * scale,
        placement.left + bounds.right * scale,
        placement.top + bounds.bottom * scale,
    )
}

/// Draws a scaled parsed metric icon with optional shadow.
///
/// Scales the 24×24 SVG viewbox to the requested icon size. Shadow distances
/// are transformed back through the inverse scale so visual blur stays in
/// output pixels.
#[allow(clippy::too_many_arguments)]
pub fn draw_metric_icon(
    canvas: &Canvas,
    icon_asset_file: Option<&str>,
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
    let Some(icon_asset_file) = icon_asset_file else {
        return;
    };
    let icon = parsed_metric_icon(icon_asset_file);
    let icon_scale = size / ICON_VIEWBOX_SIZE;

    let paint = metric_icon_paint(
        &icon,
        crate::render::text::parse_color(icon_color, widget_opacity),
    );

    canvas.save();
    canvas.translate((x, y));
    canvas.scale((icon_scale, icon_scale));
    if let Some(shadow_color) = shadow_color {
        if shadow_strength > 0.0 {
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
                None,
            ) {
                let mut shadow_paint = metric_icon_paint(
                    &icon,
                    crate::render::text::parse_color(icon_color, widget_opacity),
                );
                shadow_paint.set_image_filter(shadow_filter);
                draw_metric_icon_primitives(canvas, &icon, &shadow_paint);
            }
        }
    }
    draw_metric_icon_primitives(canvas, &icon, &paint);
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

/// Clones an existing icon paint and reconfigures it for fills.
fn metric_icon_fill_paint(paint: &Paint) -> Paint {
    let mut fill_paint = paint.clone();
    fill_paint.set_style(Style::Fill);
    fill_paint.set_stroke_width(0.0);
    fill_paint
}

/// Draws parsed SVG icon primitives onto the current canvas.
pub fn draw_metric_icon_primitives(canvas: &Canvas, icon: &ParsedSvgIcon, paint: &Paint) {
    for primitive in &icon.primitives {
        match primitive {
            SvgPrimitive::Path { path, style } => {
                if style.fill {
                    let fill_paint = metric_icon_fill_paint(paint);
                    canvas.draw_path(path, &fill_paint);
                }
                if style.stroke {
                    let mut stroke_paint = paint.clone();
                    let stroke_width = style.stroke_width.unwrap_or(icon.stroke_width);
                    stroke_paint.set_stroke_width(if style.stroke_width.is_some() {
                        stroke_width
                    } else {
                        stroke_width.max(1.0)
                    });
                    canvas.draw_path(path, &stroke_paint);
                }
            }
            SvgPrimitive::Line {
                x1,
                y1,
                x2,
                y2,
                style,
            } => {
                if style.stroke {
                    let mut stroke_paint = paint.clone();
                    let stroke_width = style.stroke_width.unwrap_or(icon.stroke_width);
                    stroke_paint.set_stroke_width(if style.stroke_width.is_some() {
                        stroke_width
                    } else {
                        stroke_width.max(1.0)
                    });
                    canvas.draw_line(Point::new(*x1, *y1), Point::new(*x2, *y2), &stroke_paint);
                }
            }
            SvgPrimitive::Circle { cx, cy, r, style } => {
                if style.fill {
                    let fill_paint = metric_icon_fill_paint(paint);
                    canvas.draw_circle(Point::new(*cx, *cy), *r, &fill_paint);
                }
                if style.stroke {
                    let mut stroke_paint = paint.clone();
                    let stroke_width = style.stroke_width.unwrap_or(icon.stroke_width);
                    stroke_paint.set_stroke_width(if style.stroke_width.is_some() {
                        stroke_width
                    } else {
                        stroke_width.max(1.0)
                    });
                    canvas.draw_circle(Point::new(*cx, *cy), *r, &stroke_paint);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{metric_icon_asset_file, parsed_metric_icon};
    use crate::model::types::MetricKind;

    #[test]
    fn all_manifest_icons_parse_into_cached_geometry() {
        let manifest: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/standard-metrics.json"
        )))
        .unwrap();
        for definition in manifest["definitions"].as_array().unwrap() {
            if !definition["current"].as_bool().unwrap() {
                continue;
            }
            let metric: MetricKind = serde_json::from_value(definition["type"].clone()).unwrap();
            let asset = metric_icon_asset_file(metric).unwrap();
            assert_eq!(asset, definition["icon"]["assetFile"].as_str().unwrap());
            let icon = parsed_metric_icon(asset);
            assert!(!icon.primitives.is_empty(), "missing geometry for {asset}");
            let cached = &*icon as *const _;
            drop(icon);
            assert_eq!(cached, &*parsed_metric_icon(asset) as *const _);
        }
        assert!(
            !parsed_metric_icon(metric_icon_asset_file(MetricKind::Time).unwrap())
                .primitives
                .is_empty()
        );
    }
}
