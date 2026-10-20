//! Cached depth remainder and ceiling area/boundary; only completed depth and marker animate.
use super::{
    geometry::{DivingFrameState, DivingGeometry},
    validation::ValidatedDivingProfile,
};
use crate::{
    error::WidgetResult,
    model::{scene::ValidatedScenePresentation, standard_widgets::widget_font_weight},
    render::text::{parse_color, resolve_font, FontResolver, ResolvedTextStyle},
    widgets::{
        elevation::label::{ElevationLabel, LabelRun},
        route::layout::{outward, rotated_rect, scaled_rect, stroke_paint},
        value::preview::{surface, ContentBounds},
    },
};
use skia_safe::{BlurStyle, Canvas, Color, Image, MaskFilter, Matrix, Paint, Rect};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct DivingProfileLayout {
    pub config: ValidatedDivingProfile,
    pub scene: ValidatedScenePresentation,
    pub geometry: Arc<DivingGeometry>,
    scale: [f32; 2],
    remaining: Image,
    remaining_bounds: Rect,
    raster: Rect,
    labels: [Option<ElevationLabel>; 2],
    bounds: Rect,
    content_bounds: ContentBounds,
    rotation: (f32, f32),
    matrix: Matrix,
    completed_line: Paint,
    marker: Paint,
    variant: Paint,
}

pub struct DivingProfileFrame {
    pub state: DivingFrameState,
    pub bounds: Rect,
    pub content_bounds: ContentBounds,
    marker: Option<(f32, f32)>,
    labels: [Option<Arc<LabelRun>>; 2],
}

fn fill_paint(color: &str, opacity: f32) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(parse_color(color, opacity));
    paint
}

impl DivingProfileLayout {
    pub fn new(
        config: ValidatedDivingProfile,
        scene: ValidatedScenePresentation,
        geometry: Arc<DivingGeometry>,
        depth_range: Option<(f64, f64)>,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
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
        let mut layer = surface(
            remaining_bounds.width() as u32,
            remaining_bounds.height() as u32,
        )?;
        let canvas = layer.canvas();
        canvas.clear(Color::TRANSPARENT);
        canvas.translate((-remaining_bounds.left, -remaining_bounds.top));
        canvas.rotate(config.rotation, None);

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
        let content = Self::measure_content(&config, &scene);
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
        let font = if config.show_depth_metric || config.show_depth_imperial {
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
            color: parse_color(&config.label_color, opacity),
            opacity: opacity,
            shadow_color: Some(parse_color(&scene.shadow_color, opacity)),
            shadow_strength: scene.shadow_strength * scene.scale,
            shadow_distance: scene.shadow_distance * scene.scale,
            border_color: Some(parse_color(&scene.border_color, opacity)),
            border_thickness: scene.border_thickness * scene.scale,
            numerals: scene.numerals,
        };
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
        let marker = fill_paint(&config.marker_color, config.marker_opacity * opacity);
        let mut variant = marker.clone();
        match config.marker_variant.as_str() {
            "ring" => {
                variant.set_style(skia_safe::paint::Style::Stroke);
                variant.set_stroke_width(1.5);
            }
            "halo" => {
                variant = fill_paint(&config.marker_color, config.marker_opacity * opacity * 0.35)
            }
            _ => {}
        }
        Ok(Self {
            config,
            scene,
            geometry,
            scale,
            remaining: layer.image_snapshot(),
            remaining_bounds,
            raster,
            labels,
            bounds,
            content_bounds,
            rotation: (sin, cos),
            matrix,
            completed_line,
            marker,
            variant,
        })
    }

    pub fn set_position(&mut self, x: f32, y: f32) {
        if self.config.x == x && self.config.y == y {
            return;
        }
        self.config.x = x;
        self.config.y = y;
        self.bounds = outward(self.raster.with_offset((x, y)).with_outset((1.0, 1.0)));
        self.content_bounds = ContentBounds::rotated_frame(
            Self::measure_content(&self.config, &self.scene),
            self.config.rotation,
            x,
            y,
        );
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
        canvas.draw_image(
            &self.remaining,
            (self.remaining_bounds.left, self.remaining_bounds.top),
            None,
        );
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
        if self.config.marker_variant != "single" {
            canvas.draw_circle(
                marker,
                self.config.marker_variant_diameter * self.scene.scale * 0.5,
                &self.variant,
            );
        }
        canvas.draw_circle(
            marker,
            (self.config.marker_size * self.scene.scale).max(2.0),
            &self.marker,
        );
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
