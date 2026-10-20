//! Prepared geometry is the source for both painting and editor bounds.
use super::{
    arc::ArcGaugeCache, linear::LinearGaugeCache, metric::GaugeFrameState,
    validation::ValidatedGauge,
};
use crate::{
    error::{WidgetError, WidgetResult},
    model::scene::Numerals,
    render::text::{
        baseline_for_top_with_line_height, draw_text_blob, layout_text, measure_text_with_font,
        FontResolver, ResolvedTextStyle,
    },
    widgets::value::layout::metric_vertical_metrics_text,
};
use skia_safe::{Canvas, Rect, TextBlob};

#[derive(Clone, Debug)]
pub enum GaugeLayout {
    Linear(LinearGaugeCache),
    Arc(ArcGaugeCache),
}

pub struct GaugeFrame {
    pub bounds: Rect,
    pub content_bounds: Rect,
    pub content_width: f32,
    pub state: GaugeFrameState,
    value: Option<TextRun>,
}

pub(crate) struct TextRun {
    blob: TextBlob,
    baseline: f32,
    style: ResolvedTextStyle,
    pub content_bounds: Rect,
    pub bounds: Rect,
}

impl TextRun {
    pub fn draw(&self, canvas: &Canvas) {
        draw_text_blob(canvas, &self.blob, self.baseline, &self.style);
    }
}

/// Static gauge text such as units and min/max labels.
pub(crate) fn prepare_text(
    text: &str,
    metrics_text: &str,
    style: ResolvedTextStyle,
    fonts: &dyn FontResolver,
) -> WidgetResult<TextRun> {
    prepare_run(text, metrics_text, style, fonts, Numerals::Default)
}

/// The changing gauge readout, laid out in the style's numerals.
fn prepare_value_text(
    text: &str,
    metrics_text: &str,
    style: ResolvedTextStyle,
    fonts: &dyn FontResolver,
) -> WidgetResult<TextRun> {
    let numerals = style.numerals;
    prepare_run(text, metrics_text, style, fonts, numerals)
}

fn prepare_run(
    text: &str,
    metrics_text: &str,
    style: ResolvedTextStyle,
    fonts: &dyn FontResolver,
    numerals: Numerals,
) -> WidgetResult<TextRun> {
    let font =
        crate::render::text::resolve_font(fonts, style.font_name.as_deref(), style.font_size)?;
    let text = layout_text(text, &font, 0.0, numerals);
    let (_, vertical) = font.measure_str(metrics_text, None);
    let baseline = if vertical.height() <= f32::EPSILON {
        baseline_for_top_with_line_height(style.y, &font, style.line_height)
    } else {
        style.y + (style.line_height - vertical.height()) * 0.5 - vertical.top
    };
    let ink = text.measurement;
    let mut content_bounds = Rect::new(
        style.x + ink.bounds_left,
        baseline + ink.bounds_top,
        style.x + ink.bounds_right,
        baseline + ink.bounds_bottom,
    );
    content_bounds.outset((style.border_thickness * 0.5, style.border_thickness * 0.5));
    let mut bounds = content_bounds;
    if style.shadow_color.is_some() && style.shadow_strength > 0.0 {
        let mut shadow = bounds;
        shadow.outset((style.shadow_strength * 3.0, style.shadow_strength * 3.0));
        shadow.offset((style.shadow_distance, style.shadow_distance));
        bounds.join(shadow);
    }
    Ok(TextRun {
        blob: text
            .blob
            .ok_or_else(|| WidgetError::Render("unable to lay out gauge text".into()))?,
        baseline,
        style,
        content_bounds,
        bounds,
    })
}

pub(crate) fn raster_bounds(mut bounds: Rect) -> Rect {
    bounds.outset((2.0, 2.0));
    Rect::new(
        bounds.left.floor(),
        bounds.top.floor(),
        bounds.right.ceil(),
        bounds.bottom.ceil(),
    )
}

impl GaugeLayout {
    pub fn new(
        widget: &ValidatedGauge,
        scene: &crate::model::scene::ValidatedScenePresentation,
        range: (f64, f64),
        altitude_offset_m: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        match widget {
            ValidatedGauge::Linear(widget) => {
                Ok(Self::Linear(super::linear::prepare_linear_gauge_cache(
                    widget,
                    scene,
                    range,
                    altitude_offset_m,
                    fonts,
                )?))
            }
            ValidatedGauge::Arc(config) => Ok(Self::Arc(super::arc::prepare_arc_gauge_cache(
                config,
                scene,
                range,
                altitude_offset_m,
                widget.unit_text().as_deref(),
                fonts,
            )?)),
        }
    }

    pub fn prepare_frame(
        &self,
        state: GaugeFrameState,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<GaugeFrame> {
        let (mut content_bounds, mut bounds, content_width) = match self {
            Self::Linear(cache) => (cache.content_bounds, cache.raster_bounds(), cache.width),
            Self::Arc(cache) => (cache.content_bounds, cache.raster_bounds(), cache.width),
        };
        let value = if let Self::Arc(cache) = self {
            let layout = cache.inner_layout();
            let mut style = cache.text_style.clone();
            let advance =
                measure_text_with_font(&state.value_text, &cache.value_font, style.numerals).width;
            // Advance-centering preserves the native monospaced readout's anchor.
            style.x = layout.center_x - advance * 0.5;
            style.y = layout.value_top;
            let run = prepare_value_text(
                &state.value_text,
                metric_vertical_metrics_text(&state.value_text),
                style,
                fonts,
            )?;
            content_bounds.join(run.content_bounds);
            bounds.join(run.bounds);
            Some(run)
        } else {
            None
        };
        Ok(GaugeFrame {
            bounds: raster_bounds(bounds),
            content_bounds,
            content_width,
            state,
            value,
        })
    }
}

impl GaugeFrame {
    pub fn draw(&self, canvas: &Canvas, layout: &GaugeLayout) {
        match layout {
            GaugeLayout::Linear(cache) => cache.draw(canvas, self.state.fill01),
            GaugeLayout::Arc(cache) => cache.draw(canvas, self.state.fill01),
        }
        if let Some(value) = &self.value {
            value.draw(canvas);
        }
    }
}
