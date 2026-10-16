//! Native frame composition for preview surfaces and borrowed video pixel buffers.
use super::{
    assets::{PreparedRenderAssets, PreparedWidget},
    report::{
        boxed_report, plot_report, FrameRenderReport, MetricPresentationReport,
        WidgetGeometryReport, WidgetRenderReport,
    },
    surface::wrap_native_surface,
    FrameSize, PreparedPreviewAssets,
};
use crate::{
    activity::schema::DenseActivityReport,
    debug::RenderProfiler,
    error::{CoreError, CoreResult},
    fonts::NativeFonts,
};
use ovrley_widgets::{
    model::standard_metrics::{display_type_layout_mode, DisplayTypeLayoutMode},
    render::text::{validated_value_style, FontResolver},
    widgets::{
        gauges::{arc::arc_point, frame::GaugeLayout},
        gradient::frame::{prepare_gradient_frame, validated_gradient_style},
        lap_timer::format::lap_timer_content,
        lean_angle::layout::LEAN_ANGLE_MAX_FILL_SWEEP,
        time::format::{format_validated_time_parts, ElapsedTimeValues},
        value::{
            format::format_metric_parts,
            layout::{draw_metric_parts, static_metric_parts_for_value, StaticMetricParts},
        },
    },
};
use skia_safe::Canvas;
use std::{path::PathBuf, time::Instant};

/// Prepared, immutable context shared by video frame-render workers.
///
/// Construction resolves the preview-owned asset bundle into the video
/// renderer's strict contract once. Per-frame callers then provide only the
/// activity-frame index, destination pixels, and profiler.
#[derive(Clone, Copy)]
pub struct VideoFrameRenderer<'a> {
    font_dirs: &'a [PathBuf],
    dense_activity: &'a DenseActivityReport,
    prepared_assets: &'a PreparedRenderAssets,
    frame_size: FrameSize,
    base_rgba: &'a [u8],
    raster_rgba: &'a [u8],
    blank_leading_frame_count: usize,
}

impl<'a> VideoFrameRenderer<'a> {
    pub fn new(
        font_dirs: &'a [PathBuf],
        dense_activity: &'a DenseActivityReport,
        prepared_preview_assets: &'a PreparedPreviewAssets,
        frame_size: FrameSize,
        blank_leading_frame_count: u64,
    ) -> CoreResult<Self> {
        let expected_len = frame_size.rgba_len()?;
        let base_rgba = prepared_preview_assets.base_rgba.as_slice();
        if base_rgba.len() != expected_len {
            return Err(CoreError::Render(format!(
                "Prepared RGBA base layer has {} bytes; expected {expected_len} for {}x{} video frames",
                base_rgba.len(), frame_size.width, frame_size.height
            )));
        }
        let raster_rgba = &prepared_preview_assets.raster_rgba;
        if raster_rgba.len() != expected_len {
            return Err(CoreError::Render(format!(
                "Prepared raster layer has {} bytes; expected {expected_len}",
                raster_rgba.len()
            )));
        }
        let blank_leading_frame_count = usize::try_from(blank_leading_frame_count)
            .map_err(|_| CoreError::Render("Blank leading frame count exceeds usize".into()))?;

        Ok(Self {
            font_dirs,
            dense_activity,
            prepared_assets: &prepared_preview_assets.prepared_assets,
            frame_size,
            base_rgba,
            raster_rgba,
            blank_leading_frame_count,
        })
    }

