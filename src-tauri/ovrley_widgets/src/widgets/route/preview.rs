//! A route handle retains geometry across translations and live resize drafts.
use super::{
    config::CoursePlotConfig,
    geometry::{GeometryKey, RouteGeometry, RouteSample},
    interaction::{resize_route, scale_route, ResizeUpdate},
    layout::RouteLayout,
    sampling::RouteWindow,
    validation::{validate_route_plot, ValidatedRoutePlot},
};
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    host::WidgetHost,
    model::scene::{
        validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
    },
    render::preview::{render_tile, PreviewRasterScale},
    widgets::value::preview::{SHAPE_COLOR_TYPE, TileBounds, WidgetGeometry},
};
use skia_safe::{Canvas, Surface};
use std::sync::Arc;

pub struct PreviewRoute {
    config: ValidatedRoutePlot,
    scene: ValidatedScenePresentation,
    window: Option<RouteWindow>,
    preview_duration: f64,
    preview_raster_scale: PreviewRasterScale,
    samples: Option<Arc<Vec<RouteSample>>>,
    pub prepared_geometry: Option<Arc<RouteGeometry>>,
    placeholder_geometry: Option<Arc<RouteGeometry>>,
    layout: Option<RouteLayout>,
    placeholder: bool,
    last_progress: Option<f32>,
    surface: Option<Surface>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    /// Scene bounds of the last rendered tile; absent until the first render.
    pub bounds: Option<TileBounds>,
    pub revision: u32,
}

impl PreviewRoute {
    pub fn new(widget: CoursePlotConfig, scene: ScenePresentationConfig) -> WidgetResult<Self> {
        let config = validate_route_plot(&widget, 0)?;
        let host = WidgetHost::from_scene(&scene)?;
        Self::from_validated(config, validate_scene_presentation(scene)?, &host)
    }

    pub fn from_validated(
        config: ValidatedRoutePlot,
        scene: ValidatedScenePresentation,
        host: &WidgetHost,
    ) -> WidgetResult<Self> {
        Ok(Self {
            window: host.plot_window(config.show_full_activity),
            preview_duration: host.placeholder_duration()?,
            preview_raster_scale: host.raster_scale,
            config,
            scene,
            samples: None,
            prepared_geometry: None,
            placeholder_geometry: None,
            layout: None,
            placeholder: true,
            last_progress: None,
            surface: None,
            pixels: Vec::new(),
            geometry: Vec::new(),
            bounds: None,
            revision: 0,
        })
    }

    pub fn update(
        &mut self,
        widget: CoursePlotConfig,
        scene: ScenePresentationConfig,
        transient: bool,
    ) -> WidgetResult<()> {
        let config = validate_route_plot(&widget, 0)?;
        let host = WidgetHost::from_scene(&scene)?;
        let window = host.plot_window(config.show_full_activity);
        let preview_duration = host.placeholder_duration()?;
        let preview_raster_scale = host.raster_scale;
        let scene = validate_scene_presentation(scene)?;
        if window != self.window {
            self.invalidate_activity();
        }
        if !transient
            && self
                .prepared_geometry
                .as_ref()
                .is_some_and(|geometry| geometry.key != GeometryKey::new(&config))
        {
            self.prepared_geometry = None;
            self.placeholder_geometry = None;
            self.layout = None;
        }
        if let Some(layout) = self.layout.as_mut() {
            // x/y never participate in path or static-layer preparation.
            layout.set_position(config.x, config.y);
            if layout.config != config
                || self.scene.scale != scene.scale
                || self.scene.opacity != scene.opacity
                || self.scene.shadow_color != scene.shadow_color
                || self.scene.shadow_strength != scene.shadow_strength
                || self.scene.shadow_distance != scene.shadow_distance
            {
                self.layout = None;
            }
        }
        self.config = config;
        self.scene = scene;
        self.window = window;
        self.preview_duration = preview_duration;
        self.preview_raster_scale = preview_raster_scale;
        self.last_progress = None;
        Ok(())
    }

    pub fn invalidate_activity(&mut self) {
        self.samples = None;
        self.prepared_geometry = None;
        self.placeholder_geometry = None;
        self.layout = None;
        self.last_progress = None;
    }

    fn prepare_geometry(&mut self, activity: Option<&ActivitySampler>) -> WidgetResult<()> {
        if self.prepared_geometry.is_some() {
            return Ok(());
        }
        if self.samples.is_none() {
            self.samples = Some(
                activity
                    .map(|activity| activity.route.samples(self.window))
                    .unwrap_or_else(|| Arc::new(Vec::new())),
            );
        }
        let samples = self.samples.as_ref().expect("route samples prepared");
        self.placeholder = samples.len() < 2;
        self.prepared_geometry = Some(Arc::new(if self.placeholder {
            RouteGeometry::placeholder(&self.config)?
        } else {
            RouteGeometry::new(&self.config, samples)?
        }));
        Ok(())
    }

