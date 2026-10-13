//! A single prepared run owns static label paint and interaction geometry.
use super::{style::validated_label_style, validation::ValidatedLabel};
use crate::{error::WidgetResult, model::scene::ValidatedScenePresentation, render::text::{
    baseline_for_top_with_line_height, draw_text_blob, layout_text, resolve_styled_font,
    FontResolver, ResolvedTextStyle,
}};
use skia_safe::{Canvas, Rect, TextBlob};

pub struct LabelFrame {
    pub bounds: Rect,
    pub content_bounds: Rect,
    pub content_width: f32,
    style: ResolvedTextStyle,
    blob: Option<TextBlob>,
    baseline: f32,
}

impl LabelFrame {
    pub fn draw(&self, canvas: &Canvas) -> WidgetResult<()> {
        // Empty text is an intentional label state, with no painted glyphs.
        if let Some(blob) = &self.blob {
            draw_text_blob(canvas, blob, self.baseline, &self.style);
        }
        Ok(())
    }
}

pub fn prepare_label_frame(label: &ValidatedLabel, scene: &ValidatedScenePresentation, fonts: &dyn FontResolver) -> WidgetResult<LabelFrame> {
    let style = validated_label_style(label, scene, scene.scale);
    let font = resolve_styled_font(fonts, Some(&label.font_name), style.font_size, style.font_weight, style.italic)?;
    let layout = layout_text(&label.text, &font, style.letter_spacing);
    let ink = layout.measurement;
    let height = ink.bounds_bottom - ink.bounds_top;
    let baseline = if height <= f32::EPSILON {
        baseline_for_top_with_line_height(style.y, &font, style.line_height)
    } else {
        style.y + (style.line_height - height) * 0.5 - ink.bounds_top
    };
    let mut content_bounds = Rect::new(style.x + ink.bounds_left, baseline + ink.bounds_top, style.x + ink.bounds_right, baseline + ink.bounds_bottom);
    if !content_bounds.is_empty() && style.border_color.is_some() {
        content_bounds.outset((style.border_thickness * 0.5, style.border_thickness * 0.5));
    }
    let mut bounds = content_bounds;
    let padding = 2.0 + if style.shadow_color.is_some() { style.shadow_strength * 3.0 + style.shadow_distance.abs() } else { 0.0 };
    bounds.outset((padding, padding));
    bounds = Rect::new(bounds.left.floor(), bounds.top.floor(), bounds.right.ceil(), bounds.bottom.ceil());
    Ok(LabelFrame { bounds, content_bounds, content_width: ink.width, style, blob: layout.blob, baseline })
}
