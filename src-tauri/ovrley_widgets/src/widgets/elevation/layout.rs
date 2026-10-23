//! Shared cached remaining profile, completed area, marker and upright labels.
use super::{
    geometry::{ElevationFrameState, ElevationGeometry},
    label::{ElevationLabel, LabelRun},
    validation::ValidatedElevationPlot,
};
use crate::{
    error::{WidgetError, WidgetResult},
    model::{scene::ValidatedScenePresentation, standard_widgets::widget_font_weight},
    render::{
        preview::{PreviewRasterScale, StaticRaster},
        text::{parse_color, resolve_font, FontResolver, ResolvedTextStyle},
    },
    widgets::{
        route::layout::{outward, rotated_rect, scaled_rect, stroke_paint},
        value::preview::{SHAPE_COLOR_TYPE, ContentBounds},
    },
};
use skia_safe::{
    gradient, path_utils, BlurStyle, Canvas, ClipOp, Color4f, MaskFilter, Matrix, Paint,
    Path, PathBuilder, Point, Rect, TileMode,
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
    marker: Paint,
    variant: Paint,
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
        let radius =
            (config.marker_size * scene.scale)
                .max(2.0)
                .max(match config.marker_variant.as_str() {
                    "ring" => config.marker_variant_diameter * scene.scale * 0.5 + 0.75,
                    "halo" => config.marker_variant_diameter * scene.scale * 0.5,
                    _ => 0.0,
                });
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
        let label_style = ResolvedTextStyle {
            x: 0.0,
            y: 0.0,
            font_name: config.label_font.clone(),
            font_size: config.label_font_size * scene.scale,
            font_weight: widget_font_weight(),
            italic: false,
            letter_spacing: 0.0,
            line_height: config.label_font_size * scene.scale * 0.92,
            color: parse_color(&config.label_color, scene.opacity),
            opacity: scene.opacity,
            shadow_color: Some(parse_color(&scene.shadow_color, scene.opacity)),
            shadow_strength: scene.shadow_strength * scene.scale,
            shadow_distance: scene.shadow_distance * scene.scale,
            border_color: Some(parse_color(&scene.border_color, scene.opacity)),
            border_thickness: scene.border_thickness * scene.scale,
            numerals: scene.numerals,
        };
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
        Ok(Self::measure(config, scene, geometry, altitude_offset_m, elevation_range, fonts)?.bounds)
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
        } = Self::measure(&config, &scene, &geometry, altitude_offset_m, elevation_range, fonts)?;
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
        let marker = fill_paint(&config.marker_color, config.marker_opacity * scene.opacity);
        let mut variant = marker.clone();
        match config.marker_variant.as_str() {
            "ring" => {
                variant.set_style(skia_safe::paint::Style::Stroke);
                variant.set_stroke_width(1.5);
            }
            "halo" => {
                variant = fill_paint(
                    &config.marker_color,
                    config.marker_opacity * scene.opacity * 0.35,
                )
            }
            _ => {}
        }
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
            variant,
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
            &*self
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
        if self.config.marker_variant != "single" {
            canvas.draw_circle(
                frame.marker,
                self.config.marker_variant_diameter * self.scene.scale * 0.5,
                &self.variant,
            );
        }
        canvas.draw_circle(
            frame.marker,
            (self.config.marker_size * self.scene.scale).max(2.0),
            &self.marker,
        );
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
