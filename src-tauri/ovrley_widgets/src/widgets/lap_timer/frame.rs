//! Prepared glyph runs supply painting and text-layout interaction bounds together.
use super::{LINE_HEIGHT_RATIO, TABLE_VERTICAL_METRICS_TEXT};
use crate::{
    error::{WidgetError, WidgetResult},
    render::text::{
        baseline_for_text_top_with_line_height, draw_text_blob, layout_text, text_selection_bounds,
        ResolvedTextStyle,
    },
};
use skia_safe::{Canvas, Font, Rect, TextBlob};

#[derive(Clone, Debug)]
pub(super) struct TextRun {
    blob: TextBlob,
    style: ResolvedTextStyle,
    baseline: f32,
    pub content: Rect,
    pub bounds: Rect,
    pub right: f32,
}

impl TextRun {
    pub fn new(
        text: &str,
        metrics_text: &str,
        style: ResolvedTextStyle,
        font: &Font,
    ) -> WidgetResult<Self> {
        let layout = layout_text(text, font, 0.0);
        let measurement = layout.measurement;
        let baseline =
            baseline_for_text_top_with_line_height(metrics_text, style.y, font, style.line_height);
        let mut content = Rect::new(
            style.x + measurement.bounds_left,
            baseline + measurement.bounds_top,
            style.x + measurement.bounds_right,
            baseline + measurement.bounds_bottom,
        );
        if style.border_color.is_some() {
            content.outset((style.border_thickness * 0.5, style.border_thickness * 0.5));
        }
        let mut bounds = content;
        content = text_selection_bounds(&measurement, style.x, baseline);
        if style.border_color.is_some() {
            content.outset((style.border_thickness * 0.5, style.border_thickness * 0.5));
        }
        let shadow = if style.shadow_color.is_some() && style.shadow_strength > 0.0 {
            style.shadow_distance.abs() + style.shadow_strength * 3.0
        } else {
            0.0
        };
        bounds.outset((shadow + 2.0, shadow + 2.0));
        Ok(Self {
            blob: layout
                .blob
                .ok_or_else(|| WidgetError::Render("unable to lay out lap timer text".into()))?,
            right: style.x + measurement.width,
            style,
            baseline,
            content,
            bounds,
        })
    }

    pub fn cell(
        text: &str,
        right: f32,
        top: f32,
        style: &ResolvedTextStyle,
        font: &Font,
    ) -> WidgetResult<Self> {
        let (width, _) = font.measure_str(text, None);
        Self::new(
            text,
            TABLE_VERTICAL_METRICS_TEXT,
            ResolvedTextStyle {
                x: right - width,
                y: top,
                line_height: style.font_size * LINE_HEIGHT_RATIO,
                ..style.clone()
            },
            font,
        )
    }

    pub fn offset_y(mut self, offset: f32) -> Self {
        self.style.y += offset;
        self.baseline += offset;
        self.content.offset((0.0, offset));
        self.bounds.offset((0.0, offset));
        self
    }
}

pub struct LapTimerFrame {
    runs: Vec<TextRun>,
    pub content_bounds: Rect,
    pub bounds: Rect,
    pub content_width: f32,
}

impl LapTimerFrame {
    pub(super) fn new(runs: Vec<TextRun>, x: f32) -> WidgetResult<Self> {
        let first = runs
            .first()
            .ok_or_else(|| WidgetError::Render("lap timer has no drawable text".into()))?;
        let mut content_bounds = first.content;
        let mut bounds = first.bounds;
        let mut right = first.right;
        for run in runs.iter().skip(1) {
            content_bounds.join(run.content);
            bounds.join(run.bounds);
            right = right.max(run.right);
        }
        bounds = Rect::new(
            bounds.left.floor(),
            bounds.top.floor(),
            bounds.right.ceil(),
            bounds.bottom.ceil(),
        );
        Ok(Self {
            runs,
            content_bounds,
            bounds,
            content_width: right - x,
        })
    }

    pub fn draw(&self, canvas: &Canvas) {
        for run in &self.runs {
            draw_text_blob(canvas, &run.blob, run.baseline, &run.style);
        }
    }
}
