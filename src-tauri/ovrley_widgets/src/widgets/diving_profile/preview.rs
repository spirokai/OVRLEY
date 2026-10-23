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
    host::WidgetHost,
    model::scene::{
        validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
    },
    render::{
        preview::{render_tile, PreviewRasterScale},
        text::FontResolver,
    },
    widgets::{
        route::sampling::RouteWindow,
        value::preview::{SHAPE_COLOR_TYPE, TileBounds, WidgetGeometry},
    },
};
use skia_safe::{Canvas, Surface};
use std::sync::Arc;

pub struct PreviewDivingProfile {
    config: ValidatedDivingProfile,
    scene: ValidatedScenePresentation,
    window: Option<RouteWindow>,
    preview_duration: f64,
    preview_raster_scale: PreviewRasterScale,
    pub prepared_geometry: Option<Arc<DivingGeometry>>,
    layout: Option<DivingProfileLayout>,
    last_state: Option<DivingFrameState>,
    surface: Option<Surface>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    /// Scene bounds of the last rendered tile; absent until the first render.
    pub bounds: Option<TileBounds>,
    pub revision: u32,
}

impl PreviewDivingProfile {
    fn timing(
        show_full_activity: bool,
        host: &WidgetHost,
    ) -> WidgetResult<(Option<RouteWindow>, f64)> {
        Ok((host.plot_window(show_full_activity), host.placeholder_duration()?))
    }

    pub fn new(widget: DivingProfileConfig, scene: ScenePresentationConfig) -> WidgetResult<Self> {
        let host = WidgetHost::from_scene(&scene)?;
        let scene = validate_scene_presentation(scene)?;
        let config = validate_diving_profile(&widget, 0, &scene)?;
        Self::from_validated(config, scene, &host)
    }

    pub fn from_validated(
        config: ValidatedDivingProfile,
        scene: ValidatedScenePresentation,
        host: &WidgetHost,
    ) -> WidgetResult<Self> {
        let (window, preview_duration) = Self::timing(config.show_full_activity, host)?;
        Ok(Self {
            config,
            scene,
            window,
            preview_duration,
            preview_raster_scale: host.raster_scale,
            prepared_geometry: None,
            layout: None,
            last_state: None,
            surface: None,
            pixels: Vec::new(),
            geometry: Vec::new(),
            bounds: None,
            revision: 0,
        })
    }

    pub fn update(
        &mut self,
        widget: DivingProfileConfig,
        scene: ScenePresentationConfig,
        transient: bool,
    ) -> WidgetResult<()> {
        let host = WidgetHost::from_scene(&scene)?;
        let (window, preview_duration) = Self::timing(widget.show_full_activity, &host)?;
        let preview_raster_scale = host.raster_scale;
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
            && self.preview_raster_scale == preview_raster_scale
        {
            return Ok(());
        }
        if let Some(layout) = self.layout.as_mut() {
            // The cached remaining raster sits on the position's sub-pixel grid, so a move rebuilds.
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

    fn state(
        &self,
        activity: Option<&ActivitySampler>,
        second: f64,
        geometry: &DivingGeometry,
    ) -> DivingFrameState {
        match activity {
            Some(a) => a.diving.frame_state(geometry, second, self.window),
            None => geometry.state(
                (second / self.preview_duration.max(1e-9)).clamp(0.0, 1.0) as f32,
                None,
            ),
        }
    }

    /// Scene rectangle a render at `second` draws into, without drawing the remaining profile.
    pub fn layout_bounds(
        &mut self,
        activity: Option<&ActivitySampler>,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let geometry = self.geometry_at(activity);
        let range = activity.and_then(|a| a.diving.depth_range());
        Ok(DivingProfileLayout::measure_bounds(&self.config, &self.scene, &geometry, range, fonts)?.into())
    }

    /// Prepares the layout for the activity's geometry; reports whether it was rebuilt.
    fn layout(
        &mut self,
        activity: Option<&ActivitySampler>,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<bool> {
        let geometry = self.geometry_at(activity);
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
                self.preview_raster_scale,
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
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        self.layout(activity, fonts)?;
        let layout = self
            .layout
            .as_ref()
            .expect("diving profile layout prepared");
        let frame = layout.frame(self.state(activity, second, &layout.geometry))?;
        layout.draw(canvas, &frame);
        Ok(frame.bounds.into())
    }

    /// What the frame at `second` depends on; equal keys draw identical pixels.
    pub fn frame_key(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<DivingFrameState> {
        self.layout(activity, fonts)?;
        let layout = self
            .layout
            .as_ref()
            .expect("diving profile layout prepared");
        Ok(self.state(activity, second, &layout.geometry))
    }

    pub fn render(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<()> {
        let changed = self.layout(activity, fonts)?;
        let layout = self
            .layout
            .as_ref()
            .expect("diving profile layout prepared");
        let state = self.state(activity, second, &layout.geometry);
        if !changed && self.last_state == Some(state) {
            return Ok(());
        }
        let frame = layout.frame(state)?;
        let (width, height, bounds) = render_tile(
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
        if changed || self.last_state.is_none() {
            #[derive(serde::Serialize)]
            struct Geometry {
                #[serde(flatten)]
                widget: WidgetGeometry,
                rotation: f32,
                rotation_origin: [f32; 2],
            }
            let tile = TileBounds::from(bounds);
            self.geometry = serde_json::to_vec(&Geometry {
                widget: WidgetGeometry::new(
                    tile,
                    (width, height),
                    self.preview_raster_scale.value(),
                    frame.content_bounds.width,
                    frame.content_bounds,
                ),
                rotation: self.config.rotation,
                rotation_origin: frame.content_bounds.rotation_origin(),
            })
            .map_err(|e| WidgetError::Render(e.to_string()))?;
            self.bounds = Some(tile);
        }
        self.revision = self.revision.wrapping_add(1);
        self.last_state = Some(state);
        Ok(())
    }
}
