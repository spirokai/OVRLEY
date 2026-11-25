//! Lap timer: current lap, best lap, delta to best and the lap log.
use crate::activity::ActivitySampler;
use crate::model::catalog::{DisplayType, MetricKind};
use crate::model::scene::{
    validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
};
use crate::model::validation::require_positive_f32;
use crate::model::validation::require_unit_opacity;
use crate::model::validation::{require_bool, require_f32, require_string, rgba_from_hex};
use crate::preview::ResizeGesture;
use crate::preview::WidgetHost;
use crate::render::text::FontResolver;
use crate::render::tile::PreviewRasterScale;
use crate::render::tile::{Presented, Tile};
use crate::render::tile::{TileBounds, TEXT_COLOR_TYPE};
use crate::series::{interpolate_numeric_series_value, interpolate_points, MissingSamplePolicy};
use crate::widgets::lap_timer::layout::{prepare_lap_timer_layout, LapTimerLayout};
use crate::widgets::value::ValueConfig;
use crate::{WidgetError, WidgetResult};
use serde::{Deserialize, Serialize};
use skia_safe::Canvas;

pub mod layout;

const LOG_COLUMN_GAP_RATIO: f32 = 1.8;
const LOG_ROW_GAP_RATIO: f32 = 0.38;
const LOG_HEADER_OPACITY: f32 = 0.7;
const TABLE_VERTICAL_METRICS_TEXT: &str = "0123456789+-:.";
const PLACEHOLDER: &str = "--:--.--";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LapTimerMode {
    CurrentLap,
    BestLap,
    Delta,
    LapLog,
}

impl LapTimerMode {
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "current_lap" => Some(Self::CurrentLap),
            "best_lap" => Some(Self::BestLap),
            "delta" => Some(Self::Delta),
            "lap_log" => Some(Self::LapLog),
            _ => None,
        }
    }

    /// The mode's manifest key.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CurrentLap => "current_lap",
            Self::BestLap => "best_lap",
            Self::Delta => "delta",
            Self::LapLog => "lap_log",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedLapTimer {
    pub x: f32,
    pub y: f32,
    pub font_name: String,
    pub font_size: f32,
    pub color: [u8; 4],
    pub positive_delta_color: [u8; 4],
    pub negative_delta_color: [u8; 4],
    /// Lap log numbers count only enabled laps instead of showing canonical lap IDs.
    pub renumber_laps: bool,
    pub opacity: f32,
    pub show_label: bool,
    pub label: String,
    pub label_font_name: String,
    pub label_font_size: f32,
    pub label_color: [u8; 4],
    pub mode: LapTimerMode,
}

pub fn validate_lap_timer(value: ValueConfig, index: usize) -> WidgetResult<ValidatedLapTimer> {
    let field = |name: &str| format!("values[{index}].{name}");
    if value.value != MetricKind::LapTimer {
        return Err(WidgetError::Config(format!(
            "{}: expected lap_timer metric",
            field("value")
        )));
    }
    if value.display_type != DisplayType::LapTimer {
        return Err(WidgetError::Config(format!(
            "{}: lap_timer widgets require display_type 'lap_timer'",
            field("display_type")
        )));
    }

    let mode = require_string(value.lap_timer_mode, &field("lap_timer_mode"))?;
    let mode = LapTimerMode::parse(&mode).ok_or_else(|| {
        WidgetError::Config(format!(
            "{}: unsupported mode '{mode}'",
            field("lap_timer_mode")
        ))
    })?;
    let font_name = require_string(value.font, &field("font"))?;
    let font_size = require_positive_f32(value.font_size, &field("font_size"))?;
    let label_font_size = require_positive_f32(value.label_font_size, &field("label_font_size"))?;
    let opacity = require_unit_opacity(value.opacity, &field("opacity"))?;
    let color = rgba_from_hex(
        require_string(value.color, &field("color"))?.as_str(),
        &field("color"),
        opacity,
    )?;
    let label_color = rgba_from_hex(
        require_string(value.label_color, &field("label_color"))?.as_str(),
        &field("label_color"),
        opacity,
    )?;
    let positive_delta_color = rgba_from_hex(
        require_string(value.positive_delta_color, &field("positive_delta_color"))?.as_str(),
        &field("positive_delta_color"),
        opacity,
    )?;
    let negative_delta_color = rgba_from_hex(
        require_string(value.negative_delta_color, &field("negative_delta_color"))?.as_str(),
        &field("negative_delta_color"),
        opacity,
    )?;

    Ok(ValidatedLapTimer {
        x: require_f32(Some(value.x), &field("x"))?,
        y: require_f32(Some(value.y), &field("y"))?,
        font_name,
        font_size,
        color,
        positive_delta_color,
        negative_delta_color,
        renumber_laps: require_bool(value.renumber_laps, &field("renumber_laps"))?,
        opacity,
        show_label: require_bool(value.show_label, &field("show_label"))?,
        label: require_string(value.label, &field("label"))?,
        label_font_name: require_string(value.label_font, &field("label_font"))?,
        label_font_size,
        label_color,
        mode,
    })
}

