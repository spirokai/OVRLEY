//! Preview sessions retain committed vectors during live resize and cache static drawing.
use super::{
    config::DivingProfileConfig,
    geometry::{DivingFrameState, DivingGeometry, GeometryKey},
    interaction::{anchored_update, resize_diving_profile, ResizeUpdate},
    layout::DivingProfileLayout,
    validation::{validate_diving_profile, ValidatedDivingProfile},
};
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    model::scene::{
        validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
    },
    render::{pixels::read_surface_rgba, text::FontResolver},
    widgets::{
        route::sampling::RouteWindow,
        value::preview::{surface, WidgetGeometry},
    },
};
use skia_safe::{Color, Surface};
use std::sync::Arc;

pub struct PreviewDivingProfile {
    config: ValidatedDivingProfile,
    scene: ValidatedScenePresentation,
    window: Option<RouteWindow>,
    preview_duration: f64,
    pub prepared_geometry: Option<Arc<DivingGeometry>>,
    layout: Option<DivingProfileLayout>,
    last_state: Option<DivingFrameState>,
    surface: Option<Surface>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    pub revision: u32,
}

impl PreviewDivingProfile {
    fn timing(
        widget: &DivingProfileConfig,
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

    pub fn new(widget: DivingProfileConfig, scene: ScenePresentationConfig) -> WidgetResult<Self> {
        let (window, preview_duration) = Self::timing(&widget, &scene)?;
        let scene = validate_scene_presentation(scene)?;
        let config = validate_diving_profile(&widget, 0, &scene)?;
        Ok(Self {
            config,
            scene,
            window,
            preview_duration,
            prepared_geometry: None,
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
        widget: DivingProfileConfig,
        scene: ScenePresentationConfig,
        transient: bool,
    ) -> WidgetResult<()> {
        let (window, preview_duration) = Self::timing(&widget, &scene)?;
        let scene = validate_scene_presentation(scene)?;
        let config = validate_diving_profile(&widget, 0, &scene)?;
        if window != self.window
            || (!transient
                && self
                    .prepared_geometry
                    .as_ref()
                    .is_some_and(|g| g.key != GeometryKey::new(&config, scene.scale)))
        {
            self.invalidate_activity();
        }
        if self.config == config
            && self.scene == scene
            && self.window == window
            && self.preview_duration == preview_duration
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
        self.last_state = None;
        Ok(())
    }

    pub fn invalidate_activity(&mut self) {
        self.prepared_geometry = None;
        self.layout = None;
        self.last_state = None;
    }

    fn geometry_at(&mut self, activity: Option<&ActivitySampler>) -> Arc<DivingGeometry> {
        self.prepared_geometry
            .get_or_insert_with(|| {
                Arc::new(match activity {
                    Some(a) => DivingGeometry::new(
                        &self.config,
                        self.scene.scale,
                        &a.diving
                            .samples(self.window, self.config.width * self.scene.scale),
                    ),
                    None => DivingGeometry::placeholder(&self.config, self.scene.scale),
                })
            })
            .clone()
    }

    pub fn resize(
        &mut self,
        widget: DivingProfileConfig,
        scene: ScenePresentationConfig,
        size: [f32; 2],
        fixed: [f32; 2],
        point: [f32; 2],
        _second: f64,
        commit: bool,
        activity: Option<&ActivitySampler>,
    ) -> WidgetResult<ResizeUpdate> {
        let scene = validate_scene_presentation(scene)?;
        let mut config = validate_diving_profile(&widget, 0, &scene)?;
        self.geometry_at(activity);
        resize_diving_profile(&mut config, &scene, size)?;
        if commit {
            self.prepared_geometry = Some(Arc::new(match activity {
                Some(a) => DivingGeometry::new(
                    &config,
                    scene.scale,
                    &a.diving.samples(self.window, config.width * scene.scale),
                ),
                None => DivingGeometry::placeholder(&config, scene.scale),
            }));
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
        let geometry = self.geometry_at(activity);
        let state = match activity {
            Some(a) => a.diving.frame_state(&geometry, second, self.window),
            None => geometry.state(
                (second / self.preview_duration.max(1e-9)).clamp(0.0, 1.0) as f32,
                None,
            ),
        };
        let changed = self
            .layout
            .as_ref()
            .is_none_or(|layout| !Arc::ptr_eq(&layout.geometry, &geometry));
        if changed {
            self.layout = Some(DivingProfileLayout::new(
                self.config.clone(),
                self.scene.clone(),
                geometry,
                activity.and_then(|a| a.diving.depth_range()),
                fonts,
            )?);
        } else if self.last_state == Some(state) {
            return Ok(());
        }
        let layout = self
            .layout
            .as_ref()
            .expect("diving profile layout prepared");
        let frame = layout.frame(state)?;
        let (width, height) = (frame.bounds.width() as u32, frame.bounds.height() as u32);
        if self.surface.as_ref().map(|s| (s.width(), s.height()))
            != Some((width as i32, height as i32))
        {
            self.surface = Some(surface(width, height)?);
            self.pixels.resize(width as usize * height as usize * 4, 0);
        }
        let surface = self
            .surface
            .as_mut()
            .expect("diving profile surface allocated");
        let canvas = surface.canvas();
        canvas.clear(Color::TRANSPARENT);
        canvas.save();
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
            })
            .map_err(|e| WidgetError::Render(e.to_string()))?;
        }
        self.revision = self.revision.wrapping_add(1);
        self.last_state = Some(state);
        Ok(())
    }
}
