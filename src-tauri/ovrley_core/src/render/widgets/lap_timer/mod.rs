//! Native dense-activity boundary for the shared lap timer renderer.
use crate::{
    activity::schema::DenseActivityReport,
    error::{CoreError, CoreResult},
    render::widgets::types::PreparedLapTimer,
};
use ovrley_widgets::widgets::lap_timer::{
    format::{self, LapHistory, LapLogTextState, LapSample},
    validation::LapTimerMode,
};
use skia_safe::Canvas;

pub fn lap_history(activity: &DenseActivityReport) -> LapHistory<'_> {
    LapHistory {
        starts: &activity.series.lap_start_elapsed_seconds,
        durations: &activity.series.lap_durations_seconds,
        best_so_far: &activity.series.lap_durations_best_so_far_seconds,
        end: activity
            .frame_elapsed_seconds
            .last()
            .copied()
            .unwrap_or(0.0),
    }
}

pub fn lap_sample(
    activity: &DenseActivityReport,
    frame_index: usize,
    mode: LapTimerMode,
) -> CoreResult<LapSample> {
    let elapsed = *activity
        .frame_elapsed_seconds
        .get(frame_index)
        .ok_or_else(|| {
            CoreError::Render(format!(
                "frame_elapsed_seconds is missing at frame {frame_index}"
            ))
        })?;
    let mut sample = LapSample::default();
    if mode != LapTimerMode::Delta {
        // Entirely absent lap telemetry is a legitimate external-data case.
        sample.lap_number = activity
            .series
            .lap_number
            .get(frame_index)
            .copied()
            .flatten()
            .unwrap_or(-1);
        sample.lap_time_seconds = activity
            .series
            .lap_time_seconds
            .get(frame_index)
            .copied()
            .flatten();
    }
    if matches!(mode, LapTimerMode::Delta | LapTimerMode::LapLog) {
        sample.delta_seconds = activity
            .series
            .delta_to_best_lap_seconds
            .get(frame_index)
            .copied()
            .flatten();
    }
    Ok(LapSample::from_aligned(
        &activity.series.lap_start_elapsed_seconds,
        sample.lap_number,
        sample.lap_time_seconds,
        elapsed,
        sample.delta_seconds,
    ))
}

pub fn lap_timer_value_text(
    mode: LapTimerMode,
    activity: &DenseActivityReport,
    frame_index: usize,
) -> CoreResult<String> {
    Ok(format::lap_timer_value_text(
        mode,
        lap_history(activity),
        lap_sample(activity, frame_index, mode)?,
    )?)
}

pub fn lap_log_text_state(
    activity: &DenseActivityReport,
    frame_index: usize,
) -> CoreResult<LapLogTextState> {
    Ok(format::lap_log_text_state(
        lap_history(activity),
        lap_sample(activity, frame_index, LapTimerMode::LapLog)?,
    )?)
}

pub fn draw_lap_timer(
    canvas: &Canvas,
    widget: &PreparedLapTimer,
    activity: &DenseActivityReport,
    frame_index: usize,
) -> CoreResult<()> {
    let config = &widget.validated;
    let layout = widget
        .cache
        .as_ref()
        .ok_or_else(|| CoreError::Render("lap timer cache is missing".into()))?;
    let history = lap_history(activity);
    let content = format::lap_timer_content(
        config,
        history,
        lap_sample(activity, frame_index, config.mode)?,
    )?;
    layout.prepare_frame(history, &content)?.draw(canvas);
    Ok(())
}