/// The canonical held lap selects its exact opening boundary; inactive laps have no timer.
pub fn lap_time_at(starts: &[f64], lap_number: i64, elapsed: f64) -> Option<f64> {
    usize::try_from(lap_number)
        .ok()
        .and_then(|index| starts.get(index))
        .map(|start| elapsed - start)
}

#[derive(Default, Deserialize)]
pub(crate) struct LapTiming {
    #[serde(default)]
    lap_number: Vec<i64>,
    #[serde(default)]
    lap_time_seconds: Vec<Option<f64>>,
    #[serde(default)]
    lap_start_elapsed_seconds: Vec<f64>,
    #[serde(default)]
    delta_to_best_lap_seconds: Vec<Option<f64>>,
    #[serde(default)]
    lap_durations_seconds: Vec<f64>,
    /// User-disabled canonical lap IDs, strictly increasing; set by [`LapTiming::select`].
    #[serde(skip)]
    disabled_laps: Vec<usize>,
    /// Delta against the best enabled completed lap; the finalized series when none is disabled.
    #[serde(skip)]
    selected_delta_seconds: Vec<Option<f64>>,
}

impl LapTiming {
    pub fn validate(&self, times: &[f64]) -> WidgetResult<()> {
        if [
            self.lap_number.len(),
            self.lap_time_seconds.len(),
            self.delta_to_best_lap_seconds.len(),
        ]
        .into_iter()
        .any(|len| len != 0 && len != times.len())
        {
            return Err(WidgetError::Config(
                "lap timing columns must align with timestamps".into(),
            ));
        }
        if self
            .lap_start_elapsed_seconds
            .iter()
            .any(|start| !start.is_finite())
            || self
                .lap_start_elapsed_seconds
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || self.lap_number.iter().any(|number| {
                *number < -1
                    || (*number >= 0 && *number as usize >= self.lap_start_elapsed_seconds.len())
            })
        {
            return Err(WidgetError::Config(
                "lap timing requires ordered boundaries and canonical lap numbers".into(),
            ));
        }
        if self.lap_durations_seconds.len() != self.lap_start_elapsed_seconds.len() {
            return Err(WidgetError::Config(
                "lap duration metadata must contain one entry per lap".into(),
            ));
        }
        if self
            .lap_durations_seconds
            .iter()
            .any(|time| !time.is_finite() || *time < 0.0)
        {
            return Err(WidgetError::Config(
                "lap duration metadata must be finite and nonnegative".into(),
            ));
        }
        Ok(())
    }

    /// Applies the user's disabled laps: they leave the history and stop serving as delta references.
    ///
    /// `disabled` must hold strictly increasing IDs of laps this activity observed.
    /// `distance` is the finalized distance column, empty when the activity has none.
    pub fn select(&mut self, disabled: Vec<usize>, distance: &[Option<f64>]) -> WidgetResult<()> {
        if disabled.windows(2).any(|pair| pair[0] >= pair[1])
            || disabled
                .last()
                .is_some_and(|lap| *lap >= self.lap_start_elapsed_seconds.len())
        {
            return Err(WidgetError::Config(
                "disabled laps must be strictly increasing IDs of observed laps".into(),
            ));
        }
        self.selected_delta_seconds = if disabled.is_empty() {
            self.delta_to_best_lap_seconds.clone()
        } else {
            if self.lap_time_seconds.len() != self.lap_number.len() {
                return Err(WidgetError::Config(
                    "lap selection requires aligned lap numbers and lap times".into(),
                ));
            }
            compute_delta_to_best(
                distance,
                &self.lap_number,
                &self.lap_time_seconds,
                &self.lap_durations_seconds,
                &disabled,
            )
        };
        self.disabled_laps = disabled;
        Ok(())
    }

