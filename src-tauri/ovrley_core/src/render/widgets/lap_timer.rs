//! Current and best lap timer text widgets.

use super::types::{LapTimerWidgetCache, StaticLayer};
use crate::activity::schema::DenseActivityReport;
use crate::debug::RenderProfiler;
use crate::error::{CoreError, CoreResult};
use crate::normalize::{LapTimerMode, ValidatedLapTimer, ValidatedSceneConfig};
use crate::render::surface::create_surface;
use crate::render::text::{
    baseline_for_text_top_with_line_height, draw_text_with_vertical_metrics_text,
    measure_text_with_font, resolve_font, ResolvedTextStyle,
};
use skia_safe::Canvas;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const LABEL_FONT_RATIO: f32 = 0.35;
const LINE_HEIGHT_RATIO: f32 = 0.92;
const PLACEHOLDER: &str = "--:--.--";

pub fn format_lap_duration(duration_seconds: f64) -> String {
    let hundredths = (duration_seconds * 100.0).round() as u64;
    let hours = hundredths / 360_000;
    let minutes = if hours > 0 {
        (hundredths / 6_000) % 60
    } else {
        hundredths / 6_000
    };
    let seconds = (hundredths / 100) % 60;
    let remainder = hundredths % 100;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}.{remainder:02}")
    } else {
        format!("{minutes:02}:{seconds:02}.{remainder:02}")
    }
}

fn lap_number_at(dense_activity: &DenseActivityReport, frame_index: usize) -> CoreResult<i64> {
    dense_activity
        .series
        .lap_number
        .get(frame_index)
        .and_then(|value| *value)
        .ok_or_else(|| CoreError::Render(format!("lap_number is missing at frame {frame_index}")))
}

fn lap_time_at(
    dense_activity: &DenseActivityReport,
    frame_index: usize,
) -> CoreResult<Option<f64>> {
    dense_activity
        .series
        .lap_time_seconds
        .get(frame_index)
        .copied()
        .ok_or_else(|| {
            CoreError::Render(format!(
                "lap_time_seconds is missing at frame {frame_index}"
            ))
        })
}

fn best_lap_text(
    dense_activity: &DenseActivityReport,
    lap_number: i64,
    current_lap_time: f64,
) -> CoreResult<String> {
    if lap_number < 0 {
        return Ok(PLACEHOLDER.to_string());
    }
    if lap_number == 0 {
        return Ok(format_lap_duration(current_lap_time));
    }
    let best = dense_activity
        .series
        .lap_durations_best_so_far_seconds
        .get((lap_number - 1) as usize)
        .copied()
        .ok_or_else(|| {
            CoreError::Render(format!(
                "best lap metadata is missing for completed lap {lap_number}"
            ))
        })?;
    Ok(format_lap_duration(best))
}

pub fn lap_timer_value_text(
    mode: LapTimerMode,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
) -> CoreResult<String> {
    let lap_number = lap_number_at(dense_activity, frame_index)?;
    let current_lap_time = lap_time_at(dense_activity, frame_index)?;
    if lap_number < 0 {
        return Ok(PLACEHOLDER.to_string());
    }
    let current_lap_time = current_lap_time.ok_or_else(|| {
        CoreError::Render(format!(
            "active lap {lap_number} is missing lap_time_seconds at frame {frame_index}"
        ))
    })?;
    match mode {
        LapTimerMode::CurrentLap => Ok(format_lap_duration(current_lap_time)),
        LapTimerMode::BestLap => best_lap_text(dense_activity, lap_number, current_lap_time),
    }
}

fn state_value_text(
    dense_activity: &DenseActivityReport,
    completed_lap_count: usize,
) -> CoreResult<String> {
    let best = dense_activity
        .series
        .lap_durations_best_so_far_seconds
        .get(completed_lap_count - 1)
        .copied()
        .ok_or_else(|| {
            CoreError::Render(format!(
                "best lap metadata is missing for completed lap {completed_lap_count}"
            ))
        })?;
    Ok(format_lap_duration(best))
}