    fn geometry_at(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> WidgetResult<Arc<RouteGeometry>> {
        self.prepare_geometry(activity)?;
        if !self.placeholder
            && activity.is_some_and(|activity| second < 0.0 || second > activity.route.end)
        {
            if self.placeholder_geometry.is_none() {
                self.placeholder_geometry =
                    Some(Arc::new(RouteGeometry::placeholder(&self.config)?));
            }
            Ok(self
                .placeholder_geometry
                .as_ref()
                .expect("placeholder route prepared")
                .clone())
        } else {
            Ok(self
                .prepared_geometry
                .as_ref()
                .expect("route geometry prepared")
                .clone())
        }
    }

    fn progress(&self, activity: Option<&ActivitySampler>, second: f64, placeholder: bool) -> f32 {
        if placeholder {
            (second / self.preview_duration.max(1e-9)).clamp(0.0, 1.0) as f32
        } else {
            activity
                .expect("route telemetry activity")
                .route
                .progress_at(second, self.window)
        }
    }

    pub fn resize(
        &mut self,
        widget: CoursePlotConfig,
        scene: ScenePresentationConfig,
        factor: f32,
        fixed: [f32; 2],
        point: [f32; 2],
        second: f64,
        commit: bool,
        activity: Option<&ActivitySampler>,
    ) -> WidgetResult<ResizeUpdate> {
        let mut config = validate_route_plot(&widget, 0)?;
        let scene = validate_scene_presentation(scene)?;
        self.prepare_geometry(activity)?;
        scale_route(&mut config, factor);
        if commit {
            let geometry = if self.placeholder {
                RouteGeometry::placeholder(&config)?
            } else {
                RouteGeometry::new(
                    &config,
                    self.samples.as_ref().expect("route samples prepared"),
                )?
            };
            self.prepared_geometry = Some(Arc::new(geometry));
            if self.placeholder_geometry.is_some() {
                self.placeholder_geometry = Some(Arc::new(RouteGeometry::placeholder(&config)?));
            }
            self.layout = None;
            self.last_progress = None;
        }
        let geometry = self.geometry_at(activity, second)?;
        Ok(resize_route(&config, &scene, &geometry, fixed, point))
    }

    /// Scene rectangle a render at `second` draws into; marker overflow is reserved for the whole path.
    pub fn layout_bounds(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> WidgetResult<TileBounds> {
        let geometry = self.geometry_at(activity, second)?;
        Ok(RouteLayout::measure(&self.config, &self.scene, &geometry).into())
    }

    /// Prepares the layout for the geometry at `second`; reports whether it was rebuilt.
    fn layout(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> WidgetResult<bool> {
        let geometry = self.geometry_at(activity, second)?;
        let changed = self
            .layout
            .as_ref()
            .is_none_or(|layout| !Arc::ptr_eq(&layout.geometry, &geometry));
        if changed {
            self.layout = Some(RouteLayout::new(
                self.config.clone(),
                self.scene.clone(),
                geometry,
            )?);
        }
        Ok(changed)
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(
        &mut self,
        canvas: &Canvas,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> WidgetResult<TileBounds> {
        self.layout(activity, second)?;
        let layout = self.layout.as_ref().expect("route layout prepared");
        let progress = self.progress(activity, second, layout.geometry.source_point_count == 0);
        let frame = layout.frame(progress);
        layout.draw(canvas, &frame);
        Ok(frame.bounds.into())
    }

    /// What the frame at `second` depends on: the drawn geometry, which the widget retains, and
    /// the marker progress. Equal keys draw identical pixels.
    pub fn frame_key(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> WidgetResult<(usize, f32)> {
        self.layout(activity, second)?;
        let geometry = &self.layout.as_ref().expect("route layout prepared").geometry;
        let progress = self.progress(activity, second, geometry.source_point_count == 0);
        Ok((Arc::as_ptr(geometry) as usize, progress))
    }

    pub fn render(&mut self, activity: Option<&ActivitySampler>, second: f64) -> WidgetResult<()> {
        let layout_changed = self.layout(activity, second)?;
        let geometry = &self.layout.as_ref().expect("route layout prepared").geometry;
        let progress = self.progress(activity, second, geometry.source_point_count == 0);
        if !layout_changed && self.last_progress == Some(progress) {
            return Ok(());
        }
        let layout = self.layout.as_ref().expect("route layout prepared");
        let frame = layout.frame(progress);
        let (width, height) = render_tile(
            &mut self.surface,
            &mut self.pixels,
            SHAPE_COLOR_TYPE,
            self.preview_raster_scale,
            frame.bounds,
            |canvas| {
                layout.draw(canvas, &frame);
                Ok(())
            },
        )?;
        if layout_changed || self.last_progress.is_none() {
            #[derive(serde::Serialize)]
            struct Geometry {
                #[serde(flatten)]
                widget: WidgetGeometry,
                rotation: f32,
                rotation_origin: [f32; 2],
            }
            let rotation_origin = frame.content_bounds.rotation_origin();
            let tile = TileBounds::from(frame.bounds);
            self.geometry = serde_json::to_vec(&Geometry {
                widget: WidgetGeometry::new(
                    tile,
                    (width, height),
                    self.preview_raster_scale.value(),
                    frame.content_bounds.width,
                    frame.content_bounds,
                ),
                rotation: self.config.rotation,
                rotation_origin,
            })
            .map_err(|e| WidgetError::Render(e.to_string()))?;
            self.bounds = Some(tile);
        }
        self.revision = self.revision.wrapping_add(1);
        self.last_progress = Some(progress);
        Ok(())
    }
}