    pub fn history(&self, end: f64) -> LapHistory<'_> {
        LapHistory {
            starts: &self.lap_start_elapsed_seconds,
            durations: &self.lap_durations_seconds,
            disabled: &self.disabled_laps,
            end,
        }
    }

    pub fn at(&self, times: &[f64], second: f64) -> LapSample {
        // Empty external telemetry and times before its first held sample are inactive laps.
        let number = times
            .partition_point(|time| *time <= second + 1e-9)
            .checked_sub(1)
            .and_then(|index| self.lap_number.get(index))
            .copied()
            .unwrap_or(-1);
        LapSample::from_aligned(
            &self.lap_start_elapsed_seconds,
            number,
            lap_time_at(&self.lap_start_elapsed_seconds, number, second),
            second,
            interpolate_numeric_series_value(
                times,
                &self.selected_delta_seconds,
                second,
                MissingSamplePolicy::Preserve,
            ),
        )
    }
}

/// Computes each sample's live time delta against the best previously completed enabled lap.
///
/// `disabled` holds strictly increasing canonical lap IDs that never serve as a
/// reference. Samples outside a lap, before any enabled lap completes, or without
/// usable distance/time values remain `None`; an absent distance series yields
/// no deltas. After passing reference distance coverage, delta keeps accumulating
/// against the completed reference lap duration.
pub fn compute_delta_to_best(
    distance_series: &[Option<f64>],
    lap_number: &[i64],
    lap_time_series: &[Option<f64>],
    lap_durations: &[f64],
    disabled: &[usize],
) -> Vec<Option<f64>> {
    let sample_count = lap_number.len();
    if sample_count == 0 || lap_durations.is_empty() || distance_series.is_empty() {
        return vec![None; sample_count];
    }

    let lap_start_distances =
        compute_lap_start_distances(distance_series, lap_number, lap_durations.len());
    let reference_points_by_lap = build_reference_lap_points(
        distance_series,
        lap_time_series,
        lap_number,
        &lap_start_distances,
        lap_durations.len(),
    );
    let reference_lap_indices = best_lap_indices(lap_durations, disabled);

    let mut delta = vec![None; sample_count];

    for index in 0..sample_count {
        let lap = lap_number[index];
        if lap < 0 {
            continue;
        }
        let lap_index = lap as usize;
        if lap_index == 0 || lap_index > lap_durations.len() {
            continue;
        }
        let Some(reference_lap_index) = reference_lap_indices[lap_index - 1] else {
            continue;
        };
        if reference_lap_index >= lap_start_distances.len()
            || lap_index >= lap_start_distances.len()
        {
            continue;
        }

        let current_lap_time = match lap_time_series[index] {
            Some(time) => time,
            None => continue,
        };
        let current_distance = match distance_series[index] {
            Some(distance) if distance.is_finite() => distance,
            _ => continue,
        };
        let current_lap_distance = current_distance - lap_start_distances[lap_index];
        if current_lap_distance < 0.0 {
            continue;
        }

        let reference_points = &reference_points_by_lap[reference_lap_index];
        if reference_points.len() < 2 {
            continue;
        }
        let reference_lap_max_distance = reference_points[reference_points.len() - 1].0;
        if reference_lap_max_distance <= 0.0 {
            continue;
        }

        let reference_lap_time = if current_lap_distance > reference_lap_max_distance {
            lap_durations[reference_lap_index]
        } else if let Some(reference_lap_time) =
            interpolate_points(reference_points, current_lap_distance)
        {
            reference_lap_time
        } else {
            continue;
        };
        delta[index] = Some(current_lap_time - reference_lap_time);
    }

    delta
}

/// Captures cumulative activity distance at the first sample of each canonical lap.
///
/// Missing distance at a lap's opening sample retains the zero placeholder used
/// by the existing delta contract.
fn compute_lap_start_distances(
    distance_series: &[Option<f64>],
    lap_number: &[i64],
    count: usize,
) -> Vec<f64> {
    let mut start_distances = vec![0.0; count];
    let mut current_lap = -1;
    for (index, &lap) in lap_number.iter().enumerate() {
        if lap != current_lap && lap >= 0 {
            let lap_index = lap as usize;
            if lap_index < count {
                if let Some(Some(distance)) = distance_series.get(index) {
                    start_distances[lap_index] = *distance;
                }
            }
            current_lap = lap;
        }
    }
    start_distances
}

