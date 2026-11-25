//! Scrolling compass tape with a heading indicator.
use crate::activity::ActivitySampler;
use crate::model::catalog::{DisplayType, MetricKind};
use crate::model::scene::{
    validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
};
use crate::model::validation::{
    require_bool, require_f32, require_hex_color, require_non_negative_f32, require_percentage,
    require_positive_f32, require_positive_u32, require_string, require_unit_opacity,
};
use crate::preview::ResizeGesture;
use crate::preview::WidgetHost;
use crate::render::text::{
    origin_x_for_centered_text, parse_color, resolve_font, shadow_paint, FontResolver,
};
use crate::render::tile::PreviewRasterScale;
use crate::render::tile::{surface, TileBounds, SHAPE_COLOR_TYPE};
use crate::render::tile::{Presented, Tile};
use crate::widgets::value::ValueConfig;
use crate::{WidgetError, WidgetResult};
use serde::Serialize;
use skia_safe::canvas::SaveLayerRec;
use skia_safe::{
    gradient, BlendMode, Canvas, ClipOp, Color, Color4f, FilterMode, Image, Paint, Path,
    PathBuilder, Point, Rect, TileMode,
};

#[derive(Clone, Debug)]
pub struct ValidatedHeading {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub pixels_per_degree: f32,
    pub major_tick_interval: u32,
    pub minor_ticks_per_major: u32,
    pub show_major_ticks: bool,
    pub show_minor_ticks: bool,
    pub major_tick_length_pct: f32,
    pub minor_tick_length_pct: f32,
    pub major_tick_thickness: f32,
    pub minor_tick_thickness: f32,
    pub tick_color: String,
    pub cardinal_tick_color: String,
    pub tick_alignment: String,
    pub show_minor_labels: bool,
    pub show_major_labels: bool,
    pub label_color: String,
    pub cardinal_label_color: String,
    pub label_font: String,
    pub label_font_size: f32,
    pub label_offset: f32,
    pub show_indicator: bool,
    pub indicator_style: String,
    pub indicator_placement: String,
    pub indicator_color: String,
    pub indicator_size: f32,
    pub opacity: f32,
}

pub fn validate_heading(value: ValueConfig, index: usize) -> WidgetResult<ValidatedHeading> {
    let p = |field: &str| format!("values[{index}].{field}");
    if value.value != MetricKind::Heading || value.display_type != DisplayType::HeadingTape {
        return Err(WidgetError::Config(format!(
            "values[{index}]: expected heading/heading_tape"
        )));
    }
    let alignment = require_string(value.tick_alignment, &p("tick_alignment"))?;
    let indicator_style = require_string(value.indicator_style, &p("indicator_style"))?;
    let indicator_placement = require_string(value.indicator_placement, &p("indicator_placement"))?;
    for (field, value, choices) in [
        (
            "tick_alignment",
            alignment.as_str(),
            &["below", "centered"][..],
        ),
        (
            "indicator_style",
            indicator_style.as_str(),
            &["chevron", "highlight_bar"][..],
        ),
        (
            "indicator_placement",
            indicator_placement.as_str(),
            &["top", "bottom", "both"][..],
        ),
    ] {
        if !choices.contains(&value) {
            return Err(WidgetError::Config(format!(
                "{}: invalid value '{value}'",
                p(field)
            )));
        }
    }
    let non_negative = |raw, field| require_non_negative_f32(raw, &p(field));
    let percentage = |raw, field| require_percentage(raw, &p(field));
    let label_font = require_string(value.label_font, &p("label_font"))?;
    if label_font.trim().is_empty() {
        return Err(WidgetError::Config(format!(
            "{}: required",
            p("label_font")
        )));
    }
    Ok(ValidatedHeading {
        x: require_f32(Some(value.x), &p("x"))?,
        y: require_f32(Some(value.y), &p("y"))?,
        width: require_positive_f32(value.width, &p("width"))?,
        height: require_positive_f32(value.height, &p("height"))?,
        pixels_per_degree: require_positive_f32(value.pixels_per_degree, &p("pixels_per_degree"))?,
        major_tick_interval: require_positive_u32(
            value.major_tick_interval,
            &p("major_tick_interval"),
        )?,
        minor_ticks_per_major: require_positive_u32(
            value.minor_ticks_per_major,
            &p("minor_ticks_per_major"),
        )?,
        show_major_ticks: require_bool(value.show_major_ticks, &p("show_major_ticks"))?,
        show_minor_ticks: require_bool(value.show_minor_ticks, &p("show_minor_ticks"))?,
        major_tick_length_pct: percentage(value.major_tick_length_pct, "major_tick_length_pct")?,
        minor_tick_length_pct: percentage(value.minor_tick_length_pct, "minor_tick_length_pct")?,
        major_tick_thickness: non_negative(value.major_tick_thickness, "major_tick_thickness")?,
        minor_tick_thickness: non_negative(value.minor_tick_thickness, "minor_tick_thickness")?,
        tick_color: require_hex_color(value.tick_color.as_deref(), &p("tick_color"))?,
        cardinal_tick_color: require_hex_color(
            value.cardinal_tick_color.as_deref(),
            &p("cardinal_tick_color"),
        )?,
        tick_alignment: alignment,
        show_minor_labels: require_bool(value.show_minor_labels, &p("show_minor_labels"))?,
        show_major_labels: require_bool(value.show_major_labels, &p("show_major_labels"))?,
        label_color: require_hex_color(value.label_color.as_deref(), &p("label_color"))?,
        cardinal_label_color: require_hex_color(
            value.cardinal_label_color.as_deref(),
            &p("cardinal_label_color"),
        )?,
        label_font,
        label_font_size: require_positive_f32(value.label_font_size, &p("label_font_size"))?,
        label_offset: non_negative(value.label_offset, "label_offset")?,
        show_indicator: require_bool(value.show_indicator, &p("show_indicator"))?,
        indicator_style,
        indicator_placement,
        indicator_color: require_hex_color(
            value.indicator_color.as_deref(),
            &p("indicator_color"),
        )?,
        indicator_size: require_positive_f32(value.indicator_size, &p("indicator_size"))?,
        opacity: require_unit_opacity(value.opacity, &p("opacity"))?,
    })
}

