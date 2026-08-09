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
use super::text::{
    completed_lap_count_at, lap_log_completed_rows, lap_number_at, state_value_text,
};
use crate::activity::schema::DenseActivityReport;
use crate::debug::RenderProfiler;
use crate::error::CoreResult;
use crate::normalize::{LapTimerMode, ValidatedLapTimer, ValidatedSceneConfig};
use crate::render::text::{validated_lap_timer_style, ResolvedTextStyle};
use crate::render::widgets::types::{LapTimerWidgetCache, StaticLayer};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// Prepares the static layers required by a validated lap timer.
///
/// Best-lap mode caches one value layer for each completed-lap state. Lap-log
/// mode caches the stable header and one completed-row layer for each visible
/// completed-lap count. Current-lap and delta modes draw their changing text
/// directly for each frame.
pub fn prepare_lap_timer_cache(
    validated: &ValidatedLapTimer,
    dense_activity: &DenseActivityReport,
    scene: &ValidatedSceneConfig,
    scale: f32,
    font_dirs: &[PathBuf],
    prepare_profiler: &mut RenderProfiler,
) -> CoreResult<LapTimerWidgetCache> {
    let style = validated_lap_timer_style(validated, scene, scale);
    let mut state_layers = BTreeMap::new();
    let mut log_header_layer = None;
    let mut cached_log_column_rights = None;

    if validated.mode == LapTimerMode::BestLap {
        prepare_best_lap_layers(
            validated,
            dense_activity,
            &style,
            font_dirs,
            prepare_profiler,
            &mut state_layers,
        )?;
    } else if validated.mode == LapTimerMode::LapLog {
        let column_rights = log_column_rights(&style, dense_activity, font_dirs)?;
        log_header_layer = Some(prepare_profiler.measure("lap_timer.cache_surface", || {
            prepare_lap_log_header_layer(&style, column_rights, font_dirs)
        })?);
        let completed_lap_counts = visible_completed_lap_counts(dense_activity)?;
        prepare_lap_log_layers(
            validated,
            dense_activity,
            &style,
            column_rights,
            font_dirs,
            prepare_profiler,
            completed_lap_counts,
            &mut state_layers,
        )?;
        cached_log_column_rights = Some(column_rights);
    }

    Ok(LapTimerWidgetCache {
        state_layers,
        log_header_layer,
        log_column_rights: cached_log_column_rights,
    })
}

/// Prepares one static text layer for each best-lap state encountered in the
/// dense activity report.
fn prepare_best_lap_layers(
    validated: &ValidatedLapTimer,
    dense_activity: &DenseActivityReport,
    style: &ResolvedTextStyle,
    font_dirs: &[PathBuf],
    prepare_profiler: &mut RenderProfiler,
    state_layers: &mut BTreeMap<usize, StaticLayer>,
) -> CoreResult<()> {
    for lap_number in dense_activity.series.lap_number.iter().flatten() {
        if *lap_number <= 0 || state_layers.contains_key(&(*lap_number as usize)) {
            continue;
        }
        let completed_lap_count = *lap_number as usize;
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
    Ok(())
}

/// Collects the completed-lap counts that occur at render frames.
fn visible_completed_lap_counts(
    dense_activity: &DenseActivityReport,
) -> CoreResult<BTreeSet<usize>> {
    let mut completed_lap_counts = BTreeSet::new();
    for frame_index in 0..dense_activity.series.lap_number.len() {
        lap_number_at(dense_activity, frame_index)?;
        completed_lap_counts.insert(completed_lap_count_at(dense_activity, frame_index)?);
    }
    Ok(completed_lap_counts)
}

/// Prepares one static text layer for each visible completed-lap count.
///
/// A zero count has no completed rows and therefore has no layer.
fn prepare_lap_log_layers(
    validated: &ValidatedLapTimer,
    dense_activity: &DenseActivityReport,
    style: &ResolvedTextStyle,
    column_rights: [f32; 3],
    font_dirs: &[PathBuf],
    prepare_profiler: &mut RenderProfiler,
    completed_lap_counts: BTreeSet<usize>,
    state_layers: &mut BTreeMap<usize, StaticLayer>,
) -> CoreResult<()> {
    for completed_lap_count in completed_lap_counts {
        if completed_lap_count == 0 {
            continue;
        }
        let completed_rows = lap_log_completed_rows(dense_activity, completed_lap_count)?;
        let layer = prepare_profiler.measure("lap_timer.cache_surface", || {
            prepare_lap_log_rows_layer(
                style,
                &completed_rows,
                validated.positive_delta_color,
                validated.negative_delta_color,
                column_rights,
                font_dirs,
            )
        })?;
        state_layers.insert(completed_lap_count, layer);
    }
    Ok(())
}
