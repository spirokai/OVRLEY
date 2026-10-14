//! Canonical lap state and formatting, independent of either host's activity storage.
use super::{
    validation::{LapTimerMode, ValidatedLapTimer},
    PLACEHOLDER,
};
use crate::error::{WidgetError, WidgetResult};

#[derive(Clone, Copy, Default)]
pub struct LapHistory<'a> {
    pub starts: &'a [f64],
    pub durations: &'a [f64],
    pub best_so_far: &'a [f64],
    pub end: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LapSample {
    pub lap_number: i64,
    pub lap_time_seconds: Option<f64>,
    pub completed_lap_count: usize,
    pub delta_seconds: Option<f64>,
}

impl Default for LapSample {
    fn default() -> Self {
        Self {
            lap_number: -1,
            lap_time_seconds: None,
            completed_lap_count: 0,
            delta_seconds: None,
        }
    }
}

impl LapSample {
    /// Aligned lap number/time stay authoritative; exact boundaries determine completed history.
    pub fn from_aligned(
        starts: &[f64],
        lap_number: i64,
        lap_time_seconds: Option<f64>,
        elapsed: f64,
        delta_seconds: Option<f64>,
    ) -> Self {
        Self {
            lap_number,
            lap_time_seconds,
            delta_seconds,
            completed_lap_count: starts
                .partition_point(|start| *start <= elapsed)
                .saturating_sub(1),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LapLogTextRow {
    pub cells: [String; 3],
    pub delta_seconds: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LapLogTextState {
    pub completed_rows: Vec<LapLogTextRow>,
    pub current_row: Option<LapLogTextRow>,
}

/// Rounded text and paint color are the presentation cache key, rather than unrounded telemetry.
#[derive(Clone, Debug, PartialEq)]
pub enum LapTimerContent {
    Value {
        text: String,
        color: [u8; 4],
    },
    Log {
        completed_lap_count: usize,
        current_row: Option<([String; 3], [u8; 4])>,
    },
}

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

pub fn delta_color(positive: [u8; 4], negative: [u8; 4], delta: Option<f64>) -> [u8; 4] {
    if delta.is_some_and(|delta| delta > 0.0) {
        positive
    } else {
        negative
    }
}

pub fn best_completed_lap(history: LapHistory<'_>, count: usize) -> WidgetResult<Option<f64>> {
    count
        .checked_sub(1)
        .map(|index| {
            history.best_so_far.get(index).copied().ok_or_else(|| {
                WidgetError::Render(format!(
                    "best lap metadata is missing for completed lap {count}"
                ))
            })
        })
        .transpose()
}

pub fn lap_timer_value_text(
    mode: LapTimerMode,
    history: LapHistory<'_>,
    sample: LapSample,
) -> WidgetResult<String> {
    match mode {
        LapTimerMode::Delta => Ok(format_lap_delta(sample.delta_seconds)),
        LapTimerMode::LapLog => Err(WidgetError::Render(
            "lap_log must use the dedicated table renderer".into(),
        )),
        LapTimerMode::BestLap if sample.completed_lap_count > 0 => Ok(format_lap_duration(
            best_completed_lap(history, sample.completed_lap_count)?
                .expect("completed laps have a best time"),
        )),
        LapTimerMode::CurrentLap | LapTimerMode::BestLap if sample.lap_number < 0 => {
            Ok(PLACEHOLDER.into())
        }
        LapTimerMode::CurrentLap | LapTimerMode::BestLap => sample
            .lap_time_seconds
            .map(format_lap_duration)
            .ok_or_else(|| {
                WidgetError::Render("active lap is missing its canonical lap time".into())
            }),
    }
}

pub fn current_log_row(sample: LapSample) -> WidgetResult<Option<LapLogTextRow>> {
    if sample.lap_number < 0 {
        return Ok(None);
    }
    let time = sample.lap_time_seconds.ok_or_else(|| {
        WidgetError::Render("active lap is missing its canonical lap time".into())
    })?;
    Ok(Some(LapLogTextRow {
        cells: [
            (sample.lap_number + 1).to_string(),
            format_lap_duration(time),
            format_lap_delta(sample.delta_seconds),
        ],
        delta_seconds: sample.delta_seconds,
    }))
}

pub fn lap_log_text_state(
    history: LapHistory<'_>,
    sample: LapSample,
) -> WidgetResult<LapLogTextState> {
    let count = sample.completed_lap_count;
    let durations = history.durations.get(..count).ok_or_else(|| {
        WidgetError::Render(format!(
            "lap duration metadata is missing for completed lap {count}"
        ))
    })?;
    let best = best_completed_lap(history, count)?;
    let completed_rows = durations
        .iter()
        .enumerate()
        .rev()
        .map(|(index, duration)| {
            let delta = best.map(|best| duration - best);
            LapLogTextRow {
                cells: [
                    (index + 1).to_string(),
                    format_lap_duration(*duration),
                    format_lap_delta(delta),
                ],
                delta_seconds: delta,
            }
        })
        .collect();
    Ok(LapLogTextState {
        completed_rows,
        current_row: current_log_row(sample)?,
    })
}

pub fn lap_timer_content(
    config: &ValidatedLapTimer,
    history: LapHistory<'_>,
    sample: LapSample,
) -> WidgetResult<LapTimerContent> {
    let color = |delta| {
        delta_color(
            config.positive_delta_color,
            config.negative_delta_color,
            delta,
        )
    };
    if config.mode == LapTimerMode::LapLog {
        Ok(LapTimerContent::Log {
            completed_lap_count: sample.completed_lap_count,
            current_row: current_log_row(sample)?.map(|row| (row.cells, color(row.delta_seconds))),
        })
    } else {
        Ok(LapTimerContent::Value {
            text: lap_timer_value_text(config.mode, history, sample)?,
            color: if config.mode == LapTimerMode::Delta {
                color(sample.delta_seconds)
            } else {
                config.color
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lap_text_preserves_rounding_signs_and_missing_reference() {
        assert_eq!(format_lap_duration(3.456), "00:03.46");
        assert_eq!(format_lap_duration(3599.999), "01:00:00.00");
        assert_eq!(format_lap_duration(3661.2), "01:01:01.20");
        assert_eq!(format_lap_delta(None), "+0.00");
        assert_eq!(format_lap_delta(Some(0.0)), "+0.00");
        assert_eq!(format_lap_delta(Some(0.125)), "+0.13");
        assert_eq!(format_lap_delta(Some(-0.125)), "-0.13");
        assert_eq!(delta_color([1; 4], [2; 4], None), [2; 4]);
        assert_eq!(delta_color([1; 4], [2; 4], Some(0.0)), [2; 4]);
    }
}
