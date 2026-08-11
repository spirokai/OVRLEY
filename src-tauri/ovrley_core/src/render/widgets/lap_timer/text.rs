//! Lap timer state and text formatting.
//!
//! This module is the single owner of the conversion from dense activity
//! series to lap-timer display state. It keeps frame indexing and lap metadata
//! validation together so cache preparation, drawing, and frontend-facing text
//! state use the same contract and formatting rules.

use super::PLACEHOLDER;
use crate::activity::schema::DenseActivityReport;
use crate::error::{CoreError, CoreResult};
use crate::normalize::LapTimerMode;

/// One formatted row in the lap-log table.
#[derive(Clone, Debug, PartialEq)]
pub struct LapLogTextRow {
    /// Formatted lap number, duration, and delta cells in display order.
    pub cells: [String; 3],
    /// Numeric delta used to choose the row's positive or negative color.
    pub delta_seconds: Option<f64>,
}

/// Formatted lap-log state for one render frame.
#[derive(Clone, Debug, PartialEq)]
pub struct LapLogTextState {
    /// Completed laps ordered from newest to oldest.
    pub completed_rows: Vec<LapLogTextRow>,
    /// The active lap row, when the activity is currently inside a lap.
    pub current_row: Option<LapLogTextRow>,
}

/// Internal frame state used by both cache preparation and drawing.
pub(super) struct LapLogFrameState {
    /// Number of completed laps visible at the frame's elapsed time.
    pub(super) completed_lap_count: usize,
    /// Formatted row for the currently active lap, if one exists.
    pub(super) current_row: Option<LapLogTextRow>,
}

/// Formats a lap duration as `MM:SS.hh` or `HH:MM:SS.hh`.
///
/// The duration is rounded to hundredths before its components are emitted.
/// Values below one hour omit the hour component.
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

/// Formats a lap delta with an explicit sign and two decimal places.
///
/// Missing and zero deltas are represented as `+0.00`.
pub fn format_lap_delta(delta_seconds: Option<f64>) -> String {
    match delta_seconds {
        Some(delta) => {
            let sign = if delta < 0.0 { '-' } else { '+' };
            let rounded_magnitude = (delta.abs() * 100.0).round() / 100.0;
            format!("{sign}{rounded_magnitude:.2}")
        }
        None => "+0.00".to_string(),
    }
}

/// Resolves canonical lap state for a dense-activity frame.
pub(super) fn lap_state_at_frame(
    dense_activity: &DenseActivityReport,
    frame_index: usize,
) -> CoreResult<crate::activity::lap_timing::LapState> {
    let elapsed = dense_activity
        .frame_elapsed_seconds
        .get(frame_index)
        .copied()
        .ok_or_else(|| {
            CoreError::Render(format!(
                "frame_elapsed_seconds is missing at frame {frame_index}"
            ))
        })?;
    let lap_number = dense_activity
        .series
        .lap_number
        .get(frame_index)
        .copied()
        .flatten()
        .ok_or_else(|| {
            CoreError::Render(format!("lap_number is missing at frame {frame_index}"))
        })?;
    let lap_time_seconds = dense_activity
        .series
        .lap_time_seconds
        .get(frame_index)
        .copied()
        .ok_or_else(|| {
            CoreError::Render(format!(
                "lap_time_seconds is missing at frame {frame_index}"
            ))
        })?;
    Ok(crate::activity::lap_timing::lap_state_from_aligned(
        &dense_activity.series.lap_start_elapsed_seconds,
        lap_number,
        lap_time_seconds,
        elapsed,
    ))
}

/// Reads the current delta to the best lap for a dense-activity frame.
pub(super) fn delta_at(
    dense_activity: &DenseActivityReport,
    frame_index: usize,
) -> CoreResult<Option<f64>> {
    dense_activity
        .series
        .delta_to_best_lap_seconds
        .get(frame_index)
        .copied()
        .ok_or_else(|| {
            CoreError::Render(format!(
                "delta_to_best_lap_seconds is missing at frame {frame_index}"
            ))
        })
}

/// Selects the configured color for a lap delta.
///
/// Positive deltas use `positive_color`; missing, zero, and negative deltas
/// use `negative_color`.
pub(super) fn delta_color(
    positive_color: [u8; 4],
    negative_color: [u8; 4],
    delta_seconds: Option<f64>,
) -> [u8; 4] {
    if delta_seconds.is_some_and(|delta| delta > 0.0) {
        positive_color
    } else {
        negative_color
    }
}

/// Converts an RGBA byte array into a Skia color.
pub(super) fn rgba_color(color: [u8; 4]) -> skia_safe::Color {
    skia_safe::Color::from_argb(color[3], color[0], color[1], color[2])
}

/// Uppercases a configured lap timer label for rendering.
pub(super) fn lap_timer_label_text(label: &str) -> String {
    label.to_uppercase()
}