fn draw_content(
    canvas: &Canvas,
    style: &ResolvedTextStyle,
    label: &str,
    show_label: bool,
    value: &str,
    font_dirs: &[PathBuf],
) -> CoreResult<()> {
    let (label_style, value_style) = content_styles(style, show_label);
    if let Some(label_style) = label_style {
        draw_text_with_vertical_metrics_text(canvas, label, label, &label_style, font_dirs)?;
    }

    draw_text_with_vertical_metrics_text(canvas, value, "0123456789-:.", &value_style, font_dirs)
}

fn content_styles(
    style: &ResolvedTextStyle,
    show_label: bool,
) -> (Option<ResolvedTextStyle>, ResolvedTextStyle) {
    let label_line_height = style.font_size * LABEL_FONT_RATIO * LINE_HEIGHT_RATIO;
    let label_style = show_label.then(|| {
        let mut label_style = style.clone();
        label_style.font_size = style.font_size * LABEL_FONT_RATIO;
        label_style.line_height = label_line_height;
        label_style
    });
    let mut value_style = style.clone();
    value_style.y = style.y + if show_label { label_line_height } else { 0.0 };
    value_style.line_height = style.font_size * LINE_HEIGHT_RATIO;
    (label_style, value_style)
}

fn include_text_bounds(
    bounds: &mut Option<(f32, f32, f32, f32)>,
    text: &str,
    vertical_metrics_text: &str,
    style: &ResolvedTextStyle,
    font_dirs: &[PathBuf],
) -> CoreResult<()> {
    if text.is_empty() {
        return Ok(());
    }

    let font = resolve_font(font_dirs, style.font_name.as_deref(), style.font_size)?;
    let measured = measure_text_with_font(text, &font);
    let baseline = baseline_for_text_top_with_line_height(
        vertical_metrics_text,
        style.y,
        &font,
        style.line_height,
    );
    let left = style.x + measured.bounds_left;
    let top = baseline + measured.bounds_top;
    let right = style.x + measured.bounds_right;
    let bottom = baseline + measured.bounds_bottom;

    *bounds = Some(match *bounds {
        Some((current_left, current_top, current_right, current_bottom)) => (
            current_left.min(left),
            current_top.min(top),
            current_right.max(right),
            current_bottom.max(bottom),
        ),
        None => (left, top, right, bottom),
    });
    Ok(())
}

fn text_effect_padding(style: &ResolvedTextStyle) -> f32 {
    let border_width = if style.border_color.is_some() {
        style.border_thickness
    } else {
        0.0
    };
    let shadow_extent = if style.shadow_color.is_some() && style.shadow_strength > 0.0 {
        style.shadow_distance.abs() + style.shadow_strength * 3.0
    } else {
        0.0
    };
    (border_width.max(1.0) * 0.5 + shadow_extent).ceil() + 1.0
}

fn prepare_content_layer(
    style: &ResolvedTextStyle,
    label: &str,
    show_label: bool,
    value: &str,
    font_dirs: &[PathBuf],
) -> CoreResult<StaticLayer> {
    let (label_style, value_style) = content_styles(style, show_label);
    let mut bounds = None;
    if let Some(label_style) = label_style {
        include_text_bounds(&mut bounds, label, label, &label_style, font_dirs)?;
    }

    include_text_bounds(&mut bounds, value, "0123456789-:.", &value_style, font_dirs)?;

    let (min_x, min_y, max_x, max_y) = bounds
        .ok_or_else(|| CoreError::Render("lap timer cache content has no drawable text".into()))?;
    let padding = text_effect_padding(style);
    let layer_x = (min_x - padding).floor();
    let layer_y = (min_y - padding).floor();
    let layer_width = ((max_x + padding).ceil() - layer_x).max(1.0) as u32;
    let layer_height = ((max_y + padding).ceil() - layer_y).max(1.0) as u32;
    let mut surface = create_surface(layer_width, layer_height)?;
    surface.canvas().clear(skia_safe::Color::TRANSPARENT);

    let mut local_style = style.clone();
    local_style.x -= layer_x;
    local_style.y -= layer_y;
    draw_content(
        surface.canvas(),
        &local_style,
        label,
        show_label,
        value,
        font_dirs,
    )?;

    Ok(StaticLayer {
        image: surface.image_snapshot(),
        x: layer_x,
        y: layer_y,
    })
}