pub const LABEL_DESCENT_PCT: f32 = 0.25;
pub const HIGHLIGHT_BAR_BODY_MARGIN_FRAME_PCT: f32 = 0.1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeadingTapeLayout {
    pub body_y: f32,
    pub body_height: f32,
    pub tick_scale_height: f32,
    pub total_height: f32,
    pub has_top_chevron: bool,
    pub has_bottom_chevron: bool,
}

pub fn heading_tape_has_chevron(
    show_indicator: bool,
    indicator_style: &str,
    indicator_placement: &str,
    placement: &str,
) -> bool {
    show_indicator
        && indicator_style == "chevron"
        && (indicator_placement == placement || indicator_placement == "both")
}

pub fn heading_tape_has_highlight_bar(show_indicator: bool, indicator_style: &str) -> bool {
    show_indicator && indicator_style == "highlight_bar"
}

#[expect(
    clippy::too_many_arguments,
    reason = "each parameter is one scalar field of the tape, shared by layout and resize"
)]
pub fn heading_tape_layout(
    frame_height: f32,
    show_indicator: bool,
    indicator_style: &str,
    indicator_placement: &str,
    indicator_size: f32,
    major_tick_length_pct: f32,
    label_offset: f32,
    font_size: f32,
) -> HeadingTapeLayout {
    let frame_height = frame_height.max(1.0);
    let indicator_size = indicator_size.max(0.0);
    let gap = indicator_size * 0.5;
    let has_top_chevron =
        heading_tape_has_chevron(show_indicator, indicator_style, indicator_placement, "top");
    let has_bottom_chevron = heading_tape_has_chevron(
        show_indicator,
        indicator_style,
        indicator_placement,
        "bottom",
    );
    let highlight_bar_margin = if heading_tape_has_highlight_bar(show_indicator, indicator_style) {
        frame_height * HIGHLIGHT_BAR_BODY_MARGIN_FRAME_PCT
    } else {
        0.0
    };
    let ideal_top_slot = if has_top_chevron {
        indicator_size + gap
    } else {
        highlight_bar_margin
    };
    let ideal_bottom_slot = if has_bottom_chevron {
        indicator_size + gap
    } else {
        highlight_bar_margin
    };
    let available_slot_height = (frame_height - 1.0).max(0.0);
    let ideal_slot_height = ideal_top_slot + ideal_bottom_slot;
    let slot_scale = if ideal_slot_height > available_slot_height && ideal_slot_height > 0.0 {
        available_slot_height / ideal_slot_height
    } else {
        1.0
    };
    let top_slot = ideal_top_slot * slot_scale;
    let bottom_slot = ideal_bottom_slot * slot_scale;
    let body_height = (frame_height - top_slot - bottom_slot).max(1.0);
    let tick_scale_height = heading_tick_scale_height_for_rendered_body(
        body_height,
        major_tick_length_pct,
        label_offset,
        font_size,
    );

    HeadingTapeLayout {
        body_y: top_slot,
        body_height,
        tick_scale_height,
        total_height: frame_height,
        has_top_chevron,
        has_bottom_chevron,
    }
}

