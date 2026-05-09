use crate::activity::schema::DenseActivityReport;
use crate::config::{RenderConfig, ValueConfig};
use crate::render::format::{format_metric_parts, MetricDisplayParts, MetricIconKind};
use crate::render::text::{draw_text, measure_text, parse_color, ResolvedTextStyle};
use skia_safe::{
    paint::Style, path::ArcSize, Canvas, Paint, PaintCap, PaintJoin, Path, PathDirection, Point,
};
use std::path::PathBuf;
use std::sync::OnceLock;

const ICON_VIEWBOX_SIZE: f32 = 24.0;
const METRIC_WIDGET_LINE_HEIGHT: f32 = 0.92;
const METRIC_WIDGET_OUTER_GAP_PX: f32 = 8.0;
const METRIC_WIDGET_UNITS_GAP_PX: f32 = 8.0;
const MIN_UNITS_FONT_SIZE: f32 = 12.0;
const GRADIENT_TRIANGLE_GAP_PX: f32 = 8.0;
const GRADIENT_ZERO_EPSILON: f64 = 0.05;
const MAX_GRADIENT_ABS_PERCENT: f64 = 25.0;
const GRADIENT_ZERO_LINE_WIDTH_PX: f32 = 1.0;

#[derive(Clone, Debug)]
struct ParsedSvgIcon {
    stroke_width: f32,
    primitives: Vec<SvgPrimitive>,
}

#[derive(Clone, Debug)]
enum SvgPrimitive {
    Path(String),
    Line { x1: f32, y1: f32, x2: f32, y2: f32 },
    Circle { cx: f32, cy: f32, r: f32 },
}

#[derive(Clone, Copy, Debug)]
enum PathToken {
    Command(char),
    Number(f32),
}

pub(crate) fn draw_metric_value_widget_with_config(
    canvas: &Canvas,
    config: &RenderConfig,
    value: &ValueConfig,
    base_style: &ResolvedTextStyle,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
    scale: f32,
    font_dirs: &[PathBuf],
) -> bool {
    if value.value == "gradient" {
        return draw_gradient_value_widget(
            canvas,
            config,
            value,
            base_style,
            dense_activity,
            frame_index,
            scale,
            font_dirs,
        );
    }

    let Some(parts) = format_metric_parts(config, value, dense_activity, frame_index) else {
        return false;
    };
    draw_metric_parts(canvas, value, base_style, &parts, scale, font_dirs);
    true
}

fn draw_gradient_value_widget(
    canvas: &Canvas,
    config: &RenderConfig,
    value: &ValueConfig,
    base_style: &ResolvedTextStyle,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
    scale: f32,
    font_dirs: &[PathBuf],
) -> bool {
    let raw_gradient = dense_activity
        .series
        .gradient
        .get(frame_index)
        .copied()
        .flatten();
    let value_text =
        crate::render::format::format_value(config, value, dense_activity, frame_index);
    let value_measure = measure_text(&value_text, base_style, font_dirs);
    let value_line_height = base_style.font_size * METRIC_WIDGET_LINE_HEIGHT;
    let value_offset = value.value_offset.unwrap_or(0.0);
    let triangle_width = value.triangle_width.unwrap_or(72.0).max(0.0) * scale;
    let max_triangle_height =
        gradient_triangle_height(Some(MAX_GRADIENT_ABS_PERCENT), triangle_width);
    let show_triangle = value.show_triangle.unwrap_or(true) && triangle_width > 0.0;
    let content_width = value_measure
        .width
        .max(if show_triangle { triangle_width } else { 0.0 });
    let value_left = base_style.x + ((content_width - value_measure.width) * 0.5);
    let triangle_top = base_style.y + value_line_height + (GRADIENT_TRIANGLE_GAP_PX * scale);
    let zero_baseline_y = triangle_top + max_triangle_height;
    let value_top = if show_triangle {
        zero_baseline_y
            - (value_line_height + (GRADIENT_TRIANGLE_GAP_PX * scale) + max_triangle_height )
            - value_offset
    } else {
        base_style.y - value_offset
    };

    let mut value_style = base_style.clone();
    value_style.x = value_left;
    value_style.y = value_top;
    value_style.line_height = value_line_height;
    draw_text(canvas, &value_text, &value_style, font_dirs);

    if !show_triangle {
        return true;
    }

    let triangle_left = base_style.x + ((content_width - triangle_width) * 0.5);
    let triangle_color = if raw_gradient.unwrap_or(0.0) < 0.0 {
        value
            .triangle_negative_color
            .as_deref()
            .unwrap_or("#c65102")
    } else {
        value
            .triangle_positive_color
            .as_deref()
            .unwrap_or("#40e0d0")
    };

    if gradient_is_zero(raw_gradient) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(Style::Stroke);
        paint.set_stroke_width((GRADIENT_ZERO_LINE_WIDTH_PX * scale).max(1.0));
        paint.set_stroke_cap(PaintCap::Round);
        paint.set_color(parse_color(triangle_color, base_style.opacity));
        canvas.draw_line(
            Point::new(triangle_left, zero_baseline_y),
            Point::new(triangle_left + triangle_width, zero_baseline_y),
            &paint,
        );
    } else if let Some(path) =
        build_gradient_triangle_path(raw_gradient, triangle_left, zero_baseline_y, triangle_width)
    {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(Style::Fill);
        paint.set_color(parse_color(triangle_color, base_style.opacity));
        canvas.draw_path(&path, &paint);
    }

    true
}

