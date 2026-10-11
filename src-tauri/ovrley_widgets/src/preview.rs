//! Preview session ownership: shared activity, fonts, and widget handles.
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    render::text::FontResolver,
    widgets::value::preview::{PreviewWidget, WidgetRequest},
};
use skia_safe::{FontMgr, Typeface};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct PreviewFonts(BTreeMap<String, Typeface>);

impl PreviewFonts {
    pub fn register(&mut self, id: String, bytes: &[u8]) -> WidgetResult<()> {
        let face = Typeface::make_deserialize(bytes, FontMgr::default())
            .ok_or_else(|| WidgetError::Render(format!("unable to load font resource: {id}")))?;
        self.0.insert(id, face);
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
        if weight != crate::model::standard_widgets::widget_font_weight() || italic {
            return Err(WidgetError::Render(
                "preview font resource does not support the requested style".into(),
            ));
        }
        let name =
            name.ok_or_else(|| WidgetError::Render("preview requires a font identity".into()))?;
        self.0
            .get(name)
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
        request: crate::widgets::value::interaction::ResizeRequest,
    ) -> WidgetResult<crate::widgets::value::interaction::ResizeUpdate> {
        if !request.second.is_finite() {
            return Err(WidgetError::Config("previewSecond must be finite".into()));
        }
        let speed = self
            .activity
            .as_mut()
            .and_then(|a| a.speed_at(request.second));
        crate::widgets::value::interaction::resize_value(request, speed, &self.fonts)
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
        replacement.revision = previous.revision;
        self.widgets.insert(id, replacement);
        Ok(())
    }
    pub fn render(&mut self, second: f64) -> WidgetResult<()> {
        if !second.is_finite() {
            return Err(WidgetError::Config("previewSecond must be finite".into()));
        }
        let speed = self.activity.as_mut().and_then(|a| a.speed_at(second));
        for widget in self.widgets.values_mut() {
            widget.render(speed, &self.fonts)?;
        }
        Ok(())
    }
}
