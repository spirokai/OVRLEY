//! Family dispatch at the preview ingress. `type` is the editor's canonical widget type.
use crate::{activity::ActivitySampler, error::WidgetResult, host::WidgetHost, model::{scene::{ScenePresentationConfig, ValidatedScenePresentation}, types::MetricKind}, render::{image::{ImageSource, RasterImage}, text::FontResolver}, widgets::{label::{config::LabelConfig, preview::PreviewLabel, validation::ValidatedLabel}, value::{config::ValueConfig, format::MetricDisplayParts, presentation::ValuePresentation, preview::{PreviewWidget as ValuePreview, TileBounds, WidgetRequest as ValueRequest}, validation::ValidatedValue}}};
use crate::widgets::{backdrop::validation::ValidatedBackdrop, diving_profile::{geometry::DivingFrameState, validation::ValidatedDivingProfile}, elevation::{geometry::ElevationFrameState, validation::ValidatedElevationPlot}, g_force::format::GForceSample, gauges::{metric::GaugeFrameState, validation::ValidatedGauge}, lap_timer::format::LapTimerContent, raster::validation::RasterGeometry, route::validation::ValidatedRoutePlot};
use serde::{Deserialize, Deserializer, Serialize};
use skia_safe::Canvas;
use crate::widgets::backdrop::{config::BackdropConfig, preview::PreviewBackdrop};

use crate::widgets::raster::{config::RasterConfig, preview::PreviewRaster};
use crate::widgets::gradient::preview::PreviewGradient;
use crate::widgets::lean_angle::preview::PreviewLeanAngle;
use crate::widgets::g_force::preview::PreviewGForce;
use crate::widgets::lap_timer::preview::PreviewLapTimer;
use crate::widgets::heading::preview::PreviewHeading;
use crate::widgets::gauges::preview::PreviewGauge;
use crate::widgets::route::{config::CoursePlotConfig, preview::PreviewRoute};
use crate::widgets::elevation::{config::ElevationPlotConfig, preview::PreviewElevation};
use crate::widgets::diving_profile::{config::DivingProfileConfig, preview::PreviewDivingProfile};

