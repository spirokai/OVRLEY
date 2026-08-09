//! Lap timer text measurement and static-layer layout.
//!
//! The layout module turns resolved text styles and formatted lap rows into
//! positioned Skia text. It owns the bounds calculations used to size cached
//! surfaces, while `draw` and `prepare` decide when those primitives are used.

use super::text::{delta_color, rgba_color, LapLogTextRow};
use super::{
    LABEL_FONT_RATIO, LINE_HEIGHT_RATIO, LOG_COLUMN_GAP_RATIO, LOG_HEADER_OPACITY,
    LOG_ROW_GAP_RATIO, TABLE_VERTICAL_METRICS_TEXT,
};
use crate::activity::schema::DenseActivityReport;
use crate::error::{CoreError, CoreResult};
use crate::render::surface::create_surface;
use crate::render::text::{
    baseline_for_text_top_with_line_height, draw_text_with_vertical_metrics_text,
    measure_text_with_font, resolve_font, ResolvedTextStyle,
};
use crate::render::widgets::types::StaticLayer;
use skia_safe::{Canvas, Color};
use std::path::PathBuf;

/// Draws a lap timer label and value using the resolved widget style.
///
/// The value color can override the resolved style for delta mode. The label
/// is expected to already be uppercased by the caller.
pub(super) fn draw_content(
    canvas: &Canvas,
    style: &ResolvedTextStyle,
    label: &str,
    show_label: bool,
    value: &str,
    value_color: Option<Color>,
    font_dirs: &[PathBuf],
) -> CoreResult<()> {
    let (label_style, mut value_style) = content_styles(style, show_label);
    if let Some(label_style) = label_style {
        draw_text_with_vertical_metrics_text(canvas, label, label, &label_style, font_dirs)?;
    }

    if let Some(value_color) = value_color {
        value_style.color = value_color;
    }
    draw_text_with_vertical_metrics_text(
        canvas,
        value,
        TABLE_VERTICAL_METRICS_TEXT,
        &value_style,
        font_dirs,
    )
}

