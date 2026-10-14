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
    model::scene::{
        validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
    },
    widgets::value::preview::{surface, WidgetGeometry},
};
use skia_safe::{AlphaType, Color, ColorType, ImageInfo, Surface};
use std::sync::Arc;

pub struct PreviewRoute {
    config: ValidatedRoutePlot,
    scene: ValidatedScenePresentation,
    window: Option<RouteWindow>,
    preview_duration: f64,
    samples: Option<Arc<Vec<RouteSample>>>,
    pub prepared_geometry: Option<Arc<RouteGeometry>>,
    placeholder_geometry: Option<Arc<RouteGeometry>>,
    layout: Option<RouteLayout>,
    placeholder: bool,
    last_progress: Option<f32>,
    surface: Option<Surface>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    pub revision: u32,
}

impl PreviewRoute {
    fn preview_duration(scene: &ScenePresentationConfig) -> WidgetResult<f64> {
        scene
            .extra
            .get("preview_duration_seconds")
            .and_then(serde_json::Value::as_f64)
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| {
                WidgetError::Config(
                    "preview_duration_seconds: required non-negative finite second".into(),
                )
            })
    }

    pub fn new(widget: CoursePlotConfig, scene: ScenePresentationConfig) -> WidgetResult<Self> {
        let config = validate_route_plot(&widget, 0)?;
        let window = RouteWindow::from_scene(&scene, config.show_full_activity)?;
        let preview_duration = Self::preview_duration(&scene)?;
        Ok(Self {
            config,
            scene: validate_scene_presentation(scene)?,
            window,
            preview_duration,
            samples: None,
            prepared_geometry: None,
            placeholder_geometry: None,
            layout: None,
            placeholder: true,
            last_progress: None,
            surface: None,
            pixels: Vec::new(),
            geometry: Vec::new(),
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
        let window = RouteWindow::from_scene(&scene, config.show_full_activity)?;
        let preview_duration = Self::preview_duration(&scene)?;
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

    pub fn render(&mut self, activity: Option<&ActivitySampler>, second: f64) -> WidgetResult<()> {
        let geometry = self.geometry_at(activity, second)?;
        let progress = self.progress(activity, second, geometry.source_point_count == 0);
        let layout_changed = self
            .layout
            .as_ref()
            .is_none_or(|layout| !Arc::ptr_eq(&layout.geometry, &geometry));
        if layout_changed {
            self.layout = Some(RouteLayout::new(
                self.config.clone(),
                self.scene.clone(),
                geometry,
            )?);
        } else if self.last_progress == Some(progress) {
            return Ok(());
        }
        let layout = self.layout.as_ref().expect("route layout prepared");
        let frame = layout.frame(progress);
        let (width, height) = (frame.bounds.width() as u32, frame.bounds.height() as u32);
        if self
            .surface
            .as_ref()
            .map(|surface| (surface.width(), surface.height()))
            != Some((width as i32, height as i32))
        {
            self.surface = Some(surface(width, height)?);
            self.pixels.resize(width as usize * height as usize * 4, 0);
        }
        let surface = self.surface.as_mut().expect("route surface allocated");
        let canvas = surface.canvas();
        canvas.clear(Color::TRANSPARENT);
        canvas.save();
        canvas.translate((-frame.bounds.left, -frame.bounds.top));
        layout.draw(canvas, &frame);
        canvas.restore();
        // Matching the surface format keeps Skia on its row-copy path instead of
        // running the floating-point conversion pipeline over the entire raster.
        let info = ImageInfo::new(
            (width as i32, height as i32),
            ColorType::RGBA8888,
            AlphaType::Premul,
            None,
        );
        if !surface.read_pixels(&info, &mut self.pixels, width as usize * 4, (0, 0)) {
            return Err(WidgetError::Render(
                "unable to read route RGBA pixels".into(),
            ));
        }
        // Browser ImageData needs straight alpha. Transparent and opaque pixels
        // already match; only translucent RGB channels need unpremultiplication.
        for pixel in self.pixels.chunks_exact_mut(4) {
            let alpha = u32::from(pixel[3]);
            if alpha == 0 || alpha == 255 {
                continue;
            }
            for channel in &mut pixel[..3] {
                *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha) as u8;
            }
        }
        if layout_changed || self.last_progress.is_none() {
            #[derive(serde::Serialize)]
            struct Geometry {
                #[serde(flatten)]
                widget: WidgetGeometry,
                rotation: f32,
                rotation_origin: [f32; 2],
            }
            let rotation_origin = frame.content_bounds.rotation_origin();
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
                rotation_origin,
            })
            .map_err(|e| WidgetError::Render(e.to_string()))?;
        }
        self.revision = self.revision.wrapping_add(1);
        self.last_progress = Some(progress);
        Ok(())
    }
}
