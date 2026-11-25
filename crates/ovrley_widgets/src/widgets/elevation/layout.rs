//! Elevation profile layout, point labels and drawing.
use crate::model::scene::ValidatedScenePresentation;
use crate::render::text::{
    layout_text, measure_text_with_font, parse_color, resolve_font, FontResolver,
    ResolvedTextStyle, TextPaints,
};
use crate::render::tile::{ContentBounds, SHAPE_COLOR_TYPE};
use crate::render::tile::{PreviewRasterScale, StaticRaster};
use crate::widgets::elevation::geometry::{ElevationFrameState, ElevationGeometry};
use crate::widgets::elevation::ValidatedElevationPlot;
use crate::widgets::plot_marker::PlotMarker;
use crate::widgets::route::geometry::{outward, rotated_rect, scaled_rect, stroke_paint};
use crate::{WidgetError, WidgetResult};
use skia_safe::{
    gradient, path_utils, BlurStyle, Canvas, ClipOp, Color4f, Font, MaskFilter, Matrix, Paint,
    Path, PathBuilder, Point, Rect, TextBlob, TileMode,
};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct ElevationLayout {
    pub config: ValidatedElevationPlot,
    pub scene: ValidatedScenePresentation,
    pub geometry: Arc<ElevationGeometry>,
    pub altitude_offset_m: f64,
    scale: [f32; 2],
    /// Remaining profile at the final angle, relative to the widget origin.
    remaining: StaticRaster,
    labels: [Option<ElevationLabel>; 2],
    bounds: Rect,
    content_bounds: ContentBounds,
    rotation: (f32, f32),
    matrix: Matrix,
    completed_line: Paint,
    completed_area: Paint,
    completed_area_clip: Arc<Mutex<Path>>,
    marker: PlotMarker,
}

struct ElevationMeasure {
    scale: [f32; 2],
    stroke: f32,
    offset: (f32, f32),
    blur: f32,
    rotation: (f32, f32),
    remaining_bounds: Rect,
    labels: [Option<ElevationLabel>; 2],
    bounds: Rect,
    content_bounds: ContentBounds,
}

pub struct ElevationFrame {
    pub state: ElevationFrameState,
    pub bounds: Rect,
    pub content_bounds: ContentBounds,
    marker: (f32, f32),
    labels: [Option<Arc<LabelRun>>; 2],
}

fn elevation_marker(
    config: &ValidatedElevationPlot,
    scene: &ValidatedScenePresentation,
) -> PlotMarker {
    PlotMarker::new(
        config.marker_variant,
        config.marker_size,
        config.marker_variant_diameter,
        &config.marker_color,
        config.marker_opacity * scene.opacity,
        scene.scale,
    )
}

fn fill_paint(color: &str, opacity: f32) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(parse_color(color, opacity));
    paint
}

/// Area fill; the optional gradient fades from full fill opacity at the plot top to
/// transparent at the plot bottom, in rotated local plot coordinates.
fn area_paint(color: &str, opacity: f32, gradient_height: Option<f32>) -> WidgetResult<Paint> {
    let mut paint = fill_paint(color, opacity);
    if let Some(height) = gradient_height {
        let top = parse_color(color, 1.0);
        let colors = [Color4f::from(top), Color4f::from(top.with_a(0))];
        let gradient = gradient::Gradient::new(
            gradient::Colors::new(&colors, None, TileMode::Clamp, None),
            gradient::Interpolation::default(),
        );
        let shader =
            gradient::shaders::linear_gradient(((0.0, 0.0), (0.0, height)), &gradient, None)
                .ok_or_else(|| {
                    WidgetError::Render("unable to prepare elevation area gradient".into())
                })?;
        paint.set_shader(shader);
    }
    Ok(paint)
}

fn stroke_outline(path: &Path, paint: &Paint) -> WidgetResult<Path> {
    let mut outline = PathBuilder::new();
    if !path_utils::fill_path_with_paint(path, paint, &mut outline, None, None) {
        return Err(WidgetError::Render(
            "unable to prepare elevation stroke boundary".into(),
        ));
    }
    Ok(outline.detach())
}