/// Derives the label and value styles for a lap timer's stacked text.
pub(super) fn content_styles(
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

/// Expands text bounds to include one rendered text item.
///
/// Empty text has no bounds and is ignored. The bounds include the font's
/// measured glyph extents after applying the style's baseline and position.
pub(super) fn include_text_bounds(
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

/// Computes padding needed around rendered text effects.
pub(super) fn text_effect_padding(style: &ResolvedTextStyle) -> f32 {
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

/// Scales a Skia color's alpha channel while preserving its RGB channels.
fn scaled_color_alpha(color: Color, multiplier: f32) -> Color {
    Color::from_argb(
        ((color.a() as f32) * multiplier).round() as u8,
        color.r(),
        color.g(),
        color.b(),
    )
}

/// Scales all opacity-bearing fields in a text style.
fn scaled_opacity_style(style: &ResolvedTextStyle, multiplier: f32) -> ResolvedTextStyle {
    let mut scaled = style.clone();
    scaled.color = scaled_color_alpha(style.color, multiplier);
    scaled.shadow_color = style
        .shadow_color
        .map(|color| scaled_color_alpha(color, multiplier));
    scaled.border_color = style
        .border_color
        .map(|color| scaled_color_alpha(color, multiplier));
    scaled.opacity *= multiplier;
    scaled
}

/// Builds the reduced-opacity style used by lap-log table headers.
pub(super) fn lap_log_header_style(style: &ResolvedTextStyle) -> ResolvedTextStyle {
    let mut header_style = scaled_opacity_style(style, LOG_HEADER_OPACITY);
    header_style.font_size = style.font_size * LABEL_FONT_RATIO;
    header_style.line_height = header_style.font_size * LINE_HEIGHT_RATIO;
    header_style
}

/// Measures the right edge of each lap-log table column.
///
/// Numeric slots use the widest digit in the widget font, so proportional fonts
/// remain stable without measuring every frame-aligned value.
pub(super) fn log_column_rights(
    style: &ResolvedTextStyle,
    dense_activity: &DenseActivityReport,
    font_dirs: &[PathBuf],
) -> CoreResult<[f32; 3]> {
    let row_font = resolve_font(font_dirs, style.font_name.as_deref(), style.font_size)?;
    let header_font = resolve_font(
        font_dirs,
        style.font_name.as_deref(),
        style.font_size * LABEL_FONT_RATIO,
    )?;
    let header_width = |text: &str| measure_text_with_font(text, &header_font).width;
    let character_width = |character: char| {
        let mut buffer = [0; 4];
        measure_text_with_font(character.encode_utf8(&mut buffer), &row_font).width
    };
    let widest_digit = ('0'..='9').map(&character_width).fold(0.0, f32::max);
    let widest_sign = character_width('+').max(character_width('-'));
    let colon_width = character_width(':');
    let decimal_point_width = character_width('.');

    let max_completed_lap_time = dense_activity
        .series
        .lap_durations_seconds
        .iter()
        .copied()
        .fold(0.0, f64::max);
    let max_current_lap_time = dense_activity
        .frame_elapsed_seconds
        .last()
        .and_then(|activity_end| {
            let started_lap_count = dense_activity
                .series
                .lap_start_elapsed_seconds
                .partition_point(|lap_start| lap_start <= activity_end);
            started_lap_count.checked_sub(1).map(|lap_index| {
                activity_end - dense_activity.series.lap_start_elapsed_seconds[lap_index]
            })
        })
        .unwrap_or(0.0);
    let max_lap_hundredths =
        (max_completed_lap_time.max(max_current_lap_time) * 100.0).round() as u64;
    let uses_hours = max_lap_hundredths >= 360_000;
    let hour_digits = if uses_hours {
        decimal_digit_count(max_lap_hundredths / 360_000).max(2)
    } else {
        0
    };
    let lap_digits =
        decimal_digit_count(dense_activity.series.lap_durations_seconds.len() as u64 + 1);
    let time_digits = if uses_hours { hour_digits + 6 } else { 6 };
    let time_colons = if uses_hours { 2 } else { 1 };
    let delta_integer_digits = decimal_digit_count(max_lap_hundredths / 100);

    let lap_width = header_width("LAP").max(lap_digits as f32 * widest_digit);
    let time_width = header_width("TIME").max(
        time_digits as f32 * widest_digit + time_colons as f32 * colon_width + decimal_point_width,
    );
    let delta_width = header_width("DELTA")
        .max(widest_sign + (delta_integer_digits + 2) as f32 * widest_digit + decimal_point_width);
    let gap = style.font_size * LOG_COLUMN_GAP_RATIO;
    let lap_right = style.x + lap_width;
    let time_right = lap_right + gap + time_width;
    let delta_right = time_right + gap + delta_width;
    Ok([lap_right, time_right, delta_right])
}

fn decimal_digit_count(value: u64) -> usize {
    value.max(1).ilog10() as usize + 1
}

/// Positions one right-aligned lap-log cell at the supplied top coordinate.
fn positioned_log_cell_style(
    style: &ResolvedTextStyle,
    text: &str,
    column_right: f32,
    top: f32,
    font_dirs: &[PathBuf],
) -> CoreResult<ResolvedTextStyle> {
    let font = resolve_font(font_dirs, style.font_name.as_deref(), style.font_size)?;
    let mut positioned = style.clone();
    positioned.x = column_right - measure_text_with_font(text, &font).width;
    positioned.y = top;
    positioned.line_height = style.font_size * LINE_HEIGHT_RATIO;
    Ok(positioned)
}

/// Draws one three-cell lap-log row.
///
/// Only the delta cell receives `delta_cell_color`; the lap number and time
/// retain the resolved row style.
pub(super) fn draw_log_row(
    canvas: &Canvas,
    style: &ResolvedTextStyle,
    row: &[String; 3],
    delta_cell_color: Option<Color>,
    column_rights: [f32; 3],
    top: f32,
    font_dirs: &[PathBuf],
) -> CoreResult<()> {
    for (column_index, (text, column_right)) in row.iter().zip(column_rights).enumerate() {
        let mut cell_style = positioned_log_cell_style(style, text, column_right, top, font_dirs)?;
        if column_index == 2 {
            if let Some(color) = delta_cell_color {
                cell_style.color = color;
            }
        }
        draw_text_with_vertical_metrics_text(
            canvas,
            text,
            TABLE_VERTICAL_METRICS_TEXT,
            &cell_style,
            font_dirs,
        )?;
    }
    Ok(())
}

/// Expands bounds to include each cell in one lap-log row.
pub(super) fn include_log_row_bounds(
    bounds: &mut Option<(f32, f32, f32, f32)>,
    style: &ResolvedTextStyle,
    row: &[String; 3],
    column_rights: [f32; 3],
    top: f32,
    font_dirs: &[PathBuf],
) -> CoreResult<()> {
    for (text, column_right) in row.iter().zip(column_rights) {
        let cell_style = positioned_log_cell_style(style, text, column_right, top, font_dirs)?;
        include_text_bounds(
            bounds,
            text,
            TABLE_VERTICAL_METRICS_TEXT,
            &cell_style,
            font_dirs,
        )?;
    }
    Ok(())
}

/// Renders the static lap-log header into a reusable image layer.
pub(super) fn prepare_lap_log_header_layer(
    style: &ResolvedTextStyle,
    column_rights: [f32; 3],
    font_dirs: &[PathBuf],
) -> CoreResult<StaticLayer> {
    let header = ["LAP".to_string(), "TIME".to_string(), "DELTA".to_string()];
    let header_style = lap_log_header_style(style);
    let mut bounds = None;
    include_log_row_bounds(
        &mut bounds,
        &header_style,
        &header,
        column_rights,
        style.y,
        font_dirs,
    )?;

    let (min_x, min_y, max_x, max_y) =
        bounds.ok_or_else(|| CoreError::Render("lap log header has no drawable text".into()))?;
    let padding = text_effect_padding(style);
    let layer_x = (min_x - padding).floor();
    let layer_y = (min_y - padding).floor();
    let layer_width = ((max_x + padding).ceil() - layer_x).max(1.0) as u32;
    let layer_height = ((max_y + padding).ceil() - layer_y).max(1.0) as u32;
    let mut surface = create_surface(layer_width, layer_height)?;
    surface.canvas().clear(Color::TRANSPARENT);

    let mut local_style = style.clone();
    local_style.x -= layer_x;
    local_style.y -= layer_y;
    let local_rights = column_rights.map(|right| right - layer_x);
    let local_header_style = lap_log_header_style(&local_style);
    draw_log_row(
        surface.canvas(),
        &local_header_style,
        &header,
        None,
        local_rights,
        local_style.y,
        font_dirs,
    )?;

    Ok(StaticLayer {
        image: surface.image_snapshot(),
        x: layer_x,
        y: layer_y,
    })
}

/// Renders completed lap-log rows into a reusable image layer.
pub(super) fn prepare_lap_log_rows_layer(
    style: &ResolvedTextStyle,
    completed_rows: &[LapLogTextRow],
    positive_delta_color: [u8; 4],
    negative_delta_color: [u8; 4],
    column_rights: [f32; 3],
    font_dirs: &[PathBuf],
) -> CoreResult<StaticLayer> {
    let header_style = lap_log_header_style(style);
    let row_gap = style.font_size * LOG_ROW_GAP_RATIO;
    let data_top = style.y + header_style.line_height + row_gap;
    let row_stride = style.font_size * LINE_HEIGHT_RATIO + row_gap;
    let mut bounds = None;

    for (row_index, row) in completed_rows.iter().enumerate() {
        include_log_row_bounds(
            &mut bounds,
            style,
            &row.cells,
            column_rights,
            data_top + row_index as f32 * row_stride,
            font_dirs,
        )?;
    }

    let (min_x, min_y, max_x, max_y) = bounds
        .ok_or_else(|| CoreError::Render("lap log completed rows have no drawable text".into()))?;
    let padding = text_effect_padding(style);
    let layer_x = (min_x - padding).floor();
    let layer_y = (min_y - padding).floor();
    let layer_width = ((max_x + padding).ceil() - layer_x).max(1.0) as u32;
    let layer_height = ((max_y + padding).ceil() - layer_y).max(1.0) as u32;
    let mut surface = create_surface(layer_width, layer_height)?;
    surface.canvas().clear(Color::TRANSPARENT);

    let mut local_style = style.clone();
    local_style.x -= layer_x;
    local_style.y -= layer_y;
    let local_rights = column_rights.map(|right| right - layer_x);
    let local_data_top = local_style.y + header_style.line_height + row_gap;
    for (row_index, row) in completed_rows.iter().enumerate() {
        draw_log_row(
            surface.canvas(),
            &local_style,
            &row.cells,
            Some(rgba_color(delta_color(
                positive_delta_color,
                negative_delta_color,
                row.delta_seconds,
            ))),
            local_rights,
            local_data_top + row_index as f32 * row_stride,
            font_dirs,
        )?;
    }

    Ok(StaticLayer {
        image: surface.image_snapshot(),
        x: layer_x,
        y: layer_y,
    })
}

/// Renders a static lap timer label and value into a reusable image layer.
pub(super) fn prepare_content_layer(
    style: &ResolvedTextStyle,
    label: &str,
    show_label: bool,
    value: &str,
    font_dirs: &[PathBuf],
) -> CoreResult<StaticLayer> {
    let label = super::text::lap_timer_label_text(label);
    let (label_style, value_style) = content_styles(style, show_label);
    let mut bounds = None;
    if let Some(label_style) = label_style {
        include_text_bounds(&mut bounds, &label, &label, &label_style, font_dirs)?;
    }

    include_text_bounds(
        &mut bounds,
        value,
        TABLE_VERTICAL_METRICS_TEXT,
        &value_style,
        font_dirs,
    )?;

    let (min_x, min_y, max_x, max_y) = bounds
        .ok_or_else(|| CoreError::Render("lap timer cache content has no drawable text".into()))?;
    let padding = text_effect_padding(style);
    let layer_x = (min_x - padding).floor();
    let layer_y = (min_y - padding).floor();
    let layer_width = ((max_x + padding).ceil() - layer_x).max(1.0) as u32;
    let layer_height = ((max_y + padding).ceil() - layer_y).max(1.0) as u32;
    let mut surface = create_surface(layer_width, layer_height)?;
    surface.canvas().clear(Color::TRANSPARENT);

    let mut local_style = style.clone();
    local_style.x -= layer_x;
    local_style.y -= layer_y;
    draw_content(
        surface.canvas(),
        &local_style,
        &label,
        show_label,
        value,
        None,
        font_dirs,
    )?;

    Ok(StaticLayer {
        image: surface.image_snapshot(),
        x: layer_x,
        y: layer_y,
    })
}

/// Draws a cached static layer at its prepared position.
pub(super) fn draw_cached_layer(canvas: &Canvas, layer: &StaticLayer) {
    canvas.draw_image(&layer.image, (layer.x, layer.y), None);
}

/// Draws a cached static layer with an additional vertical offset.
pub(super) fn draw_cached_layer_with_y_offset(canvas: &Canvas, layer: &StaticLayer, y_offset: f32) {
    canvas.draw_image(&layer.image, (layer.x, layer.y + y_offset), None);
}
