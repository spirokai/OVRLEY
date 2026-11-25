//! The editor preview session and the widget engine both hosts draw with: family dispatch,
//! placement-only edits, resize gestures and the host inputs outside widget configuration.
use crate::activity::ActivitySampler;
use crate::model::catalog::MetricKind;
use crate::model::scene::{ScenePresentationConfig, ValidatedScenePresentation};
use crate::model::validation::require_positive_f32;
use crate::render::text::FontResolver;
use crate::render::tile::{PreviewRasterScale, Tile, TileBounds, WidgetGeometry};
use crate::widgets::altitude_tape::{
    resize_altitude_tape, AltitudeTapeSample, PreviewAltitudeTape,
};
use crate::widgets::backdrop::{
    resize_backdrop, BackdropConfig, PreviewBackdrop, ValidatedBackdrop,
};
use crate::widgets::diving_profile::layout::DivingFrameState;
use crate::widgets::diving_profile::{
    DivingProfileConfig, PreviewDivingProfile, ValidatedDivingProfile,
};
use crate::widgets::elevation::geometry::ElevationFrameState;
use crate::widgets::elevation::{ElevationPlotConfig, PreviewElevation, ValidatedElevationPlot};
use crate::widgets::g_force::{resize_g_force, GForceSample, PreviewGForce};
use crate::widgets::gauges::{resize_gauge, GaugeFrameState, PreviewGauge};
use crate::widgets::gradient::{resize_gradient, PreviewGradient};
use crate::widgets::heading::{resize_heading, resize_heading_label_offset, PreviewHeading};
use crate::widgets::heading_compass::{resize_heading_compass, PreviewHeadingCompass};
use crate::widgets::heart_zones::{resize_heart_zones, PreviewHeartZones};
use crate::widgets::label::{resize_label, LabelConfig, PreviewLabel, ValidatedLabel};
use crate::widgets::lap_timer::{resize_lap_timer, LapTimerContent, PreviewLapTimer};
use crate::widgets::lean_angle::{resize_lean_angle, PreviewLeanAngle};
use crate::widgets::map::preview::{MapFrameProvider, PreviewMap, PreviewMaps};
use crate::widgets::map::{
    adjust_map_view, resize_map, MapConfig, MapContent, MapViewGesture, ValidatedMapWidget,
};
use crate::widgets::raster::{
    resize_raster, ImageSource, PreviewImages, PreviewRaster, RasterConfig, RasterGeometry,
    RasterImage,
};
use crate::widgets::rolling_chart::{
    resize_rolling_chart, PreviewRollingChart, RollingChartConfig, ValidatedRollingChart,
};
use crate::widgets::route::{CoursePlotConfig, PreviewRoute, RouteWindow, ValidatedRoutePlot};
use crate::widgets::splits_run::{resize_splits_run, PreviewSplitsRun, SplitTable, SplitsContent};
use crate::widgets::value::format::MetricDisplayParts;
use crate::widgets::value::{
    resize_value, PreviewValue, ValidatedValue, ValueConfig, ValueFamily, ValuePresentation,
};
use crate::{WidgetError, WidgetResult};
use ovrley_map::{MapCamera, MapStyleInfo};
use serde::{Deserialize, Deserializer, Serialize};
use skia_safe::{Canvas, FontMgr, Typeface};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Default)]
pub struct PreviewFonts {
    faces: BTreeMap<(String, u32, bool), Typeface>,
    /// The editor leaves text unhinted so resizing scales it continuously; hosts matching export
    /// hosts grid-fit it exactly like native export.
    grid_fit: bool,
}

#[derive(serde::Deserialize)]
pub struct FontRequest {
    pub font: String,
    /// Metric resources omit styling; the shared renderer owns their manifest weight.
    #[serde(
        default = "crate::model::catalog::widget_font_weight",
        deserialize_with = "crate::widgets::label::deserialize_font_weight"
    )]
    pub font_weight: f32,
    #[serde(default)]
    pub italic: bool,
}