impl ElevationLayout {
    /// Every rectangle the layout needs, before its remaining raster is drawn.
    fn measure(
        config: &ValidatedElevationPlot,
        scene: &ValidatedScenePresentation,
        geometry: &ElevationGeometry,
        altitude_offset_m: f64,
        elevation_range: Option<(f64, f64)>,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<ElevationMeasure> {
        let scale = [
            config.width * scene.scale / geometry.key.width,
            config.height * scene.scale / geometry.key.height,
        ];
        let stroke = (config.remaining_line_width * scene.scale).max(1.0);
        let line =
            scaled_rect(geometry.point_bounds, scale).with_outset((stroke * 0.5, stroke * 0.5));
        let mut raster = line;
        let (sin, cos) = config.rotation.to_radians().sin_cos();
        let distance = scene.shadow_distance * scene.scale;
        let offset = ((cos + sin) * distance, (cos - sin) * distance);
        let blur = scene.shadow_strength * scene.scale;
        if blur > 0.0 {
            raster.join(
                line.with_outset((blur * 3.0, blur * 3.0))
                    .with_offset(offset),
            );
        }
        raster.join(Rect::new(
            line.left,
            line.top,
            line.right,
            config.height * scene.scale,
        ));
        let remaining_bounds = outward(rotated_rect(
            raster.with_outset((1.0, 1.0)),
            config.rotation,
            0.0,
            0.0,
        ));
        let content = Self::measure_content(config, scene);
        let radius = elevation_marker(config, scene).extent();
        let mut raster = content;
        raster.join(
            Rect::new(
                0.0,
                0.0,
                config.width * scene.scale,
                config.height * scene.scale,
            )
            .with_outset((radius, radius)),
        );
        let mut raster = rotated_rect(raster, config.rotation, 0.0, 0.0);
        raster.join(remaining_bounds);
        let font = if config.show_elevation_metric || config.show_elevation_imperial {
            Some(resolve_font(
                fonts,
                config.label_font.as_deref(),
                config.label_font_size * scene.scale,
            )?)
        } else {
            None
        };
        let label_style = ResolvedTextStyle::widget(
            scene,
            scene.scale,
            config.label_font.clone(),
            config.label_font_size,
            parse_color(&config.label_color, scene.opacity),
            scene.opacity,
        );
        let labels = [
            (
                config.show_elevation_metric,
                "M",
                1.0,
                config.metric_label_offset_x,
                config.metric_label_offset_y,
            ),
            (
                config.show_elevation_imperial,
                "FT",
                3.280_84,
                config.imperial_label_offset_x,
                config.imperial_label_offset_y,
            ),
        ]
        .map(|(enabled, unit, conversion, x, y)| {
            enabled.then(|| {
                ElevationLabel::new(
                    unit,
                    conversion,
                    (x * scene.scale, y * scene.scale),
                    font.as_ref()
                        .expect("elevation label font resolved")
                        .clone(),
                    label_style.clone(),
                )
            })
        });
        let marker_bounds = rotated_rect(
            Rect::new(
                0.0,
                0.0,
                config.width * scene.scale,
                config.height * scene.scale,
            ),
            config.rotation,
            0.0,
            0.0,
        );
        for label in labels.iter().flatten() {
            raster.join(label.raster_bounds(marker_bounds, elevation_range, altitude_offset_m));
        }
        let bounds = outward(
            raster
                .with_offset((config.x, config.y))
                .with_outset((1.0, 1.0)),
        );
        let content_bounds =
            ContentBounds::rotated_frame(content, config.rotation, config.x, config.y);
        Ok(ElevationMeasure {
            scale,
            stroke,
            offset,
            blur,
            remaining_bounds,
            labels,
            bounds,
            content_bounds,
            rotation: (sin, cos),
        })
    }

    /// Frame bounds from layout alone; the cached remaining raster is not drawn.
    pub fn measure_bounds(
        config: &ValidatedElevationPlot,
        scene: &ValidatedScenePresentation,
        geometry: &ElevationGeometry,
        altitude_offset_m: f64,
        elevation_range: Option<(f64, f64)>,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Rect> {
        Ok(Self::measure(
            config,
            scene,
            geometry,
            altitude_offset_m,
            elevation_range,
            fonts,
        )?
        .bounds)
    }

    pub fn new(
        config: ValidatedElevationPlot,
        scene: ValidatedScenePresentation,
        geometry: Arc<ElevationGeometry>,
        altitude_offset_m: f64,
        elevation_range: Option<(f64, f64)>,
        fonts: &dyn FontResolver,
        raster_scale: PreviewRasterScale,
    ) -> WidgetResult<Self> {
        let ElevationMeasure {
            scale,
            stroke,
            offset,
            blur,
            remaining_bounds,
            labels,
            bounds,
            content_bounds,
            rotation: (sin, cos),
        } = Self::measure(
            &config,
            &scene,
            &geometry,
            altitude_offset_m,
            elevation_range,
            fonts,
        )?;
        let paths = geometry.paths.lock().expect("elevation paths lock");
        let matrix = Matrix::scale((scale[0], scale[1]));
        let path = paths.0.with_transform(&matrix);
        let area = paths.1.with_transform(&matrix);
        let remaining_line = stroke_paint(
            &config.remaining_line_color,
            stroke,
            config.remaining_line_opacity * scene.opacity,
        );
        let completed_line = stroke_paint(
            &config.completed_line_color,
            (config.completed_line_width * scene.scale).max(1.0),
            config.completed_line_opacity * scene.opacity,
        );
        let completed_area_clip = Arc::new(Mutex::new(stroke_outline(&path, &completed_line)?));
        let area_gradient_height = config.area_gradient.then_some(config.height * scene.scale);
        let remaining = StaticRaster::new(
            raster_scale,
            Point::new(config.x, config.y),
            remaining_bounds,
            SHAPE_COLOR_TYPE,
            |canvas| {
                canvas.rotate(config.rotation, None);
                canvas.save();
                canvas.clip_path(
                    &stroke_outline(&path, &remaining_line)?,
                    ClipOp::Difference,
                    true,
                );
                canvas.draw_path(
                    &area,
                    &area_paint(
                        &config.area_remaining_color,
                        config.area_remaining_opacity * scene.opacity,
                        area_gradient_height,
                    )?,
                );
                canvas.restore();
                if blur > 0.0 {
                    canvas.save();
                    canvas.translate(offset);
                    let mut shadow = stroke_paint(
                        &scene.shadow_color,
                        stroke,
                        config.remaining_line_opacity * scene.opacity,
                    );
                    shadow.set_mask_filter(MaskFilter::blur(BlurStyle::Normal, blur, true));
                    canvas.draw_path(&path, &shadow);
                    canvas.restore();
                }
                canvas.draw_path(&path, &remaining_line);
                Ok(())
            },
        )?;
        drop(paths);
        let completed_area = area_paint(
            &config.area_completed_color,
            config.area_completed_opacity * scene.opacity,
            area_gradient_height,
        )?;
        let marker = elevation_marker(&config, &scene);
        Ok(Self {
            config,
            scene,
            geometry,
            altitude_offset_m,
            scale,
            remaining,
            labels,
            bounds,
            content_bounds,
            rotation: (sin, cos),
            matrix,
            completed_line,
            completed_area,
            completed_area_clip,
            marker,
        })
    }

    /// Elevation keeps its plot frame, including for flat or stationary telemetry.
    /// Moving labels, marker overflow and shadows do not change that frame.
    pub fn measure_content(
        config: &ValidatedElevationPlot,
        scene: &ValidatedScenePresentation,
    ) -> Rect {
        let rect = Rect::new(
            0.0,
            0.0,
            config.width * scene.scale,
            config.height * scene.scale,
        );
        let half = (config.completed_line_width.max(config.remaining_line_width) * scene.scale)
            .max(1.0)
            * 0.5;
        rect.with_outset((half, half))
    }

    pub fn frame(&self, state: ElevationFrameState) -> WidgetResult<ElevationFrame> {
        let (sin, cos) = self.rotation;
        let (x, y) = (
            state.marker_x * self.scale[0],
            state.marker_y * self.scale[1],
        );
        let marker = (
            self.config.x + x * cos - y * sin,
            self.config.y + x * sin + y * cos,
        );
        let mut labels = [None, None];
        for (index, label) in self.labels.iter().enumerate() {
            if let Some(label) = label {
                labels[index] = Some(label.prepare(state.elevation_m, self.altitude_offset_m)?);
            }
        }
        Ok(ElevationFrame {
            state,
            bounds: self.bounds,
            content_bounds: self.content_bounds,
            marker,
            labels,
        })
    }

    pub fn draw(&self, canvas: &Canvas, frame: &ElevationFrame) {
        canvas.save();
        canvas.translate((self.config.x, self.config.y));
        self.remaining.draw(canvas, None);
        canvas.rotate(self.config.rotation, None);
        let (line, area) = self.geometry.completed_paths(&frame.state);
        let (line, area) = if self.scale == [1.0, 1.0] {
            (line, area)
        } else {
            (
                line.with_transform(&self.matrix),
                area.with_transform(&self.matrix),
            )
        };
        canvas.save();
        canvas.clip_path(
            &self
                .completed_area_clip
                .lock()
                .expect("elevation stroke boundary lock"),
            ClipOp::Difference,
            true,
        );
        canvas.draw_path(&area, &self.completed_area);
        canvas.restore();
        canvas.draw_path(&line, &self.completed_line);
        canvas.restore();
        // Circular markers need only their scene position; avoid rotating their rasterization.
        self.marker.draw(canvas, frame.marker);
        for (index, run) in frame.labels.iter().enumerate() {
            if let Some(run) = run {
                self.labels[index]
                    .as_ref()
                    .expect("elevation frame label style")
                    .draw(canvas, run, frame.marker);
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct LabelRun {
    value: Option<i64>,
    blob: TextBlob,
    baseline: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct ElevationLabel {
    pub offset: (f32, f32),
    unit: &'static str,
    conversion: f64,
    font: Font,
    style: ResolvedTextStyle,
    paints: TextPaints,
    cached: Arc<Mutex<Option<Arc<LabelRun>>>>,
}

impl ElevationLabel {
    pub fn new(
        unit: &'static str,
        conversion: f64,
        offset: (f32, f32),
        font: Font,
        style: ResolvedTextStyle,
    ) -> Self {
        Self {
            unit,
            conversion,
            offset,
            font,
            paints: TextPaints::new(&style),
            style,
            cached: Arc::new(Mutex::new(None)),
        }
    }

    fn value(&self, elevation: f64, correction: f64) -> i64 {
        ((elevation + correction) * self.conversion).round() as i64
    }

    pub fn prepare(&self, elevation: Option<f64>, correction: f64) -> WidgetResult<Arc<LabelRun>> {
        let value = elevation.map(|elevation| self.value(elevation, correction));
        let mut cached = self.cached.lock().expect("elevation label cache lock");
        if let Some(run) = cached.as_ref().filter(|run| run.value == value) {
            return Ok(run.clone());
        }
        let text = value.map_or_else(
            || format!("-- {}", self.unit),
            |value| format!("{value} {}", self.unit),
        );
        let run = layout_text(&text, &self.font, 0.0, self.style.numerals);
        let m = run.measurement;
        let baseline =
            (self.style.line_height - (m.bounds_bottom - m.bounds_top)) * 0.5 - m.bounds_top;
        let run = Arc::new(LabelRun {
            value,
            baseline,
            blob: run
                .blob
                .ok_or_else(|| WidgetError::Render("unable to lay out elevation label".into()))?,
        });
        *cached = Some(run.clone());
        Ok(run)
    }

    pub fn draw(&self, canvas: &Canvas, run: &LabelRun, marker: (f32, f32)) {
        canvas.save();
        canvas.translate((marker.0 + self.offset.0, marker.1 + self.offset.1));
        self.paints.draw(canvas, &run.blob, (0.0, run.baseline));
        canvas.restore();
    }

    /// Bound every rounded label once, including missing data and unequal digit advances.
    pub fn raster_bounds(&self, marker: Rect, range: Option<(f64, f64)>, correction: f64) -> Rect {
        let (min, max) = range.unwrap_or((0.0, 0.0));
        let digits = [min, max, 0.0]
            .map(|value| {
                self.value(value, correction)
                    .unsigned_abs()
                    .to_string()
                    .len()
            })
            .into_iter()
            .max()
            .expect("label range endpoints");
        let mut advance = 0.0_f32;
        let mut left = 0.0_f32;
        let mut overhang = 0.0_f32;
        let mut top = 0.0_f32;
        let mut bottom = 0.0_f32;
        for c in "0123456789- MFT".chars() {
            let m = measure_text_with_font(&c.to_string(), &self.font, self.style.numerals);
            if c.is_ascii_digit() {
                advance = advance.max(m.width);
            }
            left = left.min(m.bounds_left);
            overhang = overhang.max(m.bounds_right - m.width);
            top = top.min(m.bounds_top);
            bottom = bottom.max(m.bounds_bottom);
        }
        let measure = |text: String| measure_text_with_font(&text, &self.font, self.style.numerals);
        let suffix = measure(format!("- {}", self.unit)).width;
        let placeholder = measure(format!("-- {}", self.unit)).width;
        let width = (digits as f32 * advance + suffix).max(placeholder) + overhang;
        let half_border = self.style.border_thickness * 0.5;
        let height = bottom - top;
        let mut bounds = Rect::new(
            marker.left + self.offset.0 + left,
            marker.top + self.offset.1 + (self.style.line_height - height) * 0.5,
            marker.right + self.offset.0 + width,
            marker.bottom + self.offset.1 + (self.style.line_height + height) * 0.5,
        )
        .with_outset((half_border, half_border));
        if self.style.shadow_strength > 0.0 {
            bounds.join(
                bounds
                    .with_outset((
                        self.style.shadow_strength * 3.0,
                        self.style.shadow_strength * 3.0,
                    ))
                    .with_offset((self.style.shadow_distance, self.style.shadow_distance)),
            );
        }
        bounds
    }
}
