//! Shared cached remaining profile, completed area, marker and upright labels.
use super::{
    geometry::{ElevationFrameState, ElevationGeometry},
    validation::ValidatedElevationPlot,
};
use crate::{
    error::{WidgetError, WidgetResult},
    model::{scene::ValidatedScenePresentation, standard_widgets::widget_font_weight},
    render::text::{
        draw_text_blob, layout_text, parse_color, resolve_font, FontResolver, ResolvedTextStyle,
    },
    widgets::{
        route::layout::{outward, rotated_rect, scaled_rect, stroke_paint},
        value::preview::{surface, ContentBounds},
    },
};
use skia_safe::{BlurStyle, Canvas, Color, Font, Image, MaskFilter, Matrix, Paint, Rect, TextBlob};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct ElevationLayout {
    pub config: ValidatedElevationPlot,
    pub scene: ValidatedScenePresentation,
    pub geometry: Arc<ElevationGeometry>,
    pub altitude_offset_m: f64,
    scale: [f32; 2],
    remaining: Image,
    remaining_bounds: Rect,
    raster: Rect,
    font: Option<Font>,
    completed_line: Paint,
    completed_area: Paint,
    marker: Paint,
    variant: Paint,
}

struct ElevationLabel {
    blob: TextBlob,
    baseline: f32,
    style: ResolvedTextStyle,
}

pub struct ElevationFrame {
    pub state: ElevationFrameState,
    pub bounds: Rect,
    pub content_bounds: ContentBounds,
    labels: Vec<ElevationLabel>,
}

fn fill_paint(color: &str, opacity: f32) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(parse_color(color, opacity));
    paint
}