impl PreviewFonts {
    pub fn register(&mut self, request: FontRequest, bytes: &[u8]) -> WidgetResult<()> {
        let face = Typeface::make_deserialize(bytes, FontMgr::default()).ok_or_else(|| {
            WidgetError::Render(format!("unable to load font resource: {}", request.font))
        })?;
        self.faces.insert(
            (request.font, request.font_weight.to_bits(), request.italic),
            face,
        );
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
    pub images: PreviewImages,
    /// The installed map styles, once the host has set them, and a native host's frames.
    pub maps: PreviewMaps,
    activity: Option<ActivitySampler>,
    /// The editor's disabled laps, applied to every activity this session samples.
    disabled_laps: Vec<usize>,
    pub widgets: BTreeMap<u32, PreviewInstance>,
    next_id: u32,
    /// The editor's display density; every widget rasterizes at it.
    raster_scale: PreviewRasterScale,
}

impl PreviewSession {
    pub fn resize_widget(&mut self, request: ResizeRequest) -> WidgetResult<ResizeUpdate> {
        let ResizeRequest {
            origin: WidgetRequest { widget, scene, .. },
            factor,
            size,
            handle,
            label_offset,
            map_view,
            gesture,
        } = request;
        gesture.validate()?;
        let exclusive = |mode: &str| -> WidgetResult<()> {
            if size.is_some() || factor.is_some() {
                return Err(WidgetError::Config(format!(
                    "{mode} resize cannot include size or factor"
                )));
            }
            Ok(())
        };
        let require_size = |family: &str| {
            size.ok_or_else(|| WidgetError::Config(format!("{family} resize size is required")))
        };
        let require_factor = || require_positive_f32(factor, "resize factor");
        let activity = self.activity.as_ref();
        let fonts = &self.fonts;
        if let Some(label_offset) = label_offset {
            exclusive("label_offset")?;
            return match widget {
                WidgetConfig::Value(widget)
                    if ValueFamily::of(widget.value, widget.display_type)
                        == ValueFamily::HeadingTape =>
                {
                    Ok(ResizeUpdate::HeadingTape(resize_heading_label_offset(
                        *widget,
                        scene,
                        label_offset,
                    )?))
                }
                _ => Err(WidgetError::Config(
                    "label_offset resize requires heading_tape".into(),
                )),
            };
        }
        if let Some(gesture) = map_view {
            exclusive("map_view")?;
            let WidgetConfig::Map(widget) = widget else {
                return Err(WidgetError::Config("map_view requires a map".into()));
            };
            let PreviewWidget::Map(map) = handle_widget(&mut self.widgets, handle, "map_view")?
            else {
                return Err(WidgetError::Config("map_view requires a map handle".into()));
            };
            let route = map.route_extent(activity)?;
            return Ok(ResizeUpdate::MapView(adjust_map_view(
                widget, scene, gesture, route,
            )?));
        }
        Ok(match widget {
            WidgetConfig::Backdrop(widget) => ResizeUpdate::Backdrop(resize_backdrop(
                widget,
                scene,
                require_size("backdrop")?,
                gesture,
            )?),
            WidgetConfig::Raster(widget) => ResizeUpdate::Raster(resize_raster(
                widget,
                scene,
                require_size("raster")?,
                gesture,
            )?),
            WidgetConfig::Map(widget) => {
                ResizeUpdate::Map(resize_map(widget, scene, require_size("map")?, gesture)?)
            }
            WidgetConfig::RollingChart(widget) => ResizeUpdate::RollingChart(resize_rolling_chart(
                widget,
                scene,
                require_size("rolling chart")?,
                gesture,
                fonts,
            )?),
            WidgetConfig::Label(widget) => ResizeUpdate::Label(resize_label(
                widget,
                scene,
                require_factor()?,
                gesture,
                fonts,
            )?),
            WidgetConfig::Elevation(widget) => {
                let size = require_size("elevation")?;
                let PreviewWidget::Elevation(elevation) =
                    handle_widget(&mut self.widgets, handle, "elevation resize")?
                else {
                    return Err(WidgetError::Config(
                        "elevation resize requires an elevation handle".into(),
                    ));
                };
                ResizeUpdate::Elevation(elevation.resize(widget, scene, size, gesture, activity)?)
            }
            WidgetConfig::DivingProfile(widget) => {
                let size = require_size("diving profile")?;
                let PreviewWidget::DivingProfile(profile) =
                    handle_widget(&mut self.widgets, handle, "diving profile resize")?
                else {
                    return Err(WidgetError::Config(
                        "diving profile resize requires a diving profile handle".into(),
                    ));
                };
                ResizeUpdate::DivingProfile(profile.resize(widget, scene, size, gesture, activity)?)
            }
            WidgetConfig::Route(widget) => {
                let factor = require_factor()?;
                let PreviewWidget::Route(route) =
                    handle_widget(&mut self.widgets, handle, "route resize")?
                else {
                    return Err(WidgetError::Config(
                        "route resize requires a route handle".into(),
                    ));
                };
                ResizeUpdate::Route(route.resize(widget, scene, factor, gesture, activity)?)
            }
            WidgetConfig::Value(widget) => match ValueFamily::of(widget.value, widget.display_type)
            {
                ValueFamily::HeadingTape => ResizeUpdate::HeadingTape(resize_heading(
                    *widget,
                    scene,
                    require_size("heading")?,
                    gesture,
                    activity,
                    fonts,
                )?),
                ValueFamily::HeartZones => ResizeUpdate::HeartZones(resize_heart_zones(
                    *widget,
                    scene,
                    require_size("heart zones")?,
                    gesture,
                )?),
                ValueFamily::AltitudeTape => ResizeUpdate::AltitudeTape(resize_altitude_tape(
                    *widget,
                    scene,
                    require_size("altitude tape")?,
                    gesture,
                )?),
                ValueFamily::Gauge => ResizeUpdate::Gauge(resize_gauge(
                    *widget, scene, factor, size, gesture, activity, fonts,
                )?),
                ValueFamily::Gradient => ResizeUpdate::Gradient(resize_gradient(
                    *widget,
                    scene,
                    require_factor()?,
                    gesture,
                    activity,
                    fonts,
                )?),
                ValueFamily::HeadingCompass => {
                    ResizeUpdate::HeadingCompass(resize_heading_compass(
                        *widget,
                        scene,
                        require_factor()?,
                        gesture,
                        activity,
                        fonts,
                    )?)
                }
                ValueFamily::LeanAngle => ResizeUpdate::LeanAngle(resize_lean_angle(
                    *widget,
                    scene,
                    require_factor()?,
                    gesture,
                    activity,
                    fonts,
                )?),
                ValueFamily::GForce => ResizeUpdate::GForce(resize_g_force(
                    *widget,
                    scene,
                    require_factor()?,
                    gesture,
                    activity,
                    fonts,
                )?),
                ValueFamily::LapTimer => ResizeUpdate::LapTimer(resize_lap_timer(
                    *widget,
                    scene,
                    require_factor()?,
                    gesture,
                    activity,
                    fonts,
                )?),
                ValueFamily::SplitsRun => ResizeUpdate::SplitsRun(resize_splits_run(
                    *widget,
                    scene,
                    require_factor()?,
                    gesture,
                    activity,
                    fonts,
                )?),
                ValueFamily::Text => ResizeUpdate::Value(resize_value(
                    *widget,
                    scene,
                    require_factor()?,
                    gesture,
                    activity,
                    fonts,
                )?),
            },
        })
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
        for instance in self.widgets.values_mut() {
            instance.widget.invalidate_activity();
        }
        Ok(())
    }
    /// Adds a split table derived from the current activity; widgets of that length re-layout.
    pub fn replace_split_table(&mut self, table: SplitTable) -> WidgetResult<()> {
        table.validate()?;
        let activity = self
            .activity
            .as_mut()
            .ok_or_else(|| WidgetError::Config("split tables require a loaded activity".into()))?;
        activity.insert_split_table(table);
        for widget in self.widgets.values_mut() {
            if let PreviewWidget::SplitsRun(widget) = &mut widget.widget {
                widget.invalidate_activity();
            }
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
            if let PreviewWidget::LapTimer(widget) = &mut widget.widget {
                widget.invalidate_activity();
            }
        }
        Ok(())
    }
    pub fn create_widget(&mut self, request: WidgetRequest) -> WidgetResult<u32> {
        let widget = PreviewInstance::new(
            request,
            self.raster_scale,
            &self.fonts,
            &self.images,
            &self.maps,
        )?;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| WidgetError::Render("widget handles exhausted".into()))?;
        self.widgets.insert(self.next_id, widget);
        Ok(self.next_id)
    }
    pub fn update_widget(&mut self, id: u32, request: WidgetRequest) -> WidgetResult<()> {
        self.widgets
            .get_mut(&id)
            .ok_or_else(|| WidgetError::Render("unknown widget handle".into()))?
            .update(
                request,
                self.raster_scale,
                &self.fonts,
                &self.images,
                &self.maps,
            )
    }
    /// Zoom changes only the density, so widgets re-rasterize from their applied requests.
    pub fn set_raster_scale(&mut self, raster_scale: PreviewRasterScale) -> WidgetResult<()> {
        if raster_scale == self.raster_scale {
            return Ok(());
        }
        self.raster_scale = raster_scale;
        // One failing widget must not leave the rest at the previous density.
        let mut result = Ok(());
        for widget in self.widgets.values_mut() {
            let rebuilt =
                widget.set_raster_scale(raster_scale, &self.fonts, &self.images, &self.maps);
            if result.is_ok() {
                result = rebuilt;
            }
        }
        result
    }
    pub fn render(&mut self, second: f64) -> WidgetResult<()> {
        if !second.is_finite() {
            return Err(WidgetError::Config("previewSecond must be finite".into()));
        }
        for instance in self.widgets.values_mut() {
            instance
                .widget
                .render(self.activity.as_ref(), second, &self.fonts)?;
            instance.sync();
        }
        Ok(())
    }
}