    pub fn render_rgba(
        &self,
        frame_index: usize,
        pixels: &mut [u8],
        frame_profiler: &mut RenderProfiler,
    ) -> CoreResult<()> {
        if pixels.len() != self.base_rgba.len() {
            return Err(CoreError::Render(format!(
                "Video frame buffer has {} bytes; expected {}",
                pixels.len(),
                self.base_rgba.len()
            )));
        }

        let started = Instant::now();
        let activity_frame_index = prepare_output_frame(
            frame_index,
            self.blank_leading_frame_count,
            self.dense_activity.frame_count,
            pixels,
        );
        let Some(activity_frame_index) = activity_frame_index else {
            pixels.copy_from_slice(self.raster_rgba);
            frame_profiler.record_ms(
                "raster.restore",
                Instant::now().duration_since(started).as_secs_f64() * 1000.0,
            );
            return Ok(());
        };

        pixels.copy_from_slice(self.base_rgba);
        let restore_ms = started.elapsed().as_secs_f64() * 1000.0;
        frame_profiler.record_ms("base.restore", restore_ms);

        let mut surface = frame_profiler.measure("surface.create", || {
            wrap_native_surface(self.frame_size.width, self.frame_size.height, pixels)
        })?;
        self.prepared_assets.draw(
            surface.canvas(),
            self.dense_activity,
            activity_frame_index,
            &NativeFonts(self.font_dirs),
            true,
            frame_profiler,
        )?;
        Ok(())
    }
}

fn prepare_output_frame(
    output_frame_index: usize,
    blank_leading_frame_count: usize,
    activity_frame_count: usize,
    pixels: &mut [u8],
) -> Option<usize> {
    let activity_frame_index = output_frame_index.checked_sub(blank_leading_frame_count);
    if activity_frame_index.is_none_or(|index| index >= activity_frame_count) {
        pixels.fill(0);
        return None;
    }
    activity_frame_index
}

#[cfg(test)]
mod video_frame_mapping_tests {
    use super::prepare_output_frame;

    #[test]
    fn clears_frames_outside_activity_and_maps_frames_inside_it() {
        let mut pixels = [255; 4];
        assert_eq!(prepare_output_frame(0, 2, 3, &mut pixels), None);
        assert_eq!(pixels, [0; 4]);

        pixels.fill(255);
        assert_eq!(prepare_output_frame(2, 2, 3, &mut pixels), Some(0));
        assert_eq!(pixels, [255; 4]);

        assert_eq!(prepare_output_frame(4, 2, 3, &mut pixels), Some(2));
        assert_eq!(prepare_output_frame(5, 2, 3, &mut pixels), None);
        assert_eq!(pixels, [0; 4]);
    }
}

