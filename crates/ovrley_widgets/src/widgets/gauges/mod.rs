//! Linear, arc, corner and GT gauges: validation, sampling, text layout, preview and resize.
use crate::activity::ActivitySampler;
use crate::model::catalog::{catalog, DisplayType, MetricKind};
use crate::model::scene::{
    validate_scene_presentation, Numerals, ScenePresentationConfig, ValidatedScenePresentation,
};
use crate::model::validation::require_positive_f32;
use crate::preview::ResizeGesture;
use crate::preview::WidgetHost;
use crate::render::text::{
    baseline_for_top_with_line_height, draw_text_blob, layout_text, measure_text_with_font,
    FontResolver, ResolvedTextStyle,
};
use crate::render::tile::PreviewRasterScale;
use crate::render::tile::{Presented, Tile};
use crate::render::tile::{TileBounds, SHAPE_COLOR_TYPE};
use crate::widgets::gauges::arc::{
    measure_arc_gauge, validate_arc_gauge, validate_corner_gauge, validate_gt_gauge, ArcGaugeCache,
    ArcGaugeMeasure, ArcInnerWidgetLayout, ValidatedArcGaugeWidget,
};
use crate::widgets::gauges::linear::{
    measure_linear_gauge, validate_linear_gauge, LinearGaugeCache, LinearGaugeMeasure,
    LinearOrientation, ValidatedLinearGaugeWidget,
};
use crate::widgets::gauges::track::{
    resolve_arc_track_geometry, resolve_bar_style_geometry, track_corner_radius_max,
};
use crate::widgets::value::format::{convert_standard_metric_value, format_metric_parts};
use crate::widgets::value::layout::metric_vertical_metrics_text;
use crate::widgets::value::ValueConfig;
use crate::{WidgetError, WidgetResult};
use serde::Serialize;
use skia_safe::{Canvas, Font, Rect, TextBlob};
use std::collections::BTreeMap;

pub mod arc;
pub mod linear;
pub mod track;

/// A validated linear, arc, corner or GT gauge.
#[derive(Clone, Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "built once per widget at validation, never stored in bulk"
)]
pub enum ValidatedGauge {
    Linear(ValidatedLinearGaugeWidget),
    Arc(ValidatedArcGaugeWidget),
}

impl ValidatedGauge {
    pub fn validate(widget: ValueConfig, index: usize) -> WidgetResult<Self> {
        match widget.display_type {
            DisplayType::Linear => Ok(Self::Linear(validate_linear_gauge(widget, index)?)),
            DisplayType::Arc => Ok(Self::Arc(validate_arc_gauge(widget, index)?)),
            DisplayType::Corner => Ok(Self::Arc(validate_corner_gauge(widget, index)?)),
            DisplayType::GtGauge => Ok(Self::Arc(validate_gt_gauge(widget, index)?)),
            _ => Err(WidgetError::Config("unsupported gauge display type".into())),
        }
    }

    pub fn display_type(&self) -> DisplayType {
        match self {
            Self::Linear(_) => DisplayType::Linear,
            Self::Arc(widget) => widget.display_type,
        }
    }

    pub fn metric(&self) -> MetricKind {
        match self {
            Self::Linear(widget) => widget.metric,
            Self::Arc(widget) => widget.metric,
        }
    }

    pub fn starting_altitude_m(&self) -> Option<f64> {
        match self {
            Self::Linear(widget) => widget.starting_altitude_m,
            Self::Arc(widget) => widget.inner_value.starting_altitude_m,
        }
    }

    pub fn unit_text(&self) -> Option<String> {
        match self {
            Self::Linear(_) => None,
            Self::Arc(widget) => format_metric_parts(&widget.inner_value, &Default::default())
                .standard_text()
                .1
                .map(str::to_owned),
        }
    }
}

fn validate_supported_metric(metric: MetricKind, display: DisplayType) -> WidgetResult<()> {
    if !catalog().supports_display(metric, display) {
        return Err(WidgetError::Config(
            "unsupported gauge metric/display type".into(),
        ));
    }
    Ok(())
}

fn validate_label_font(font: Option<String>, field: &str) -> WidgetResult<String> {
    let font = crate::model::validation::require_string(font, field)?;
    if font.trim().is_empty() {
        return Err(WidgetError::Config(format!("{field}: must not be empty")));
    }
    Ok(font)
}

/// Missing or constant external telemetry uses the existing neutral gauge range.
pub fn metric_range(range: Option<(f64, f64)>) -> (f64, f64) {
    range.filter(|(min, max)| max > min).unwrap_or((0.0, 100.0))
}

