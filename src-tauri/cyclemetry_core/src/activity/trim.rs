use super::interpolate::{
    interpolate_course_value, interpolate_numeric_series_value, interpolate_time_series_value,
};
use super::schema::{ParsedActivity, TrimmedActivity};
use crate::config::RenderDataRequirements;
use chrono::{DateTime, SecondsFormat, Utc};

fn validate_trim_window(duration: f64, start: f64, end: f64) -> Result<(), String> {
    if start < 0.0 || start >= duration {
        return Err(format!(
            "Invalid scene start value in config. Value should be at least 0 and less than {duration:.3}. Current value is {start}"
        ));
    }
    if end <= start || end > duration {
        return Err(format!(
            "Invalid scene end value in config. Value should be at most {duration:.3} and greater than {start}. Current value is {end}"
        ));
    }
    Ok(())
}

fn split_trim_indices(elapsed: &[f64], start: f64, end: f64) -> (usize, usize) {
    let start_inner_index = elapsed.partition_point(|value| *value <= start);
    let end_inner_index = elapsed.partition_point(|value| *value < end);
    (start_inner_index, end_inner_index)
}

fn trim_numeric_series(
    elapsed: &[f64],
    data: &[Option<f64>],
    start: f64,
    end: f64,
    start_inner_index: usize,
    end_inner_index: usize,
) -> Vec<Option<f64>> {
    let start_value = interpolate_numeric_series_value(elapsed, data, start);
    let end_value = interpolate_numeric_series_value(elapsed, data, end);
    let mut trimmed = Vec::with_capacity(end_inner_index.saturating_sub(start_inner_index) + 2);
    trimmed.push(start_value);
    trimmed.extend_from_slice(&data[start_inner_index..end_inner_index]);
    trimmed.push(end_value);
    trimmed
}

pub fn trim_activity(
    activity: &ParsedActivity,
    start: f64,
    end: f64,
    requirements: &RenderDataRequirements,
) -> Result<TrimmedActivity, String> {
    if activity.sample_elapsed_seconds.len() < 2 {
        return Err(
            "parsedActivity must contain at least two sample_elapsed_seconds values".to_string(),
        );
    }

    let duration = activity.trim_end_seconds.max(
        activity
            .sample_elapsed_seconds
            .last()
            .copied()
            .unwrap_or_default(),
    );
    validate_trim_window(duration, start, end)?;

    let elapsed = &activity.sample_elapsed_seconds;
    let (start_inner_index, end_inner_index) = split_trim_indices(elapsed, start, end);

    let mut trimmed_elapsed =
        Vec::with_capacity(end_inner_index.saturating_sub(start_inner_index) + 2);
    trimmed_elapsed.push(0.0);
    trimmed_elapsed.extend(
        elapsed[start_inner_index..end_inner_index]
            .iter()
            .map(|value| *value - start),
    );
    trimmed_elapsed.push(end - start);

    let mut trimmed_distance_progress = if !requirements.distance_progress
        || activity.sample_distance_progress.is_empty()
    {
        Vec::new()
    } else {
        let source = activity
            .sample_distance_progress
            .iter()
            .copied()
            .map(Some)
            .collect::<Vec<_>>();
        let start_progress =
            interpolate_numeric_series_value(elapsed, &source, start).unwrap_or(0.0);
        let end_progress =
            interpolate_numeric_series_value(elapsed, &source, end).unwrap_or(start_progress);
        let mut trimmed = Vec::with_capacity(end_inner_index.saturating_sub(start_inner_index) + 2);
        trimmed.push(Some(start_progress));
        trimmed.extend(
            activity.sample_distance_progress[start_inner_index..end_inner_index]
                .iter()
                .copied()
                .map(Some),
        );
        trimmed.push(Some(end_progress));
        let span = (end_progress - start_progress).max(1e-9);
        trimmed
            .into_iter()
            .map(|value| value.map(|point| (point - start_progress) / span))
            .collect::<Vec<_>>()
    };

    if trimmed_distance_progress.is_empty() {
        trimmed_distance_progress = Vec::new();
    }

    let course = if requirements.course {
        let start_course = interpolate_course_value(elapsed, &activity.course, start);
        let end_course = interpolate_course_value(elapsed, &activity.course, end);
        let mut course = Vec::with_capacity(end_inner_index.saturating_sub(start_inner_index) + 2);
        course.push(start_course);
        course.extend_from_slice(&activity.course[start_inner_index..end_inner_index]);
        course.push(end_course);
        course
    } else {
        Vec::new()
    };

    let start_time = activity
        .source_start_time
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| {
            (value + chrono::TimeDelta::milliseconds((start * 1000.0).round() as i64))
                .with_timezone(&Utc)
        })
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Millis, true));

    Ok(TrimmedActivity {
        source_start_time: start_time,
        sample_elapsed_seconds: trimmed_elapsed,
        sample_distance_progress: trimmed_distance_progress,
        course,
        elevation: if requirements.elevation {
            trim_numeric_series(
                elapsed,
                &activity.elevation,
                start,
                end,
                start_inner_index,
                end_inner_index,
            )
        } else {
            Vec::new()
        },
        speed: if requirements.speed {
            trim_numeric_series(
                elapsed,
                &activity.speed,
                start,
                end,
                start_inner_index,
                end_inner_index,
            )
        } else {
            Vec::new()
        },
        heartrate: if requirements.heartrate {
            trim_numeric_series(
                elapsed,
                &activity.heartrate,
                start,
                end,
                start_inner_index,
                end_inner_index,
            )
        } else {
            Vec::new()
        },
        cadence: if requirements.cadence {
            trim_numeric_series(
                elapsed,
                &activity.cadence,
                start,
                end,
                start_inner_index,
                end_inner_index,
            )
        } else {
            Vec::new()
        },
        power: if requirements.power {
            trim_numeric_series(
                elapsed,
                &activity.power,
                start,
                end,
                start_inner_index,
                end_inner_index,
            )
        } else {
            Vec::new()
        },
        temperature: if requirements.temperature {
            trim_numeric_series(
                elapsed,
                &activity.temperature,
                start,
                end,
                start_inner_index,
                end_inner_index,
            )
        } else {
            Vec::new()
        },
        gradient: if requirements.gradient {
            trim_numeric_series(
                elapsed,
                &activity.gradient,
                start,
                end,
                start_inner_index,
                end_inner_index,
            )
        } else {
            Vec::new()
        },
        time: if requirements.time {
            let start_value = interpolate_time_series_value(elapsed, &activity.time, start);
            let end_value = interpolate_time_series_value(elapsed, &activity.time, end);
            let mut trimmed =
                Vec::with_capacity(end_inner_index.saturating_sub(start_inner_index) + 2);
            trimmed.push(start_value);
            trimmed.extend_from_slice(&activity.time[start_inner_index..end_inner_index]);
            trimmed.push(end_value);
            trimmed
        } else {
            Vec::new()
        },
    })
}