pub fn heading_tick_scale_height_for_rendered_body(
    rendered_body_height: f32,
    major_tick_length_pct: f32,
    label_offset: f32,
    font_size: f32,
) -> f32 {
    let major_tick_ratio = (major_tick_length_pct.max(0.001)) / 100.0;
    let fixed_label_stack = label_offset + font_size * (1.0 + LABEL_DESCENT_PCT);

    ((rendered_body_height - fixed_label_stack) / major_tick_ratio).max(1.0)
}

pub fn heading_tick_position(
    body_height: f32,
    major_tick_length_pct: f32,
    minor_tick_length_pct: f32,
    tick_alignment: &str,
    is_major: bool,
) -> (f32, f32) {
    let major_length = body_height * major_tick_length_pct / 100.0;
    let minor_length = body_height * minor_tick_length_pct / 100.0;
    let length = if is_major { major_length } else { minor_length };
    let top = if !is_major && tick_alignment == "centered" {
        (major_length - minor_length) / 2.0
    } else {
        0.0
    };

    (top, length)
}

pub fn heading_label_baseline(
    body_height: f32,
    major_tick_length_pct: f32,
    label_offset: f32,
    font_size: f32,
) -> f32 {
    body_height * major_tick_length_pct / 100.0 + label_offset + font_size
}

pub fn heading_label_bottom(
    body_height: f32,
    major_tick_length_pct: f32,
    label_offset: f32,
    font_size: f32,
) -> f32 {
    heading_label_baseline(body_height, major_tick_length_pct, label_offset, font_size)
        + font_size * LABEL_DESCENT_PCT
}

/// A single visible tick on the tape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TapeTick {
    /// Degree position (0–360) on the compass.
    pub degree: f32,
    /// Pixel x-position relative to the tape image origin.
    pub x: f32,
    /// Whether this tick is at a cardinal/intercardinal position (45° multiple).
    pub is_cardinal: bool,
    /// Whether this tick is a major tick (at `major_tick_interval` multiples).
    pub is_major: bool,
}

/// A single visible label on the tape.
#[derive(Clone, Debug, PartialEq)]
pub struct TapeLabel {
    /// Degree position (0–360) on the compass.
    pub degree: f32,
    /// Pixel x-position relative to the tape image origin.
    pub x: f32,
    /// Text to display (e.g. "N", "NE", "30", "60").
    pub text: String,
    /// Whether this is a major label (takes priority over minor numeric labels).
    pub is_major_label: bool,
}

/// Cardinal direction labels at 45° multiples.
const CARDINAL_LABELS: &[(f32, &str)] = &[
    (0.0, "N"),
    (45.0, "NE"),
    (90.0, "E"),
    (135.0, "SE"),
    (180.0, "S"),
    (225.0, "SW"),
    (270.0, "W"),
    (315.0, "NW"),
];

/// Checks if a degree value is at a cardinal/intercardinal position (45° multiple).
pub fn is_cardinal_degree(degree: f32) -> bool {
    CARDINAL_LABELS
        .iter()
        .any(|(cardinal, _)| (degree - cardinal).abs() < 0.01)
}

/// Returns the cardinal label for a degree, or None if not cardinal.
pub fn cardinal_label_for_degree(degree: f32) -> Option<&'static str> {
    CARDINAL_LABELS
        .iter()
        .find(|(cardinal, _)| (degree - cardinal).abs() < 0.01)
        .map(|(_, label)| *label)
}

/// Computes the scroll offset in pixels for a given heading.
///
/// The active heading should sit under the widget's center indicator, so the
/// tape offset is left-anchored heading minus half the visible width.
pub fn heading_offset(heading: f32, pixels_per_degree: f32, width: f32) -> f32 {
    heading * pixels_per_degree - width / 2.0
}