/// The widget a gesture's preview handle names, which keeps its prepared vectors.
fn handle_widget<'a>(
    widgets: &'a mut BTreeMap<u32, PreviewInstance>,
    handle: Option<u32>,
    gesture: &str,
) -> WidgetResult<&'a mut PreviewWidget> {
    let handle = handle
        .ok_or_else(|| WidgetError::Config(format!("{gesture} requires its preview handle")))?;
    widgets
        .get_mut(&handle)
        .map(|instance| &mut instance.widget)
        .ok_or_else(|| WidgetError::Config(format!("{gesture} names an unknown preview handle")))
}

/// A widget's configuration, keyed by the editor's widget type.
#[derive(Clone, Debug, PartialEq)]
pub enum WidgetConfig {
    DivingProfile(DivingProfileConfig),
    RollingChart(RollingChartConfig),
    Elevation(ElevationPlotConfig),
    Route(CoursePlotConfig),
    Map(MapConfig),
    Raster(RasterConfig),
    Backdrop(BackdropConfig),
    Label(LabelConfig),
    Value(Box<ValueConfig>),
}

impl WidgetConfig {
    fn position(&self) -> WidgetResult<[f64; 2]> {
        let (x, y) = match self {
            Self::DivingProfile(widget) => (widget.x, widget.y),
            Self::RollingChart(widget) => (widget.x, widget.y),
            Self::Elevation(widget) => (widget.x, widget.y),
            Self::Route(widget) => (widget.x, widget.y),
            Self::Map(widget) => (widget.x, widget.y),
            Self::Raster(widget) => return Ok([widget.x, widget.y]),
            Self::Backdrop(widget) => {
                let required = |value: Option<f32>, field: &str| {
                    value
                        .ok_or_else(|| WidgetError::Config(format!("backdrop.{field} is required")))
                };
                (required(widget.x, "x")?, required(widget.y, "y")?)
            }
            Self::Label(widget) => (widget.x, widget.y),
            Self::Value(widget) => (widget.x, widget.y),
        };
        Ok([f64::from(x), f64::from(y)])
    }

    fn set_origin(&mut self) {
        match self {
            Self::DivingProfile(widget) => (widget.x, widget.y) = (0.0, 0.0),
            Self::RollingChart(widget) => (widget.x, widget.y) = (0.0, 0.0),
            Self::Elevation(widget) => (widget.x, widget.y) = (0.0, 0.0),
            Self::Route(widget) => (widget.x, widget.y) = (0.0, 0.0),
            Self::Map(widget) => (widget.x, widget.y) = (0.0, 0.0),
            Self::Raster(widget) => (widget.x, widget.y) = (0.0, 0.0),
            Self::Backdrop(widget) => (widget.x, widget.y) = (Some(0.0), Some(0.0)),
            Self::Label(widget) => (widget.x, widget.y) = (0.0, 0.0),
            Self::Value(widget) => (widget.x, widget.y) = (0.0, 0.0),
        }
    }
}

/// One widget for the preview: its configuration, the scene it presents in, and whether it is a
/// drag or resize draft, which keeps cached pixels until the gesture commits.
#[derive(Clone, Debug, PartialEq)]
pub struct WidgetRequest {
    pub widget: WidgetConfig,
    pub scene: ScenePresentationConfig,
    pub transient: bool,
}

impl WidgetRequest {
    /// The request without its placement and draft state: equal identities draw equal pixels.
    fn pixel_identity(&self) -> Self {
        let mut identity = self.clone();
        identity.widget.set_origin();
        identity.transient = false;
        identity
    }
}

impl<'de> Deserialize<'de> for WidgetRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Input {
            r#type: String,
            widget: serde_json::Value,
            scene: ScenePresentationConfig,
            #[serde(default)]
            transient: bool,
        }
        fn parse<T: serde::de::DeserializeOwned, E: serde::de::Error>(
            widget: serde_json::Value,
        ) -> Result<T, E> {
            serde_json::from_value(widget).map_err(E::custom)
        }
        let Input {
            r#type,
            widget,
            scene,
            transient,
        } = Input::deserialize(deserializer)?;
        let widget = match r#type.as_str() {
            "diving_profile" => WidgetConfig::DivingProfile(parse(widget)?),
            "rolling_chart" => WidgetConfig::RollingChart(parse(widget)?),
            "course" => WidgetConfig::Route(parse(widget)?),
            "map" => WidgetConfig::Map(parse(widget)?),
            "elevation" => WidgetConfig::Elevation(parse(widget)?),
            "label" => WidgetConfig::Label(parse(widget)?),
            "backdrop" => WidgetConfig::Backdrop(parse(widget)?),
            "raster" => WidgetConfig::Raster(parse(widget)?),
            metric => {
                let metric: MetricKind = parse(serde_json::Value::String(metric.to_owned()))?;
                let widget: ValueConfig = parse(widget)?;
                if widget.value != metric {
                    return Err(serde::de::Error::custom(
                        "widget type must match its value metric",
                    ));
                }
                WidgetConfig::Value(Box::new(widget))
            }
        };
        Ok(Self {
            widget,
            scene,
            transient,
        })
    }
}

