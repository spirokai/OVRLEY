//! Diving profile geometry, layout and drawing.
use crate::model::scene::ValidatedScenePresentation;
use crate::render::text::{parse_color, resolve_font, FontResolver, ResolvedTextStyle};
use crate::render::tile::{ContentBounds, SHAPE_COLOR_TYPE};
use crate::render::tile::{PreviewRasterScale, StaticRaster};
use crate::widgets::diving_profile::{DivingSamples, ValidatedDivingProfile};
use crate::widgets::elevation::geometry::{
    build_elevation_completed_points, downsample_elevation_points, simplify_elevation_samples,
    ElevationSample,
};
use crate::widgets::elevation::layout::{ElevationLabel, LabelRun};
use crate::widgets::plot_marker::PlotMarker;
use crate::widgets::route::geometry::{outward, rotated_rect, scaled_rect, stroke_paint};
use crate::WidgetResult;
use skia_safe::{BlurStyle, Canvas, MaskFilter, Matrix, Paint, Path, PathBuilder, Point, Rect};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq)]
pub struct GeometryKey {
    pub width: f32,
    pub height: f32,
    y_scale: f32,
    tolerance: f32,
    density: f32,
}
impl GeometryKey {
    pub fn new(config: &ValidatedDivingProfile, scale: f32) -> Self {
        Self {
            width: config.width * scale,
            height: config.height * scale,
            y_scale: config.y_scale,
            tolerance: config.simplify_tolerance_px,
            density: config.target_density,
        }
    }
}

#[derive(Debug)]
pub struct DepthSegment {
    pub points: Vec<(f32, f32)>,
    pub progress: Vec<f32>,
}

#[derive(Debug)]
pub struct DivingPaths {
    pub remaining: Path,
    pub ceiling_area: Path,
    pub ceiling_line: Path,
}

#[derive(Debug)]
pub struct DivingGeometry {
    pub key: GeometryKey,
    pub segments: Vec<DepthSegment>,
    pub source_point_count: usize,
    pub paths: Mutex<DivingPaths>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DivingFrameState {
    pub progress01: f32,
    pub marker: Option<(f32, f32)>,
    pub depth_m: Option<f64>,
}

impl DivingGeometry {
    pub fn new(config: &ValidatedDivingProfile, scale: f32, samples: &DivingSamples) -> Self {
        let key = GeometryKey::new(config, scale);
        let max_depth = samples
            .depth
            .iter()
            .flatten()
            .map(|p| p.1)
            .chain(samples.ceiling.iter().flatten().map(|p| p.1))
            .fold(0.0, f64::max)
            .max(1e-9);
        // Zero stays at the water level even when the vertical exaggeration changes.
        let project_y = |depth: f64| {
            ((depth.max(0.0) / max_depth) as f32 * key.y_scale).clamp(0.0, 1.0) * key.height
        };
        let count = (key.width * config.target_density).round().max(2.0) as usize;
        let segments: Vec<_> = samples
            .depth
            .iter()
            .map(|run| {
                let reduced = downsample_elevation_points(run, count.min(run.len()));
                let projected: Vec<_> = reduced
                    .iter()
                    .map(|p| ElevationSample {
                        point: (p.progress01 * key.width, project_y(p.elevation)),
                        progress01: p.progress01,
                        elapsed_fraction: p.elapsed_fraction,
                        preserve: p.preserve,
                    })
                    .collect();
                let simplified =
                    simplify_elevation_samples(&projected, config.simplify_tolerance_px);
                DepthSegment {
                    points: simplified.iter().map(|p| p.point).collect(),
                    progress: simplified.iter().map(|p| p.progress01).collect(),
                }
            })
            .collect();
        let mut remaining = PathBuilder::new();
        for segment in &segments {
            append_line(&mut remaining, &segment.points);
        }
        let mut ceiling_area = PathBuilder::new();
        let mut ceiling_line = PathBuilder::new();
        for run in &samples.ceiling {
            // Split at surface runs: the water-level edge is never stroked.
            for pair in run.windows(2) {
                if pair[0].1 == 0.0 && pair[1].1 == 0.0 {
                    continue;
                }
                let (a, b) = (
                    (pair[0].0 * key.width, project_y(pair[0].1)),
                    (pair[1].0 * key.width, project_y(pair[1].1)),
                );
                ceiling_line.move_to(a).line_to(b);
            }
            if run.len() < 2 {
                continue;
            }
            ceiling_area.move_to((run[0].0 * key.width, 0.0));
            for &(x, depth) in run {
                ceiling_area.line_to((x * key.width, project_y(depth)));
            }
            ceiling_area
                .line_to((run.last().unwrap().0 * key.width, 0.0))
                .close();
        }
        Self {
            key,
            segments,
            source_point_count: samples.depth.iter().map(Vec::len).sum(),
            paths: Mutex::new(DivingPaths {
                remaining: remaining.detach(),
                ceiling_area: ceiling_area.detach(),
                ceiling_line: ceiling_line.detach(),
            }),
        }
    }