/// Selects the fastest enabled completed lap available at each completion point.
///
/// Entry `n` identifies the best enabled lap among completed laps `0..=n`, or
/// `None` while every such lap is disabled; equal durations preserve the
/// earliest matching lap.
fn best_lap_indices(durations: &[f64], disabled: &[usize]) -> Vec<Option<usize>> {
    let mut best_index: Option<usize> = None;
    durations
        .iter()
        .enumerate()
        .map(|(index, duration)| {
            if disabled.binary_search(&index).is_err()
                && best_index.is_none_or(|best| *duration < durations[best])
            {
                best_index = Some(index);
            }
            best_index
        })
        .collect()
}

/// Builds lap-relative distance/time interpolation points for every lap.
///
/// A lap serves as a reference only for laps that start after it closes.
fn build_reference_lap_points(
    distance_series: &[Option<f64>],
    lap_time_series: &[Option<f64>],
    lap_number: &[i64],
    lap_start_distances: &[f64],
    completed_lap_count: usize,
) -> Vec<Vec<(f64, f64)>> {
    assert!(completed_lap_count <= lap_start_distances.len());
    let mut points_by_lap = vec![Vec::new(); completed_lap_count];
    for ((distance, lap_time), lap) in distance_series.iter().zip(lap_time_series).zip(lap_number) {
        let Ok(lap_index) = usize::try_from(*lap) else {
            continue;
        };
        if lap_index >= completed_lap_count {
            continue;
        }
        if let (Some(distance), Some(lap_time)) = (distance, lap_time) {
            points_by_lap[lap_index].push((distance - lap_start_distances[lap_index], *lap_time));
        }
    }
    points_by_lap
}

#[derive(Clone, Copy, Default)]
pub struct LapHistory<'a> {
    pub starts: &'a [f64],
    pub durations: &'a [f64],
    /// User-disabled canonical lap IDs, strictly increasing; they neither draw nor count as best.
    pub disabled: &'a [usize],
    pub end: f64,
}

impl LapHistory<'_> {
    pub fn is_disabled(&self, lap: usize) -> bool {
        self.disabled.binary_search(&lap).is_ok()
    }

    /// Whether `lap_number` is an observed lap the user left enabled; the out-lap never is.
    pub fn is_enabled(&self, lap_number: i64) -> bool {
        usize::try_from(lap_number).is_ok_and(|lap| !self.is_disabled(lap))
    }

    /// 1-based label for canonical lap `lap`; renumbering skips disabled laps before it.
    pub fn display_number(&self, lap: usize, renumber: bool) -> usize {
        let skipped = if renumber {
            self.disabled.partition_point(|disabled| *disabled < lap)
        } else {
            0
        };
        lap - skipped + 1
    }
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
    /// A disabled current lap draws nothing; the placeholder's footprint keeps the widget selectable.
    Hidden,
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

/// Fastest enabled lap among the first `count` completed laps; none while all are disabled.
pub fn best_completed_lap(history: LapHistory<'_>, count: usize) -> WidgetResult<Option<f64>> {
    let durations = history.durations.get(..count).ok_or_else(|| {
        WidgetError::Render(format!(
            "lap duration metadata is missing for completed lap {count}"
        ))
    })?;
    Ok(durations
        .iter()
        .enumerate()
        .filter(|(index, _)| !history.is_disabled(*index))
        .map(|(_, duration)| *duration)
        .reduce(f64::min))
}

/// Running time of the enabled active lap; the out-lap and disabled laps show the placeholder.
fn active_lap_text(history: LapHistory<'_>, sample: LapSample) -> WidgetResult<String> {
    if !history.is_enabled(sample.lap_number) {
        return Ok(PLACEHOLDER.into());
    }
    sample
        .lap_time_seconds
        .map(format_lap_duration)
        .ok_or_else(|| WidgetError::Render("active lap is missing its canonical lap time".into()))
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
        LapTimerMode::BestLap => match best_completed_lap(history, sample.completed_lap_count)? {
            Some(best) => Ok(format_lap_duration(best)),
            None => active_lap_text(history, sample),
        },
        LapTimerMode::CurrentLap => active_lap_text(history, sample),
    }
}

