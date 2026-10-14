//! Family dispatch at the preview ingress. `type` is the editor's canonical widget type.
use crate::{error::WidgetResult, model::{scene::ScenePresentationConfig, types::MetricKind}, preview::PreviewFonts, widgets::{label::{config::LabelConfig, preview::PreviewLabel}, value::{config::ValueConfig, preview::{PreviewWidget as ValuePreview, WidgetRequest as ValueRequest}}}};
use serde::{Deserialize, Deserializer, Serialize};
use crate::widgets::backdrop::{config::BackdropConfig, preview::PreviewBackdrop};

use crate::widgets::raster::{config::RasterConfig, preview::{PreviewRaster, PreviewImages}};
use crate::widgets::gradient::preview::PreviewGradient;
use crate::widgets::lean_angle::preview::PreviewLeanAngle;
use crate::widgets::g_force::preview::PreviewGForce;

pub enum WidgetRequest {
    Raster { widget: RasterConfig, scene: ScenePresentationConfig },
    Backdrop { widget: BackdropConfig, scene: ScenePresentationConfig },
    Label { widget: LabelConfig, scene: ScenePresentationConfig },
    Gradient(ValueRequest),
    LeanAngle(ValueRequest),
    GForce(ValueRequest),
    Value(ValueRequest),
}

impl<'de> Deserialize<'de> for WidgetRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Input {
            r#type: String,
            widget: serde_json::Value,
            scene: ScenePresentationConfig,
        }
        let input = Input::deserialize(deserializer)?;
        if input.r#type == "label" {
            Ok(Self::Label { widget: serde_json::from_value(input.widget).map_err(serde::de::Error::custom)?, scene: input.scene })
        } else if input.r#type == "backdrop" {
            Ok(Self::Backdrop { widget: serde_json::from_value(input.widget).map_err(serde::de::Error::custom)?, scene: input.scene })
        } else if input.r#type == "raster" {
            Ok(Self::Raster { widget: serde_json::from_value(input.widget).map_err(serde::de::Error::custom)?, scene: input.scene })
        } else {
            let metric: MetricKind = serde_json::from_value(input.r#type.into()).map_err(serde::de::Error::custom)?;
            let widget: ValueConfig = serde_json::from_value(input.widget).map_err(serde::de::Error::custom)?;
            if widget.value != metric {
                return Err(serde::de::Error::custom("widget type must match its value metric"));
            }
            let request = ValueRequest { widget, scene: input.scene };
            Ok(if metric == MetricKind::Gradient { Self::Gradient(request) }
                else if metric == MetricKind::LeanAngle && request.widget.display_type == crate::model::types::DisplayType::LeanAngle { Self::LeanAngle(request) }
                else if metric == MetricKind::GForce && request.widget.display_type == crate::model::types::DisplayType::GForce { Self::GForce(request) }
                else { Self::Value(request) })
        }
    }
}

pub enum PreviewWidget {
    Raster(PreviewRaster),
    Backdrop(PreviewBackdrop),
    Label(PreviewLabel),
    Gradient(PreviewGradient),
    LeanAngle(PreviewLeanAngle),
    GForce(PreviewGForce),
    Value(ValuePreview),
}

impl PreviewWidget {
    pub fn new(request: WidgetRequest, fonts: &PreviewFonts, images: &PreviewImages) -> WidgetResult<Self> {
        match request {
            WidgetRequest::Raster {widget, scene} => Ok(Self::Raster(PreviewRaster::new(widget, scene, images)?)),
            WidgetRequest::Backdrop {widget, scene} => Ok(Self::Backdrop(PreviewBackdrop::new(widget, scene)?)),
            WidgetRequest::Label {widget, scene} => Ok(Self::Label(PreviewLabel::new(widget, scene, fonts)?)),
            WidgetRequest::Gradient(request) => Ok(Self::Gradient(PreviewGradient::new(request.widget, request.scene)?)),
            WidgetRequest::LeanAngle(request) => Ok(Self::LeanAngle(PreviewLeanAngle::new(request.widget, request.scene)?)),
            WidgetRequest::GForce(request) => Ok(Self::GForce(PreviewGForce::new(request.widget, request.scene)?)),
            WidgetRequest::Value(request) => Ok(Self::Value(ValuePreview::new(request, fonts)?)),
        }
    }
    pub fn pixels(&self) -> &Vec<u8> {
        match self { Self::Raster(widget) => &widget.pixels, Self::Backdrop(widget) => &widget.pixels, Self::Label(widget) => &widget.pixels, Self::Gradient(widget) => &widget.pixels, Self::LeanAngle(widget) => &widget.pixels, Self::GForce(widget) => &widget.pixels, Self::Value(widget) => &widget.pixels }
    }
    pub fn geometry(&self) -> &Vec<u8> {
        match self { Self::Raster(widget) => &widget.geometry, Self::Backdrop(widget) => &widget.geometry, Self::Label(widget) => &widget.geometry, Self::Gradient(widget) => &widget.geometry, Self::LeanAngle(widget) => &widget.geometry, Self::GForce(widget) => &widget.geometry, Self::Value(widget) => &widget.geometry }
    }
    pub fn revision(&self) -> u32 {
        match self { Self::Raster(widget) => widget.revision, Self::Backdrop(widget) => widget.revision, Self::Label(widget) => widget.revision, Self::Gradient(widget) => widget.revision, Self::LeanAngle(widget) => widget.revision, Self::GForce(widget) => widget.revision, Self::Value(widget) => widget.revision }
    }
    pub fn inherit_revision(&mut self, previous: u32) {
        match self {
            Self::Raster(widget) => widget.revision = previous.wrapping_add(1),
            Self::Backdrop(widget) => widget.revision = previous.wrapping_add(1),
            Self::Label(widget) => widget.revision = previous.wrapping_add(1),
            Self::Gradient(widget) => widget.revision = previous,
            Self::LeanAngle(widget) => widget.revision = previous,
            Self::GForce(widget) => widget.revision = previous,
            Self::Value(widget) => widget.revision = previous,
        }
    }
}

#[derive(Deserialize)]
pub struct ResizeRequest {
    #[serde(flatten)]
    pub origin: WidgetRequest,
    pub factor: Option<f32>,
    /// Framed families resize to scene-pixel dimensions instead of a uniform factor.
    pub size: Option<[f32; 2]>,
    pub fixed: [f32; 2],
    pub point: [f32; 2],
    pub second: f64,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum ResizeUpdate {
    Raster(crate::widgets::raster::interaction::ResizeUpdate),
    Backdrop(crate::widgets::backdrop::interaction::ResizeUpdate),
    Label(crate::widgets::label::interaction::ResizeUpdate),
    Gradient(crate::widgets::gradient::interaction::ResizeUpdate),
    LeanAngle(crate::widgets::lean_angle::interaction::ResizeUpdate),
    GForce(crate::widgets::g_force::interaction::ResizeUpdate),
    Value(crate::widgets::value::interaction::ResizeUpdate),
}
