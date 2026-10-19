//! Profile handles retain reduced geometry throughout translation and live resizing.
use super::{
    config::ElevationPlotConfig,
    geometry::{ElevationFrameState, ElevationGeometry, GeometryKey},
    interaction::{anchored_update, resize_elevation, ResizeUpdate},
    layout::ElevationLayout,
    validation::{validate_elevation_plot, ValidatedElevationPlot},
};
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    model::{
        scene::{validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation},
        validation::require_positive_f32,
    },
    render::{pixels::read_surface_rgba, text::FontResolver},
    widgets::{
        route::sampling::RouteWindow,
        value::preview::{surface, WidgetGeometry},
    },
};
use skia_safe::{Color, Surface};
use std::sync::Arc;

pub struct PreviewElevation {
    config: ValidatedElevationPlot,
    scene: ValidatedScenePresentation,
    window: Option<RouteWindow>,
    preview_duration: f64,
    preview_raster_scale: f32,
    samples: Option<Arc<Vec<(f32, f64, f32)>>>,
    pub prepared_geometry: Option<Arc<ElevationGeometry>>,
    placeholder_geometry: Option<Arc<ElevationGeometry>>,
    layout: Option<ElevationLayout>,
    last_state: Option<ElevationFrameState>,
    surface: Option<Surface>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    pub revision: u32,
}

impl PreviewElevation {
    /// Hosts without a viewport density keep native resolution; scene coordinates stay unchanged.
    fn preview_raster_scale(scene: &ScenePresentationConfig) -> WidgetResult<f32> {
        match scene.extra.get("preview_raster_scale") {
            None => Ok(1.0),
            Some(value) => require_positive_f32(
                value.as_f64().map(|value| value as f32),
                "scene.preview_raster_scale",
            ),
        }
    }

    fn timing(
        widget: &ElevationPlotConfig,
        scene: &ScenePresentationConfig,
    ) -> WidgetResult<(Option<RouteWindow>, f64)> {
        let window = RouteWindow::from_scene(scene, widget.show_full_activity)?;
        let duration = scene
            .extra
            .get("preview_duration_seconds")
            .and_then(serde_json::Value::as_f64)
            .filter(|v| v.is_finite() && *v >= 0.0)
            .ok_or_else(|| {
                WidgetError::Config(
                    "preview_duration_seconds: required non-negative finite second".into(),
                )
            })?;
        Ok((window, duration))
    }

    pub fn new(widget: ElevationPlotConfig, scene: ScenePresentationConfig) -> WidgetResult<Self> {
        let (window, preview_duration) = Self::timing(&widget, &scene)?;
        let preview_raster_scale = Self::preview_raster_scale(&scene)?;
        let scene = validate_scene_presentation(scene)?;
        let config = validate_elevation_plot(&widget, 0, &scene)?;
        Ok(Self {
            config,
            scene,
            window,
            preview_duration,
            preview_raster_scale,
            samples: None,
            prepared_geometry: None,
            placeholder_geometry: None,
            layout: None,
            last_state: None,
            surface: None,
            pixels: Vec::new(),
            geometry: Vec::new(),
            revision: 0,
        })
    }

    pub fn update(
        &mut self,
        widget: ElevationPlotConfig,
        scene: ScenePresentationConfig,
        transient: bool,
    ) -> WidgetResult<()> {
        let (window, preview_duration) = Self::timing(&widget, &scene)?;
        let preview_raster_scale = Self::preview_raster_scale(&scene)?;
        let scene = validate_scene_presentation(scene)?;
        let config = validate_elevation_plot(&widget, 0, &scene)?;
        if window != self.window {
            self.invalidate_activity();
        }
        if !transient
            && self
                .prepared_geometry
                .as_ref()
                .is_some_and(|g| g.key != GeometryKey::new(&config, scene.scale))
        {
            self.prepared_geometry = None;
            self.placeholder_geometry = None;
        }
        if self.config == config
            && self.scene == scene
            && self.window == window
            && self.preview_duration == preview_duration
            && self.preview_raster_scale == preview_raster_scale
        {
            return Ok(());
        }
        if let Some(layout) = self.layout.as_mut() {
            layout.set_position(config.x, config.y);
            if layout.config != config || layout.scene != scene {
                self.layout = None;
            }
        }
        self.config = config;
        self.scene = scene;
        self.window = window;
        self.preview_duration = preview_duration;
        self.preview_raster_scale = preview_raster_scale;
        self.last_state = None;
        Ok(())
    }

    pub fn invalidate_activity(&mut self) {
        self.samples = None;
        self.prepared_geometry = None;
        self.placeholder_geometry = None;
        self.layout = None;
        self.last_state = None;
    }

    fn prepare_geometry(&mut self, activity: Option<&ActivitySampler>) -> WidgetResult<()> {
        if self.prepared_geometry.is_some() {
            return Ok(());
        }
        if self.samples.is_none() {
            self.samples = Some(
                activity
                    .map(|a| a.elevation.samples(self.window))
                    .unwrap_or_else(|| Arc::new(Vec::new())),
            );
        }
        let samples = self.samples.as_ref().expect("elevation samples prepared");
        self.prepared_geometry = Some(Arc::new(if samples.is_empty() {
            ElevationGeometry::placeholder(&self.config, self.scene.scale)
        } else {
            ElevationGeometry::new(&self.config, self.scene.scale, samples)?
        }));
        Ok(())
    }

