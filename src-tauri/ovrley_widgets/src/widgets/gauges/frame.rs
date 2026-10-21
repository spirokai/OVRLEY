//! Prepared geometry is the source for both painting and editor bounds.
use super::{
    arc::{inner_widget::ArcInnerWidgetLayout, measure_arc_gauge, ArcGaugeCache, ArcGaugeMeasure},
    linear::{measure_linear_gauge, LinearGaugeCache, LinearGaugeMeasure},
    metric::GaugeFrameState,
    validation::ValidatedGauge,
};
use crate::{
    error::{WidgetError, WidgetResult},
    render::text::{
        baseline_for_top_with_line_height, draw_text_blob, layout_text, FontResolver,
        ResolvedTextStyle,
    },
    widgets::value::layout::metric_vertical_metrics_text,
};
use skia_safe::{Canvas, Font, Rect, TextBlob};

#[derive(Clone, Debug)]
pub enum GaugeLayout {
    Linear(LinearGaugeCache),
    Arc(ArcGaugeCache),
}

/// Layout-only gauge measurement: frame bounds without the cached static raster.
pub(crate) enum GaugeMeasure {
    Linear(LinearGaugeMeasure),
    Arc(ArcGaugeMeasure),
}

/// Inputs of a gauge frame's bounds, from a cache or a layout-only measure.
pub(crate) struct GaugeFrameLayout<'a> {
    pub(crate) content_bounds: Rect,
    pub(crate) raster_bounds: Rect,
    pub(crate) content_width: f32,
    /// The arc readout is laid out per frame; linear gauges have no value text.
    pub(crate) value: Option<ArcValueLayout<'a>>,
}

pub(crate) struct ArcValueLayout<'a> {
    pub(crate) inner: ArcInnerWidgetLayout,
    pub(crate) style: &'a ResolvedTextStyle,
    pub(crate) font: &'a Font,
}

impl GaugeFrameLayout<'_> {
    /// One bounds formula for cached and layout-only frames.
    fn frame(self, state: GaugeFrameState, fonts: &dyn FontResolver) -> WidgetResult<GaugeFrame> {
        let (mut content_bounds, mut bounds) = (self.content_bounds, self.raster_bounds);
        let value = if let Some(value) = self.value {
            let mut style = value.style.clone();
            let (advance, _) = value.font.measure_str(&state.value_text, None);
            // Advance-centering preserves the native monospaced readout's anchor.
            style.x = value.inner.center_x - advance * 0.5;
            style.y = value.inner.value_top;
            let run = prepare_text(
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
            content_width: self.content_width,
            state,
            value,
        })
    }
}

impl GaugeMeasure {
    pub(crate) fn new(
        widget: &ValidatedGauge,
        scene: &crate::model::scene::ValidatedScenePresentation,
        range: (f64, f64),
        altitude_offset_m: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        match widget {
            ValidatedGauge::Linear(gauge) => Ok(Self::Linear(measure_linear_gauge(
                gauge,
                scene,
                range,
                altitude_offset_m,
                fonts,
            )?)),
            ValidatedGauge::Arc(gauge) => Ok(Self::Arc(measure_arc_gauge(
                gauge,
                scene,
                range,
                altitude_offset_m,
                widget.unit_text().as_deref(),
                fonts,
            )?)),
        }
    }

    pub(crate) fn frame_bounds(
        &self,
        state: GaugeFrameState,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Rect> {
        let layout = match self {
            Self::Linear(measure) => measure.frame_layout(),
            Self::Arc(measure) => measure.frame_layout(),
        };
        Ok(layout.frame(state, fonts)?.bounds)
    }
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

pub(crate) fn prepare_text(
    text: &str,
    metrics_text: &str,
    style: ResolvedTextStyle,
    fonts: &dyn FontResolver,
) -> WidgetResult<TextRun> {
    let font =
        crate::render::text::resolve_font(fonts, style.font_name.as_deref(), style.font_size)?;
    let text = layout_text(text, &font, 0.0);
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
        match self {
            Self::Linear(cache) => cache.frame_layout(),
            Self::Arc(cache) => cache.frame_layout(),
        }
        .frame(state, fonts)
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