fn draw_cached_layer(canvas: &Canvas, layer: &StaticLayer) {
    canvas.draw_image(&layer.image, (layer.x, layer.y), None);
}

pub fn prepare_lap_timer_cache(
    validated: &ValidatedLapTimer,
    dense_activity: &DenseActivityReport,
    scene: &ValidatedSceneConfig,
    scale: f32,
    font_dirs: &[PathBuf],
    prepare_profiler: &mut RenderProfiler,
) -> CoreResult<LapTimerWidgetCache> {
    let mut states = BTreeSet::new();
    if validated.mode == LapTimerMode::BestLap {
        for lap_number in &dense_activity.series.lap_number {
            if let Some(lap_number) = lap_number {
                if *lap_number > 0 {
                    states.insert(*lap_number as usize);
                }
            }
        }
    }

    let style = crate::render::text::validated_lap_timer_style(validated, scene, scale);
    let mut state_layers = BTreeMap::new();
    for completed_lap_count in states {
        let value = state_value_text(dense_activity, completed_lap_count)?;
        let layer = prepare_profiler.measure("lap_timer.cache_surface", || {
            prepare_content_layer(
                &style,
                &validated.label,
                validated.show_label,
                &value,
                font_dirs,
            )
        })?;
        state_layers.insert(completed_lap_count, layer);
    }

    Ok(LapTimerWidgetCache { state_layers })
}

pub fn draw_lap_timer(
    canvas: &Canvas,
    validated: &ValidatedLapTimer,
    cache: &LapTimerWidgetCache,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
    style: &ResolvedTextStyle,
    font_dirs: &[PathBuf],
) -> CoreResult<()> {
    if validated.mode == LapTimerMode::BestLap {
        let lap_number = lap_number_at(dense_activity, frame_index)?;
        if lap_number > 0 {
            let layer = cache
                .state_layers
                .get(&(lap_number as usize))
                .ok_or_else(|| {
                    CoreError::Render(format!(
                        "best lap cache is missing completed lap {lap_number}"
                    ))
                })?;
            draw_cached_layer(canvas, layer);
            return Ok(());
        }
    }

    let value = lap_timer_value_text(validated.mode, dense_activity, frame_index)?;
    draw_content(
        canvas,
        style,
        &validated.label,
        validated.show_label,
        &value,
        font_dirs,
    )
}

#[cfg(test)]
mod tests {
    use super::{format_lap_duration, prepare_content_layer};
    use crate::render::text::ResolvedTextStyle;
    use skia_safe::Color;

    #[test]
    fn formats_sub_hour_and_hour_plus_laps() {
        assert_eq!(format_lap_duration(3.456), "00:03.46");
        assert_eq!(format_lap_duration(3599.999), "01:00:00.00");
        assert_eq!(format_lap_duration(3661.2), "01:01:01.20");
    }

    #[test]
    fn prepares_best_lap_text_as_a_positioned_widget_local_layer() {
        let style = ResolvedTextStyle {
            x: 900.0,
            y: 500.0,
            font_name: None,
            font_size: 72.0,
            line_height: 66.24,
            color: Color::WHITE,
            opacity: 1.0,
            shadow_color: Some(Color::BLACK),
            shadow_strength: 5.0,
            shadow_distance: 8.0,
            border_color: Some(Color::BLACK),
            border_thickness: 4.0,
        };

        let layer = prepare_content_layer(&style, "Best Lap", true, "01:23.45", &[]).unwrap();

        assert!(layer.x > 800.0);
        assert!(layer.y > 400.0);
        assert!(layer.image.width() < 500);
        assert!(layer.image.height() < 250);
    }
}
