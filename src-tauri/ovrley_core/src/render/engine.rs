//! Export host of the shared widget engine.
//!
//! Every widget draws through the same engine as the editor preview, in the
//! export's layer order: backdrops, rasters and labels (the static layer), then
//! intrinsic values, boxed values, routes, elevation plots, diving profiles and rolling charts.
//!
//! Widgets sample the activity like the preview does: at the activity second of
//! each dense frame. Export planning keeps ownership of update-rate decimation
//! (which dense frames are rendered) and of activity coverage (which output
//! frames show widgets other than rasters).
//!
//! Frames are produced as atlas frames: each slot of the [`AtlasPlan`] shows its
//! scene rectangle, and the full-frame plan reproduces the full scene frame.
//! Static widgets never change, so they are drawn once per export into two
//! shared atlas bases, one inside the activity coverage and one with only the
//! rasters outside it. Each frame copies its base and draws the dynamic widgets
//! straight onto it.
//!
//! Widget instances hold Skia surfaces and caches, so each frame worker builds
//! its own set from the shared, immutable [`ExportWidgets`].

use super::{atlas::{AtlasPlan, SlotPlacement}, surface::wrap_native_surface, FrameSize};
use crate::{
    activity::schema::{DenseActivityReport, ParsedActivity},
    debug::RenderProfiler,
    error::{CoreError, CoreResult},
    fonts::NativeFonts,
    normalize::{RasterSource, ValidatedRaster, ValidatedRenderConfig},
    raster::load_selected_raster,
};
use ovrley_widgets::{
    activity::ActivitySampler,
    host::WidgetHost,
    model::standard_metrics::{display_type_layout_mode, DisplayTypeLayoutMode},
    preview_widget::{FrameKey, PreviewWidget, ValidatedWidget},
    render::{image::RasterImage, text::FontResolver},
    widgets::{raster::image::downsample_raster, value::preview::TileBounds},
};
use skia_safe::{AlphaType, ColorType, Data, IRect, Image};
use std::{num::NonZeroUsize, path::PathBuf, sync::Arc, thread, time::Instant};

/// Immutable export inputs shared by every frame worker.
pub struct ExportWidgets<'a> {
    config: &'a ValidatedRenderConfig,
    dense: &'a DenseActivityReport,
    font_dirs: &'a [PathBuf],
    activity: ActivitySampler,
    /// Raster sources resampled to their export size, in config order.
    rasters: Vec<RasterImage>,
    host: WidgetHost,
    frame_size: FrameSize,
    blank_leading_frame_count: usize,
}

/// How a layer is drawn and when it is visible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Layer {
    /// Backdrops and labels: drawn once into the covered base.
    Static,
    /// Drawn once into both bases; the only layer shown outside the coverage.
    Raster,
    /// Drawn every covered frame.
    Dynamic,
}

/// An atlas plan with its static layers drawn into straight-alpha RGBA atlas bases.
pub struct PreparedAtlas {
    plan: AtlasPlan,
    /// Backdrops, rasters and labels, for frames inside the activity coverage.
    covered: Vec<u8>,
    /// Rasters only, for frames outside the activity coverage.
    uncovered: Vec<u8>,
}

impl PreparedAtlas {
    pub fn plan(&self) -> &AtlasPlan {
        &self.plan
    }
}

impl<'a> ExportWidgets<'a> {
    pub fn prepare(
        font_dirs: &'a [PathBuf],
        config: &'a ValidatedRenderConfig,
        activity: &ParsedActivity,
        dense: &'a DenseActivityReport,
        frame_size: FrameSize,
        blank_leading_frame_count: u64,
        profiler: &mut RenderProfiler,
    ) -> CoreResult<Self> {
        let scene = &config.scene;
        if (scene.presentation.width, scene.presentation.height)
            != (frame_size.width, frame_size.height)
        {
            return Err(CoreError::Render(format!(
                "Export frames are {}x{}; the scene is {}x{}",
                frame_size.width, frame_size.height, scene.presentation.width, scene.presentation.height
            )));
        }
        let blank_leading_frame_count = usize::try_from(blank_leading_frame_count)
            .map_err(|_| CoreError::Render("Blank leading frame count exceeds usize".into()))?;
        // The editor preview receives this same serialized activity.
        let activity = profiler.measure("prepare.activity_sampler", || -> CoreResult<_> {
            let bytes = serde_json::to_vec(activity)
                .map_err(|error| CoreError::Render(format!("activity serialization: {error}")))?;
            let mut activity = ActivitySampler::from_json(&bytes)?;
            activity.select_laps(scene.disabled_laps.clone())?;
            Ok(activity)
        })?;
        let rasters = profiler.measure("prepare.rasters", || {
            config
                .rasters
                .iter()
                .map(|raster| export_raster(raster, scene.presentation.scale))
                .collect::<CoreResult<Vec<_>>>()
        })?;
        Ok(Self {
            config,
            dense,
            font_dirs,
            activity,
            rasters,
            // Plots drawn without activity samples advance over the scene's activity seconds.
            host: WidgetHost::export(scene.plot_window(false), scene.end, scene.export_start_seconds),
            frame_size,
            blank_leading_frame_count,
        })
    }

