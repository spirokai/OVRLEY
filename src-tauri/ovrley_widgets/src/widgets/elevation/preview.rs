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
        value::preview::{TileBounds, WidgetGeometry, SHAPE_COLOR_TYPE},
    },
};
use skia_safe::{Canvas, Surface};
use std::sync::Arc;

pub struct PreviewElevation {
    config: ValidatedElevationPlot,
    scene: ValidatedScenePresentation,
    window: Option<RouteWindow>,
    preview_duration: f64,
    preview_raster_scale: PreviewRasterScale,
    samples: Option<Arc<Vec<(f32, f64, f32)>>>,
    pub prepared_geometry: Option<Arc<ElevationGeometry>>,
    placeholder_geometry: Option<Arc<ElevationGeometry>>,
    layout: Option<ElevationLayout>,
    last_state: Option<ElevationFrameState>,
    surface: Option<Surface>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    /// Scene bounds of the last rendered tile; absent until the first render.
    pub bounds: Option<TileBounds>,
    pub revision: u32,
}

impl PreviewElevation {
    fn timing(
        show_full_activity: bool,
        host: &WidgetHost,
    ) -> WidgetResult<(Option<RouteWindow>, f64)> {
        Ok((
            host.plot_window(show_full_activity),
            host.placeholder_duration()?,
        ))
    }

    pub fn new(widget: ElevationPlotConfig, scene: ScenePresentationConfig) -> WidgetResult<Self> {
        let host = WidgetHost::from_scene(&scene)?;
        let scene = validate_scene_presentation(scene)?;
        let config = validate_elevation_plot(&widget, 0, &scene)?;
        Self::from_validated(config, scene, &host)
    }

    pub fn from_validated(
        config: ValidatedElevationPlot,
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
            samples: None,
            prepared_geometry: None,
            placeholder_geometry: None,
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
        widget: ElevationPlotConfig,
        scene: ScenePresentationConfig,
        transient: bool,
    ) -> WidgetResult<()> {
        let host = WidgetHost::from_scene(&scene)?;
        let (window, preview_duration) = Self::timing(widget.show_full_activity, &host)?;
        let preview_raster_scale = host.raster_scale;
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

    /// Altitude correction and label range for the activity, shared by layout and its bounds.
    fn label_range(&self, activity: Option<&ActivitySampler>) -> (f64, Option<(f64, f64)>) {
        (
            activity.map_or(0.0, |a| {
                a.elevation.altitude_offset(self.config.starting_altitude_m)
            }),
            activity.and_then(|a| a.elevation.elevation_range()),
        )
    }

    fn state(
        &self,
        activity: Option<&ActivitySampler>,
        second: f64,
        geometry: &ElevationGeometry,
    ) -> ElevationFrameState {
        if geometry.source_point_count == 0 {
            let progress = (second / self.preview_duration.max(1e-9)).clamp(0.0, 1.0) as f32;
            geometry.state(progress, None, progress)
        } else {
            let activity = &activity.expect("elevation activity prepared").elevation;
            activity.frame_state(geometry, second, self.window, activity.elevation_at(second))
        }
    }

    /// Scene rectangle a render at `second` draws into, without drawing the remaining profile.
    pub fn layout_bounds(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let geometry = self.geometry_at(activity, second)?;
        let (offset, range) = self.label_range(activity);
        Ok(ElevationLayout::measure_bounds(
            &self.config,
            &self.scene,
            &geometry,
            offset,
            range,
            fonts,
        )?
        .into())
    }

    /// Prepares the layout for the geometry at `second`; reports whether it was rebuilt.
    fn layout(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<bool> {
        let geometry = self.geometry_at(activity, second)?;
        let changed = self
            .layout
            .as_ref()
            .is_none_or(|layout| !Arc::ptr_eq(&layout.geometry, &geometry));
        if changed {
            let (offset, range) = self.label_range(activity);
            self.layout = Some(ElevationLayout::new(
                self.config.clone(),
                self.scene.clone(),
                geometry,
                offset,
                range,
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
        self.layout(activity, second, fonts)?;
        let layout = self.layout.as_ref().expect("elevation layout prepared");
        let frame = layout.frame(self.state(activity, second, &layout.geometry))?;
        layout.draw(canvas, &frame);
        Ok(frame.bounds.into())
    }

    /// What the frame at `second` depends on: the drawn geometry, which the widget retains, and
    /// its state. Equal keys draw identical pixels.
    pub fn frame_key(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<(usize, ElevationFrameState)> {
        self.layout(activity, second, fonts)?;
        let layout = self.layout.as_ref().expect("elevation layout prepared");
        Ok((
            Arc::as_ptr(&layout.geometry) as usize,
            self.state(activity, second, &layout.geometry),
        ))
    }

    pub fn render(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<()> {
        let changed = self.layout(activity, second, fonts)?;
        let layout = self.layout.as_ref().expect("elevation layout prepared");
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