/// Computes which ticks are visible within the widget bounds.
///
/// The tape image is 360 × pixels_per_degree wide and repeats via TileMode::Repeat.
/// We need to find all tick degree positions whose pixel x falls within
/// [0, width) after accounting for the scroll offset.
///
/// # Arguments
/// * `heading` - Current heading in degrees (0–360)
/// * `pixels_per_degree` - Horizontal scale
/// * `width` - Widget width in pixels
/// * `major_tick_interval` - Degrees between major ticks (default 15)
/// * `minor_ticks_per_major` - Subdivisions between majors (default 3)
/// * `show_major_ticks` - Whether to include major ticks
/// * `show_minor_ticks` - Whether to include minor ticks
pub fn visible_ticks(
    heading: f32,
    pixels_per_degree: f32,
    width: f32,
    major_tick_interval: u32,
    minor_ticks_per_major: u32,
    show_major_ticks: bool,
    show_minor_ticks: bool,
) -> Vec<TapeTick> {
    let tape_width = 360.0 * pixels_per_degree;
    let offset = heading * pixels_per_degree;
    let minor_interval = major_tick_interval as f32 / minor_ticks_per_major as f32;
    let mut degrees: Vec<f32> = CARDINAL_LABELS.iter().map(|(degree, _)| *degree).collect();

    let mut degree = 0.0_f32;
    while degree < 360.0 {
        degrees.push((degree * 1000.0).round() / 1000.0);
        degree += minor_interval;
    }

    degrees.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    degrees.dedup_by(|a, b| (*a - *b).abs() < 0.01);

    let mut ticks = Vec::new();

    for degree in degrees {
        let is_cardinal = is_cardinal_degree(degree);
        let is_major = is_cardinal || (degree % major_tick_interval as f32).abs() < 0.01;
        let is_minor = !is_major;

        let show = is_cardinal || (is_major && show_major_ticks) || (is_minor && show_minor_ticks);

        if show {
            // Compute pixel position in the tape image
            let tape_x = degree * pixels_per_degree;
            // Apply scroll offset and wrap into [0, tape_width)
            let wrapped_x = ((tape_x - offset) % tape_width + tape_width) % tape_width;

            // Check if this tick falls within the visible widget width
            if wrapped_x < width {
                ticks.push(TapeTick {
                    degree,
                    x: wrapped_x,
                    is_cardinal,
                    is_major,
                });
            }
        }
    }

    ticks
}

/// Computes labels for visible tick positions, with cardinal priority override.
///
/// Cardinal labels (N/NE/E/SE/S/SW/W/NW) at 45° multiples take priority over
/// numeric labels at the same position. Both label types share a single row
/// below the ticks.
///
/// # Arguments
/// * `ticks` - Pre-computed visible ticks from `visible_ticks`
/// * `show_minor_labels` - Whether to show degree numbers
/// * `show_major_labels` - Whether to show cardinal letters
pub fn visible_labels(
    ticks: &[TapeTick],
    show_minor_labels: bool,
    show_major_labels: bool,
) -> Vec<TapeLabel> {
    let mut labels = Vec::new();

    for tick in ticks {
        if tick.is_cardinal {
            if let Some(text) = cardinal_label_for_degree(tick.degree) {
                labels.push(TapeLabel {
                    degree: tick.degree,
                    x: tick.x,
                    text: text.to_string(),
                    is_major_label: true,
                });
            }
        } else if (tick.is_major && show_major_labels) || (!tick.is_major && show_minor_labels) {
            labels.push(TapeLabel {
                degree: tick.degree,
                x: tick.x,
                text: format!("{}", tick.degree as u32),
                is_major_label: false,
            });
        }
    }

    labels
}

// ── Indicator geometry ────────────────────────────────────────────────

/// A single triangle point for indicator shapes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IndicatorPoint {
    pub x: f32,
    pub y: f32,
}

/// Computes chevron triangle vertices for a given placement edge.
///
/// The chevron is an isosceles triangle pointing toward the tape center.
/// At the top edge it points downward; at the bottom edge it points upward.
///
/// # Arguments
/// * `center_x` - Horizontal center of the widget
/// * `edge_y` - Y coordinate of the edge (top or bottom of widget)
/// * `size` - Chevron height in pixels
/// * `pointing_down` - true for top placement (points toward tape center)
pub fn chevron_vertices(
    center_x: f32,
    edge_y: f32,
    size: f32,
    pointing_down: bool,
) -> [IndicatorPoint; 3] {
    let half_base = size * 0.6;
    if pointing_down {
        // Top edge: triangle points down
        [
            IndicatorPoint {
                x: center_x - half_base,
                y: edge_y,
            },
            IndicatorPoint {
                x: center_x + half_base,
                y: edge_y,
            },
            IndicatorPoint {
                x: center_x,
                y: edge_y + size,
            },
        ]
    } else {
        // Bottom edge: triangle points up
        [
            IndicatorPoint {
                x: center_x - half_base,
                y: edge_y,
            },
            IndicatorPoint {
                x: center_x + half_base,
                y: edge_y,
            },
            IndicatorPoint {
                x: center_x,
                y: edge_y - size,
            },
        ]
    }
}

/// Plain geometry keeps the native cache shareable by export worker threads.
#[derive(Clone, Debug)]
pub(crate) enum IndicatorShape {
    Chevron([Point; 3]),
    HighlightBar(Rect),
}