    /// Builds one worker's widget instances in layer order.
    pub fn worker(&self) -> CoreResult<WorkerWidgets<'_>> {
        let config = self.config;
        let fonts = NativeFonts(self.font_dirs);
        let widgets = self
            .layers()
            .into_iter()
            .map(|(widget, layer)| {
                Ok(ExportWidget {
                    widget: PreviewWidget::from_validated(
                        widget,
                        &config.scene.presentation,
                        &self.host,
                        &fonts,
                    )?,
                    layer,
                    layout: None,
                })
            })
            .collect::<CoreResult<_>>()?;
        Ok(WorkerWidgets {
            export: self,
            widgets,
        })
    }

    /// The plan whose single slot is the full frame with every layer.
    pub fn full_frame_atlas(&self) -> AtlasPlan {
        AtlasPlan::full_frame(self.frame_size, self.layers().len())
    }

    /// Draws the static layers of `plan` once into its shared atlas bases.
    pub fn prepare_atlas(&self, plan: AtlasPlan, profiler: &mut RenderProfiler) -> CoreResult<PreparedAtlas> {
        profiler.measure("prepare.atlas_bases", || {
            let mut worker = self.worker()?;
            let start = self.config.scene.start;
            let mut covered = vec![0; plan.size().rgba_len()?];
            worker.draw_slots(&plan, &mut covered, start, |layer| layer != Layer::Dynamic)?;
            let mut uncovered = vec![0; covered.len()];
            worker.draw_slots(&plan, &mut uncovered, start, |layer| layer == Layer::Raster)?;
            Ok(PreparedAtlas {
                plan,
                covered,
                uncovered,
            })
        })
    }

    /// Plans the atlas from layout alone over `frame_indices`, the output frames export
    /// renders, for slots placed back on the frame by `placement`. Contiguous chunks
    /// run in parallel so unchanged samples skip layout.
    pub fn plan_atlas(
        &self,
        frame_indices: &[usize],
        placement: SlotPlacement,
        profiler: &mut RenderProfiler,
    ) -> CoreResult<AtlasPlan> {
        let started = Instant::now();
        let threads = thread::available_parallelism().map_or(1, NonZeroUsize::get);
        let chunk = frame_indices.len().div_ceil(threads).max(1);
        let unions = thread::scope(|scope| {
            let chunks: Vec<_> = frame_indices
                .chunks(chunk)
                .map(|frames| {
                    scope.spawn(move || -> CoreResult<Vec<Option<IRect>>> {
                        let mut worker = self.worker()?;
                        let mut union = vec![None; worker.widgets.len()];
                        for frame in frames {
                            for (union, bounds) in union.iter_mut().zip(worker.frame_bounds(*frame)?) {
                                *union = union_bounds(*union, bounds);
                            }
                        }
                        Ok(union)
                    })
                })
                .collect();
            chunks
                .into_iter()
                .map(|chunk| {
                    chunk
                        .join()
                        .unwrap_or_else(|_| Err(CoreError::Render("Atlas planning worker panicked".into())))
                })
                .collect::<CoreResult<Vec<_>>>()
        })?;
        let mut union = vec![None; self.layers().len()];
        for chunk in unions {
            for (union, bounds) in union.iter_mut().zip(chunk) {
                *union = union_bounds(*union, bounds);
            }
        }
        profiler.record_ms("atlas.plan.layout", started.elapsed().as_secs_f64() * 1000.0);
        let plan = profiler.measure("atlas.plan.pack", || AtlasPlan::from_layer_bounds(self.frame_size, &union, placement))?;
        profiler.record_ms("atlas.plan", started.elapsed().as_secs_f64() * 1000.0);
        Ok(plan)
    }

    /// Widgets in layer order. Static layers come first, so in every slot they lie below
    /// the dynamic ones.
    fn layers(&self) -> Vec<(ValidatedWidget<'_>, Layer)> {
        let config = self.config;
        let mut layers = Vec::new();
        layers.extend(config.backdrops.iter().map(|widget| (ValidatedWidget::Backdrop(widget), Layer::Static)));
        layers.extend(config.rasters.iter().zip(&self.rasters).map(|(raster, image)| {
            (
                ValidatedWidget::Raster {
                    geometry: &raster.geometry,
                    source_id: raster.id.clone(),
                    image: image.clone(),
                },
                Layer::Raster,
            )
        }));
        layers.extend(config.labels.iter().map(|widget| (ValidatedWidget::Label(widget), Layer::Static)));
        for mode in [DisplayTypeLayoutMode::Intrinsic, DisplayTypeLayoutMode::Boxed] {
            layers.extend(
                config
                    .values
                    .iter()
                    .filter(|value| display_type_layout_mode(value.display_type()) == mode)
                    .map(|value| (ValidatedWidget::Value(value), Layer::Dynamic)),
            );
        }
        layers.extend(config.course_plots.iter().map(|widget| (ValidatedWidget::Route(widget), Layer::Dynamic)));
        layers.extend(config.elevation_plots.iter().map(|widget| (ValidatedWidget::Elevation(widget), Layer::Dynamic)));
        layers.extend(config.diving_profiles.iter().map(|widget| (ValidatedWidget::DivingProfile(widget), Layer::Dynamic)));
        layers.extend(config.rolling_charts.iter().map(|widget| (ValidatedWidget::RollingChart(widget), Layer::Dynamic)));
        layers
    }

    pub fn frame_size(&self) -> FrameSize {
        self.frame_size
    }

    /// Activity second of output frame `frame_index`; none outside the activity coverage,
    /// where only rasters, which never sample activity, are shown.
    fn frame_second(&self, frame_index: usize) -> Option<f64> {
        frame_index
            .checked_sub(self.blank_leading_frame_count)
            .filter(|index| *index < self.dense.frame_count)
            .map(|index| self.config.scene.start + self.dense.frame_elapsed_seconds[index])
    }

    fn frame_rect(&self) -> IRect {
        IRect::from_wh(self.frame_size.width as i32, self.frame_size.height as i32)
    }
}

