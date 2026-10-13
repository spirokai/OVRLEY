//! Family dispatch at the preview ingress. `type` is the editor's canonical widget type.
use crate::{error::WidgetResult, model::{scene::ScenePresentationConfig, types::MetricKind}, preview::PreviewFonts, widgets::{label::{config::LabelConfig, preview::PreviewLabel}, value::{config::ValueConfig, preview::{PreviewWidget as ValuePreview, WidgetRequest as ValueRequest}}}};
use serde::{Deserialize, Deserializer, Serialize};
use crate::widgets::backdrop::{config::BackdropConfig, preview::PreviewBackdrop};

pub enum WidgetRequest {
    Backdrop { widget: BackdropConfig, scene: ScenePresentationConfig },
    Label { widget: LabelConfig, scene: ScenePresentationConfig },
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
        } else {
            let metric: MetricKind = serde_json::from_value(input.r#type.into()).map_err(serde::de::Error::custom)?;
            let widget: ValueConfig = serde_json::from_value(input.widget).map_err(serde::de::Error::custom)?;
            if widget.value != metric {
                return Err(serde::de::Error::custom("widget type must match its value metric"));
            }
            Ok(Self::Value(ValueRequest { widget, scene: input.scene }))
        }
    }
}

pub enum PreviewWidget {
    Backdrop(PreviewBackdrop),
    Label(PreviewLabel),
    Value(ValuePreview),
}

impl PreviewWidget {
    pub fn new(request: WidgetRequest, fonts: &PreviewFonts) -> WidgetResult<Self> {
        match request {
            WidgetRequest::Backdrop {widget, scene} => Ok(Self::Backdrop(PreviewBackdrop::new(widget, scene)?)),
            WidgetRequest::Label {widget, scene} => Ok(Self::Label(PreviewLabel::new(widget, scene, fonts)?)),
            WidgetRequest::Value(request) => Ok(Self::Value(ValuePreview::new(request, fonts)?)),
        }
    }
    pub fn pixels(&self) -> &Vec<u8> {
        match self { Self::Backdrop(widget) => &widget.pixels, Self::Label(widget) => &widget.pixels, Self::Value(widget) => &widget.pixels }
    }
    pub fn geometry(&self) -> &Vec<u8> {
        match self { Self::Backdrop(widget) => &widget.geometry, Self::Label(widget) => &widget.geometry, Self::Value(widget) => &widget.geometry }
    }
    pub fn revision(&self) -> u32 {
        match self { Self::Backdrop(widget) => widget.revision, Self::Label(widget) => widget.revision, Self::Value(widget) => widget.revision }
    }
    pub fn inherit_revision(&mut self, previous: u32) {
        match self {
            Self::Backdrop(widget) => widget.revision = previous.wrapping_add(1),
            Self::Label(widget) => widget.revision = previous.wrapping_add(1),
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
    Backdrop(crate::widgets::backdrop::interaction::ResizeUpdate),
    Label(crate::widgets::label::interaction::ResizeUpdate),
    Value(crate::widgets::value::interaction::ResizeUpdate),
}