#[derive(Clone, Debug)]
pub struct HeadingCache {
    pub tape_image: Image,
    pub tape_width: f32,
    pub tape_body_y: f32,
    pub tape_body_height: f32,
    pub frame: Rect,
    pub pixels_per_degree: f32,
    pub(crate) tape_content_y: f32,
    pub(crate) tape_ink: Vec<Rect>,
    pub(crate) indicators: Vec<(IndicatorShape, Paint)>,
    pub(crate) indicator_shadow: Option<Paint>,
    pub(crate) shadow_distance: f32,
    pub(crate) edge_fade: Paint,
    pub(crate) edge_fade_bounds: [Rect; 2],
    pub(crate) opacity: f32,
}

pub fn prepare_heading_cache(
    heading: &ValidatedHeading,
    scene: &ValidatedScenePresentation,
    fonts: &dyn FontResolver,
) -> WidgetResult<HeadingCache> {
    let scale = scene.scale;
    let tape_width = (360.0 * heading.pixels_per_degree * scale).ceil() as u32;
    // The scroll period excludes the one-pixel sampling gutter at each image edge.
    let pixels_per_degree = tape_width as f32 / 360.0;
    let frame = Rect::from_xywh(
        heading.x,
        heading.y,
        heading.width * scale,
        heading.height * scale,
    );
    // Fade the outer 45 intrinsic pixels; keep narrow tapes' centre fully visible.
    let fade_stop = (45.0 * scale / frame.width()).min(0.25);
    let fade_width = fade_stop * frame.width();
    let edge_fade_bounds = [
        Rect::new(frame.left, frame.top, frame.left + fade_width, frame.bottom),
        Rect::new(
            frame.right - fade_width,
            frame.top,
            frame.right,
            frame.bottom,
        ),
    ];
    let colors = [
        Color4f::from(Color::TRANSPARENT),
        Color4f::from(Color::WHITE),
        Color4f::from(Color::WHITE),
        Color4f::from(Color::TRANSPARENT),
    ];
    let positions = [0.0, fade_stop, 1.0 - fade_stop, 1.0];
    let gradient = gradient::Gradient::new(
        gradient::Colors::new(&colors, Some(&positions), TileMode::Clamp, None),
        gradient::Interpolation::default(),
    );
    let shader = gradient::shaders::linear_gradient(
        ((frame.left, frame.top), (frame.right, frame.top)),
        &gradient,
        None,
    )
    .ok_or_else(|| WidgetError::Render("unable to prepare heading edge fade".into()))?;
    let mut edge_fade = Paint::default();
    edge_fade.set_blend_mode(BlendMode::DstIn);
    edge_fade.set_shader(shader);
    let indicator_size = heading.indicator_size * scale;
    let font_size = heading.label_font_size * scale;
    let label_offset = heading.label_offset * scale;
    let layout = heading_tape_layout(
        frame.height(),
        heading.show_indicator,
        &heading.indicator_style,
        &heading.indicator_placement,
        indicator_size,
        heading.major_tick_length_pct,
        label_offset,
        font_size,
    );
    let image_width = tape_width
        .checked_add(2)
        .ok_or_else(|| WidgetError::Render("heading strip image width overflow".into()))?;
    let mut tape = surface(
        image_width,
        layout.body_height.ceil() as u32,
        SHAPE_COLOR_TYPE,
    )?;
    let canvas = tape.canvas();
    canvas.clear(skia_safe::Color::TRANSPARENT);
    // Wrapped strokes/labels also fill these gutters, so bilinear sampling sees across north.
    canvas.translate((1.0, 0.0));
    let clip = Rect::from_xywh(0.0, 0.0, tape_width as f32, layout.body_height);
    canvas.clip_rect(
        Rect::from_xywh(-1.0, 0.0, image_width as f32, layout.body_height),
        skia_safe::ClipOp::Intersect,
        false,
    );
    let font = resolve_font(fonts, Some(&heading.label_font), font_size)?;
    let label_y = heading_label_baseline(
        layout.tick_scale_height,
        heading.major_tick_length_pct,
        label_offset,
        font_size,
    );
    let ticks = visible_ticks(
        0.0,
        pixels_per_degree,
        tape_width as f32,
        heading.major_tick_interval,
        heading.minor_ticks_per_major,
        heading.show_major_ticks,
        heading.show_minor_ticks,
    );
    let labels = visible_labels(&ticks, heading.show_minor_labels, heading.show_major_labels);
    let mut ink = Vec::new();
    let shadow_color = parse_color(&scene.shadow_color, 1.0);
    let shadow_distance = scene.shadow_distance * scale;
    // Shadows sit behind all ticks and labels, matching native composition.
    for shadow_pass in [true, false] {
        if shadow_pass && scene.shadow_strength == 0.0 {
            continue;
        }
        canvas.save();
        if shadow_pass {
            canvas.translate((shadow_distance, shadow_distance));
        }
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        for tick in &ticks {
            let thickness = if tick.is_major {
                heading.major_tick_thickness
            } else {
                heading.minor_tick_thickness
            } * scale;
            let (top, length) = heading_tick_position(
                layout.tick_scale_height,
                heading.major_tick_length_pct,
                heading.minor_tick_length_pct,
                &heading.tick_alignment,
                tick.is_major,
            );
            let color = if tick.is_cardinal && !shadow_pass {
                &heading.cardinal_tick_color
            } else {
                &heading.tick_color
            };
            paint.set_color(parse_color(color, 1.0));
            paint.set_stroke_width(thickness);
            let shadow = shadow_pass.then(|| {
                shadow_paint(&paint, shadow_color, scene.shadow_strength * scale)
                    .expect("positive shadow sigma")
            });
            let draw_paint = shadow.as_ref().unwrap_or(&paint);
            // Wrap strokes as well as labels, including the half of north beyond either edge.
            for copy in [-(tape_width as f32), 0.0, tape_width as f32] {
                let x = tick.x + copy;
                canvas.draw_line(Point::new(x, top), Point::new(x, top + length), draw_paint);
                if !shadow_pass {
                    // A zero-width Skia stroke is a one-pixel hairline.
                    let width = if thickness == 0.0 { 1.0 } else { thickness };
                    let mut bounds = Rect::from_xywh(x - width * 0.5, top, width, length);
                    if bounds.intersect(clip) {
                        ink.push(bounds);
                    }
                }
            }
        }
        for label in &labels {
            let color = if label.is_major_label {
                &heading.cardinal_label_color
            } else {
                &heading.label_color
            };
            paint.set_color(parse_color(color, 1.0));
            let shadow = shadow_pass.then(|| {
                shadow_paint(&paint, shadow_color, scene.shadow_strength * scale)
                    .expect("positive shadow sigma")
            });
            let draw_paint = shadow.as_ref().unwrap_or(&paint);
            let x = origin_x_for_centered_text(&label.text, label.x, &font);
            for copy in [-(tape_width as f32), 0.0, tape_width as f32] {
                canvas.draw_str(
                    &label.text,
                    Point::new(x + copy, label_y),
                    &font,
                    draw_paint,
                );
                if !shadow_pass {
                    let (_, mut bounds) = font.measure_str(&label.text, None);
                    bounds.offset((x + copy, label_y));
                    if bounds.intersect(clip) {
                        ink.push(bounds);
                    }
                }
            }
        }
        canvas.restore();
    }
    let mut tape_bounds = Rect::default();
    for bounds in &ink {
        tape_bounds.join(*bounds);
    }
    // Centre painted strokes/glyphs rather than the font's reserved descent allowance.
    let tape_content_y =
        layout.body_y + (layout.body_height - tape_bounds.height()) * 0.5 - tape_bounds.top;
    let tape_image = tape.image_snapshot();
    let mut indicator_paint = Paint::default();
    indicator_paint.set_anti_alias(true);
    indicator_paint.set_color(parse_color(&heading.indicator_color, 1.0));
    let mut indicators = Vec::new();
    if heading.show_indicator {
        if heading.indicator_style == "highlight_bar" {
            indicator_paint.set_alpha(76);
            indicators.push((
                IndicatorShape::HighlightBar(Rect::from_xywh(
                    frame.center_x() - indicator_size * 0.5,
                    frame.top,
                    indicator_size,
                    frame.height(),
                )),
                indicator_paint.clone(),
            ));
        } else {
            for (visible, edge, down) in [
                (layout.has_top_chevron, frame.top, true),
                (layout.has_bottom_chevron, frame.bottom, false),
            ] {
                if !visible {
                    continue;
                }
                let vertices = chevron_vertices(frame.center_x(), edge, indicator_size, down);
                indicators.push((
                    IndicatorShape::Chevron(vertices.map(|point| Point::new(point.x, point.y))),
                    indicator_paint.clone(),
                ));
            }
        }
    }
    let indicator_shadow = if heading.indicator_style == "chevron" {
        shadow_paint(
            &indicator_paint,
            shadow_color,
            scene.shadow_strength * scale,
        )
    } else {
        None
    };
    Ok(HeadingCache {
        tape_image,
        tape_width: tape_width as f32,
        tape_body_y: layout.body_y,
        tape_body_height: layout.body_height,
        frame,
        pixels_per_degree,
        tape_content_y,
        tape_ink: ink,
        indicators,
        indicator_shadow,
        shadow_distance,
        edge_fade,
        edge_fade_bounds,
        opacity: heading.opacity * scene.opacity,
    })
}