/// One frame worker's widget instances.
pub struct WorkerWidgets<'a> {
    export: &'a ExportWidgets<'a>,
    widgets: Vec<ExportWidget>,
}

impl WorkerWidgets<'_> {
    /// Renders output frame `frame_index` into the straight-alpha RGBA atlas `pixels`: its
    /// base, then inside the coverage every slot's dynamic widgets in layer order.
    pub fn render_atlas(
        &mut self,
        frame_index: usize,
        atlas: &PreparedAtlas,
        pixels: &mut [u8],
        profiler: &mut RenderProfiler,
    ) -> CoreResult<()> {
        if pixels.len() != atlas.covered.len() {
            let FrameSize { width, height } = atlas.plan.size();
            return Err(CoreError::Render(format!(
                "Atlas frame buffer has {} bytes; expected {width}x{height} RGBA",
                pixels.len()
            )));
        }
        let second = self.export.frame_second(frame_index);
        let base = if second.is_some() { &atlas.covered } else { &atlas.uncovered };
        profiler.measure("frame.base", || pixels.copy_from_slice(base));
        let started = Instant::now();
        if let Some(second) = second {
            self.draw_slots(&atlas.plan, pixels, second, |layer| layer == Layer::Dynamic)?;
        }
        profiler.record_ms("frame.draw", started.elapsed().as_secs_f64() * 1000.0);
        Ok(())
    }

    /// Frame-clipped whole-pixel rectangle each layer draws into at output frame
    /// `frame_index`, from layout alone; none for layers hidden or outside the frame.
    pub fn frame_bounds(&mut self, frame_index: usize) -> CoreResult<Vec<Option<IRect>>> {
        let export = self.export;
        let second = export.frame_second(frame_index);
        let frame = export.frame_rect();
        let fonts = NativeFonts(export.font_dirs);
        self.widgets
            .iter_mut()
            .map(|widget| {
                if second.is_none() && widget.layer != Layer::Raster {
                    return Ok(None);
                }
                widget.layout_rect(&export.activity, second.unwrap_or(export.config.scene.start), &fonts, frame)
            })
            .collect()
    }

    /// Draws the `drawn` layers of every slot at `second` onto the atlas `pixels`, each
    /// slot showing its frame-clipped scene rectangle.
    fn draw_slots(
        &mut self,
        plan: &AtlasPlan,
        pixels: &mut [u8],
        second: f64,
        drawn: impl Fn(Layer) -> bool,
    ) -> CoreResult<()> {
        let export = self.export;
        let FrameSize { width, height } = plan.size();
        let mut surface = wrap_native_surface(width, height, pixels)?;
        let canvas = surface.canvas();
        let fonts = NativeFonts(export.font_dirs);
        for slot in plan.slots() {
            let Some(visible) = IRect::intersect(&slot.scene, &export.frame_rect()) else {
                continue;
            };
            let offset = (slot.atlas_origin.x - slot.scene.left, slot.atlas_origin.y - slot.scene.top);
            canvas.save();
            canvas.clip_irect(visible.with_offset(offset), None);
            canvas.translate((offset.0 as f32, offset.1 as f32));
            let result = slot.layers.iter().try_for_each(|layer| -> CoreResult<()> {
                let widget = &mut self.widgets[*layer];
                if drawn(widget.layer) {
                    widget.widget.draw(canvas, Some(&export.activity), second, &fonts)?;
                }
                Ok(())
            });
            canvas.restore();
            result?;
        }
        Ok(())
    }
}