    fn geometry_at(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> WidgetResult<Arc<ElevationGeometry>> {
        self.prepare_geometry(activity)?;
        if activity.is_some_and(|a| second < 0.0 || second > a.elevation.end) {
            if self.placeholder_geometry.is_none() {
                self.placeholder_geometry = Some(Arc::new(ElevationGeometry::placeholder(
                    &self.config,
                    self.scene.scale,
                )));
            }
            Ok(self
                .placeholder_geometry
                .as_ref()
                .expect("placeholder elevation prepared")
                .clone())
        } else {
            Ok(self
                .prepared_geometry
                .as_ref()
                .expect("elevation geometry prepared")
                .clone())
        }
    }

    pub fn resize(
        &mut self,
        widget: ElevationPlotConfig,
        scene: ScenePresentationConfig,
        size: [f32; 2],
        fixed: [f32; 2],
        point: [f32; 2],
        second: f64,
        commit: bool,
        activity: Option<&ActivitySampler>,
    ) -> WidgetResult<ResizeUpdate> {
        let scene = validate_scene_presentation(scene)?;
        let mut config = validate_elevation_plot(&widget, 0, &scene)?;
        self.geometry_at(activity, second)?;
        resize_elevation(&mut config, &scene, size)?;
        if commit {
            let samples = self.samples.as_ref().expect("elevation samples prepared");
            self.prepared_geometry = Some(Arc::new(if samples.is_empty() {
                ElevationGeometry::placeholder(&config, scene.scale)
            } else {
                ElevationGeometry::new(&config, scene.scale, samples)?
            }));
            if self.placeholder_geometry.is_some() {
                self.placeholder_geometry = Some(Arc::new(ElevationGeometry::placeholder(
                    &config,
                    scene.scale,
                )));
            }
            self.layout = None;
            self.last_state = None;
        }
        Ok(anchored_update(&config, &scene, fixed, point))
    }

    pub fn render(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<()> {
        let geometry = self.geometry_at(activity, second)?;
        let state = if geometry.source_point_count == 0 {
            let progress = (second / self.preview_duration.max(1e-9)).clamp(0.0, 1.0) as f32;
            geometry.state(progress, None, progress)
        } else {
            let activity = &activity.expect("elevation activity prepared").elevation;
            activity.frame_state(
                &geometry,
                second,
                self.window,
                activity.elevation_at(second),
            )
        };
        let changed = self
            .layout
            .as_ref()
            .is_none_or(|layout| !Arc::ptr_eq(&layout.geometry, &geometry));
        if changed {
            let offset = activity.map_or(0.0, |a| {
                a.elevation.altitude_offset(self.config.starting_altitude_m)
            });
            self.layout = Some(ElevationLayout::new(
                self.config.clone(),
                self.scene.clone(),
                geometry,
                offset,
                activity.and_then(|a| a.elevation.elevation_range()),
                fonts,
            )?);
        } else if self.last_state == Some(state) {
            return Ok(());
        }
        let layout = self.layout.as_ref().expect("elevation layout prepared");
        let frame = layout.frame(state)?;
        let (width, height) = (
            (frame.bounds.width() * self.preview_raster_scale).ceil() as u32,
            (frame.bounds.height() * self.preview_raster_scale).ceil() as u32,
        );
        if self.surface.as_ref().map(|s| (s.width(), s.height()))
            != Some((width as i32, height as i32))
        {
            self.surface = Some(surface(width, height)?);
            self.pixels.resize(width as usize * height as usize * 4, 0);
        }
        let surface = self.surface.as_mut().expect("elevation surface allocated");
        let canvas = surface.canvas();
        canvas.clear(Color::TRANSPARENT);
        canvas.save();
        canvas.scale((self.preview_raster_scale, self.preview_raster_scale));
        canvas.translate((-frame.bounds.left, -frame.bounds.top));
        layout.draw(canvas, &frame);
        canvas.restore();
        read_surface_rgba(surface, &mut self.pixels)?;
        if changed || self.last_state.is_none() {
            #[derive(serde::Serialize)]
            struct Geometry {
                #[serde(flatten)]
                widget: WidgetGeometry,
                rotation: f32,
                rotation_origin: [f32; 2],
                preview_raster_scale: f32,
            }
            self.geometry = serde_json::to_vec(&Geometry {
                widget: WidgetGeometry {
                    left: frame.bounds.left,
                    top: frame.bounds.top,
                    width,
                    height,
                    content_width: frame.content_bounds.width,
                    content: frame.content_bounds,
                },
                rotation: self.config.rotation,
                rotation_origin: frame.content_bounds.rotation_origin(),
                preview_raster_scale: self.preview_raster_scale,
            })
            .map_err(|e| WidgetError::Render(e.to_string()))?;
        }
        self.revision = self.revision.wrapping_add(1);
        self.last_state = Some(state);
        Ok(())
    }
}