pub struct HeadingFrame {
    pub bounds: Rect,
    pub content_bounds: Rect,
    pub content_width: f32,
    first_copy: f32,
    indicators: Vec<(Path, Paint)>,
}

impl HeadingCache {
    pub fn prepare_frame(&self, raw: Option<f64>) -> HeadingFrame {
        // Missing external heading deliberately displays the north-facing tape.
        let heading = raw.unwrap_or(0.0) as f32;
        let offset = heading_offset(heading, self.pixels_per_degree, self.frame.width())
            .rem_euclid(self.tape_width);
        let mut x = self.frame.left - offset;
        let mut content = Rect::default();
        while x < self.frame.right {
            for ink in &self.tape_ink {
                let mut bounds = *ink;
                bounds.offset((x, self.frame.top + self.tape_content_y));
                if bounds.intersect(self.frame) {
                    content.join(bounds);
                }
            }
            x += self.tape_width;
        }
        let indicators: Vec<_> = self
            .indicators
            .iter()
            .map(|(shape, paint)| {
                let path = match shape {
                    IndicatorShape::HighlightBar(rect) => Path::rect(*rect, None),
                    IndicatorShape::Chevron(vertices) => {
                        let mut path = PathBuilder::new();
                        path.move_to(vertices[0]);
                        path.line_to(vertices[1]);
                        path.line_to(vertices[2]);
                        path.close();
                        path.detach()
                    }
                };
                (path, paint.clone())
            })
            .collect();
        for (path, _) in &indicators {
            let mut bounds = path.compute_tight_bounds();
            if bounds.intersect(self.frame) {
                content.join(bounds);
            }
        }
        HeadingFrame {
            bounds: Rect::new(
                self.frame.left.floor(),
                self.frame.top.floor(),
                self.frame.right.ceil(),
                self.frame.bottom.ceil(),
            ),
            // The tape is a fixed viewport; gaps at either edge do not move its horizontal bounds.
            content_bounds: Rect::new(
                self.frame.left,
                content.top,
                self.frame.right,
                content.bottom,
            ),
            content_width: self.frame.width(),
            first_copy: self.frame.left - offset,
            indicators,
        }
    }
}