fn gradient_is_zero(raw_gradient: Option<f64>) -> bool {
    let Some(gradient) = raw_gradient else {
        return true;
    };
    gradient.abs() <= GRADIENT_ZERO_EPSILON
}

fn gradient_triangle_height(raw_gradient: Option<f64>, triangle_width: f32) -> f32 {
    if triangle_width <= 0.0 {
        return 0.0;
    }

    let Some(gradient) = raw_gradient else {
        return 0.0;
    };
    let magnitude = gradient.abs().min(MAX_GRADIENT_ABS_PERCENT);
    if magnitude <= GRADIENT_ZERO_EPSILON {
        return 0.0;
    }

    let half_angle_radians = ((magnitude * 0.5) as f32).to_radians();
    triangle_width * half_angle_radians.tan()
}

fn build_gradient_triangle_path(
    raw_gradient: Option<f64>,
    left: f32,
    baseline_y: f32,
    width: f32,
) -> Option<Path> {
    let height = gradient_triangle_height(raw_gradient, width);
    if height <= 0.0 {
        return None;
    }

    let mut path = Path::new();
    if raw_gradient.unwrap_or(0.0) > 0.0 {
        path.move_to(Point::new(left, baseline_y));
        path.line_to(Point::new(left + width, baseline_y));
        path.line_to(Point::new(left + width, baseline_y - height));
    } else if raw_gradient.unwrap_or(0.0) < 0.0 {
        path.move_to(Point::new(left, baseline_y));
        path.line_to(Point::new(left + width, baseline_y));
        path.line_to(Point::new(left + width, baseline_y + height));
    } else {
        return None;
    }
    path.close();
    Some(path)
}