    pub fn placeholder(config: &ValidatedDivingProfile, scale: f32) -> Self {
        Self::new(
            config,
            scale,
            &DivingSamples {
                depth: vec![vec![
                    (0.0, 0.0, 0.0),
                    (0.05, 36.0, 0.05),
                    (0.20, 36.0, 0.20),
                    (0.25, 18.0, 0.25),
                    (0.45, 6.0, 0.45),
                    (0.75, 3.0, 0.75),
                    (1.0, 0.0, 1.0),
                ]],
                ceiling: vec![vec![
                    (0.06, 0.0),
                    (0.14, 12.0),
                    (0.25, 9.0),
                    (0.45, 3.0),
                    (0.65, 0.0),
                ]],
            },
        )
    }

    pub fn state(&self, progress01: f32, depth_m: Option<f64>) -> DivingFrameState {
        let marker = self.segments.iter().find_map(|s| {
            if progress01 < s.progress[0] || progress01 > *s.progress.last().unwrap() {
                return None;
            }
            let i = s.progress.partition_point(|p| *p < progress01);
            if i == 0 {
                return Some(s.points[0]);
            }
            let u = (progress01 - s.progress[i - 1]) / (s.progress[i] - s.progress[i - 1]);
            Some((
                s.points[i - 1].0 + (s.points[i].0 - s.points[i - 1].0) * u,
                s.points[i - 1].1 + (s.points[i].1 - s.points[i - 1].1) * u,
            ))
        });
        DivingFrameState {
            progress01,
            marker,
            depth_m,
        }
    }

    pub fn completed_path(&self, state: &DivingFrameState) -> Path {
        let mut path = PathBuilder::new();
        for s in &self.segments {
            if state.progress01 < s.progress[0] {
                break;
            }
            let points = build_elevation_completed_points(
                &s.points,
                &s.progress,
                &s.progress,
                state.progress01,
                state.progress01,
            );
            append_line(&mut path, &points);
        }
        path.detach()
    }

    pub fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, self.key.width, self.key.height)
    }
}

fn append_line(path: &mut PathBuilder, points: &[(f32, f32)]) {
    if let Some(&first) = points.first() {
        path.move_to(first);
        for &point in &points[1..] {
            path.line_to(point);
        }
    }
}

#[derive(Clone, Debug)]
pub struct DivingProfileLayout {
    pub config: ValidatedDivingProfile,
    pub scene: ValidatedScenePresentation,
    pub geometry: Arc<DivingGeometry>,
    scale: [f32; 2],
    /// Remaining profile at the final angle, relative to the widget origin.
    remaining: StaticRaster,
    labels: [Option<ElevationLabel>; 2],
    bounds: Rect,
    content_bounds: ContentBounds,
    rotation: (f32, f32),
    matrix: Matrix,
    completed_line: Paint,
    marker: PlotMarker,
}