pub struct PreviewInstance {
    pub widget: PreviewWidget,
    /// The last applied request, so a new raster scale rebuilds without the host resending it.
    request: WidgetRequest,
    raster_scale: PreviewRasterScale,
    origin: [f64; 2],
    position: [f64; 2],
    /// The rendered geometry moved to the current placement; `None` while it is unmoved.
    placed_geometry: Option<WidgetGeometry>,
    rendered_revision: Option<u32>,
    revision: u32,
    pixels_revision: u32,
}

impl PreviewInstance {
    pub fn new(
        request: WidgetRequest,
        raster_scale: PreviewRasterScale,
        fonts: &dyn FontResolver,
        images: &(impl ImageSource + ?Sized),
        maps: &PreviewMaps,
    ) -> WidgetResult<Self> {
        let position = request.widget.position()?;
        let mut instance = Self {
            widget: PreviewWidget::new(request.clone(), raster_scale, fonts, images, maps)?,
            request,
            raster_scale,
            origin: position,
            position,
            placed_geometry: None,
            rendered_revision: None,
            revision: 0,
            pixels_revision: 0,
        };
        instance.sync();
        Ok(instance)
    }

    /// Re-rasterizes at a new display density from the last applied request.
    pub fn set_raster_scale(
        &mut self,
        raster_scale: PreviewRasterScale,
        fonts: &dyn FontResolver,
        images: &(impl ImageSource + ?Sized),
        maps: &PreviewMaps,
    ) -> WidgetResult<()> {
        self.update(self.request.clone(), raster_scale, fonts, images, maps)
    }

    pub fn update(
        &mut self,
        request: WidgetRequest,
        raster_scale: PreviewRasterScale,
        fonts: &dyn FontResolver,
        images: &(impl ImageSource + ?Sized),
        maps: &PreviewMaps,
    ) -> WidgetResult<()> {
        let position = request.widget.position()?;
        let delta = [position[0] - self.origin[0], position[1] - self.origin[1]];
        // A live drag may resample cached pixels at a fractional position. Once
        // committed, rebuild at that position to match native export exactly.
        if request.pixel_identity() == self.request.pixel_identity()
            && raster_scale == self.raster_scale
            && (request.transient || delta.iter().all(|value| value.fract() == 0.0))
        {
            // Pixel-changing requests use the family's validation below. Only
            // placement-only input bypasses it and validates its new x/y here.
            if position.iter().any(|value| !(*value as f32).is_finite()) {
                return Err(WidgetError::Config(
                    "widget x/y must be finite scene coordinates".into(),
                ));
            }
            if position != self.position {
                self.position = position;
                self.place_geometry();
                self.revision = self.revision.wrapping_add(1);
            }
            self.request = request;
            return Ok(());
        }
        let applied = request.clone();
        let WidgetRequest {
            widget,
            scene,
            transient,
        } = request;
        match (&mut self.widget, widget) {
            (PreviewWidget::DivingProfile(profile), WidgetConfig::DivingProfile(widget)) => {
                profile.update(widget, scene, raster_scale, transient)?;
            }
            (PreviewWidget::Elevation(elevation), WidgetConfig::Elevation(widget)) => {
                elevation.update(widget, scene, raster_scale, transient)?;
            }
            (PreviewWidget::Route(route), WidgetConfig::Route(widget)) => {
                route.update(widget, scene, raster_scale, transient)?;
            }
            (PreviewWidget::Map(map), WidgetConfig::Map(widget)) => {
                map.update(widget, scene, raster_scale, maps, fonts)?;
            }
            (PreviewWidget::Raster(raster), WidgetConfig::Raster(widget)) => {
                raster.update(widget, scene, raster_scale, images)?;
            }
            _ => {
                self.widget =
                    PreviewWidget::new(applied.clone(), raster_scale, fonts, images, maps)?;
                self.rendered_revision = None;
            }
        }
        self.request = applied;
        self.raster_scale = raster_scale;
        self.origin = position;
        self.position = position;
        self.placed_geometry = None;
        self.sync();
        Ok(())
    }

    /// Picks up a family render. A placement edit does not touch family state, so its cached
    /// frame returns without drawing or readback.
    pub fn sync(&mut self) {
        let revision = self.widget.tile().revision;
        if self.rendered_revision == Some(revision) {
            return;
        }
        self.rendered_revision = Some(revision);
        self.pixels_revision = self.pixels_revision.wrapping_add(1);
        self.place_geometry();
        self.revision = self.revision.wrapping_add(1);
    }

    fn place_geometry(&mut self) {
        let [dx, dy] = self.offset();
        // Always translate the original frame, avoiding accumulated drag error.
        self.placed_geometry = match &self.widget.tile().geometry {
            Some(geometry) if self.position != self.origin => Some(geometry.translated(dx, dy)),
            _ => None,
        };
    }

    fn offset(&self) -> [f64; 2] {
        [
            self.position[0] - self.origin[0],
            self.position[1] - self.origin[1],
        ]
    }

    pub fn pixels(&self) -> &[u8] {
        &self.widget.tile().pixels
    }

    /// The presented geometry at the current placement, or `None` while the widget presents nothing.
    pub fn geometry(&self) -> Option<&WidgetGeometry> {
        self.placed_geometry
            .as_ref()
            .or(self.widget.tile().geometry.as_ref())
    }

    /// Tile bounds at the instance's current placement, matching [`Self::geometry`].
    pub fn bounds(&self) -> Option<TileBounds> {
        let [dx, dy] = self.offset();
        self.widget
            .tile()
            .bounds
            .map(|bounds| bounds.translated(dx, dy))
    }

    /// Changes with every placement or pixel change.
    pub fn revision(&self) -> u32 {
        self.revision
    }

