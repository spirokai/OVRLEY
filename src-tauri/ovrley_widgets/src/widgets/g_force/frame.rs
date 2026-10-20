//! One prepared frame supplies both visible bounds and painting.
use super::{
    format::{g_force_frame_state, GForceFrameState, GForceSample},
    layout::{g_force_layout, GForceCache},
    validation::ValidatedGForceWidget,
};
use crate::{
    error::{WidgetError, WidgetResult},
    model::{scene::ValidatedScenePresentation, standard_widgets::widget_font_weight},
    render::text::{
        draw_text_blob, layout_text, parse_color, resolve_font, text_selection_bounds, FontResolver,
        ResolvedTextStyle,
    },
};
use skia_safe::{Canvas, Paint, Point, Rect, TextBlob};

struct TextRun {
    blob: TextBlob,
    baseline: f32,
    style: ResolvedTextStyle,
}

pub struct GForceFrame {
    value: TextRun,
    unit: Option<TextRun>,
    marker_radius: f32,
    marker_paint: Paint,
    pub state: GForceFrameState,
    pub bounds: Rect,
    pub content_bounds: Rect,
    pub content_width: f32,
}

impl GForceFrame {
    pub fn draw(&self, canvas: &Canvas, cache: &GForceCache) {
        canvas.draw_image(
            &cache.parent_circle_image,
            (cache.parent_circle_image_x, cache.parent_circle_image_y),
            None,
        );
        canvas.draw_circle(
            Point::new(cache.x + self.state.marker_x, cache.y + self.state.marker_y),
            self.marker_radius,
            &self.marker_paint,
        );
        draw_text_blob(
            canvas,
            &self.value.blob,
            self.value.baseline,
            &self.value.style,
        );
        if let Some(unit) = &self.unit {
            draw_text_blob(canvas, &unit.blob, unit.baseline, &unit.style);
        }
    }
}

pub fn prepare_g_force_frame(
    widget: &ValidatedGForceWidget,
    scene: &ValidatedScenePresentation,
    sample: GForceSample,
    fonts: &dyn FontResolver,
) -> WidgetResult<GForceFrame> {
    let scale = scene.scale;
    let layout = g_force_layout(widget, scale);
    let state = g_force_frame_state(
        sample.horizontal,
        sample.vertical,
        sample.max_g,
        layout.center_x,
        layout.center_y,
        layout.radius,
        widget.label_decimals,
    );
    let font_size = widget.label_font_size * scale;
    let line_height = font_size * 0.92;
    let margin = font_size * 0.05;
    let top = widget.y + layout.center_y + layout.radius - line_height - margin
        + widget.label_offset_y * scale;
    let style = ResolvedTextStyle {
        x: widget.x + layout.center_x + layout.radius + margin + widget.label_offset_x * scale,
        y: top,
        font_name: Some(widget.label_font.clone()),
        font_size,
        font_weight: widget_font_weight(),
        italic: false,
        letter_spacing: 0.0,
        line_height,
        color: parse_color(&widget.label_color, widget.opacity),
        opacity: widget.opacity,
        shadow_color: (scene.shadow_strength > 0.0)
            .then(|| parse_color(&scene.shadow_color, widget.opacity)),
        shadow_strength: scene.shadow_strength * scale,
        shadow_distance: scene.shadow_distance * scale,
        border_color: None,
        border_thickness: 0.0,
        numerals: scene.numerals,
    };
    let font = resolve_font(fonts, Some(&widget.label_font), font_size)?;
    let mut content_bounds = Rect::from_xywh(
        widget.x + layout.center_x - layout.radius,
        widget.y + layout.center_y - layout.radius,
        layout.radius * 2.0,
        layout.radius * 2.0,
    );
    let marker_radius = widget.marker_size * scale * 0.5;
    content_bounds.join(Rect::from_xywh(
        widget.x + state.marker_x - marker_radius,
        widget.y + state.marker_y - marker_radius,
        marker_radius * 2.0,
        marker_radius * 2.0,
    ));
    let mut bounds = content_bounds;
    let mut prepare_text = |text: &str, style: ResolvedTextStyle| -> WidgetResult<(TextRun, f32)> {
        let text = layout_text(text, &font, 0.0, style.numerals);
        let ink = &text.measurement;
        let baseline =
            style.y + (line_height - (ink.bounds_bottom - ink.bounds_top)) * 0.5 - ink.bounds_top;
        bounds.join(Rect::new(
            style.x + ink.bounds_left,
            baseline + ink.bounds_top,
            style.x + ink.bounds_right,
            baseline + ink.bounds_bottom,
        ));
        content_bounds.join(text_selection_bounds(ink, style.x, baseline));
        Ok((
            TextRun {
                blob: text
                    .blob
                    .ok_or_else(|| WidgetError::Render("unable to lay out G-force label".into()))?,
                baseline,
                style,
            },
            ink.width,
        ))
    };
    let value_text = if state.magnitude.is_some() {
        state
            .label
            .strip_suffix(" G")
            .expect("formatted G-force text")
    } else {
        &state.label
    };
    let (value, value_width) = prepare_text(value_text, style.clone())?;
    let unit = if state.magnitude.is_some() {
        Some(
            prepare_text(
                &widget.label_unit,
                ResolvedTextStyle {
                    x: style.x + value_width + 3.0,
                    color: parse_color(&widget.label_unit_color, widget.opacity),
                    ..style
                },
            )?
            .0,
        )
    } else {
        None
    };
    let shadow_padding = if scene.shadow_strength > 0.0 {
        (scene.shadow_strength * 3.0 + scene.shadow_distance) * scale
    } else {
        0.0
    };
    bounds.outset((2.0 + shadow_padding, 2.0 + shadow_padding));
    bounds = Rect::new(
        bounds.left.floor(),
        bounds.top.floor(),
        bounds.right.ceil(),
        bounds.bottom.ceil(),
    );
    let mut marker_paint = Paint::default();
    marker_paint.set_anti_alias(true);
    marker_paint.set_color(parse_color(
        &widget.marker_color,
        widget.marker_opacity * widget.opacity,
    ));
    Ok(GForceFrame {
        value,
        unit,
        marker_radius,
        marker_paint,
        state,
        bounds,
        content_bounds,
        content_width: layout.width,
    })
}