struct DivingProfileMeasure {
    scale: [f32; 2],
    stroke: f32,
    offset: (f32, f32),
    blur: f32,
    opacity: f32,
    rotation: (f32, f32),
    remaining_bounds: Rect,
    labels: [Option<ElevationLabel>; 2],
    bounds: Rect,
    content_bounds: ContentBounds,
}

pub struct DivingProfileFrame {
    pub state: DivingFrameState,
    pub bounds: Rect,
    pub content_bounds: ContentBounds,
    marker: Option<(f32, f32)>,
    labels: [Option<Arc<LabelRun>>; 2],
}

fn diving_marker(
    config: &ValidatedDivingProfile,
    scene: &ValidatedScenePresentation,
    opacity: f32,
) -> PlotMarker {
    PlotMarker::new(
        config.marker_variant,
        config.marker_size,
        config.marker_variant_diameter,
        &config.marker_color,
        config.marker_opacity * opacity,
        scene.scale,
    )
}

fn fill_paint(color: &str, opacity: f32) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(parse_color(color, opacity));
    paint
}

impl DivingProfileLayout {
    /// Every rectangle the layout needs, before its remaining raster is drawn.
    fn measure(
        config: &ValidatedDivingProfile,
        scene: &ValidatedScenePresentation,
        geometry: &DivingGeometry,
        depth_range: Option<(f64, f64)>,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<DivingProfileMeasure> {
        let opacity = scene.opacity * config.opacity;
        let scale = [
            config.width * scene.scale / geometry.key.width,
            config.height * scene.scale / geometry.key.height,
        ];
        let stroke = (config.remaining_line_width * scene.scale).max(1.0);
        let line = scaled_rect(geometry.bounds(), scale).with_outset((
            stroke.max(config.ceiling_line_width * scene.scale) * 0.5,
            stroke.max(config.ceiling_line_width * scene.scale) * 0.5,
        ));
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
        let radius = diving_marker(config, scene, opacity).extent();
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
        let font = if config.show_depth_metric || config.show_depth_imperial {
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
            parse_color(&config.label_color, opacity),
            opacity,
        );
        let labels = [
            (
                config.show_depth_metric,
                "M",
                1.0,
                config.metric_label_offset_x,
                config.metric_label_offset_y,
            ),
            (
                config.show_depth_imperial,
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
            raster.join(label.raster_bounds(marker_bounds, depth_range, 0.0));
        }
        let bounds = outward(
            raster
                .with_offset((config.x, config.y))
                .with_outset((1.0, 1.0)),
        );
        let content_bounds =
            ContentBounds::rotated_frame(content, config.rotation, config.x, config.y);
        Ok(DivingProfileMeasure {
            scale,
            stroke,
            offset,
            blur,
            opacity,
            remaining_bounds,
            labels,
            bounds,
            content_bounds,
            rotation: (sin, cos),
        })
    }

    /// Frame bounds from layout alone; the cached remaining raster is not drawn.
    pub fn measure_bounds(
        config: &ValidatedDivingProfile,
        scene: &ValidatedScenePresentation,
        geometry: &DivingGeometry,
        depth_range: Option<(f64, f64)>,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Rect> {
        Ok(Self::measure(config, scene, geometry, depth_range, fonts)?.bounds)
    }

    pub fn new(
        config: ValidatedDivingProfile,
        scene: ValidatedScenePresentation,
        geometry: Arc<DivingGeometry>,
        depth_range: Option<(f64, f64)>,
        fonts: &dyn FontResolver,
        raster_scale: PreviewRasterScale,
    ) -> WidgetResult<Self> {
        let DivingProfileMeasure {
            scale,
            stroke,
            offset,
            blur,
            opacity,
            remaining_bounds,
            labels,
            bounds,
            content_bounds,
            rotation: (sin, cos),
        } = Self::measure(&config, &scene, &geometry, depth_range, fonts)?;
        let matrix = Matrix::scale((scale[0], scale[1]));
        let paths = geometry.paths.lock().expect("diving profile paths lock");
        let path = paths.remaining.with_transform(&matrix);
        let area = paths.ceiling_area.with_transform(&matrix);
        let ceiling = paths.ceiling_line.with_transform(&matrix);
        drop(paths);
        let remaining_line = stroke_paint(
            &config.remaining_line_color,
            stroke,
            config.remaining_line_opacity * opacity,
        );
        let completed_line = stroke_paint(
            &config.completed_line_color,
            (config.completed_line_width * scene.scale).max(1.0),
            config.completed_line_opacity * opacity,
        );
        let remaining = StaticRaster::new(
            raster_scale,
            Point::new(config.x, config.y),
            remaining_bounds,
            SHAPE_COLOR_TYPE,
            |canvas| {
                canvas.rotate(config.rotation, None);
                canvas.draw_path(
                    &area,
                    &fill_paint(
                        &config.ceiling_area_color,
                        config.ceiling_area_opacity * opacity,
                    ),
                );
                if config.ceiling_line_width > 0.0 {
                    canvas.draw_path(
                        &ceiling,
                        &stroke_paint(
                            &config.ceiling_line_color,
                            config.ceiling_line_width * scene.scale,
                            config.ceiling_line_opacity * opacity,
                        ),
                    );
                }
                if blur > 0.0 {
                    canvas.save();
                    canvas.translate(offset);
                    let mut shadow = stroke_paint(
                        &scene.shadow_color,
                        stroke,
                        config.remaining_line_opacity * opacity,
                    );
                    shadow.set_mask_filter(MaskFilter::blur(BlurStyle::Normal, blur, true));
                    canvas.draw_path(&path, &shadow);
                    canvas.restore();
                }
                canvas.draw_path(&path, &remaining_line);
                Ok(())
            },
        )?;
        let marker = diving_marker(&config, &scene, opacity);
        Ok(Self {
            config,
            scene,
            geometry,
            scale,
            remaining,
            labels,
            bounds,
            content_bounds,
            rotation: (sin, cos),
            matrix,
            completed_line,
            marker,
        })
    }

    /// The plot frame stays stable for flat or missing external telemetry.
    /// Moving labels, marker overflow and shadows do not change that frame.
    pub fn measure_content(
        config: &ValidatedDivingProfile,
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

    pub fn frame(&self, state: DivingFrameState) -> WidgetResult<DivingProfileFrame> {
        let (sin, cos) = self.rotation;
        let marker = state.marker.map(|(x, y)| {
            let (x, y) = (x * self.scale[0], y * self.scale[1]);
            (
                self.config.x + x * cos - y * sin,
                self.config.y + x * sin + y * cos,
            )
        });
        let mut labels = [None, None];
        for (index, label) in self.labels.iter().enumerate() {
            if let Some(label) = label {
                labels[index] = Some(label.prepare(state.depth_m, 0.0)?);
            }
        }
        Ok(DivingProfileFrame {
            state,
            bounds: self.bounds,
            content_bounds: self.content_bounds,
            marker,
            labels,
        })
    }

    pub fn draw(&self, canvas: &Canvas, frame: &DivingProfileFrame) {
        canvas.save();
        canvas.translate((self.config.x, self.config.y));
        self.remaining.draw(canvas, None);
        canvas.rotate(self.config.rotation, None);
        let line = self
            .geometry
            .completed_path(&frame.state)
            .with_transform(&self.matrix);
        canvas.draw_path(&line, &self.completed_line);
        canvas.restore();
        let Some(marker) = frame.marker else {
            return;
        };
        // Circular markers need only their scene position; avoid rotating their rasterization.
        self.marker.draw(canvas, marker);
        for (index, run) in frame.labels.iter().enumerate() {
            if let Some(run) = run {
                self.labels[index]
                    .as_ref()
                    .expect("elevation frame label style")
                    .draw(canvas, run, marker);
            }
        }
    }
}
