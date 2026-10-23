//! One prepared frame supplies both visible bounds and painting.
use super::{
    format::{g_force_frame_state, GForceFrameState, GForceSample},
    layout::{g_force_layout, GForceCache},
    validation::ValidatedGForceWidget,
};
use crate::{
    error::WidgetResult,
    model::{scene::ValidatedScenePresentation, standard_widgets::widget_font_weight},
    render::text::{
        layout_text_path, parse_color, resolve_font, FontResolver, ResolvedTextStyle, TextPaints,
    },
};
use skia_safe::{Canvas, Paint, Path, Point, Rect};

/// Labels are placed outlines so they track the dial continuously while it resizes.
struct TextRun {
    path: Path,
    paints: TextPaints,
}

impl TextRun {
    fn draw(&self, canvas: &Canvas) {
        self.paints.draw_path(canvas, &self.path);
    }
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
        canvas.save();
        canvas.translate((cache.x, cache.y));
        cache.parent_circle.draw(canvas, None);
        canvas.restore();
        canvas.draw_circle(
            Point::new(cache.x + self.state.marker_x, cache.y + self.state.marker_y),
            self.marker_radius,
            &self.marker_paint,
        );
        self.value.draw(canvas);
        if let Some(unit) = &self.unit {
            unit.draw(canvas);
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
    let mut prepare_text = |text: &str, style: ResolvedTextStyle| -> (TextRun, f32) {
        let (path, width) = layout_text_path(text, &font, style.numerals);
        // Outline bounds scale continuously; glyph-mask bounds step in whole pixels.
        let ink = path.compute_tight_bounds();
        let baseline = style.y + (line_height - ink.height()) * 0.5 - ink.top;
        bounds.join(ink.with_offset((style.x, baseline)));
        content_bounds.join(Rect::new(
            style.x,
            baseline + ink.top,
            style.x + width,
            baseline + ink.bottom,
        ));
        let run = TextRun {
            path: path.with_offset((style.x, baseline)),
            paints: TextPaints::new(&style),
        };
        (run, width)
    };
    let value_text = if state.magnitude.is_some() {
        state
            .label
            .strip_suffix(" G")
            .expect("formatted G-force text")
    } else {
        &state.label
    };
    let (value, value_width) = prepare_text(value_text, style.clone());
    let unit = state.magnitude.is_some().then(|| {
        prepare_text(
            &widget.label_unit,
            ResolvedTextStyle {
                x: style.x + value_width + 3.0,
                color: parse_color(&widget.label_unit_color, widget.opacity),
                ..style
            },
        )
        .0
    });
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