fn union_bounds(a: Option<IRect>, b: Option<IRect>) -> Option<IRect> {
    match (a, b) {
        (Some(a), Some(b)) => Some(IRect::new(
            a.left.min(b.left),
            a.top.min(b.top),
            a.right.max(b.right),
            a.bottom.max(b.bottom),
        )),
        (a, b) => a.or(b),
    }
}

/// Whole pixels a widget's layout bounds touch inside the frame.
fn pixel_rect(bounds: TileBounds, frame: IRect) -> Option<IRect> {
    let rect = IRect::new(
        bounds.left.floor() as i32,
        bounds.top.floor() as i32,
        (bounds.left + bounds.width).ceil() as i32,
        (bounds.top + bounds.height).ceil() as i32,
    );
    IRect::intersect(&rect, &frame)
}

/// A widget instance and its layer.
struct ExportWidget {
    widget: PreviewWidget,
    layer: Layer,
    /// Sample and pixel rectangle of the last layout-only pass.
    layout: Option<(FrameKey, Option<IRect>)>,
}

impl ExportWidget {
    /// Equal samples lay out equally, so layout reruns only when the sample changes.
    fn layout_rect(
        &mut self,
        activity: &ActivitySampler,
        second: f64,
        fonts: &dyn FontResolver,
        frame: IRect,
    ) -> CoreResult<Option<IRect>> {
        let key = self.widget.frame_key(Some(activity), second, fonts)?;
        if let Some((_, rect)) = self.layout.as_ref().filter(|(last, _)| *last == key) {
            return Ok(*rect);
        }
        let rect = pixel_rect(self.widget.layout_bounds(Some(activity), second, fonts)?, frame);
        self.layout = Some((key, rect));
        Ok(rect)
    }
}

/// Loads the oriented source and resamples it once to its export size, as straight-alpha RGBA.
fn export_raster(config: &ValidatedRaster, global_scale: f32) -> CoreResult<RasterImage> {
    let id = &config.id;
    let selected = match &config.source {
        RasterSource::Image(image) => Arc::clone(image),
        RasterSource::File(path) => Arc::new(load_selected_raster(path).map_err(|error| error.for_widget(id))?),
    };
    let image = Image::from_encoded(Data::new_copy(selected.preview_png()))
        .ok_or_else(|| CoreError::Render(format!("Raster {id} could not be decoded [raster_error:decode:{id}]")))?;
    let image = downsample_raster(image, &config.geometry, id, global_scale)?;
    let (width, height) = (image.width() as u32, image.height() as u32);
    let info = image
        .image_info()
        .with_color_type(ColorType::RGBA8888)
        .with_alpha_type(AlphaType::Unpremul);
    let mut pixels = vec![0; width as usize * height as usize * 4];
    if !image.read_pixels(
        &info,
        &mut pixels,
        width as usize * 4,
        (0, 0),
        skia_safe::image::CachingHint::Disallow,
    ) {
        return Err(CoreError::Render(format!(
            "Raster {id} pixels could not be read [raster_error:decode:{id}]"
        )));
    }
    Ok(RasterImage {
        width,
        height,
        pixels: Arc::new(pixels),
    })
}
