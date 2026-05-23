/// Lightweight SVG parser for bundled metric widget icons.
///
/// Parses a deliberately small SVG subset (paths, lines, circles, one shared
/// stroke width) and converts path data into Skia paths. The bundled Lucide-
/// style icons use only a handful of commands, so the parser does not attempt
/// full SVG compliance.
use skia_safe::{path::ArcSize, Path, PathDirection, Point};

use super::icons::{ParsedSvgIcon, SvgPrimitive};

/// Token emitted by the SVG path tokenizer.
#[derive(Clone, Copy, Debug)]
pub(crate) enum PathToken {
    Command(char),
    Number(f32),
}

/// Parses supported SVG icon markup into local drawing primitives.
///
/// The bundled metric icons use a deliberately small SVG subset: paths,
/// lines, circles, and one shared stroke width.
pub(crate) fn parse_svg_icon(svg_markup: &str) -> Option<ParsedSvgIcon> {
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

/// Extracts a double-quoted XML attribute from a small SVG tag string.
fn parse_xml_attr<'a>(markup: &'a str, name: &str) -> Option<&'a str> {
    let pattern = format!("{name}=\"");
    let start = markup.find(&pattern)? + pattern.len();
    let rest = &markup[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

/// Converts supported SVG path data into a Skia path.
///
/// # Two-phase conversion
///
/// 1. **Tokenize** — split compact SVG path data into command letters and
///    operand numbers (handles implicit separators like `M1-2.5`).
/// 2. **Emit** — walk the token stream, mapping each SVG command to the
///    equivalent Skia path method. Maintains current position and subpath
///    start for relative commands and `Z` closure.
///
/// Implements only the path commands emitted by the bundled Lucide-style
/// icons. Returning None for unsupported commands fails closed.
pub(crate) fn svg_path_to_skia_path(data: &str) -> Option<Path> {
    // Phase 1: tokenize the compact SVG path data into commands and numbers.
    let tokens = tokenize_path_data(data);
    if tokens.is_empty() {
        return None;
    }

    // Phase 2: walk the token stream, mapping SVG commands to Skia path calls.
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
            'C' | 'c' => {
                let is_relative = command == 'c';
                while peek_is_number(&tokens, index) {
                    let x1 = next_number(&tokens, &mut index)?;
                    let y1 = next_number(&tokens, &mut index)?;
                    let x2 = next_number(&tokens, &mut index)?;
                    let y2 = next_number(&tokens, &mut index)?;
                    let x = next_number(&tokens, &mut index)?;
                    let y = next_number(&tokens, &mut index)?;
                    let control1 = point_from_command(current, x1, y1, is_relative);
                    let control2 = point_from_command(current, x2, y2, is_relative);
                    let end = point_from_command(current, x, y, is_relative);
                    path.cubic_to(control1, control2, end);
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

/// Tokenizes SVG path data into commands and numeric operands.
///
/// SVG permits compact number lists such as `M1-2.5`. The tokenizer splits
/// on command letters, commas/whitespace, signs, and repeated decimals.
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

/// Consumes the next numeric SVG path token.
fn next_number(tokens: &[PathToken], index: &mut usize) -> Option<f32> {
    let value = match tokens.get(*index)? {
        PathToken::Number(value) => *value,
        PathToken::Command(_) => return None,
    };
    *index += 1;
    Some(value)
}

/// Returns whether the token at `index` is numeric.
fn peek_is_number(tokens: &[PathToken], index: usize) -> bool {
    matches!(tokens.get(index), Some(PathToken::Number(_)))
}

/// Resolves absolute or relative SVG path coordinates into a Skia point.
fn point_from_command(current: Point, x: f32, y: f32, is_relative: bool) -> Point {
    if is_relative {
        Point::new(current.x + x, current.y + y)
    } else {
        Point::new(x, y)
    }
}
