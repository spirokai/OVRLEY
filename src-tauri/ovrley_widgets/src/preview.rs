//! Preview session ownership: shared activity, fonts, and widget handles.
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    render::text::FontResolver,
    preview_instance::PreviewInstance,
    preview_widget::{PreviewWidget, WidgetRequest, ResizeRequest, ResizeUpdate},
};
use skia_safe::{FontMgr, Typeface};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct PreviewFonts {
    faces: BTreeMap<(String, u32, bool), Typeface>,
    /// The editor leaves text unhinted so resizing scales it continuously; export-parity
    /// hosts grid-fit it exactly like native export.
    grid_fit: bool,
}

#[derive(serde::Deserialize)]
pub struct FontRequest {
    pub font: String,
    /// Metric resources omit styling; the shared renderer owns their manifest weight.
    #[serde(default = "crate::model::standard_widgets::widget_font_weight", deserialize_with = "crate::widgets::label::config::deserialize_font_weight")]
    pub font_weight: f32,
    #[serde(default)]
    pub italic: bool,
}

impl PreviewFonts {
    pub fn register(&mut self, request: FontRequest, bytes: &[u8]) -> WidgetResult<()> {
        let face = Typeface::make_deserialize(bytes, FontMgr::default())
            .ok_or_else(|| WidgetError::Render(format!("unable to load font resource: {}", request.font)))?;
        self.faces.insert((request.font, request.font_weight.to_bits(), request.italic), face);
        Ok(())
    }
    pub fn set_grid_fit(&mut self, grid_fit: bool) {
        self.grid_fit = grid_fit;
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
        self.faces
            .get(&(name.to_owned(), weight.to_bits(), italic))
            .cloned()
            .ok_or_else(|| WidgetError::Render(format!("font not registered: {name}")))
    }
    fn grid_fit(&self) -> bool {
        self.grid_fit
    }
}