/// Resolves the best lap text for the current lap number.
///
/// Lap zero has no completed-lap metadata and therefore uses the active lap
/// duration. Later laps use the best completed duration recorded before the
/// active lap.
pub(super) fn best_lap_text(
    dense_activity: &DenseActivityReport,
    lap_number: i64,
    current_lap_time: f64,
) -> CoreResult<String> {
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

/// Resolves the scalar value displayed by a non-table lap timer mode.
///
/// The table mode is rejected because it has a separate row-oriented state
/// contract.
pub fn lap_timer_value_text(
    mode: LapTimerMode,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
) -> CoreResult<String> {
    if mode == LapTimerMode::LapLog {
        return Err(CoreError::Render(
            "lap_log must use the dedicated table renderer".into(),
        ));
    }
    if mode == LapTimerMode::Delta {
        return delta_at(dense_activity, frame_index).map(format_lap_delta);
    }
    let lap_state = lap_state_at_frame(dense_activity, frame_index)?;
    if lap_state.lap_number < 0 {
        if mode == LapTimerMode::CurrentLap {
            return Ok(PLACEHOLDER.to_string());
        }
        return if lap_state.completed_lap_count == 0 {
            Ok(PLACEHOLDER.to_string())
        } else {
            state_value_text(dense_activity, lap_state.completed_lap_count)
        };
    }
    let current_lap_time = lap_state
        .lap_time_seconds
        .expect("validated active lap state has a lap time");
    match mode {
        LapTimerMode::CurrentLap => Ok(format_lap_duration(current_lap_time)),
        LapTimerMode::BestLap => {
            best_lap_text(dense_activity, lap_state.lap_number, current_lap_time)
        }
        LapTimerMode::Delta => unreachable!("delta mode returns before lap-time resolution"),
        LapTimerMode::LapLog => unreachable!("lap_log returns before value resolution"),
    }
}

/// Builds the completed rows visible in a lap-log frame.
///
/// Rows are returned newest-first so the current and most recent laps remain
/// closest to the active row.
pub(super) fn lap_log_completed_rows(
    dense_activity: &DenseActivityReport,
    completed_lap_count: usize,
) -> CoreResult<Vec<LapLogTextRow>> {
    if completed_lap_count > dense_activity.series.lap_durations_seconds.len() {
        return Err(CoreError::Render(format!(
            "lap duration metadata is missing for completed lap {completed_lap_count}"
        )));
    }

    if completed_lap_count == 0 {
        return Ok(Vec::new());
    }

    let best_completed_lap = dense_activity
        .series
        .lap_durations_best_so_far_seconds
        .get(completed_lap_count - 1)
        .copied()
        .ok_or_else(|| {
            CoreError::Render(format!(
                "best lap metadata is missing for completed lap {completed_lap_count}"
            ))
        })?;

    (0..completed_lap_count)
        .rev()
        .map(|lap_index| {
            let duration = dense_activity.series.lap_durations_seconds[lap_index];
            let delta = Some(duration - best_completed_lap);
            Ok(LapLogTextRow {
                cells: [
                    (lap_index + 1).to_string(),
                    format_lap_duration(duration),
                    format_lap_delta(delta),
                ],
                delta_seconds: delta,
            })
        })
        .collect()
}

/// Derives the lap-log state visible at one dense-activity frame.
pub(super) fn lap_log_frame_state(
    dense_activity: &DenseActivityReport,
    frame_index: usize,
) -> CoreResult<LapLogFrameState> {
    let lap_state = lap_state_at_frame(dense_activity, frame_index)?;

    let current_row = if lap_state.lap_number < 0 {
        None
    } else {
        let current_lap_time = lap_state
            .lap_time_seconds
            .expect("validated active lap state has a lap time");
        let delta = delta_at(dense_activity, frame_index)?;
        Some(LapLogTextRow {
            cells: [
                (lap_state.lap_number + 1).to_string(),
                format_lap_duration(current_lap_time),
                format_lap_delta(delta),
            ],
            delta_seconds: delta,
        })
    };

    Ok(LapLogFrameState {
        completed_lap_count: lap_state.completed_lap_count,
        current_row,
    })
}

/// Builds the complete formatted lap-log state for one frame.
pub fn lap_log_text_state(
    dense_activity: &DenseActivityReport,
    frame_index: usize,
) -> CoreResult<LapLogTextState> {
    let frame_state = lap_log_frame_state(dense_activity, frame_index)?;
    Ok(LapLogTextState {
        completed_rows: lap_log_completed_rows(dense_activity, frame_state.completed_lap_count)?,
        current_row: frame_state.current_row,
    })
}

/// Resolves the best completed-lap value for a frame without an active lap.
pub(super) fn state_value_text(
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
