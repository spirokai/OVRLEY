//! Prepared glyph runs supply painting and text-layout interaction bounds together.
use super::{LINE_HEIGHT_RATIO, TABLE_VERTICAL_METRICS_TEXT};
use crate::{
    error::{WidgetError, WidgetResult},
    render::preview::{PreviewRasterScale, StaticRaster},
    render::text::{
        baseline_for_text_top_with_line_height, draw_text_blob, layout_text,
        measure_text_with_font, text_selection_bounds, ResolvedTextStyle,
    },
    widgets::value::preview::TEXT_COLOR_TYPE,
};
use skia_safe::{Canvas, Font, Point, Rect, TextBlob};
use std::sync::Arc;

/// One prepared table cell; split tables share lap-log typography.
#[derive(Clone, Debug)]
pub(crate) struct TextRun {
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
        let layout = layout_text(text, font, 0.0, style.numerals);
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
        let width = measure_text_with_font(text, font, style.numerals).width;
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

    pub fn draw(&self, canvas: &Canvas) {
        draw_text_blob(canvas, &self.blob, self.baseline, &self.style);
    }
}

#[derive(Clone, Debug)]
pub(super) struct TextLayer {
    raster: StaticRaster,
    content_bounds: Rect,
    content_width: f32,
}

impl TextLayer {
    pub fn new(runs: Vec<TextRun>, x: f32, raster_scale: PreviewRasterScale) -> WidgetResult<Self> {
        let frame = LapTimerFrame::new(runs, None, x)?;
        let raster = StaticRaster::new(
            raster_scale,
            Point::default(),
            frame.bounds,
            TEXT_COLOR_TYPE,
            |canvas| {
                frame.draw(canvas);
                Ok(())
            },
        )?;
        Ok(Self {
            raster,
            content_bounds: frame.content_bounds,
            content_width: frame.content_width,
        })
    }
}

pub struct LapTimerFrame {
    runs: Vec<TextRun>,
    background: Option<Arc<TextLayer>>,
    /// Hidden frames keep their layout bounds but draw nothing.
    shown: bool,
    pub content_bounds: Rect,
    pub bounds: Rect,
    pub content_width: f32,
}

impl LapTimerFrame {
    pub(super) fn new(
        runs: Vec<TextRun>,
        background: Option<Arc<TextLayer>>,
        x: f32,
    ) -> WidgetResult<Self> {
        let (mut content_bounds, mut bounds, mut content_width) = if let Some(layer) = &background {
            (
                layer.content_bounds,
                layer.raster.bounds(),
                layer.content_width,
            )
        } else {
            let first = runs
                .first()
                .ok_or_else(|| WidgetError::Render("lap timer has no drawable text".into()))?;
            (first.content, first.bounds, first.right - x)
        };
        for run in &runs {
            content_bounds.join(run.content);
            bounds.join(run.bounds);
            content_width = content_width.max(run.right - x);
        }
        bounds = Rect::new(
            bounds.left.floor(),
            bounds.top.floor(),
            bounds.right.ceil(),
            bounds.bottom.ceil(),
        );
        Ok(Self {
            runs,
            background,
            shown: true,
            content_bounds,
            bounds,
            content_width,
        })
    }

    pub fn shown(mut self, shown: bool) -> Self {
        self.shown = shown;
        self
    }

    pub fn draw(&self, canvas: &Canvas) {
        if !self.shown {
            return;
        }
        if let Some(layer) = &self.background {
            layer.raster.draw(canvas, None);
        }
        for run in &self.runs {
            run.draw(canvas);
        }
    }
}
