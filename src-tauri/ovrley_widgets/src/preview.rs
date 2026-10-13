//! Preview session ownership: shared activity, fonts, and widget handles.
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    render::text::FontResolver,
    preview_widget::{PreviewWidget, WidgetRequest, ResizeRequest, ResizeUpdate},
};
use skia_safe::{FontMgr, Typeface};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct PreviewFonts(BTreeMap<(String, u32, bool), Typeface>);

#[derive(serde::Deserialize)]
pub struct FontRequest {
    pub font: String,
    #[serde(deserialize_with = "crate::widgets::label::config::deserialize_font_weight")]
    pub font_weight: f32,
    pub italic: bool,
}

impl PreviewFonts {
    pub fn register(&mut self, request: FontRequest, bytes: &[u8]) -> WidgetResult<()> {
        let face = Typeface::make_deserialize(bytes, FontMgr::default())
            .ok_or_else(|| WidgetError::Render(format!("unable to load font resource: {}", request.font)))?;
        self.0.insert((request.font, request.font_weight.to_bits(), request.italic), face);
        Ok(())
    }
}

impl FontResolver for PreviewFonts {
    fn resolve_typeface(
        &self,
        name: Option<&str>,
        weight: f32,
        italic: bool,
    ) -> WidgetResult<Typeface> {
        let name =
            name.ok_or_else(|| WidgetError::Render("preview requires a font identity".into()))?;
        self.0
            .get(&(name.to_owned(), weight.to_bits(), italic))
            .cloned()
            .ok_or_else(|| WidgetError::Render(format!("font not registered: {name}")))
    }
}

#[derive(Default)]
pub struct PreviewSession {
    pub fonts: PreviewFonts,
    activity: Option<ActivitySampler>,
    pub widgets: BTreeMap<u32, PreviewWidget>,
    next_id: u32,
}

impl PreviewSession {
    pub fn resize_widget(
        &mut self,
        request: ResizeRequest,
    ) -> WidgetResult<ResizeUpdate> {
        if !request.second.is_finite() {
            return Err(WidgetError::Config("previewSecond must be finite".into()));
        }
        if !request.factor.is_finite() || request.factor <= 0.0
            || request.fixed.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
            || request.point.iter().any(|v| !v.is_finite()) {
            return Err(WidgetError::Config("invalid widget resize gesture".into()));
        }
        match request.origin {
            WidgetRequest::Label {widget, scene} => Ok(ResizeUpdate::Label(crate::widgets::label::interaction::resize_label(widget, scene, request.factor, request.fixed, request.point, &self.fonts)?)),
            WidgetRequest::Value(origin) => Ok(ResizeUpdate::Value(crate::widgets::value::interaction::resize_value(
                crate::widgets::value::interaction::ResizeRequest {origin, factor: request.factor, fixed: request.fixed, point: request.point, second: request.second}, self.activity.as_ref(), &self.fonts,
            )?)),
        }
    }
    pub fn replace_activity(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        self.activity = if bytes.is_empty() {
            None
        } else {
            Some(ActivitySampler::from_json(bytes)?)
        };
        Ok(())
    }
    pub fn create_widget(&mut self, request: WidgetRequest) -> WidgetResult<u32> {
        let widget = PreviewWidget::new(request, &self.fonts)?;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| WidgetError::Render("widget handles exhausted".into()))?;
        self.widgets.insert(self.next_id, widget);
        Ok(self.next_id)
    }
    pub fn update_widget(&mut self, id: u32, request: WidgetRequest) -> WidgetResult<()> {
        let mut replacement = PreviewWidget::new(request, &self.fonts)?;
        let previous = self
            .widgets
            .get(&id)
            .ok_or_else(|| WidgetError::Render("unknown widget handle".into()))?;
        replacement.inherit_revision(previous.revision());
        self.widgets.insert(id, replacement);
        Ok(())
    }
    pub fn render(&mut self, second: f64) -> WidgetResult<()> {
        if !second.is_finite() {
            return Err(WidgetError::Config("previewSecond must be finite".into()));
        }
        for widget in self.widgets.values_mut() {
            // Labels are fully rendered on creation/update and never visit the playhead path.
            if let PreviewWidget::Value(widget) = widget {
                widget.render(self.activity.as_ref(), second, &self.fonts)?;
            }
        }
        Ok(())
    }
}