/// A validated widget handed to the engine by a host other than the editor preview.
/// Rasters carry the image the host resolved and prepared for its raster scale.
pub enum ValidatedWidget<'a> {
    DivingProfile(&'a ValidatedDivingProfile),
    Elevation(&'a ValidatedElevationPlot),
    Route(&'a ValidatedRoutePlot),
    Raster { geometry: &'a RasterGeometry, source_id: String, image: RasterImage },
    Backdrop(&'a ValidatedBackdrop),
    Label(&'a ValidatedLabel),
    Value(&'a ValidatedValue),
}

/// Per-family sampling result a frame is drawn from; static families always draw the same.
#[derive(Clone, PartialEq)]
pub enum FrameKey {
    Static,
    Sample(Option<f64>),
    GForce(GForceSample),
    Gauge(GaugeFrameState),
    LapTimer(LapTimerContent),
    Parts(MetricDisplayParts),
    /// Retained geometry identity and marker progress.
    Route(usize, f32),
    /// Retained geometry identity and frame state.
    Elevation(usize, ElevationFrameState),
    DivingProfile(DivingFrameState),
}

#[derive(Clone, PartialEq)]
pub enum WidgetRequest {
    DivingProfile { widget: DivingProfileConfig, scene: ScenePresentationConfig, transient: bool },
    Elevation { widget: ElevationPlotConfig, scene: ScenePresentationConfig, transient: bool },
    Route { widget: CoursePlotConfig, scene: ScenePresentationConfig, transient: bool },
    Raster { widget: RasterConfig, scene: ScenePresentationConfig, transient: bool },
    Backdrop { widget: BackdropConfig, scene: ScenePresentationConfig, transient: bool },
    Label { widget: LabelConfig, scene: ScenePresentationConfig, transient: bool },
    Gradient(ValueRequest),
    LeanAngle(ValueRequest),
    GForce(ValueRequest),
    LapTimer(ValueRequest),
    HeadingTape(ValueRequest),
    Gauge(ValueRequest),
    Value(ValueRequest),
}

impl<'de> Deserialize<'de> for WidgetRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Input {
            r#type: String,
            widget: serde_json::Value,
            scene: ScenePresentationConfig,
            /// Preview-only drag/resize draft: retain cached pixels/vectors until mouseup.
            #[serde(default)]
            transient: bool,
        }
        let input = Input::deserialize(deserializer)?;
        if input.r#type == "diving_profile" {
            Ok(Self::DivingProfile { widget: serde_json::from_value(input.widget).map_err(serde::de::Error::custom)?, scene: input.scene, transient: input.transient })
        } else if input.r#type == "course" {
            Ok(Self::Route { widget: serde_json::from_value(input.widget).map_err(serde::de::Error::custom)?, scene: input.scene, transient: input.transient })
        } else if input.r#type == "elevation" {
            Ok(Self::Elevation { widget: serde_json::from_value(input.widget).map_err(serde::de::Error::custom)?, scene: input.scene, transient: input.transient })
        } else if input.r#type == "label" {
            Ok(Self::Label { widget: serde_json::from_value(input.widget).map_err(serde::de::Error::custom)?, scene: input.scene, transient: input.transient })
        } else if input.r#type == "backdrop" {
            Ok(Self::Backdrop { widget: serde_json::from_value(input.widget).map_err(serde::de::Error::custom)?, scene: input.scene, transient: input.transient })
        } else if input.r#type == "raster" {
            Ok(Self::Raster { widget: serde_json::from_value(input.widget).map_err(serde::de::Error::custom)?, scene: input.scene, transient: input.transient })
        } else {
            let metric: MetricKind = serde_json::from_value(input.r#type.into()).map_err(serde::de::Error::custom)?;
            let widget: ValueConfig = serde_json::from_value(input.widget).map_err(serde::de::Error::custom)?;
            if widget.value != metric {
                return Err(serde::de::Error::custom("widget type must match its value metric"));
            }
            let request = ValueRequest { widget, scene: input.scene, transient: input.transient };
            Ok(if metric == MetricKind::Gradient { Self::Gradient(request) }
                else if metric == MetricKind::LapTimer { Self::LapTimer(request) }
                else if metric == MetricKind::LeanAngle && request.widget.display_type == crate::model::types::DisplayType::LeanAngle { Self::LeanAngle(request) }
                else if metric == MetricKind::GForce && request.widget.display_type == crate::model::types::DisplayType::GForce { Self::GForce(request) }
                else if metric == MetricKind::Heading && request.widget.display_type == crate::model::types::DisplayType::Tape { Self::HeadingTape(request) }
                else if matches!(request.widget.display_type, crate::model::types::DisplayType::Linear | crate::model::types::DisplayType::Arc | crate::model::types::DisplayType::Corner) { Self::Gauge(request) }
                else { Self::Value(request) })
        }
    }
}

pub enum PreviewWidget {
    DivingProfile(PreviewDivingProfile),
    Elevation(PreviewElevation),
    Route(PreviewRoute),
    Raster(PreviewRaster),
    Backdrop(PreviewBackdrop),
    Label(PreviewLabel),
    Gradient(PreviewGradient),
    LeanAngle(PreviewLeanAngle),
    GForce(PreviewGForce),
    LapTimer(PreviewLapTimer),
    HeadingTape(PreviewHeading),
    Gauge(PreviewGauge),
    Value(ValuePreview),
}

