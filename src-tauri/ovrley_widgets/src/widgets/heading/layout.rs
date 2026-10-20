//! Prepare the full compass strip once; playback only changes its scroll offset.
use super::{
    geometry::{
        chevron_vertices, heading_label_baseline, heading_tape_layout, heading_tick_position,
        visible_labels, visible_ticks,
    },
    validation::ValidatedHeading,
};
use crate::{
    error::{WidgetError, WidgetResult},
    model::scene::ValidatedScenePresentation,
    render::{
        shadow::shadow_paint,
        text::{origin_x_for_centered_text, parse_color, resolve_font, FontResolver},
    },
    widgets::value::preview::{surface, SHAPE_COLOR_TYPE},
};
use skia_safe::{gradient, BlendMode, Color, Color4f, Image, Paint, Point, Rect, TileMode};

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
    let mut tape = surface(image_width, layout.body_height.ceil() as u32, SHAPE_COLOR_TYPE)?;
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