fn draw_metric_parts(
    canvas: &Canvas,
    value: &ValueConfig,
    base_style: &ResolvedTextStyle,
    parts: &MetricDisplayParts,
    scale: f32,
    font_dirs: &[PathBuf],
) {
    let value_measure = measure_text(&parts.value_text, base_style, font_dirs);
    let value_line_height = base_style.font_size * METRIC_WIDGET_LINE_HEIGHT;

    let mut units_style = base_style.clone();
    units_style.font_size = (base_style.font_size * 0.28).max(MIN_UNITS_FONT_SIZE * scale);
    units_style.line_height = units_style.font_size * METRIC_WIDGET_LINE_HEIGHT;
    let units_measure = parts
        .unit_text
        .as_deref()
        .map(|unit_text| measure_text(unit_text, &units_style, font_dirs));
    let units_line_height = units_style.font_size * METRIC_WIDGET_LINE_HEIGHT;

    let icon_size = value.icon_size.unwrap_or(28.0) * scale;
    let show_units = parts.unit_text.is_some();
    let show_icon = parts.show_icon && parts.icon_kind.is_some();
    let icon_margin_right = (base_style.font_size * 0.08).max(METRIC_WIDGET_OUTER_GAP_PX * scale);
    let text_group_height = if show_units {
        value_line_height.max(units_line_height)
    } else {
        value_line_height
    };
    let row_height = if show_icon {
        icon_size.max(text_group_height)
    } else {
        text_group_height
    };
    let text_group_left = if show_icon {
        icon_size + (METRIC_WIDGET_OUTER_GAP_PX * scale) + icon_margin_right
    } else {
        0.0
    };
    let text_group_top = base_style.y + ((row_height - text_group_height) * 0.5);
    let text_group_bottom = text_group_top + text_group_height;
    let value_glyph_height = (value_measure.bounds_bottom - value_measure.bounds_top).abs();
    let value_top = text_group_bottom - (value_line_height + value_glyph_height) * 0.5;

    let mut value_style = base_style.clone();
    value_style.x = base_style.x + text_group_left;
    value_style.y = value_top;
    value_style.line_height = value_line_height;

    if show_icon {
        draw_metric_icon(
            canvas,
            parts.icon_kind,
            value.icon_color.as_deref().unwrap_or("#40e0d0"),
            base_style.opacity,
            base_style.x + value.icon_offset_x.unwrap_or(0.0) * scale,
            base_style.y
                + ((row_height - icon_size) * 0.5)
                + value.icon_offset_y.unwrap_or(0.0) * scale,
            icon_size,
        );
    }

    draw_text(canvas, &parts.value_text, &value_style, font_dirs);

    if let (Some(unit_text), Some(unit_measure)) = (parts.unit_text.as_deref(), units_measure) {
        let mut units_style = units_style;
        units_style.x = value_style.x + value_measure.width + (METRIC_WIDGET_UNITS_GAP_PX * scale);
        let units_glyph_height = (unit_measure.bounds_bottom - unit_measure.bounds_top).abs();
        units_style.y = text_group_bottom - (units_line_height + units_glyph_height) * 0.5;
        draw_text(canvas, unit_text, &units_style, font_dirs);
    }
}

fn draw_metric_icon(
    canvas: &Canvas,
    icon_kind: Option<MetricIconKind>,
    icon_color: &str,
    widget_opacity: f32,
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

    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(Style::Stroke);
    paint.set_stroke_width(icon.stroke_width.max(1.0));
    paint.set_stroke_cap(PaintCap::Round);
    paint.set_stroke_join(PaintJoin::Round);
    paint.set_color(crate::render::text::parse_color(icon_color, widget_opacity));

    canvas.save();
    canvas.translate((x, y));
    canvas.scale((size / ICON_VIEWBOX_SIZE, size / ICON_VIEWBOX_SIZE));
    for primitive in &icon.primitives {
        match primitive {
            SvgPrimitive::Path(data) => {
                if let Some(path) = svg_path_to_skia_path(data) {
                    canvas.draw_path(&path, &paint);
                }
            }
            SvgPrimitive::Line { x1, y1, x2, y2 } => {
                canvas.draw_line(Point::new(*x1, *y1), Point::new(*x2, *y2), &paint);
            }
            SvgPrimitive::Circle { cx, cy, r } => {
                canvas.draw_circle(Point::new(*cx, *cy), *r, &paint);
            }
        }
    }
    canvas.restore();
}

fn parsed_metric_icon(icon_kind: MetricIconKind) -> Option<&'static ParsedSvgIcon> {
    match icon_kind {
        MetricIconKind::Gauge => {
            static CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
            CACHE
                .get_or_init(|| parse_svg_icon(metric_icon_svg_markup(icon_kind)))
                .as_ref()
        }
        MetricIconKind::Heart => {
            static CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
            CACHE
                .get_or_init(|| parse_svg_icon(metric_icon_svg_markup(icon_kind)))
                .as_ref()
        }
        MetricIconKind::RefreshCw => {
            static CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
            CACHE
                .get_or_init(|| parse_svg_icon(metric_icon_svg_markup(icon_kind)))
                .as_ref()
        }
        MetricIconKind::Zap => {
            static CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
            CACHE
                .get_or_init(|| parse_svg_icon(metric_icon_svg_markup(icon_kind)))
                .as_ref()
        }
        MetricIconKind::Clock3 => {
            static CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
            CACHE
                .get_or_init(|| parse_svg_icon(metric_icon_svg_markup(icon_kind)))
                .as_ref()
        }
        MetricIconKind::Thermometer => {
            static CACHE: OnceLock<Option<ParsedSvgIcon>> = OnceLock::new();
            CACHE
                .get_or_init(|| parse_svg_icon(metric_icon_svg_markup(icon_kind)))
                .as_ref()
        }
    }
}