impl HeadingFrame {
    pub fn draw(&self, canvas: &Canvas, cache: &HeadingCache) {
        canvas.save();
        canvas.clip_rect(cache.frame, ClipOp::Intersect, false);
        let mut opacity = Paint::default();
        opacity.set_alpha_f(cache.opacity);
        canvas.save_layer(&SaveLayerRec::default().bounds(&cache.frame).paint(&opacity));
        let mut x = self.first_copy;
        while x < cache.frame.right {
            // Adjacent hard clips assign each destination pixel to exactly one copy.
            // Sampling gutters keep north continuous without blending two image edges.
            canvas.save();
            canvas.clip_rect(
                Rect::new(x, cache.frame.top, x + cache.tape_width, cache.frame.bottom),
                ClipOp::Intersect,
                false,
            );
            canvas.draw_image_with_sampling_options(
                &cache.tape_image,
                (x - 1.0, cache.frame.top + cache.tape_content_y),
                FilterMode::Linear,
                None,
            );
            canvas.restore();
            x += cache.tape_width;
        }
        // The opaque centre of the mask is a no-op; only shade the fading edges.
        for bounds in &cache.edge_fade_bounds {
            canvas.draw_rect(bounds, &cache.edge_fade);
        }
        if let Some(shadow) = &cache.indicator_shadow {
            canvas.save();
            canvas.translate((cache.shadow_distance, cache.shadow_distance));
            for (path, _) in &self.indicators {
                canvas.draw_path(path, shadow);
            }
            canvas.restore();
        }
        for (path, paint) in &self.indicators {
            canvas.draw_path(path, paint);
        }
        canvas.restore();
        canvas.restore();
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
    pub heading_tape: ResizeVariant,
}
#[derive(Serialize)]
pub struct ResizeVariant {
    pub width: f32,
    pub height: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_offset: Option<f32>,
}

/// Changing the label gap grows/shrinks the frame while preserving tick lengths.
pub fn resize_heading_label_offset(
    widget: ValueConfig,
    scene: ScenePresentationConfig,
    label_offset: f32,
) -> WidgetResult<ResizeUpdate> {
    let config = validate_heading(widget, 0)?;
    validate_scene_presentation(scene)?;
    let label_offset = require_non_negative_f32(Some(label_offset), "heading label_offset")?;
    let body_ratio =
        if heading_tape_has_highlight_bar(config.show_indicator, &config.indicator_style) {
            1.0 - 2.0 * HIGHLIGHT_BAR_BODY_MARGIN_FRAME_PCT
        } else {
            1.0
        };
    let height =
        (config.height + (label_offset - config.label_offset) / body_ratio).clamp(8.0, 10_000.0);
    Ok(ResizeUpdate {
        x: config.x,
        y: config.y,
        display_variants: ResizeVariants {
            heading_tape: ResizeVariant {
                width: config.width,
                height,
                label_offset: Some(label_offset),
            },
        },
    })
}

pub fn resize_heading(
    widget: ValueConfig,
    scene: ScenePresentationConfig,
    size: [f32; 2],
    gesture: ResizeGesture,
    activity: Option<&ActivitySampler>,
    fonts: &dyn FontResolver,
) -> WidgetResult<ResizeUpdate> {
    let ResizeGesture {
        fixed,
        point,
        second,
        ..
    } = gesture;
    if size.iter().any(|value| !value.is_finite() || *value <= 0.0) {
        return Err(WidgetError::Config("invalid heading resize size".into()));
    }
    let mut config = validate_heading(widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    let raw = activity.and_then(|activity| activity.heading_at(second));
    let original = prepare_heading_cache(&config, &scene, fonts)?
        .prepare_frame(raw)
        .content_bounds;
    if original.is_empty() {
        return Err(WidgetError::Render(
            "heading tape has no visible content to resize".into(),
        ));
    }
    config.width = (config.width * size[0] / original.width()).clamp(8.0, 10_000.0);
    config.height = (config.height * size[1] / original.height()).clamp(8.0, 10_000.0);
    let bounds = prepare_heading_cache(&config, &scene, fonts)?
        .prepare_frame(raw)
        .content_bounds;
    Ok(ResizeUpdate {
        x: config.x + point[0] - (bounds.left + bounds.width() * fixed[0]),
        y: config.y + point[1] - (bounds.top + bounds.height() * fixed[1]),
        display_variants: ResizeVariants {
            heading_tape: ResizeVariant {
                width: config.width,
                height: config.height,
                label_offset: None,
            },
        },
    })
}

pub struct PreviewHeading {
    cache: HeadingCache,
    preview_raster_scale: PreviewRasterScale,
    last_heading: Option<f32>,
    pub tile: Tile,
}

impl PreviewHeading {
    pub fn new(
        widget: ValueConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let widget = validate_heading(widget, 0)?;
        let host = WidgetHost::from_scene(&scene, raster_scale)?;
        Self::from_validated(&widget, &validate_scene_presentation(scene)?, &host, fonts)
    }

    pub fn from_validated(
        widget: &ValidatedHeading,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        Ok(Self {
            cache: prepare_heading_cache(widget, scene, fonts)?,
            preview_raster_scale: host.raster_scale,
            last_heading: None,
            tile: Tile::default(),
        })
    }

    /// The sample the frame at `second` draws; equal samples draw identical pixels.
    pub fn frame_key(&self, activity: Option<&ActivitySampler>, second: f64) -> Option<f64> {
        activity.and_then(|activity| activity.heading_at(second))
    }

    /// Scene rectangle a render at `second` draws into; the tape's frame is fixed.
    pub fn layout_bounds(&self, activity: Option<&ActivitySampler>, second: f64) -> TileBounds {
        let raw = activity.and_then(|activity| activity.heading_at(second));
        self.cache.prepare_frame(raw).bounds.into()
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(
        &self,
        canvas: &Canvas,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> TileBounds {
        let raw = activity.and_then(|activity| activity.heading_at(second));
        let frame = self.cache.prepare_frame(raw);
        frame.draw(canvas, &self.cache);
        frame.bounds.into()
    }

    pub fn render(&mut self, activity: Option<&ActivitySampler>, second: f64) -> WidgetResult<()> {
        let raw = activity.and_then(|activity| activity.heading_at(second));
        let heading = raw.unwrap_or(0.0) as f32;
        // The strip scrolls continuously, even between integer-degree headings.
        if self.last_heading == Some(heading) {
            return Ok(());
        }
        let frame = self.cache.prepare_frame(raw);
        let bounds = frame.bounds;
        self.tile.render(
            self.preview_raster_scale,
            SHAPE_COLOR_TYPE,
            bounds,
            Presented::new(frame.content_bounds, frame.content_width),
            |canvas| {
                frame.draw(canvas, &self.cache);
                Ok(())
            },
        )?;
        self.last_heading = Some(heading);
        Ok(())
    }
}