pub fn fill_percentage(value: f64, min: f64, max: f64) -> f32 {
    if max <= min {
        return 0.0;
    }
    ((value - min) / (max - min)).clamp(0.0, 1.0) as f32
}

pub fn bar_fill_count(fill01: f32, count: u32) -> usize {
    (fill01.clamp(0.0, 1.0) * count as f32).floor() as usize
}

pub(crate) fn format_gauge_label(
    kind: MetricKind,
    display_unit: Option<&str>,
    value: f64,
) -> String {
    (convert_standard_metric_value(kind, display_unit, value).round() as i64).to_string()
}

#[derive(Clone, Debug, PartialEq)]
pub struct GaugeFrameState {
    pub fill01: f32,
    pub value_text: String,
}

pub fn sample_gauge(
    widget: &ValidatedGauge,
    activity: Option<&crate::activity::ActivitySampler>,
    second: f64,
) -> GaugeFrameState {
    let range = activity.map_or((0.0, 100.0), |activity| {
        activity.gauge_range(widget.metric())
    });
    let sample = activity
        .map(|activity| activity.gauge_sample(widget.metric(), second))
        .unwrap_or_default();
    let offset = activity.map_or(0.0, |activity| {
        activity.gauge_altitude_offset(widget.starting_altitude_m())
    });
    gauge_frame_state(widget, sample, range, offset)
}