    /// Changes only when the pixels do.
    pub fn pixels_revision(&self) -> u32 {
        self.pixels_revision
    }
}

/// A validated widget handed to the engine by a host other than the editor preview.
/// Rasters carry the image the host resolved and prepared for its raster scale.
pub enum ValidatedWidget<'a> {
    DivingProfile(&'a ValidatedDivingProfile),
    RollingChart(&'a ValidatedRollingChart),
    Elevation(&'a ValidatedElevationPlot),
    Route(&'a ValidatedRoutePlot),
    /// Maps carry the frames their host renders and the content it resolved, so that every
    /// instance of one widget shares one route line.
    Map {
        widget: &'a ValidatedMapWidget,
        /// The installed style the widget's own resolved to.
        style: MapStyleInfo,
        frames: Arc<dyn MapFrameProvider>,
        content: MapContent,
    },
    Raster {
        geometry: &'a RasterGeometry,
        source_id: String,
        image: RasterImage,
    },
    Backdrop(&'a ValidatedBackdrop),
    Label(&'a ValidatedLabel),
    Value(&'a ValidatedValue),
}

/// Per-family sampling result a frame is drawn from; static families always draw the same.
#[derive(Clone, PartialEq)]
pub enum FrameKey {
    Static,
    Sample(Option<f64>),
    /// Offset altitude and vertical speed.
    AltitudeTape(AltitudeTapeSample),
    GForce(GForceSample),
    Gauge(GaugeFrameState),
    LapTimer(LapTimerContent),
    SplitsRun(SplitsContent),
    Parts(MetricDisplayParts),
    /// Retained geometry identity and marker progress.
    Route(usize, f32),
    /// The camera, when GPS exists, and the marker's progress.
    Map {
        camera: Option<MapCamera>,
        progress: f32,
    },
    /// Retained geometry identity and frame state.
    Elevation(usize, ElevationFrameState),
    DivingProfile(DivingFrameState),
    /// The playhead second; the window slides with it.
    RollingChart(f64),
}

pub enum PreviewWidget {
    DivingProfile(Box<PreviewDivingProfile>),
    RollingChart(PreviewRollingChart),
    Elevation(Box<PreviewElevation>),
    Route(PreviewRoute),
    Map(Box<PreviewMap>),
    Raster(PreviewRaster),
    Backdrop(PreviewBackdrop),
    Label(PreviewLabel),
    Gradient(PreviewGradient),
    LeanAngle(PreviewLeanAngle),
    GForce(PreviewGForce),
    LapTimer(PreviewLapTimer),
    SplitsRun(PreviewSplitsRun),
    HeadingTape(PreviewHeading),
    HeadingCompass(PreviewHeadingCompass),
    HeartZones(PreviewHeartZones),
    AltitudeTape(PreviewAltitudeTape),
    Gauge(Box<PreviewGauge>),
    Value(PreviewValue),
}

impl PreviewWidget {
    pub fn new(
        request: WidgetRequest,
        raster_scale: PreviewRasterScale,
        fonts: &dyn FontResolver,
        images: &(impl ImageSource + ?Sized),
        maps: &PreviewMaps,
    ) -> WidgetResult<Self> {
        let WidgetRequest { widget, scene, .. } = request;
        let scale = raster_scale;
        Ok(match widget {
            WidgetConfig::DivingProfile(widget) => {
                Self::DivingProfile(Box::new(PreviewDivingProfile::new(widget, scene, scale)?))
            }
            WidgetConfig::RollingChart(widget) => {
                Self::RollingChart(PreviewRollingChart::new(widget, scene, scale, fonts)?)
            }
            WidgetConfig::Elevation(widget) => {
                Self::Elevation(Box::new(PreviewElevation::new(widget, scene, scale)?))
            }
            WidgetConfig::Route(widget) => Self::Route(PreviewRoute::new(widget, scene, scale)?),
            WidgetConfig::Map(widget) => Self::Map(Box::new(PreviewMap::new(
                widget, scene, scale, maps, fonts,
            )?)),
            WidgetConfig::Raster(widget) => {
                Self::Raster(PreviewRaster::new(widget, scene, scale, images)?)
            }
            WidgetConfig::Backdrop(widget) => {
                Self::Backdrop(PreviewBackdrop::new(widget, scene, scale)?)
            }
            WidgetConfig::Label(widget) => {
                Self::Label(PreviewLabel::new(widget, scene, scale, fonts)?)
            }
            WidgetConfig::Value(widget) => match ValueFamily::of(widget.value, widget.display_type)
            {
                ValueFamily::Gradient => {
                    Self::Gradient(PreviewGradient::new(*widget, scene, scale)?)
                }
                ValueFamily::LeanAngle => {
                    Self::LeanAngle(PreviewLeanAngle::new(*widget, scene, scale)?)
                }
                ValueFamily::GForce => Self::GForce(PreviewGForce::new(*widget, scene, scale)?),
                ValueFamily::LapTimer => {
                    Self::LapTimer(PreviewLapTimer::new(*widget, scene, scale)?)
                }
                ValueFamily::SplitsRun => {
                    Self::SplitsRun(PreviewSplitsRun::new(*widget, scene, scale)?)
                }
                ValueFamily::HeadingTape => {
                    Self::HeadingTape(PreviewHeading::new(*widget, scene, scale, fonts)?)
                }
                ValueFamily::HeadingCompass => {
                    Self::HeadingCompass(PreviewHeadingCompass::new(*widget, scene, scale, fonts)?)
                }
                ValueFamily::HeartZones => {
                    Self::HeartZones(PreviewHeartZones::new(*widget, scene, scale, fonts)?)
                }
                ValueFamily::AltitudeTape => {
                    Self::AltitudeTape(PreviewAltitudeTape::new(*widget, scene, scale, fonts)?)
                }
                ValueFamily::Gauge => {
                    Self::Gauge(Box::new(PreviewGauge::new(*widget, scene, scale)?))
                }
                ValueFamily::Text => Self::Value(PreviewValue::new(*widget, scene, scale, fonts)?),
            },
        })
    }

    /// Builds a validated widget for a host other than the editor preview.
    pub fn from_validated(
        widget: ValidatedWidget<'_>,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        Ok(match widget {
            ValidatedWidget::DivingProfile(widget) => Self::DivingProfile(Box::new(
                PreviewDivingProfile::from_validated(widget.clone(), scene.clone(), host)?,
            )),
            ValidatedWidget::RollingChart(widget) => Self::RollingChart(
                PreviewRollingChart::from_validated(widget, scene, host, fonts)?,
            ),
            ValidatedWidget::Elevation(widget) => Self::Elevation(Box::new(
                PreviewElevation::from_validated(widget.clone(), scene.clone(), host)?,
            )),
            ValidatedWidget::Route(widget) => Self::Route(PreviewRoute::from_validated(
                widget.clone(),
                scene.clone(),
                host,
            )?),
            ValidatedWidget::Map {
                widget,
                style,
                frames,
                content,
            } => Self::Map(Box::new(PreviewMap::from_validated(
                widget.clone(),
                style,
                scene.clone(),
                host,
                frames,
                content,
                fonts,
            )?)),
            ValidatedWidget::Raster {
                geometry,
                source_id,
                image,
            } => Self::Raster(PreviewRaster::from_validated(
                geometry, scene, host, source_id, image,
            )?),
            ValidatedWidget::Backdrop(widget) => Self::Backdrop(PreviewBackdrop::from_validated(
                widget.clone(),
                scene,
                host,
            )?),
            ValidatedWidget::Label(widget) => {
                Self::Label(PreviewLabel::from_validated(widget, scene, host, fonts)?)
            }
            ValidatedWidget::Value(value) => match value {
                ValidatedValue::StandardText(widget) => Self::Value(PreviewValue::from_validated(
                    ValuePresentation::Metric(widget.clone()),
                    scene,
                    host,
                    fonts,
                )?),
                ValidatedValue::TimeText(widget) => Self::Value(PreviewValue::from_validated(
                    ValuePresentation::time(widget.clone(), host)?,
                    scene,
                    host,
                    fonts,
                )?),
                ValidatedValue::Gradient(widget) => {
                    Self::Gradient(PreviewGradient::from_validated(widget.clone(), scene, host))
                }
                ValidatedValue::HeadingTape(widget) => {
                    Self::HeadingTape(PreviewHeading::from_validated(widget, scene, host, fonts)?)
                }
                ValidatedValue::HeadingCompass(widget) => Self::HeadingCompass(
                    PreviewHeadingCompass::from_validated(widget, scene, host, fonts)?,
                ),
                ValidatedValue::HeartZones(widget) => Self::HeartZones(
                    PreviewHeartZones::from_validated(widget, scene, host, fonts)?,
                ),
                ValidatedValue::AltitudeTape(widget) => Self::AltitudeTape(
                    PreviewAltitudeTape::from_validated(widget, scene, host, fonts)?,
                ),
                ValidatedValue::LeanAngle(widget) => {
                    Self::LeanAngle(PreviewLeanAngle::from_validated(widget, scene, host)?)
                }
                ValidatedValue::Gauge(widget) => Self::Gauge(Box::new(
                    PreviewGauge::from_validated(widget.clone(), scene.clone(), host),
                )),
                ValidatedValue::GForce(widget) => {
                    Self::GForce(PreviewGForce::from_validated(widget.clone(), scene, host)?)
                }
                ValidatedValue::LapTimer(widget) => Self::LapTimer(
                    PreviewLapTimer::from_validated(widget.clone(), scene.clone(), host),
                ),
                ValidatedValue::SplitsRun(widget) => Self::SplitsRun(
                    PreviewSplitsRun::from_validated(widget.clone(), scene.clone(), host),
                ),
            },
        })
    }
    /// What the frame at `second` depends on, from sampling alone. Equal keys draw identical
    /// pixels, so a host may reuse the tile it drew for the previous key.
    pub fn frame_key(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<FrameKey> {
        Ok(match self {
            Self::DivingProfile(widget) => {
                FrameKey::DivingProfile(widget.frame_key(activity, second, fonts)?)
            }
            Self::RollingChart(widget) => FrameKey::RollingChart(widget.frame_key(second)),
            Self::Elevation(widget) => {
                let (geometry, state) = widget.frame_key(activity, second, fonts)?;
                FrameKey::Elevation(geometry, state)
            }
            Self::Route(widget) => {
                let (geometry, progress) = widget.frame_key(activity, second)?;
                FrameKey::Route(geometry, progress)
            }
            Self::Map(widget) => {
                let state = widget.frame_state(activity, second)?;
                FrameKey::Map {
                    camera: state.camera,
                    progress: state.progress,
                }
            }
            Self::Raster(_) | Self::Backdrop(_) | Self::Label(_) => FrameKey::Static,
            Self::Gradient(widget) => FrameKey::Sample(widget.frame_key(activity, second)),
            Self::LeanAngle(widget) => FrameKey::Sample(widget.frame_key(activity, second)),
            Self::GForce(widget) => FrameKey::GForce(widget.frame_key(activity, second)),
            Self::LapTimer(widget) => FrameKey::LapTimer(widget.frame_key(activity, second)?),
            Self::SplitsRun(widget) => FrameKey::SplitsRun(widget.frame_key(activity, second)),
            Self::HeadingTape(widget) => FrameKey::Sample(widget.frame_key(activity, second)),
            Self::HeadingCompass(widget) => FrameKey::Sample(widget.frame_key(activity, second)),
            Self::HeartZones(widget) => FrameKey::Sample(widget.frame_key(activity, second)),
            Self::AltitudeTape(widget) => {
                FrameKey::AltitudeTape(widget.frame_key(activity, second))
            }
            Self::Gauge(widget) => FrameKey::Gauge(widget.frame_key(activity, second)),
            Self::Value(widget) => FrameKey::Parts(widget.frame_key(activity, second)),
        })
    }
    /// The raster the widget last drew.
    pub fn tile(&self) -> &Tile {
        match self {
            Self::DivingProfile(widget) => &widget.tile,
            Self::RollingChart(widget) => &widget.tile,
            Self::Elevation(widget) => &widget.tile,
            Self::Route(widget) => &widget.tile,
            Self::Map(widget) => &widget.tile,
            Self::Raster(widget) => &widget.tile,
            Self::Backdrop(widget) => &widget.tile,
            Self::Label(widget) => &widget.tile,
            Self::Gradient(widget) => &widget.tile,
            Self::LeanAngle(widget) => &widget.tile,
            Self::GForce(widget) => &widget.tile,
            Self::LapTimer(widget) => &widget.tile,
            Self::SplitsRun(widget) => &widget.tile,
            Self::HeadingTape(widget) => &widget.tile,
            Self::HeadingCompass(widget) => &widget.tile,
            Self::HeartZones(widget) => &widget.tile,
            Self::AltitudeTape(widget) => &widget.tile,
            Self::Gauge(widget) => &widget.tile,
            Self::Value(widget) => &widget.tile,
        }
    }

    /// Scene rectangle a render at `second` would draw into, from layout alone: no surface
    /// is allocated and nothing is drawn. Rasters report their rotated scene extent.
    pub fn layout_bounds(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        match self {
            Self::DivingProfile(widget) => widget.layout_bounds(activity, fonts),
            Self::RollingChart(widget) => Ok(widget.layout_bounds()),
            Self::Elevation(widget) => widget.layout_bounds(activity, second, fonts),
            Self::Route(widget) => widget.layout_bounds(activity, second),
            Self::Map(widget) => Ok(widget.layout_bounds()),
            Self::Raster(widget) => Ok(widget.layout_bounds()),
            Self::Backdrop(widget) => Ok(widget.layout_bounds()),
            Self::Label(widget) => Ok(widget.layout_bounds()),
            Self::Gradient(widget) => widget.layout_bounds(activity, second, fonts),
            Self::LeanAngle(widget) => widget.layout_bounds(activity, second, fonts),
            Self::GForce(widget) => widget.layout_bounds(activity, second, fonts),
            Self::LapTimer(widget) => widget.layout_bounds(activity, second, fonts),
            Self::SplitsRun(widget) => widget.layout_bounds(activity, second, fonts),
            Self::HeadingTape(widget) => Ok(widget.layout_bounds(activity, second)),
            Self::HeadingCompass(widget) => widget.layout_bounds(activity, second, fonts),
            Self::HeartZones(widget) => Ok(widget.layout_bounds()),
            Self::AltitudeTape(widget) => Ok(widget.layout_bounds()),
            Self::Gauge(widget) => widget.layout_bounds(activity, second, fonts),
            Self::Value(widget) => widget.layout_bounds(activity, second, fonts),
        }
    }
    /// Starts any frame drawn outside this thread for `second`, so `draw` need not wait for all of it.
    pub fn start_frame(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> WidgetResult<()> {
        match self {
            Self::Map(widget) => widget.start_frame(activity, second),
            _ => Ok(()),
        }
    }

    /// Whether `draw` may wait on a frame `start_frame` began.
    pub fn draws_a_started_frame(&self) -> bool {
        matches!(self, Self::Map(_))
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates at raster scale 1,
    /// returning the rectangle it drew into, which equals `layout_bounds`.
    pub fn draw(
        &mut self,
        canvas: &Canvas,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        match self {
            Self::DivingProfile(widget) => widget.draw(canvas, activity, second, fonts),
            Self::RollingChart(widget) => Ok(widget.draw(canvas, activity, second)),
            Self::Elevation(widget) => widget.draw(canvas, activity, second, fonts),
            Self::Route(widget) => widget.draw(canvas, activity, second),
            Self::Map(widget) => widget.draw(canvas, activity, second),
            Self::Raster(widget) => widget.draw(canvas),
            Self::Backdrop(widget) => Ok(widget.draw(canvas)),
            Self::Label(widget) => widget.draw(canvas),
            Self::Gradient(widget) => widget.draw(canvas, activity, second, fonts),
            Self::LeanAngle(widget) => widget.draw(canvas, activity, second, fonts),
            Self::GForce(widget) => widget.draw(canvas, activity, second, fonts),
            Self::LapTimer(widget) => widget.draw(canvas, activity, second, fonts),
            Self::SplitsRun(widget) => widget.draw(canvas, activity, second, fonts),
            Self::HeadingTape(widget) => Ok(widget.draw(canvas, activity, second)),
            Self::HeadingCompass(widget) => widget.draw(canvas, activity, second, fonts),
            Self::HeartZones(widget) => Ok(widget.draw(canvas, activity, second)),
            Self::AltitudeTape(widget) => Ok(widget.draw(canvas, activity, second)),
            Self::Gauge(widget) => widget.draw(canvas, activity, second, fonts),
            Self::Value(widget) => widget.draw(canvas, activity, second, fonts),
        }
    }

    /// Draws the frame at `second` into the widget's tile. Static families render on creation
    /// and update only.
    pub fn render(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<()> {
        match self {
            Self::DivingProfile(widget) => widget.render(activity, second, fonts),
            Self::RollingChart(widget) => widget.render(activity, second),
            Self::Elevation(widget) => widget.render(activity, second, fonts),
            Self::Route(widget) => widget.render(activity, second),
            Self::Map(widget) => widget.render(activity, second),
            Self::Raster(_) | Self::Backdrop(_) | Self::Label(_) => Ok(()),
            Self::Gradient(widget) => widget.render(activity, second, fonts),
            Self::LeanAngle(widget) => widget.render(activity, second, fonts),
            Self::GForce(widget) => widget.render(activity, second, fonts),
            Self::LapTimer(widget) => widget.render(activity, second, fonts),
            Self::SplitsRun(widget) => widget.render(activity, second, fonts),
            Self::HeadingTape(widget) => widget.render(activity, second),
            Self::HeadingCompass(widget) => widget.render(activity, second, fonts),
            Self::HeartZones(widget) => widget.render(activity, second),
            Self::AltitudeTape(widget) => widget.render(activity, second),
            Self::Gauge(widget) => widget.render(activity, second, fonts),
            Self::Value(widget) => widget.render(activity, second, fonts),
        }
    }

    /// Forgets what the widget prepared from the previous activity.
    pub fn invalidate_activity(&mut self) {
        match self {
            Self::DivingProfile(widget) => widget.invalidate_activity(),
            Self::RollingChart(widget) => widget.invalidate_activity(),
            Self::Elevation(widget) => widget.invalidate_activity(),
            Self::Route(widget) => widget.invalidate_activity(),
            Self::Map(widget) => widget.invalidate_activity(),
            Self::LapTimer(widget) => widget.invalidate_activity(),
            Self::SplitsRun(widget) => widget.invalidate_activity(),
            Self::Gauge(widget) => widget.invalidate_activity(),
            _ => {}
        }
    }
}

/// A resize gesture: the point of the content rectangle (`fixed`, a fraction of its size) that
/// stays at scene `point`, the playhead second whose layout is measured, and whether the gesture
/// ends here.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct ResizeGesture {
    pub fixed: [f32; 2],
    pub point: [f32; 2],
    pub second: f64,
    /// Profile families rebuild their prepared vectors on commit.
    #[serde(default)]
    pub commit: bool,
}

impl ResizeGesture {
    fn validate(&self) -> WidgetResult<()> {
        if !self.second.is_finite() {
            return Err(WidgetError::Config("previewSecond must be finite".into()));
        }
        if self
            .fixed
            .iter()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
            || self.point.iter().any(|value| !value.is_finite())
        {
            return Err(WidgetError::Config("invalid widget resize gesture".into()));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
pub struct ResizeRequest {
    #[serde(flatten)]
    pub origin: WidgetRequest,
    /// Intrinsic families scale uniformly by this factor.
    pub factor: Option<f32>,
    /// Framed families resize to these scene-pixel dimensions.
    pub size: Option<[f32; 2]>,
    /// Profile families measure the session's prepared vectors rather than rebuilding them.
    pub handle: Option<u32>,
    /// Heading inspector edits resize the frame to preserve tick lengths.
    pub label_offset: Option<f32>,
    /// A map's view mode pans and zooms inside its frame.
    pub map_view: Option<MapViewGesture>,
    #[serde(flatten)]
    pub gesture: ResizeGesture,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum ResizeUpdate {
    DivingProfile(crate::widgets::diving_profile::ResizeUpdate),
    RollingChart(crate::widgets::rolling_chart::ResizeUpdate),
    Elevation(crate::widgets::elevation::ResizeUpdate),
    Route(crate::widgets::route::ResizeUpdate),
    Map(crate::widgets::map::ResizeUpdate),
    MapView(crate::widgets::map::MapViewUpdate),
    Raster(crate::widgets::raster::ResizeUpdate),
    Backdrop(crate::widgets::backdrop::ResizeUpdate),
    Label(crate::widgets::label::ResizeUpdate),
    Gradient(crate::widgets::gradient::ResizeUpdate),
    LeanAngle(crate::widgets::lean_angle::ResizeUpdate),
    GForce(crate::widgets::g_force::ResizeUpdate),
    LapTimer(crate::widgets::lap_timer::ResizeUpdate),
    SplitsRun(crate::widgets::splits_run::ResizeUpdate),
    HeadingTape(crate::widgets::heading::ResizeUpdate),
    HeadingCompass(crate::widgets::heading_compass::ResizeUpdate),
    HeartZones(crate::widgets::heart_zones::ResizeUpdate),
    AltitudeTape(crate::widgets::altitude_tape::ResizeUpdate),
    Gauge(crate::widgets::gauges::ResizeUpdate),
    Value(crate::widgets::value::ResizeUpdate),
}

/// The editor preview reads these from scene extras and its session's raster scale; export supplies them typed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WidgetHost {
    pub(crate) raster_scale: PreviewRasterScale,
    /// Custom export range followed by plots that do not show the full activity.
    export_window: Option<RouteWindow>,
    /// Placeholder playback length for plots drawn without activity.
    placeholder_duration: Option<f64>,
    /// Clock origin for elapsed export time.
    export_start_seconds: Option<f64>,
}

impl WidgetHost {
    /// Export draws at native raster resolution and always knows its range and clock origin.
    pub fn export(
        export_window: Option<RouteWindow>,
        placeholder_duration: f64,
        export_start_seconds: f64,
    ) -> Self {
        Self {
            raster_scale: PreviewRasterScale::NATIVE,
            export_window,
            placeholder_duration: Some(placeholder_duration),
            export_start_seconds: Some(export_start_seconds),
        }
    }

    /// Preview extras are optional per family; a present extra must be well formed.
    pub fn from_scene(
        scene: &ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
    ) -> WidgetResult<Self> {
        let finite = |field: &str| {
            scene
                .extra
                .get(field)
                .map(|value| {
                    value
                        .as_f64()
                        .filter(|value| value.is_finite())
                        .ok_or_else(|| {
                            WidgetError::Config(format!("{field}: required finite second"))
                        })
                })
                .transpose()
        };
        let active = scene
            .extra
            .get("custom_export_range_active")
            .map(|value| {
                value.as_bool().ok_or_else(|| {
                    WidgetError::Config("custom_export_range_active must be boolean".into())
                })
            })
            .transpose()?
            .unwrap_or(false);
        let export_window = if active {
            let required = |field| {
                finite(field)?
                    .ok_or_else(|| WidgetError::Config(format!("{field}: required finite second")))
            };
            let (start, end) = (required("start")?, required("end")?);
            if start < 0.0 || end <= start {
                return Err(WidgetError::Config("invalid route export window".into()));
            }
            Some(RouteWindow { start, end })
        } else {
            None
        };
        let placeholder_duration = finite("preview_duration_seconds")?;
        if placeholder_duration.is_some_and(|duration| duration < 0.0) {
            return Err(WidgetError::Config(
                "preview_duration_seconds: required non-negative finite second".into(),
            ));
        }
        Ok(Self {
            raster_scale,
            export_window,
            placeholder_duration,
            export_start_seconds: finite("export_start_seconds")?,
        })
    }

    pub(crate) fn plot_window(&self, show_full_activity: bool) -> Option<RouteWindow> {
        self.export_window.filter(|_| !show_full_activity)
    }

    pub(crate) fn placeholder_duration(&self) -> WidgetResult<f64> {
        self.placeholder_duration.ok_or_else(|| {
            WidgetError::Config(
                "preview_duration_seconds: required non-negative finite second".into(),
            )
        })
    }

    pub(crate) fn export_start_seconds(&self) -> WidgetResult<f64> {
        self.export_start_seconds.ok_or_else(|| {
            WidgetError::Config("scene.export_start_seconds must be a finite number".into())
        })
    }
}
