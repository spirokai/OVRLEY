//! Per-frame lap timer composition.
//!
//! Drawing owns the frame-time branch between cached best-lap/table content and
//! dynamic scalar or current-row content. Cache invariants are checked at this
//! boundary so an incomplete preparation result fails as a render error.

use super::layout::{
    draw_cached_layer, draw_cached_layer_with_y_offset, draw_content, draw_log_row,
    lap_log_header_style,
};
use super::text::{
    delta_at, delta_color, lap_log_frame_state, lap_number_at, lap_timer_label_text,
    lap_timer_value_text, rgba_color,
};
use super::{LINE_HEIGHT_RATIO, LOG_ROW_GAP_RATIO};
use crate::activity::schema::DenseActivityReport;
use crate::error::{CoreError, CoreResult};
use crate::normalize::{LapTimerMode, ValidatedLapTimer};
use crate::render::text::ResolvedTextStyle;
use crate::render::widgets::types::LapTimerWidgetCache;
use skia_safe::Canvas;
use std::path::PathBuf;

/// Draws one lap timer frame from the prepared cache and dense activity state.
///
/// Lap-log and completed best-lap states use cached layers. Current-lap, delta,
/// and the initial best-lap state draw their changing value directly.
pub fn draw_lap_timer(
    canvas: &Canvas,
    validated: &ValidatedLapTimer,
    cache: &LapTimerWidgetCache,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
    style: &ResolvedTextStyle,
    font_dirs: &[PathBuf],
) -> CoreResult<()> {
    if validated.mode == LapTimerMode::LapLog {
        return draw_lap_log_frame(
            canvas,
            validated,
            cache,
            dense_activity,
            frame_index,
            style,
            font_dirs,
        );
    }

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
    let label = lap_timer_label_text(&validated.label);
    let value_color = if validated.mode == LapTimerMode::Delta {
        let color = delta_color(
            validated.positive_delta_color,
            validated.negative_delta_color,
            delta_at(dense_activity, frame_index)?,
        );
        Some(rgba_color(color))
    } else {
        None
    };
    draw_content(
        canvas,
        style,
        &label,
        validated.show_label,
        &value,
        value_color,
        font_dirs,
    )
}

/// Draws one lap-log frame from its cached header and completed rows.
///
/// The active row remains dynamic because its duration and delta change on
/// every frame. When present, it is drawn above the cached completed rows.
fn draw_lap_log_frame(
    canvas: &Canvas,
    validated: &ValidatedLapTimer,
    cache: &LapTimerWidgetCache,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
    style: &ResolvedTextStyle,
    font_dirs: &[PathBuf],
) -> CoreResult<()> {
    let state = lap_log_frame_state(dense_activity, frame_index)?;
    let completed_lap_count = state.completed_lap_count;
    let header_layer = cache
        .log_header_layer
        .as_ref()
        .ok_or_else(|| CoreError::Render("lap log cache is missing the header layer".into()))?;
    draw_cached_layer(canvas, header_layer);
    if completed_lap_count > 0 {
        let completed_layer = cache
            .state_layers
            .get(&completed_lap_count)
            .ok_or_else(|| {
                CoreError::Render(format!(
                    "lap log cache is missing completed-lap state {completed_lap_count}"
                ))
            })?;
        let completed_rows_offset = if state.current_row.is_some() {
            style.font_size * (LINE_HEIGHT_RATIO + LOG_ROW_GAP_RATIO)
        } else {
            0.0
        };
        draw_cached_layer_with_y_offset(canvas, completed_layer, completed_rows_offset);
    }
    if let Some(current_row) = state.current_row {
        draw_current_lap_log_row(
            canvas,
            validated,
            cache,
            style,
            &current_row.cells,
            current_row.delta_seconds,
            font_dirs,
        )?;
    }
    Ok(())
}

/// Draws the dynamic current row in a lap-log frame.
fn draw_current_lap_log_row(
    canvas: &Canvas,
    validated: &ValidatedLapTimer,
    cache: &LapTimerWidgetCache,
    style: &ResolvedTextStyle,
    cells: &[String; 3],
    delta_seconds: Option<f64>,
    font_dirs: &[PathBuf],
) -> CoreResult<()> {
    let column_rights = cache
        .log_column_rights
        .ok_or_else(|| CoreError::Render("lap log cache is missing column layout".into()))?;
    let row_gap = style.font_size * LOG_ROW_GAP_RATIO;
    draw_log_row(
        canvas,
        style,
        cells,
        Some(rgba_color(delta_color(
            validated.positive_delta_color,
            validated.negative_delta_color,
            delta_seconds,
        ))),
        column_rights,
        style.y + lap_log_header_style(style).line_height + row_gap,
        font_dirs,
    )
}