/// Both hosts derive gauge fill and inner text from the same canonical sample.
pub fn gauge_frame_state(
    widget: &ValidatedGauge,
    mut sample: crate::widgets::value::format::MetricSample<'_>,
    range: (f64, f64),
    altitude_offset_m: f64,
) -> GaugeFrameState {
    use crate::widgets::value::format::{format_metric_parts, resolve_metric_display_value};
    let value = resolve_metric_display_value(widget.metric(), sample.numeric, sample.available)
        .unwrap_or(range.0);
    let value_text = match widget {
        crate::widgets::gauges::ValidatedGauge::Linear(_) => String::new(),
        crate::widgets::gauges::ValidatedGauge::Arc(config) => {
            sample.numeric = sample.numeric.map(|value| value + altitude_offset_m);
            format_metric_parts(&config.inner_value, &sample)
                .standard_text()
                .0
                .to_owned()
        }
    };
    GaugeFrameState {
        fill01: fill_percentage(value, range.0, range.1),
        value_text,
    }
}

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
            let advance =
                measure_text_with_font(&state.value_text, value.font, style.numerals).width;
            // Advance-centering preserves the native monospaced readout's anchor.
            style.x = value.inner.center_x - advance * 0.5;
            style.y = value.inner.value_top;
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
        raster_scale: crate::render::tile::PreviewRasterScale,
    ) -> WidgetResult<Self> {
        match widget {
            ValidatedGauge::Linear(widget) => Ok(Self::Linear(
                crate::widgets::gauges::linear::prepare_linear_gauge_cache(
                    widget,
                    scene,
                    range,
                    altitude_offset_m,
                    fonts,
                    raster_scale,
                )?,
            )),
            ValidatedGauge::Arc(config) => Ok(Self::Arc(
                crate::widgets::gauges::arc::prepare_arc_gauge_cache(
                    config,
                    scene,
                    range,
                    altitude_offset_m,
                    widget.unit_text().as_deref(),
                    fonts,
                    raster_scale,
                )?,
            )),
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

#[derive(Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
    pub display_variants: BTreeMap<String, ResizeVariant>,
}

#[derive(Serialize)]
pub struct ResizeVariant {
    pub width: f32,
    pub height: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_thickness: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_corner_radius: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_border_thickness: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inner_widget_offset_x: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inner_widget_offset_y: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_max_label_font_size: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bar_gap: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tick_length: Option<f32>,
}

pub fn resize_gauge(
    widget: ValueConfig,
    scene: ScenePresentationConfig,
    factor: Option<f32>,
    size: Option<[f32; 2]>,
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
    let mut widget = ValidatedGauge::validate(widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    let range = activity.map_or((0.0, 100.0), |activity| {
        activity.gauge_range(widget.metric())
    });
    let offset = activity.map_or(0.0, |activity| {
        activity.gauge_altitude_offset(widget.starting_altitude_m())
    });
    let origin = GaugeLayout::new(
        &widget,
        &scene,
        range,
        offset,
        fonts,
        PreviewRasterScale::NATIVE,
    )?
    .prepare_frame(sample_gauge(&widget, activity, second), fonts)?
    .content_bounds;
    if origin.is_empty() {
        return Err(WidgetError::Render(
            "gauge has no visible content to resize".into(),
        ));
    }
    let (x, y, display_type, font_size, variant) =
        match &mut widget {
            ValidatedGauge::Linear(config) => {
                if factor.is_some() {
                    return Err(WidgetError::Config(
                        "linear gauge resize requires size".into(),
                    ));
                }
                let size = size.ok_or_else(|| {
                    WidgetError::Config("linear gauge resize size is required".into())
                })?;
                if size.iter().any(|value| !value.is_finite() || *value <= 0.0) {
                    return Err(WidgetError::Config("invalid gauge resize size".into()));
                }
                config.width =
                    (config.width + (size[0] - origin.width()) / scene.scale).clamp(8.0, 10_000.0);
                config.height = (config.height + (size[1] - origin.height()) / scene.scale)
                    .clamp(8.0, 10_000.0);
                let (span, cross_extent) = match config.orientation {
                    LinearOrientation::Horizontal => (config.width, config.height),
                    LinearOrientation::Vertical => (config.height, config.width),
                };
                if let Some(bars) = config.bar_geometry {
                    config.bar_geometry = resolve_bar_style_geometry(
                        config.track_fill_style,
                        span,
                        Some(bars.count),
                        Some(bars.gap),
                        "linear resize",
                    )?;
                }
                config.track_corner_radius = config.track_corner_radius.min(
                    track_corner_radius_max(cross_extent, span, config.bar_geometry.as_ref()),
                );
                (
                    config.x,
                    config.y,
                    "linear",
                    None,
                    ResizeVariant {
                        width: config.width,
                        height: config.height,
                        track_thickness: None,
                        track_corner_radius: Some(config.track_corner_radius),
                        track_border_thickness: None,
                        inner_widget_offset_x: None,
                        inner_widget_offset_y: None,
                        min_max_label_font_size: None,
                        bar_gap: config.bar_geometry.map(|bars| bars.gap),
                        tick_length: None,
                    },
                )
            }
            ValidatedGauge::Arc(config) => {
                if size.is_some() {
                    return Err(WidgetError::Config(
                        "arc gauge resize requires factor".into(),
                    ));
                }
                let factor = require_positive_f32(factor, "gauge resize factor")?;
                config.width = (config.width * factor).clamp(8.0, 10_000.0);
                config.height = (config.height * factor).clamp(8.0, 10_000.0);
                config.inner_value.font_size =
                    (config.inner_value.font_size * factor).clamp(8.0, 400.0);
                config.track_thickness = (config.track_thickness * factor).clamp(1.0, 100.0);
                config.track_corner_radius =
                    (config.track_corner_radius * factor).clamp(0.0, config.track_thickness * 0.5);
                config.track_border_thickness =
                    (config.track_border_thickness * factor).clamp(0.0, 24.0);
                config.inner_widget_offset_x =
                    (config.inner_widget_offset_x * factor).clamp(-10_000.0, 10_000.0);
                config.inner_widget_offset_y =
                    (config.inner_widget_offset_y * factor).clamp(-10_000.0, 10_000.0);
                config.min_max_label_font_size =
                    (config.min_max_label_font_size * factor).clamp(6.0, 50.0);
                if let Some(style) = config.gt_style.as_mut() {
                    style.tick_length = (style.tick_length * factor).clamp(0.0, 100.0);
                }
                let bars = config.bar_geometry;
                (config.track_corner_radius, config.bar_geometry) = resolve_arc_track_geometry(
                    config.width.min(config.height),
                    config.track_thickness,
                    config.track_corner_radius,
                    config.track_border_thickness,
                    config.arc_angle,
                    config.corner_orientation.is_some(),
                    config.track_fill_style,
                    bars.map(|bars| bars.count),
                    bars.map(|bars| bars.gap),
                    "arc resize",
                )?;
                (
                    config.x,
                    config.y,
                    config.display_type.as_str(),
                    Some(config.inner_value.font_size),
                    ResizeVariant {
                        width: config.width,
                        height: config.height,
                        track_thickness: Some(config.track_thickness),
                        track_corner_radius: Some(config.track_corner_radius),
                        track_border_thickness: Some(config.track_border_thickness),
                        inner_widget_offset_x: Some(config.inner_widget_offset_x),
                        inner_widget_offset_y: Some(config.inner_widget_offset_y),
                        min_max_label_font_size: Some(config.min_max_label_font_size),
                        bar_gap: config.bar_geometry.map(|bars| bars.gap),
                        tick_length: config.gt_style.as_ref().map(|style| style.tick_length),
                    },
                )
            }
        };
    let display_type = display_type.to_owned();
    let bounds = GaugeLayout::new(
        &widget,
        &scene,
        range,
        offset,
        fonts,
        PreviewRasterScale::NATIVE,
    )?
    .prepare_frame(sample_gauge(&widget, activity, second), fonts)?
    .content_bounds;
    Ok(ResizeUpdate {
        x: x + point[0] - (bounds.left + bounds.width() * fixed[0]),
        y: y + point[1] - (bounds.top + bounds.height() * fixed[1]),
        font_size,
        display_variants: BTreeMap::from([(display_type, variant)]),
    })
}

pub struct PreviewGauge {
    widget: ValidatedGauge,
    scene: ValidatedScenePresentation,
    preview_raster_scale: PreviewRasterScale,
    layout: Option<GaugeLayout>,
    /// Layout-only measurement for bounds requested before the layout is drawn.
    measure: Option<GaugeMeasure>,
    last_state: Option<GaugeFrameState>,
    pub tile: Tile,
}

impl PreviewGauge {
    pub fn new(
        widget: ValueConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
    ) -> WidgetResult<Self> {
        let widget = ValidatedGauge::validate(widget, 0)?;
        let host = WidgetHost::from_scene(&scene, raster_scale)?;
        Ok(Self::from_validated(
            widget,
            validate_scene_presentation(scene)?,
            &host,
        ))
    }

    pub fn from_validated(
        widget: ValidatedGauge,
        scene: ValidatedScenePresentation,
        host: &WidgetHost,
    ) -> Self {
        Self {
            widget,
            scene,
            preview_raster_scale: host.raster_scale,
            layout: None,
            measure: None,
            last_state: None,
            tile: Tile::default(),
        }
    }

    /// The state the frame at `second` draws; equal states draw identical pixels.
    pub fn frame_key(&self, activity: Option<&ActivitySampler>, second: f64) -> GaugeFrameState {
        sample_gauge(&self.widget, activity, second)
    }
    pub fn invalidate_activity(&mut self) {
        self.layout = None;
        self.measure = None;
        self.last_state = None;
    }
    /// Range labels follow the activity's metric range and altitude correction.
    fn activity_range(&self, activity: Option<&ActivitySampler>) -> ((f64, f64), f64) {
        let range = activity.map_or((0.0, 100.0), |activity| {
            activity.gauge_range(self.widget.metric())
        });
        let offset = activity.map_or(0.0, |activity| {
            activity.gauge_altitude_offset(self.widget.starting_altitude_m())
        });
        (range, offset)
    }

    fn layout(
        &mut self,
        activity: Option<&ActivitySampler>,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<&GaugeLayout> {
        if self.layout.is_none() {
            let (range, offset) = self.activity_range(activity);
            self.layout = Some(GaugeLayout::new(
                &self.widget,
                &self.scene,
                range,
                offset,
                fonts,
                self.preview_raster_scale,
            )?);
        }
        Ok(self.layout.as_ref().expect("gauge layout prepared"))
    }

    /// Scene rectangle a render at `second` draws into, without drawing the static layer.
    pub fn layout_bounds(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let state = sample_gauge(&self.widget, activity, second);
        if let Some(layout) = &self.layout {
            return Ok(layout.prepare_frame(state, fonts)?.bounds.into());
        }
        if self.measure.is_none() {
            let (range, offset) = self.activity_range(activity);
            self.measure = Some(GaugeMeasure::new(
                &self.widget,
                &self.scene,
                range,
                offset,
                fonts,
            )?);
        }
        let measure = self.measure.as_ref().expect("gauge measure prepared");
        Ok(measure.frame_bounds(state, fonts)?.into())
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(
        &mut self,
        canvas: &Canvas,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let state = sample_gauge(&self.widget, activity, second);
        let layout = self.layout(activity, fonts)?;
        let frame = layout.prepare_frame(state, fonts)?;
        frame.draw(canvas, layout);
        Ok(frame.bounds.into())
    }

    pub fn render(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<()> {
        self.layout(activity, fonts)?;
        let state = sample_gauge(&self.widget, activity, second);
        // A continuous fill can change even while the formatted integer stays unchanged.
        if self.last_state.as_ref() == Some(&state) {
            return Ok(());
        }
        let layout = self.layout.as_ref().expect("gauge layout prepared");
        let frame = layout.prepare_frame(state, fonts)?;
        let bounds = frame.bounds;
        self.tile.render(
            self.preview_raster_scale,
            SHAPE_COLOR_TYPE,
            bounds,
            Presented::new(frame.content_bounds, frame.content_width),
            |canvas| {
                frame.draw(canvas, layout);
                Ok(())
            },
        )?;
        self.last_state = Some(frame.state);
        Ok(())
    }
}