impl PreviewWidget {
    pub fn new(request: WidgetRequest, fonts: &dyn FontResolver, images: &(impl ImageSource + ?Sized)) -> WidgetResult<Self> {
        match request {
            WidgetRequest::DivingProfile {widget, scene, ..} => Ok(Self::DivingProfile(PreviewDivingProfile::new(widget, scene)?)),
            WidgetRequest::Elevation {widget, scene, ..} => Ok(Self::Elevation(PreviewElevation::new(widget, scene)?)),
            WidgetRequest::Route {widget, scene, ..} => Ok(Self::Route(PreviewRoute::new(widget, scene)?)),
            WidgetRequest::Raster {widget, scene, ..} => Ok(Self::Raster(PreviewRaster::new(widget, scene, images)?)),
            WidgetRequest::Backdrop {widget, scene, ..} => Ok(Self::Backdrop(PreviewBackdrop::new(widget, scene)?)),
            WidgetRequest::Label {widget, scene, ..} => Ok(Self::Label(PreviewLabel::new(widget, scene, fonts)?)),
            WidgetRequest::Gradient(request) => Ok(Self::Gradient(PreviewGradient::new(request.widget, request.scene)?)),
            WidgetRequest::LeanAngle(request) => Ok(Self::LeanAngle(PreviewLeanAngle::new(request.widget, request.scene)?)),
            WidgetRequest::GForce(request) => Ok(Self::GForce(PreviewGForce::new(request.widget, request.scene)?)),
            WidgetRequest::LapTimer(request) => Ok(Self::LapTimer(PreviewLapTimer::new(request.widget, request.scene)?)),
            WidgetRequest::HeadingTape(request) => Ok(Self::HeadingTape(PreviewHeading::new(request.widget, request.scene, fonts)?)),
            WidgetRequest::Gauge(request) => Ok(Self::Gauge(PreviewGauge::new(request.widget, request.scene)?)),
            WidgetRequest::Value(request) => Ok(Self::Value(ValuePreview::new(request, fonts)?)),
        }
    }
    /// Builds a validated widget for a host other than the editor preview.
    pub fn from_validated(widget: ValidatedWidget<'_>, scene: &ValidatedScenePresentation, host: &WidgetHost, fonts: &dyn FontResolver) -> WidgetResult<Self> {
        Ok(match widget {
            ValidatedWidget::DivingProfile(widget) => Self::DivingProfile(PreviewDivingProfile::from_validated(widget.clone(), scene.clone(), host)?),
            ValidatedWidget::Elevation(widget) => Self::Elevation(PreviewElevation::from_validated(widget.clone(), scene.clone(), host)?),
            ValidatedWidget::Route(widget) => Self::Route(PreviewRoute::from_validated(widget.clone(), scene.clone(), host)?),
            ValidatedWidget::Raster { geometry, source_id, image } => Self::Raster(PreviewRaster::from_validated(geometry, scene, source_id, image)?),
            ValidatedWidget::Backdrop(widget) => Self::Backdrop(PreviewBackdrop::from_validated(widget.clone(), scene, host)?),
            ValidatedWidget::Label(widget) => Self::Label(PreviewLabel::from_validated(widget, scene, host, fonts)?),
            ValidatedWidget::Value(value) => match value {
                ValidatedValue::StandardText(widget) => Self::Value(ValuePreview::from_validated(ValuePresentation::Metric(widget.clone()), scene, host, fonts)?),
                ValidatedValue::TimeText(widget) => Self::Value(ValuePreview::from_validated(ValuePresentation::time(widget.clone(), host)?, scene, host, fonts)?),
                ValidatedValue::Gradient(widget) => Self::Gradient(PreviewGradient::from_validated(widget.clone(), scene, host)),
                ValidatedValue::HeadingTape(widget) => Self::HeadingTape(PreviewHeading::from_validated(widget, scene, host, fonts)?),
                ValidatedValue::LeanAngle(widget) => Self::LeanAngle(PreviewLeanAngle::from_validated(widget, scene, host)?),
                ValidatedValue::LinearGauge(widget) => Self::Gauge(PreviewGauge::from_validated(ValidatedGauge::Linear(widget.clone()), scene.clone(), host)),
                ValidatedValue::ArcGauge(widget) => Self::Gauge(PreviewGauge::from_validated(ValidatedGauge::Arc(widget.clone()), scene.clone(), host)),
                ValidatedValue::GForce(widget) => Self::GForce(PreviewGForce::from_validated(widget.clone(), scene, host)?),
                ValidatedValue::LapTimer(widget) => Self::LapTimer(PreviewLapTimer::from_validated(widget.clone(), scene.clone(), host)),
            },
        })
    }
    /// What the frame at `second` depends on, from sampling alone. Equal keys draw identical
    /// pixels, so a host may reuse the tile it drew for the previous key.
    pub fn frame_key(&mut self, activity: Option<&ActivitySampler>, second: f64, fonts: &dyn FontResolver) -> WidgetResult<FrameKey> {
        Ok(match self {
            Self::DivingProfile(widget) => FrameKey::DivingProfile(widget.frame_key(activity, second, fonts)?),
            Self::Elevation(widget) => { let (geometry, state) = widget.frame_key(activity, second, fonts)?; FrameKey::Elevation(geometry, state) }
            Self::Route(widget) => { let (geometry, progress) = widget.frame_key(activity, second)?; FrameKey::Route(geometry, progress) }
            Self::Raster(_) | Self::Backdrop(_) | Self::Label(_) => FrameKey::Static,
            Self::Gradient(widget) => FrameKey::Sample(widget.frame_key(activity, second)),
            Self::LeanAngle(widget) => FrameKey::Sample(widget.frame_key(activity, second)),
            Self::GForce(widget) => FrameKey::GForce(widget.frame_key(activity, second)),
            Self::LapTimer(widget) => FrameKey::LapTimer(widget.frame_key(activity, second)?),
            Self::HeadingTape(widget) => FrameKey::Sample(widget.frame_key(activity, second)),
            Self::Gauge(widget) => FrameKey::Gauge(widget.frame_key(activity, second)),
            Self::Value(widget) => FrameKey::Parts(widget.frame_key(activity, second)),
        })
    }
    pub fn pixels(&self) -> &Vec<u8> {
        match self { Self::DivingProfile(widget) => &widget.pixels, Self::Elevation(widget) => &widget.pixels, Self::Route(widget) => &widget.pixels, Self::Raster(widget) => &widget.pixels, Self::Backdrop(widget) => &widget.pixels, Self::Label(widget) => &widget.pixels, Self::Gradient(widget) => &widget.pixels, Self::LeanAngle(widget) => &widget.pixels, Self::GForce(widget) => &widget.pixels, Self::LapTimer(widget) => &widget.pixels, Self::HeadingTape(widget) => &widget.pixels, Self::Gauge(widget) => &widget.pixels, Self::Value(widget) => &widget.pixels }
    }
    pub fn geometry(&self) -> &Vec<u8> {
        match self { Self::DivingProfile(widget) => &widget.geometry, Self::Elevation(widget) => &widget.geometry, Self::Route(widget) => &widget.geometry, Self::Raster(widget) => &widget.geometry, Self::Backdrop(widget) => &widget.geometry, Self::Label(widget) => &widget.geometry, Self::Gradient(widget) => &widget.geometry, Self::LeanAngle(widget) => &widget.geometry, Self::GForce(widget) => &widget.geometry, Self::LapTimer(widget) => &widget.geometry, Self::HeadingTape(widget) => &widget.geometry, Self::Gauge(widget) => &widget.geometry, Self::Value(widget) => &widget.geometry }
    }
    /// Scene rectangle a render at `second` would draw into, from layout alone: no surface
    /// is allocated and nothing is drawn. Rasters report their rotated scene extent.
    pub fn layout_bounds(&mut self, activity: Option<&ActivitySampler>, second: f64, fonts: &dyn FontResolver) -> WidgetResult<TileBounds> {
        match self {
            Self::DivingProfile(widget) => widget.layout_bounds(activity, fonts),
            Self::Elevation(widget) => widget.layout_bounds(activity, second, fonts),
            Self::Route(widget) => widget.layout_bounds(activity, second),
            Self::Raster(widget) => Ok(widget.layout_bounds()),
            Self::Backdrop(widget) => Ok(widget.layout_bounds()),
            Self::Label(widget) => Ok(widget.layout_bounds()),
            Self::Gradient(widget) => widget.layout_bounds(activity, second, fonts),
            Self::LeanAngle(widget) => widget.layout_bounds(activity, second, fonts),
            Self::GForce(widget) => widget.layout_bounds(activity, second, fonts),
            Self::LapTimer(widget) => widget.layout_bounds(activity, second, fonts),
            Self::HeadingTape(widget) => Ok(widget.layout_bounds(activity, second)),
            Self::Gauge(widget) => widget.layout_bounds(activity, second, fonts),
            Self::Value(widget) => widget.layout_bounds(activity, second, fonts),
        }
    }
    /// Draws the frame at `second` onto a host canvas in scene coordinates at raster scale 1,
    /// returning the rectangle it drew into, which equals `layout_bounds`.
    pub fn draw(&mut self, canvas: &Canvas, activity: Option<&ActivitySampler>, second: f64, fonts: &dyn FontResolver) -> WidgetResult<TileBounds> {
        match self {
            Self::DivingProfile(widget) => widget.draw(canvas, activity, second, fonts),
            Self::Elevation(widget) => widget.draw(canvas, activity, second, fonts),
            Self::Route(widget) => widget.draw(canvas, activity, second),
            Self::Raster(widget) => widget.draw(canvas),
            Self::Backdrop(widget) => Ok(widget.draw(canvas)),
            Self::Label(widget) => widget.draw(canvas),
            Self::Gradient(widget) => widget.draw(canvas, activity, second, fonts),
            Self::LeanAngle(widget) => widget.draw(canvas, activity, second, fonts),
            Self::GForce(widget) => widget.draw(canvas, activity, second, fonts),
            Self::LapTimer(widget) => widget.draw(canvas, activity, second, fonts),
            Self::HeadingTape(widget) => Ok(widget.draw(canvas, activity, second)),
            Self::Gauge(widget) => widget.draw(canvas, activity, second, fonts),
            Self::Value(widget) => widget.draw(canvas, activity, second, fonts),
        }
    }
    /// Scene bounds of the rendered tile, which the serialized geometry is derived from.
    /// Playhead families have none before their first render.
    pub fn bounds(&self) -> Option<TileBounds> {
        match self { Self::DivingProfile(widget) => widget.bounds, Self::Elevation(widget) => widget.bounds, Self::Route(widget) => widget.bounds, Self::Raster(widget) => Some(widget.bounds), Self::Backdrop(widget) => Some(widget.bounds), Self::Label(widget) => Some(widget.bounds), Self::Gradient(widget) => widget.bounds, Self::LeanAngle(widget) => widget.bounds, Self::GForce(widget) => widget.bounds, Self::LapTimer(widget) => widget.bounds, Self::HeadingTape(widget) => widget.bounds, Self::Gauge(widget) => widget.bounds, Self::Value(widget) => widget.bounds }
    }
    pub fn revision(&self) -> u32 {
        match self { Self::DivingProfile(widget) => widget.revision, Self::Elevation(widget) => widget.revision, Self::Route(widget) => widget.revision, Self::Raster(widget) => widget.revision, Self::Backdrop(widget) => widget.revision, Self::Label(widget) => widget.revision, Self::Gradient(widget) => widget.revision, Self::LeanAngle(widget) => widget.revision, Self::GForce(widget) => widget.revision, Self::LapTimer(widget) => widget.revision, Self::HeadingTape(widget) => widget.revision, Self::Gauge(widget) => widget.revision, Self::Value(widget) => widget.revision }
    }
}

