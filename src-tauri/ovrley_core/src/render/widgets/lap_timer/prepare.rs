//! Lap timer cache preparation.
//!
//! Preparation resolves the widget style once, measures the complete set of
//! quasi-static states present in the activity, and stores those states as
//! reusable image layers. Dynamic current-lap and delta values remain outside
//! the cache so they are rendered from the frame state during composition.

use super::layout::{
    log_column_rights, prepare_content_layer, prepare_lap_log_header_layer,
    prepare_lap_log_rows_layer,
};
use super::text::{lap_log_completed_rows, state_value_text};
use crate::activity::schema::DenseActivityReport;
use crate::debug::RenderProfiler;
use crate::error::CoreResult;
use crate::normalize::{LapTimerMode, ValidatedLapTimer, ValidatedSceneConfig};
use crate::render::text::{validated_lap_timer_style, ResolvedTextStyle};
use crate::render::widgets::types::{LapTimerWidgetCache, StaticLayer};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Prepares the static layers required by a validated lap timer.
///
/// Best-lap mode caches one value layer for each completed-lap state. Lap-log
/// mode caches the stable header and each completed row exactly once. Current-
/// lap and delta modes draw their changing text directly for each frame.
pub(crate) fn prepare_lap_timer_cache(
    validated: &ValidatedLapTimer,
    dense_activity: &DenseActivityReport,
    scene: &ValidatedSceneConfig,
    scale: f32,
    font_dirs: &[PathBuf],
    prepare_profiler: &mut RenderProfiler,
) -> CoreResult<LapTimerWidgetCache> {
    let style = validated_lap_timer_style(validated, scene, scale);
    match validated.mode {
        LapTimerMode::CurrentLap | LapTimerMode::Delta => Ok(LapTimerWidgetCache::Dynamic),
        LapTimerMode::BestLap => Ok(LapTimerWidgetCache::BestLap {
            state_layers: prepare_best_lap_layers(
                validated,
                dense_activity,
                &style,
                font_dirs,
                prepare_profiler,
            )?,
        }),
        LapTimerMode::LapLog => {
            let column_rights = log_column_rights(&style, dense_activity, font_dirs)?;
            let header_layer = prepare_profiler.measure("lap_timer.cache_surface", || {
                prepare_lap_log_header_layer(&style, column_rights, font_dirs)
            })?;
            let completed_row_layers = prepare_lap_log_row_layers(
                validated,
                dense_activity,
                &style,
                column_rights,
                font_dirs,
                prepare_profiler,
            )?;
            Ok(LapTimerWidgetCache::LapLog {
                header_layer,
                column_rights,
                completed_row_layers,
            })
        }
    }
}

/// Prepares one static text layer for each best-lap state encountered in the
/// dense activity report.
fn prepare_best_lap_layers(
    validated: &ValidatedLapTimer,
    dense_activity: &DenseActivityReport,
    style: &ResolvedTextStyle,
    font_dirs: &[PathBuf],
    prepare_profiler: &mut RenderProfiler,
) -> CoreResult<BTreeMap<usize, StaticLayer>> {
    let mut state_layers = BTreeMap::new();
    for completed_lap_count in 1..=dense_activity
        .series
        .lap_durations_best_so_far_seconds
        .len()
    {
        let value = state_value_text(dense_activity, completed_lap_count)?;
        let layer = prepare_profiler.measure("lap_timer.cache_surface", || {
            prepare_content_layer(
                style,
                &validated.label,
                validated.show_label,
                &value,
                font_dirs,
            )
        })?;
        state_layers.insert(completed_lap_count, layer);
    }
    Ok(state_layers)
}

/// Prepares one reusable static image for each completed lap row.
fn prepare_lap_log_row_layers(
    validated: &ValidatedLapTimer,
    dense_activity: &DenseActivityReport,
    style: &ResolvedTextStyle,
    column_rights: [f32; 3],
    font_dirs: &[PathBuf],
    prepare_profiler: &mut RenderProfiler,
) -> CoreResult<Vec<StaticLayer>> {
    let completed_lap_count = dense_activity.series.lap_durations_seconds.len();
    let completed_rows = lap_log_completed_rows(dense_activity, completed_lap_count)?;
    let mut row_layers = Vec::with_capacity(completed_rows.len());
    for row in completed_rows.iter().rev() {
        let layer = prepare_profiler.measure("lap_timer.cache_surface", || {
            prepare_lap_log_rows_layer(
                style,
                std::slice::from_ref(row),
                validated.positive_delta_color,
                validated.negative_delta_color,
                column_rights,
                font_dirs,
            )
        })?;
        row_layers.push(layer);
    }
    Ok(row_layers)
}