fn metric_icon_svg_markup(icon_kind: MetricIconKind) -> &'static str {
    match icon_kind {
        MetricIconKind::Gauge => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../app/src/components/widgets/icons/widget-speed.svg"
        )),
        MetricIconKind::Heart => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../app/src/components/widgets/icons/widget-heartrate.svg"
        )),
        MetricIconKind::RefreshCw => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../app/src/components/widgets/icons/widget-cadence.svg"
        )),
        MetricIconKind::Zap => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../app/src/components/widgets/icons/widget-power.svg"
        )),
        MetricIconKind::Clock3 => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../app/src/components/widgets/icons/widget-time.svg"
        )),
        MetricIconKind::Thermometer => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../app/src/components/widgets/icons/widget-temperature.svg"
        )),
    }
}

fn parse_svg_icon(svg_markup: &str) -> Option<ParsedSvgIcon> {
    let stroke_width = parse_xml_attr(svg_markup, "stroke-width")
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(2.0);
    let mut primitives = Vec::new();
    let mut rest = svg_markup;

    while let Some(start) = rest.find('<') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('>') else {
            break;
        };
        let tag = &rest[..end];
        rest = &rest[end + 1..];

        if tag.starts_with("path") {
            primitives.push(SvgPrimitive::Path(parse_xml_attr(tag, "d")?.to_string()));
        } else if tag.starts_with("line") {
            primitives.push(SvgPrimitive::Line {
                x1: parse_xml_attr(tag, "x1")?.parse().ok()?,
                y1: parse_xml_attr(tag, "y1")?.parse().ok()?,
                x2: parse_xml_attr(tag, "x2")?.parse().ok()?,
                y2: parse_xml_attr(tag, "y2")?.parse().ok()?,
            });
        } else if tag.starts_with("circle") {
            primitives.push(SvgPrimitive::Circle {
                cx: parse_xml_attr(tag, "cx")?.parse().ok()?,
                cy: parse_xml_attr(tag, "cy")?.parse().ok()?,
                r: parse_xml_attr(tag, "r")?.parse().ok()?,
            });
        }
    }

    Some(ParsedSvgIcon {
        stroke_width,
        primitives,
    })
}