impl PreparedRenderAssets {
    /// Composes dynamic widgets; the host restores the static layer before calling this.
    pub fn draw(
        &self,
        canvas: &Canvas,
        activity: &DenseActivityReport,
        index: usize,
        fonts: &dyn FontResolver,
        static_parts_rendered: bool,
        profiler: &mut RenderProfiler,
    ) -> CoreResult<FrameRenderReport> {
        let started = Instant::now();
        let mut report = FrameRenderReport::default();
        // Preserve layer order: intrinsic values precede boxed presentations.
        for mode in [
            DisplayTypeLayoutMode::Intrinsic,
            DisplayTypeLayoutMode::Boxed,
        ] {
            for (value_idx, value) in self.values.iter().enumerate() {
                if display_type_layout_mode(value.display_type) != mode {
                    continue;
                }
                let timing = match &value.widget {
                    PreparedWidget::HeadingTape(_) => "heading.draw",
                    PreparedWidget::LeanAngle(_) => "lean_angle.draw",
                    PreparedWidget::Gauge(_) => "gauge.draw",
                    PreparedWidget::GForce(_) => "g_force.draw",
                    _ => "text.dynamic",
                };
                if let Some(widget) = profiler.measure(timing, || {
                    self.draw_value(
                        &value.widget,
                        canvas,
                        activity,
                        index,
                        fonts,
                        static_parts_rendered,
                    )
                })? {
                    report.metric_presentations.push(MetricPresentationReport {
                        value_idx,
                        metric_kind: value.metric_kind,
                        display_type: value.display_type,
                        widget,
                    });
                }
            }
        }
        for cache in &self.route_caches {
            let layout = &cache.layout;
            let progress = cache.frame_progress[index];
            let frame = layout.frame(progress);
            profiler.measure("composite.route", || layout.draw(canvas, &frame));
            let config = &layout.config;
            let scale = layout.scene.scale;
            report.route_widgets.push(plot_report(
                WidgetGeometryReport {
                    point_count: layout.geometry.points.len(),
                    source_point_count: layout.geometry.source_point_count,
                    simplification: layout.geometry.simplification.clone(),
                    bbox: [0.0, 0.0, config.width * scale, config.height * scale],
                    widget_width: (config.width * scale).round() as u32,
                    widget_height: (config.height * scale).round() as u32,
                    rotation_deg: config.rotation,
                },
                (config.x, config.y),
                progress,
                (frame.state.marker_x * scale, frame.state.marker_y * scale),
            ));
        }
        for cache in &self.elevation_caches {
            let layout = &cache.layout;
            let state = cache.frame_states[index];
            let frame = layout.frame(state)?;
            profiler.measure("composite.elevation", || layout.draw(canvas, &frame));
            let config = &layout.config;
            let scale = layout.scene.scale;
            report.elevation_widgets.push(plot_report(
                WidgetGeometryReport {
                    point_count: layout.geometry.points.len(),
                    source_point_count: layout.geometry.source_point_count,
                    simplification: layout.geometry.simplification.clone(),
                    bbox: [0.0, 0.0, config.width * scale, config.height * scale],
                    widget_width: (config.width * scale).round() as u32,
                    widget_height: (config.height * scale).round() as u32,
                    rotation_deg: config.rotation,
                },
                (config.x, config.y),
                state.progress01,
                (state.marker_x, state.marker_y),
            ));
        }
        profiler.record_ms("frame.draw", started.elapsed().as_secs_f64() * 1000.0);
        Ok(report)
    }