#[derive(Default)]
pub struct PreviewSession {
    pub fonts: PreviewFonts,
    pub images: crate::widgets::raster::preview::PreviewImages,
    activity: Option<ActivitySampler>,
    /// The editor's disabled laps, applied to every activity this session samples.
    disabled_laps: Vec<usize>,
    pub widgets: BTreeMap<u32, PreviewInstance>,
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
        if request.fixed.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
            || request.point.iter().any(|v| !v.is_finite()) {
            return Err(WidgetError::Config("invalid widget resize gesture".into()));
        }
        if let Some(label_offset) = request.label_offset {
            if request.size.is_some() || request.factor.is_some() {
                return Err(WidgetError::Config("label_offset resize cannot include size or factor".into()));
            }
            let WidgetRequest::HeadingTape(origin) = request.origin else {
                return Err(WidgetError::Config("label_offset resize requires heading_tape".into()));
            };
            return Ok(ResizeUpdate::HeadingTape(crate::widgets::heading::interaction::resize_heading_label_offset(
                origin.widget, origin.scene, label_offset,
            )?));
        }
        if let WidgetRequest::Backdrop {widget, scene, ..} = request.origin {
            let size = request.size.ok_or_else(|| WidgetError::Config("backdrop resize size is required".into()))?;
            return Ok(ResizeUpdate::Backdrop(crate::widgets::backdrop::interaction::resize_backdrop(widget, scene, size, request.fixed, request.point)?));
        }
        if let WidgetRequest::Raster {widget, scene, ..} = request.origin {
            let size = request.size.ok_or_else(|| WidgetError::Config("raster resize size is required".into()))?;
            return Ok(ResizeUpdate::Raster(crate::widgets::raster::interaction::resize_raster(widget, scene, size, request.fixed, request.point)?));
        }
        if let WidgetRequest::HeadingTape(origin) = request.origin {
            let size = request.size.ok_or_else(|| WidgetError::Config("heading resize size is required".into()))?;
            return Ok(ResizeUpdate::HeadingTape(crate::widgets::heading::interaction::resize_heading(
                origin.widget, origin.scene, size, request.fixed, request.point, self.activity.as_ref(), request.second, &self.fonts,
            )?));
        }
        if let WidgetRequest::RollingChart {widget, scene, ..} = request.origin {
            let size = request.size.ok_or_else(|| WidgetError::Config("rolling chart resize size is required".into()))?;
            return Ok(ResizeUpdate::RollingChart(crate::widgets::rolling_chart::interaction::resize_rolling_chart(widget, scene, size, request.fixed, request.point, &self.fonts)?));
        }
        if let WidgetRequest::HeartZones(origin) = request.origin {
            let size = request.size.ok_or_else(|| WidgetError::Config("heart zones resize size is required".into()))?;
            return Ok(ResizeUpdate::HeartZones(crate::widgets::heart_zones::interaction::resize_heart_zones(
                origin.widget, origin.scene, size, request.fixed, request.point,
            )?));
        }
        if let WidgetRequest::Gauge(origin) = request.origin {
            return Ok(ResizeUpdate::Gauge(crate::widgets::gauges::interaction::resize_gauge(origin.widget, origin.scene, request.factor, request.size, request.fixed, request.point, self.activity.as_ref(), request.second, &self.fonts)?));
        }
        if let WidgetRequest::Elevation {widget, scene, ..} = request.origin {
            let size = request.size.ok_or_else(|| WidgetError::Config("elevation resize size is required".into()))?;
            let handle = request.handle.ok_or_else(|| WidgetError::Config("elevation resize requires its preview handle".into()))?;
            let Some(PreviewInstance { widget: PreviewWidget::Elevation(elevation), .. }) = self.widgets.get_mut(&handle) else {
                return Err(WidgetError::Config("elevation resize requires an elevation handle".into()));
            };
            return Ok(ResizeUpdate::Elevation(elevation.resize(widget, scene, size, request.fixed, request.point, request.second, request.commit, self.activity.as_ref())?));
        }
        if let WidgetRequest::DivingProfile {widget, scene, ..} = request.origin {
            let size = request.size.ok_or_else(|| WidgetError::Config("diving profile resize size is required".into()))?;
            let handle = request.handle.ok_or_else(|| WidgetError::Config("diving profile resize requires its preview handle".into()))?;
            let Some(PreviewInstance { widget: PreviewWidget::DivingProfile(profile), .. }) = self.widgets.get_mut(&handle) else {
                return Err(WidgetError::Config("diving profile resize requires a diving profile handle".into()));
            };
            return Ok(ResizeUpdate::DivingProfile(profile.resize(widget, scene, size, request.fixed, request.point, request.second, request.commit, self.activity.as_ref())?));
        }
        let factor = crate::model::validation::require_positive_f32(request.factor, "resize factor")?;
        match request.origin {
            WidgetRequest::Route {widget, scene, ..} => {
                let handle = request.handle.ok_or_else(|| WidgetError::Config("route resize requires its preview handle".into()))?;
                let Some(PreviewInstance { widget: PreviewWidget::Route(route), .. }) = self.widgets.get_mut(&handle) else {
                    return Err(WidgetError::Config("route resize requires a route handle".into()));
                };
                Ok(ResizeUpdate::Route(route.resize(widget, scene, factor, request.fixed, request.point, request.second, request.commit, self.activity.as_ref())?))
            },
            WidgetRequest::DivingProfile { .. } | WidgetRequest::RollingChart { .. } | WidgetRequest::Elevation { .. } | WidgetRequest::Backdrop { .. } | WidgetRequest::Raster { .. } | WidgetRequest::HeadingTape(_) | WidgetRequest::HeartZones(_) | WidgetRequest::Gauge(_) => unreachable!(),
            WidgetRequest::Label {widget, scene, ..} => Ok(ResizeUpdate::Label(crate::widgets::label::interaction::resize_label(widget, scene, factor, request.fixed, request.point, &self.fonts)?)),
            WidgetRequest::Gradient(origin) => Ok(ResizeUpdate::Gradient(crate::widgets::gradient::interaction::resize_gradient(
                origin.widget, origin.scene, factor, request.fixed, request.point, self.activity.as_ref(), request.second, &self.fonts,
            )?)),
            WidgetRequest::HeadingCompass(origin) => Ok(ResizeUpdate::HeadingCompass(crate::widgets::heading_compass::interaction::resize_heading_compass(
                origin.widget, origin.scene, factor, request.fixed, request.point, self.activity.as_ref(), request.second, &self.fonts,
            )?)),
            WidgetRequest::LeanAngle(origin) => Ok(ResizeUpdate::LeanAngle(crate::widgets::lean_angle::interaction::resize_lean_angle(
                origin.widget, origin.scene, factor, request.fixed, request.point, self.activity.as_ref(), request.second, &self.fonts,
            )?)),
            WidgetRequest::GForce(origin) => Ok(ResizeUpdate::GForce(crate::widgets::g_force::interaction::resize_g_force(
                origin.widget, origin.scene, factor, request.fixed, request.point, self.activity.as_ref(), request.second, &self.fonts,
            )?)),
            WidgetRequest::LapTimer(origin) => Ok(ResizeUpdate::LapTimer(crate::widgets::lap_timer::interaction::resize_lap_timer(
                origin.widget, origin.scene, factor, request.fixed, request.point, self.activity.as_ref(), request.second, &self.fonts,
            )?)),
            WidgetRequest::SplitsRun(origin) => Ok(ResizeUpdate::SplitsRun(crate::widgets::splits_run::interaction::resize_splits_run(
                origin.widget, origin.scene, factor, request.fixed, request.point, self.activity.as_ref(), request.second, &self.fonts,
            )?)),
            WidgetRequest::Value(origin) => Ok(ResizeUpdate::Value(crate::widgets::value::interaction::resize_value(
                crate::widgets::value::interaction::ResizeRequest {origin, factor, fixed: request.fixed, point: request.point, second: request.second}, self.activity.as_ref(), &self.fonts,
            )?)),
        }
    }
    pub fn activity(&self) -> Option<&ActivitySampler> {
        self.activity.as_ref()
    }
    pub fn replace_activity(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        self.activity = if bytes.is_empty() {
            None
        } else {
            let mut activity = ActivitySampler::from_json(bytes)?;
            activity.select_laps(self.disabled_laps.clone())?;
            Some(activity)
        };
        for widget in self.widgets.values_mut() {
            let widget = &mut widget.widget;
            if let PreviewWidget::DivingProfile(widget) = widget { widget.invalidate_activity(); }
            if let PreviewWidget::RollingChart(widget) = widget { widget.invalidate_activity(); }
            if let PreviewWidget::Elevation(widget) = widget { widget.invalidate_activity(); }
            if let PreviewWidget::Route(widget) = widget { widget.invalidate_activity(); }
            if let PreviewWidget::LapTimer(widget) = widget { widget.invalidate_activity(); }
            if let PreviewWidget::SplitsRun(widget) = widget { widget.invalidate_activity(); }
            if let PreviewWidget::Gauge(widget) = widget { widget.invalidate_activity(); }
        }
        Ok(())
    }
    /// Adds a split table core derived from the current activity; widgets of that length re-layout.
    pub fn replace_split_table(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let table = crate::widgets::splits_run::table::SplitTable::from_json(bytes)?;
        let activity = self
            .activity
            .as_mut()
            .ok_or_else(|| WidgetError::Config("split tables require a loaded activity".into()))?;
        activity.insert_split_table(table);
        for widget in self.widgets.values_mut() {
            if let PreviewWidget::SplitsRun(widget) = &mut widget.widget { widget.invalidate_activity(); }
        }
        Ok(())
    }
    /// Replaces the disabled laps; an activity still in flight receives them on arrival.
    pub fn select_laps(&mut self, disabled: Vec<usize>) -> WidgetResult<()> {
        if let Some(activity) = &mut self.activity {
            activity.select_laps(disabled.clone())?;
        }
        self.disabled_laps = disabled;
        for widget in self.widgets.values_mut() {
            if let PreviewWidget::LapTimer(widget) = &mut widget.widget { widget.invalidate_activity(); }
        }
        Ok(())
    }
    pub fn create_widget(&mut self, request: WidgetRequest) -> WidgetResult<u32> {
        let widget = PreviewInstance::new(request, &self.fonts, &self.images)?;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| WidgetError::Render("widget handles exhausted".into()))?;
        self.widgets.insert(self.next_id, widget);
        Ok(self.next_id)
    }
    pub fn update_widget(&mut self, id: u32, request: WidgetRequest) -> WidgetResult<()> {
        self
            .widgets
            .get_mut(&id)
            .ok_or_else(|| WidgetError::Render("unknown widget handle".into()))?
            .update(request, &self.fonts, &self.images)
    }
    pub fn render(&mut self, second: f64) -> WidgetResult<()> {
        if !second.is_finite() {
            return Err(WidgetError::Config("previewSecond must be finite".into()));
        }
        for widget in self.widgets.values_mut() {
            // Static families are fully rendered on creation/update and never visit the playhead path.
            match &mut widget.widget {
                PreviewWidget::DivingProfile(widget) => widget.render(self.activity.as_ref(), second, &self.fonts)?,
                PreviewWidget::RollingChart(widget) => widget.render(self.activity.as_ref(), second)?,
                PreviewWidget::Elevation(widget) => widget.render(self.activity.as_ref(), second, &self.fonts)?,
                PreviewWidget::Route(widget) => widget.render(self.activity.as_ref(), second)?,
                PreviewWidget::Value(widget) => widget.render(self.activity.as_ref(), second, &self.fonts)?,
                PreviewWidget::Gradient(widget) => widget.render(self.activity.as_ref(), second, &self.fonts)?,
                PreviewWidget::LeanAngle(widget) => widget.render(self.activity.as_ref(), second, &self.fonts)?,
                PreviewWidget::GForce(widget) => widget.render(self.activity.as_ref(), second, &self.fonts)?,
                PreviewWidget::LapTimer(widget) => widget.render(self.activity.as_ref(), second, &self.fonts)?,
                PreviewWidget::SplitsRun(widget) => widget.render(self.activity.as_ref(), second, &self.fonts)?,
                PreviewWidget::Gauge(widget) => widget.render(self.activity.as_ref(), second, &self.fonts)?,
                PreviewWidget::HeadingTape(widget) => widget.render(self.activity.as_ref(), second)?,
                PreviewWidget::HeadingCompass(widget) => widget.render(self.activity.as_ref(), second, &self.fonts)?,
                PreviewWidget::HeartZones(widget) => widget.render(self.activity.as_ref(), second)?,
                _ => {},
            }
            widget.sync()?;
        }
        Ok(())
    }
}