/// The enabled active lap's row; the out-lap and disabled laps have none.
pub fn current_log_row(
    history: LapHistory<'_>,
    sample: LapSample,
    renumber: bool,
) -> WidgetResult<Option<LapLogTextRow>> {
    let Ok(lap) = usize::try_from(sample.lap_number) else {
        return Ok(None);
    };
    if history.is_disabled(lap) {
        return Ok(None);
    }
    let time = sample.lap_time_seconds.ok_or_else(|| {
        WidgetError::Render("active lap is missing its canonical lap time".into())
    })?;
    Ok(Some(LapLogTextRow {
        cells: [
            history.display_number(lap, renumber).to_string(),
            format_lap_duration(time),
            format_lap_delta(sample.delta_seconds),
        ],
        delta_seconds: sample.delta_seconds,
    }))
}

pub fn lap_log_text_state(
    history: LapHistory<'_>,
    sample: LapSample,
    renumber: bool,
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
        .filter(|(index, _)| !history.is_disabled(*index))
        .map(|(index, duration)| {
            let delta = best.map(|best| duration - best);
            LapLogTextRow {
                cells: [
                    history.display_number(index, renumber).to_string(),
                    format_lap_duration(*duration),
                    format_lap_delta(delta),
                ],
                delta_seconds: delta,
            }
        })
        .collect();
    Ok(LapLogTextState {
        completed_rows,
        current_row: current_log_row(history, sample, renumber)?,
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
            current_row: current_log_row(history, sample, config.renumber_laps)?
                .map(|row| (row.cells, color(row.delta_seconds))),
        })
    } else if matches!(config.mode, LapTimerMode::CurrentLap | LapTimerMode::Delta)
        && sample.lap_number >= 0
        && !history.is_enabled(sample.lap_number)
    {
        Ok(LapTimerContent::Hidden)
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

#[derive(Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub label_font_size: f32,
}