impl ElevationLayout {
    pub fn new(
        config: ValidatedElevationPlot,
        scene: ValidatedScenePresentation,
        geometry: Arc<ElevationGeometry>,
        altitude_offset_m: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
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
        let mut layer = surface(
            remaining_bounds.width() as u32,
            remaining_bounds.height() as u32,
        )?;
        let canvas = layer.canvas();
        canvas.clear(Color::TRANSPARENT);
        canvas.translate((-remaining_bounds.left, -remaining_bounds.top));
        canvas.rotate(config.rotation, None);
        let paths = geometry.paths.lock().expect("elevation paths lock");
        let matrix = Matrix::scale((scale[0], scale[1]));
        let path = paths.0.with_transform(&matrix);
        let area = paths.1.with_transform(&matrix);
        canvas.draw_path(
            &area,
            &fill_paint(
                &config.area_remaining_color,
                config.area_remaining_opacity * scene.opacity,
            ),
        );
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
        canvas.draw_path(
            &path,
            &stroke_paint(
                &config.remaining_line_color,
                stroke,
                config.remaining_line_opacity * scene.opacity,
            ),
        );
        drop(paths);
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
        let font = if config.show_elevation_metric || config.show_elevation_imperial {
            Some(resolve_font(
                fonts,
                config.label_font.as_deref(),
                config.label_font_size * scene.scale,
            )?)
        } else {
            None
        };
        let completed_line = stroke_paint(
            &config.completed_line_color,
            (config.completed_line_width * scene.scale).max(1.0),
            config.completed_line_opacity * scene.opacity,
        );
        let completed_area = fill_paint(
            &config.area_completed_color,
            config.area_completed_opacity * scene.opacity,
        );
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
            remaining: layer.image_snapshot(),
            remaining_bounds,
            raster,
            font,
            completed_line,
            completed_area,
            marker,
            variant,
        })
    }

    pub fn set_position(&mut self, x: f32, y: f32) {
        self.config.x = x;
        self.config.y = y;
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
        let config = &self.config;
        let scene = &self.scene;
        let scale = scene.scale;
        let content_bounds = ContentBounds::rotated_frame(
            Self::measure_content(config, scene),
            config.rotation,
            config.x,
            config.y,
        );
        let mut bounds = self.raster.with_offset((config.x, config.y));
        let mut labels = Vec::with_capacity(2);
        if let Some(font) = &self.font {
            let (sin, cos) = config.rotation.to_radians().sin_cos();
            let (mx, my) = (
                state.marker_x * self.scale[0],
                state.marker_y * self.scale[1],
            );
            let marker = (
                config.x + mx * cos - my * sin,
                config.y + mx * sin + my * cos,
            );
            for (enabled, unit, conversion, ox, oy) in [
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
            ] {
                if !enabled {
                    continue;
                }
                let text = state.elevation_m.map_or_else(
                    || format!("-- {unit}"),
                    |value| {
                        format!(
                            "{} {unit}",
                            ((value + self.altitude_offset_m) * conversion).round() as i64
                        )
                    },
                );
                let run = layout_text(&text, font, 0.0);
                let blob = run.blob.ok_or_else(|| {
                    WidgetError::Render("unable to lay out elevation label".into())
                })?;
                let m = run.measurement;
                let (x, y) = (marker.0 + ox * scale, marker.1 + oy * scale);
                let font_size = config.label_font_size * scale;
                let baseline =
                    y + (font_size * 0.92 - (m.bounds_bottom - m.bounds_top)) * 0.5 - m.bounds_top;
                let border = scene.border_thickness * scale;
                let ink = Rect::new(
                    x + m.bounds_left,
                    baseline + m.bounds_top,
                    x + m.bounds_right,
                    baseline + m.bounds_bottom,
                )
                .with_outset((border * 0.5, border * 0.5));
                bounds.join(ink);
                let blur = scene.shadow_strength * scale;
                let offset = scene.shadow_distance * scale;
                if blur > 0.0 {
                    bounds.join(
                        ink.with_outset((blur * 3.0, blur * 3.0))
                            .with_offset((offset, offset)),
                    );
                }
                labels.push(ElevationLabel {
                    blob,
                    baseline,
                    style: ResolvedTextStyle {
                        x,
                        y,
                        font_name: config.label_font.clone(),
                        font_size,
                        font_weight: widget_font_weight(),
                        italic: false,
                        letter_spacing: 0.0,
                        line_height: font_size * 0.92,
                        color: parse_color(&config.label_color, scene.opacity),
                        opacity: scene.opacity,
                        shadow_color: Some(parse_color(&scene.shadow_color, scene.opacity)),
                        shadow_strength: blur,
                        shadow_distance: offset,
                        border_color: Some(parse_color(&scene.border_color, scene.opacity)),
                        border_thickness: border,
                    },
                });
            }
        }
        Ok(ElevationFrame {
            state,
            bounds: outward(bounds.with_outset((1.0, 1.0))),
            content_bounds,
            labels,
        })
    }

    pub fn draw(&self, canvas: &Canvas, frame: &ElevationFrame) {
        canvas.save();
        canvas.translate((self.config.x, self.config.y));
        canvas.draw_image(
            &self.remaining,
            (self.remaining_bounds.left, self.remaining_bounds.top),
            None,
        );
        canvas.rotate(self.config.rotation, None);
        let matrix = Matrix::scale((self.scale[0], self.scale[1]));
        let (line, area) = self.geometry.completed_paths(&frame.state);
        canvas.draw_path(&area.with_transform(&matrix), &self.completed_area);
        canvas.draw_path(&line.with_transform(&matrix), &self.completed_line);
        let point = (
            frame.state.marker_x * self.scale[0],
            frame.state.marker_y * self.scale[1],
        );
        canvas.restore();
        // Circular markers need only their scene position; avoid rotating their rasterization.
        let (sin, cos) = self.config.rotation.to_radians().sin_cos();
        let point = (
            self.config.x + point.0 * cos - point.1 * sin,
            self.config.y + point.0 * sin + point.1 * cos,
        );
        if self.config.marker_variant != "single" {
            canvas.draw_circle(
                point,
                self.config.marker_variant_diameter * self.scene.scale * 0.5,
                &self.variant,
            );
        }
        canvas.draw_circle(
            point,
            (self.config.marker_size * self.scene.scale).max(2.0),
            &self.marker,
        );
        for label in &frame.labels {
            draw_text_blob(canvas, &label.blob, label.baseline, &label.style);
        }
    }
}