    fn draw_value(
        &self,
        value: &PreparedWidget,
        canvas: &Canvas,
        activity: &DenseActivityReport,
        index: usize,
        fonts: &dyn FontResolver,
        static_parts_rendered: bool,
    ) -> CoreResult<Option<WidgetRenderReport>> {
        let scene = &self.scene.presentation;
        let scale = scene.scale;
        let report = match value {
            PreparedWidget::StandardText(widget) => {
                let config = &widget.validated;
                let style = validated_value_style(config, scene, scale);
                let parts = format_metric_parts(
                    config,
                    &activity.metric_sample(config.metric, index, widget.altitude_offset_m),
                );
                draw_metric_parts(
                    canvas,
                    &style,
                    &parts,
                    scale,
                    fonts,
                    if static_parts_rendered {
                        static_metric_parts_for_value(config)
                    } else {
                        StaticMetricParts::default()
                    },
                    config,
                )?;
                return Ok(None);
            }
            PreparedWidget::TimeText(config) => {
                let style = validated_value_style(&config.base, scene, scale);
                let second = self.scene.start + activity.frame_elapsed_seconds[index];
                let parts = format_validated_time_parts(
                    config,
                    activity.series.time.get(index).and_then(Option::as_deref),
                    ElapsedTimeValues {
                        activity_seconds: second,
                        export_seconds: second - self.scene.export_start_seconds,
                    },
                    self.timezone,
                );
                draw_metric_parts(
                    canvas,
                    &style,
                    &parts,
                    scale,
                    fonts,
                    if static_parts_rendered {
                        static_metric_parts_for_value(&config.base)
                    } else {
                        StaticMetricParts::default()
                    },
                    &config.base,
                )?;
                return Ok(None);
            }
            PreparedWidget::Gradient(config) => {
                let style = validated_gradient_style(config, scene, scale);
                prepare_gradient_frame(
                    config,
                    &style,
                    activity.series.gradient.get(index).copied().flatten(),
                    scale,
                    scene.opacity,
                    fonts,
                )?
                .draw(canvas, fonts)?;
                return Ok(None);
            }
            PreparedWidget::LapTimer(widget) => {
                let history = activity.lap_history();
                let content =
                    lap_timer_content(&widget.validated, history, activity.lap_sample(index))?;
                widget.layout.prepare_cached_frame(history, &content)?.draw(canvas);
                return Ok(None);
            }
            PreparedWidget::HeadingTape(cache) => {
                let raw = activity.series.heading.get(index).copied().flatten();
                cache.prepare_frame(raw).draw(canvas, cache);
                let bounds = cache.frame;
                boxed_report(
                    [bounds.left, bounds.top, bounds.width(), bounds.height()],
                    (bounds.width().ceil() as u32, bounds.height().ceil() as u32),
                    0.0,
                    "heading_tape",
                    0,
                    raw.unwrap_or(0.0) as f32 / 360.0,
                    (
                        bounds.width() * 0.5,
                        cache.tape_body_y + cache.tape_body_height * 0.5,
                    ),
                )
            }
            PreparedWidget::LeanAngle(cache) => {
                let frame = cache.prepare_frame(
                    activity.series.lean_angle.get(index).copied().flatten(),
                    fonts,
                )?;
                frame.draw(canvas, cache);
                boxed_report(
                    [cache.x, cache.y, cache.layout.width, cache.layout.height],
                    (cache.width, cache.height),
                    cache.rotation,
                    "lean_angle",
                    0,
                    frame.fill_sweep.abs() / LEAN_ANGLE_MAX_FILL_SWEEP,
                    (frame.marker.x - cache.x, frame.marker.y - cache.y),
                )
            }
            PreparedWidget::Gauge(cache) => {
                let frame = cache
                    .layout
                    .prepare_frame(cache.frame_states[index].clone(), fonts)?;
                frame.draw(canvas, &cache.layout);
                let (x, y, width, height, rotation, kind, marker) = match &cache.layout {
                    GaugeLayout::Linear(layout) => (
                        layout.x,
                        layout.y,
                        layout.width,
                        layout.height,
                        layout.rotation,
                        "linear_gauge",
                        (
                            layout.width * frame.state.fill01,
                            layout.height * (1.0 - frame.state.fill01),
                        ),
                    ),
                    GaugeLayout::Arc(layout) => {
                        let geometry = layout.geometry;
                        let marker = arc_point(
                            geometry.center_x,
                            geometry.center_y,
                            geometry.radius,
                            geometry.start_angle + geometry.sweep_angle * frame.state.fill01,
                        );
                        (
                            layout.x,
                            layout.y,
                            layout.width,
                            layout.height,
                            layout.rotation,
                            "arc_gauge",
                            (marker.x - layout.x, marker.y - layout.y),
                        )
                    }
                };
                boxed_report(
                    [x, y, width, height],
                    (width.round() as u32, height.round() as u32),
                    rotation,
                    kind,
                    0,
                    frame.state.fill01,
                    marker,
                )
            }
            PreparedWidget::GForce(cache) => {
                let sample = cache.samples[index];
                let drawing = &cache.layout;
                let frame = drawing.prepare_frame(sample, fonts)?;
                frame.draw(canvas, drawing);
                let state = frame.state;
                boxed_report(
                    [
                        drawing.x,
                        drawing.y,
                        drawing.layout.width,
                        drawing.layout.height,
                    ],
                    (
                        drawing.layout.width.ceil() as u32,
                        drawing.layout.height.ceil() as u32,
                    ),
                    0.0,
                    "g_force",
                    cache.source_point_count,
                    state
                        .magnitude
                        .filter(|_| sample.max_g > 0.0)
                        .map_or(0.0, |magnitude| (magnitude / sample.max_g).min(1.0) as f32),
                    (state.marker_x, state.marker_y),
                )
            }
        };
        Ok(Some(report))
    }
}