#[derive(Deserialize)]
pub struct ResizeRequest {
    #[serde(flatten)]
    pub origin: WidgetRequest,
    pub factor: Option<f32>,
    /// Profile resizing measures the session's prepared vectors rather than rebuilding them.
    pub handle: Option<u32>,
    #[serde(default)]
    pub commit: bool,
    /// Framed families resize to scene-pixel dimensions instead of a uniform factor.
    pub size: Option<[f32; 2]>,
    /// Heading inspector edits resize the frame to preserve tick lengths.
    pub label_offset: Option<f32>,
    pub fixed: [f32; 2],
    pub point: [f32; 2],
    pub second: f64,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum ResizeUpdate {
    DivingProfile(crate::widgets::diving_profile::interaction::ResizeUpdate),
    Elevation(crate::widgets::elevation::interaction::ResizeUpdate),
    Route(crate::widgets::route::interaction::ResizeUpdate),
    Raster(crate::widgets::raster::interaction::ResizeUpdate),
    Backdrop(crate::widgets::backdrop::interaction::ResizeUpdate),
    Label(crate::widgets::label::interaction::ResizeUpdate),
    Gradient(crate::widgets::gradient::interaction::ResizeUpdate),
    LeanAngle(crate::widgets::lean_angle::interaction::ResizeUpdate),
    GForce(crate::widgets::g_force::interaction::ResizeUpdate),
    LapTimer(crate::widgets::lap_timer::interaction::ResizeUpdate),
    HeadingTape(crate::widgets::heading::interaction::ResizeUpdate),
    Gauge(crate::widgets::gauges::interaction::ResizeUpdate),
    Value(crate::widgets::value::interaction::ResizeUpdate),
}