pub fn resize_lap_timer(
    widget: ValueConfig,
    scene: ScenePresentationConfig,
    factor: f32,
    gesture: ResizeGesture,
    activity: Option<&ActivitySampler>,
    fonts: &dyn FontResolver,
) -> WidgetResult<ResizeUpdate> {
    let ResizeGesture {
        fixed,
        point,
        second,
        ..
    } = gesture;
    let mut config = validate_lap_timer(widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    config.font_size = (config.font_size * factor).clamp(8.0, 400.0);
    config.label_font_size = (config.label_font_size * factor).clamp(6.0, 400.0);
    let history = activity.map_or_else(LapHistory::default, ActivitySampler::lap_history);
    let sample = activity.map_or_else(LapSample::default, |activity| activity.lap_timer_at(second));
    let content = lap_timer_content(&config, history, sample)?;
    let bounds = prepare_lap_timer_layout(&config, &scene, history, fonts)?
        .prepare_frame(history, &content)?
        .content_bounds;
    Ok(ResizeUpdate {
        x: config.x + point[0] - (bounds.left + bounds.width() * fixed[0]),
        y: config.y + point[1] - (bounds.top + bounds.height() * fixed[1]),
        font_size: config.font_size,
        label_font_size: config.label_font_size,
    })
}

pub struct PreviewLapTimer {
    config: ValidatedLapTimer,
    scene: ValidatedScenePresentation,
    preview_raster_scale: PreviewRasterScale,
    layout: Option<LapTimerLayout>,
    last_content: Option<LapTimerContent>,
    pub tile: Tile,
}

impl PreviewLapTimer {
    pub fn new(
        widget: ValueConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
    ) -> WidgetResult<Self> {
        let config = validate_lap_timer(widget, 0)?;
        let host = WidgetHost::from_scene(&scene, raster_scale)?;
        Ok(Self::from_validated(
            config,
            validate_scene_presentation(scene)?,
            &host,
        ))
    }

    pub fn from_validated(
        config: ValidatedLapTimer,
        scene: ValidatedScenePresentation,
        host: &WidgetHost,
    ) -> Self {
        Self {
            config,
            scene,
            preview_raster_scale: host.raster_scale,
            layout: None,
            last_content: None,
            tile: Tile::default(),
        }
    }

    /// The content the frame at `second` draws; equal content draws identical pixels.
    pub fn frame_key(
        &self,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> WidgetResult<LapTimerContent> {
        let history = activity.map_or_else(LapHistory::default, ActivitySampler::lap_history);
        let sample =
            activity.map_or_else(LapSample::default, |activity| activity.lap_timer_at(second));
        lap_timer_content(&self.config, history, sample)
    }

    pub fn invalidate_activity(&mut self) {
        self.layout = None;
        self.last_content = None;
    }

    /// Glyph layout for the activity's lap history; it allocates no surface.
    fn layout(
        &mut self,
        history: LapHistory<'_>,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<&LapTimerLayout> {
        if self.layout.is_none() {
            self.layout = Some(prepare_lap_timer_layout(
                &self.config,
                &self.scene,
                history,
                fonts,
            )?);
        }
        Ok(self.layout.as_ref().expect("layout prepared"))
    }

    /// Scene rectangle a render at `second` draws into, from glyph layout alone.
    pub fn layout_bounds(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let history = activity.map_or_else(LapHistory::default, ActivitySampler::lap_history);
        let sample =
            activity.map_or_else(LapSample::default, |activity| activity.lap_timer_at(second));
        let content = lap_timer_content(&self.config, history, sample)?;
        // Direct glyph runs bound the same rectangle as the cached static layer.
        Ok(self
            .layout(history, fonts)?
            .prepare_frame(history, &content)?
            .bounds
            .into())
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(
        &mut self,
        canvas: &Canvas,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let history = activity.map_or_else(LapHistory::default, ActivitySampler::lap_history);
        let sample =
            activity.map_or_else(LapSample::default, |activity| activity.lap_timer_at(second));
        let content = lap_timer_content(&self.config, history, sample)?;
        let raster_scale = self.preview_raster_scale;
        let frame =
            self.layout(history, fonts)?
                .prepare_cached_frame(history, &content, raster_scale)?;
        frame.draw(canvas);
        Ok(frame.bounds.into())
    }

    pub fn render(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<()> {
        let history = activity.map_or_else(LapHistory::default, ActivitySampler::lap_history);
        let sample =
            activity.map_or_else(LapSample::default, |activity| activity.lap_timer_at(second));
        let content = lap_timer_content(&self.config, history, sample)?;
        if self.last_content.as_ref() == Some(&content) {
            return Ok(());
        }
        let raster_scale = self.preview_raster_scale;
        let frame =
            self.layout(history, fonts)?
                .prepare_cached_frame(history, &content, raster_scale)?;
        let bounds = frame.bounds;
        self.tile.render(
            self.preview_raster_scale,
            TEXT_COLOR_TYPE,
            bounds,
            Presented::new(frame.content_bounds, frame.content_width),
            |canvas| {
                frame.draw(canvas);
                Ok(())
            },
        )?;
        self.last_content = Some(content);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{best_lap_indices, compute_delta_to_best, *};

    #[test]
    fn timers_reset_at_exact_boundaries_and_inactive_laps_retain_completed_history() {
        let starts = [0.8, 2.9];
        let times: Vec<_> = [0.0, 0.8, 1.0, 2.8, 2.9, 3.0]
            .into_iter()
            .zip([-1, 0, 0, 0, 1, 1])
            .map(|(second, number)| lap_time_at(&starts, number, second))
            .collect();
        assert_eq!(times[0], None);
        assert_eq!(times[1], Some(0.0));
        assert!((times[3].unwrap() - 2.0).abs() < 1e-9);
        assert_eq!(times[4], Some(0.0));
        assert!((times[5].unwrap() - 0.1).abs() < 1e-9);
        let inactive = LapSample::from_aligned(&[0.8, 2.9, 5.0], -1, None, 6.0, None);
        assert_eq!(inactive.lap_number, -1);
        assert_eq!(inactive.lap_time_seconds, None);
        assert_eq!(inactive.completed_lap_count, 2);
    }

    /// Verifies reference selection tracks the earliest fastest completed lap.
    #[test]
    fn best_lap_selection_uses_prefix_minimum_indices() {
        assert_eq!(
            best_lap_indices(&[10.0, 5.0, 7.0, 4.0], &[]),
            vec![Some(0), Some(1), Some(1), Some(3)]
        );
        assert_eq!(best_lap_indices(&[5.0, 5.0], &[]), vec![Some(0), Some(0)]);
    }

    /// Verifies an overdue lap keeps accumulating delta after passing reference distance coverage.
    #[test]
    fn delta_continues_after_reference_lap_distance() {
        let delta = compute_delta_to_best(
            &[
                Some(0.0),
                Some(50.0),
                Some(100.0),
                Some(120.0),
                Some(230.0),
                Some(250.0),
            ],
            &[0, 0, 0, 1, 1, 1],
            &[
                Some(0.0),
                Some(5.0),
                Some(9.0),
                Some(0.0),
                Some(12.0),
                Some(20.0),
            ],
            &[10.0, 20.0],
            &[],
        );

        assert_eq!(delta[4], Some(2.0));
        assert_eq!(delta[5], Some(10.0));
    }

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