fn parse_xml_attr<'a>(markup: &'a str, name: &str) -> Option<&'a str> {
    let pattern = format!("{name}=\"");
    let start = markup.find(&pattern)? + pattern.len();
    let rest = &markup[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn svg_path_to_skia_path(data: &str) -> Option<Path> {
    let tokens = tokenize_path_data(data);
    if tokens.is_empty() {
        return None;
    }

    let mut path = Path::new();
    let mut index = 0usize;
    let mut current_command = None;
    let mut current = Point::new(0.0, 0.0);
    let mut subpath_start = Point::new(0.0, 0.0);

    while index < tokens.len() {
        if let PathToken::Command(command) = tokens[index] {
            current_command = Some(command);
            index += 1;
        }

        let command = current_command?;
        match command {
            'M' | 'm' => {
                let is_relative = command == 'm';
                let x = next_number(&tokens, &mut index)?;
                let y = next_number(&tokens, &mut index)?;
                current = point_from_command(current, x, y, is_relative);
                path.move_to(current);
                subpath_start = current;
                current_command = Some(if is_relative { 'l' } else { 'L' });

                while peek_is_number(&tokens, index) {
                    let x = next_number(&tokens, &mut index)?;
                    let y = next_number(&tokens, &mut index)?;
                    current = point_from_command(current, x, y, is_relative);
                    path.line_to(current);
                }
            }
            'L' | 'l' => {
                let is_relative = command == 'l';
                while peek_is_number(&tokens, index) {
                    let x = next_number(&tokens, &mut index)?;
                    let y = next_number(&tokens, &mut index)?;
                    current = point_from_command(current, x, y, is_relative);
                    path.line_to(current);
                }
            }
            'H' | 'h' => {
                let is_relative = command == 'h';
                while peek_is_number(&tokens, index) {
                    let x = next_number(&tokens, &mut index)?;
                    current = if is_relative {
                        Point::new(current.x + x, current.y)
                    } else {
                        Point::new(x, current.y)
                    };
                    path.line_to(current);
                }
            }
            'V' | 'v' => {
                let is_relative = command == 'v';
                while peek_is_number(&tokens, index) {
                    let y = next_number(&tokens, &mut index)?;
                    current = if is_relative {
                        Point::new(current.x, current.y + y)
                    } else {
                        Point::new(current.x, y)
                    };
                    path.line_to(current);
                }
            }
            'A' | 'a' => {
                let is_relative = command == 'a';
                while peek_is_number(&tokens, index) {
                    let rx = next_number(&tokens, &mut index)?;
                    let ry = next_number(&tokens, &mut index)?;
                    let x_axis_rotation = next_number(&tokens, &mut index)?;
                    let large_arc = next_number(&tokens, &mut index)? != 0.0;
                    let sweep = next_number(&tokens, &mut index)? != 0.0;
                    let x = next_number(&tokens, &mut index)?;
                    let y = next_number(&tokens, &mut index)?;
                    let end = point_from_command(current, x, y, is_relative);
                    if rx.abs() <= f32::EPSILON || ry.abs() <= f32::EPSILON {
                        path.line_to(end);
                    } else {
                        path.arc_to_rotated(
                            (rx, ry),
                            x_axis_rotation,
                            if large_arc {
                                ArcSize::Large
                            } else {
                                ArcSize::Small
                            },
                            if sweep {
                                PathDirection::CW
                            } else {
                                PathDirection::CCW
                            },
                            end,
                        );
                    }
                    current = end;
                }
            }
            'Z' | 'z' => {
                path.close();
                current = subpath_start;
            }
            _ => return None,
        }
    }

    Some(path)
}

fn tokenize_path_data(data: &str) -> Vec<PathToken> {
    let mut tokens = Vec::new();
    let chars = data.chars().collect::<Vec<_>>();
    let mut index = 0usize;

    while index < chars.len() {
        let current = chars[index];
        if current.is_ascii_alphabetic() {
            tokens.push(PathToken::Command(current));
            index += 1;
            continue;
        }

        if current.is_ascii_whitespace() || current == ',' {
            index += 1;
            continue;
        }

        let start = index;
        let mut saw_decimal = current == '.';
        index += 1;
        while index < chars.len() {
            let next = chars[index];
            let previous = chars[index - 1];
            let is_sign_break = (next == '-' || next == '+') && previous != 'e' && previous != 'E';
            let is_decimal_break = next == '.' && saw_decimal && previous != 'e' && previous != 'E';
            if next.is_ascii_alphabetic()
                || next == ','
                || next.is_ascii_whitespace()
                || is_sign_break
                || is_decimal_break
            {
                break;
            }
            if next == '.' {
                saw_decimal = true;
            }
            index += 1;
        }

        if let Ok(number) = chars[start..index]
            .iter()
            .collect::<String>()
            .parse::<f32>()
        {
            tokens.push(PathToken::Number(number));
        }
    }

    tokens
}

fn next_number(tokens: &[PathToken], index: &mut usize) -> Option<f32> {
    let value = match tokens.get(*index)? {
        PathToken::Number(value) => *value,
        PathToken::Command(_) => return None,
    };
    *index += 1;
    Some(value)
}

fn peek_is_number(tokens: &[PathToken], index: usize) -> bool {
    matches!(tokens.get(index), Some(PathToken::Number(_)))
}

fn point_from_command(current: Point, x: f32, y: f32, is_relative: bool) -> Point {
    if is_relative {
        Point::new(current.x + x, current.y + y)
    } else {
        Point::new(x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::gradient_triangle_height;

    #[test]
    fn gradient_triangle_height_is_zero_for_zero_and_missing_values() {
        assert_eq!(gradient_triangle_height(None, 72.0), 0.0);
        assert_eq!(gradient_triangle_height(Some(0.0), 72.0), 0.0);
    }

    #[test]
    fn gradient_triangle_height_uses_half_angle_rule() {
        let expected = (72.0_f32) * (5.0_f32.to_radians().tan());
        let actual = gradient_triangle_height(Some(10.0), 72.0);
        assert!((actual - expected).abs() < 0.001);
    }
}
