use super::schema::{
    CourseSeries, DenseActivityReport, DenseSeriesReport, NumericSeries, TimeSeries,
    TrimmedActivity,
};
use crate::config::RenderDataRequirements;
use chrono::{DateTime, SecondsFormat, Utc};

fn collect_valid_numeric_points(x_values: &[f64], y_values: &[Option<f64>]) -> Vec<(f64, f64)> {
    x_values
        .iter()
        .copied()
        .zip(y_values.iter().copied())
        .filter_map(|(x, y)| y.map(|value| (x, value)))
        .collect()
}

fn interpolate_points(points: &[(f64, f64)], target_x: f64) -> Option<f64> {
    match points.len() {
        0 => None,
        1 => Some(points[0].1),
        _ => {
            if target_x <= points[0].0 {
                return Some(points[0].1);
            }
            let last = points.len() - 1;
            if target_x >= points[last].0 {
                return Some(points[last].1);
            }
            let right_index = points.partition_point(|(x, _)| *x < target_x);
            if right_index < points.len() && (points[right_index].0 - target_x).abs() <= 1e-9 {
                return Some(points[right_index].1);
            }
            let left_index = right_index.saturating_sub(1);
            let (left_x, left_y) = points[left_index];
            let (right_x, right_y) = points[right_index];
            if (right_x - left_x).abs() <= f64::EPSILON {
                return Some(right_y);
            }
            let ratio = (target_x - left_x) / (right_x - left_x);
            Some(left_y + (right_y - left_y) * ratio)
        }
    }
}

pub fn interpolate_numeric_series_value(
    x_values: &[f64],
    y_values: &[Option<f64>],
    target_x: f64,
) -> Option<f64> {
    let points = collect_valid_numeric_points(x_values, y_values);
    interpolate_points(&points, target_x)
}

pub fn interpolate_course_value(
    x_values: &[f64],
    course_series: &CourseSeries,
    target_x: f64,
) -> (Option<f64>, Option<f64>) {
    let latitudes = course_series
        .iter()
        .map(|point| point.0)
        .collect::<Vec<_>>();
    let longitudes = course_series
        .iter()
        .map(|point| point.1)
        .collect::<Vec<_>>();
    (
        interpolate_numeric_series_value(x_values, &latitudes, target_x),
        interpolate_numeric_series_value(x_values, &longitudes, target_x),
    )
}

pub fn interpolate_time_series_value(
    x_values: &[f64],
    time_series: &TimeSeries,
    target_x: f64,
) -> Option<String> {
    let numeric_points = x_values
        .iter()
        .copied()
        .zip(time_series.iter())
        .filter_map(|(x, value)| {
            value
                .as_deref()
                .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
                .map(|time| (x, time.timestamp_millis() as f64))
        })
        .collect::<Vec<_>>();

    interpolate_points(&numeric_points, target_x).map(|millis| {
        DateTime::<Utc>::from_timestamp_millis(millis.round() as i64)
            .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
            .to_rfc3339_opts(SecondsFormat::Millis, true)
    })
}

fn build_target_x_values(duration: f64, fps: f64) -> Vec<f64> {
    let safe_fps = fps.max(1.0);
    let mut values = Vec::new();
    let mut frame_index = 0usize;
    loop {
        let target = frame_index as f64 / safe_fps;
        if target + 1e-9 >= duration && frame_index > 0 {
            break;
        }
        values.push(target.min(duration));
        frame_index += 1;
        if frame_index > 10_000_000 {
            break;
        }
    }
    values
}

fn interpolate_numeric_series(
    x_values: &[f64],
    y_values: &NumericSeries,
    target_x_values: &[f64],
) -> Vec<Option<f64>> {
    let points = collect_valid_numeric_points(x_values, y_values);
    target_x_values
        .iter()
        .map(|target| interpolate_points(&points, *target))
        .collect()
}

fn densify_optional_numeric_series(
    x_values: &[f64],
    y_values: &NumericSeries,
    target_x_values: &[f64],
    enabled: bool,
) -> Vec<Option<f64>> {
    if !enabled || y_values.is_empty() {
        return Vec::new();
    }
    interpolate_numeric_series(x_values, y_values, target_x_values)
}

fn interpolate_course_series(
    x_values: &[f64],
    y_values: &CourseSeries,
    target_x_values: &[f64],
) -> (Vec<Option<f64>>, Vec<Option<f64>>) {
    let latitudes = y_values.iter().map(|point| point.0).collect::<Vec<_>>();
    let longitudes = y_values.iter().map(|point| point.1).collect::<Vec<_>>();
    (
        interpolate_numeric_series(x_values, &latitudes, target_x_values),
        interpolate_numeric_series(x_values, &longitudes, target_x_values),
    )
}

fn interpolate_time_series(
    source_start_time: Option<&str>,
    x_values: &[f64],
    y_values: &TimeSeries,
    target_x_values: &[f64],
) -> Vec<Option<String>> {
    if let Some(start_time) = source_start_time
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
    {
        return target_x_values
            .iter()
            .map(|target| {
                Some(
                    (start_time
                        + chrono::TimeDelta::milliseconds((target * 1000.0).round() as i64))
                    .to_rfc3339_opts(SecondsFormat::Millis, true),
                )
            })
            .collect();
    }

    target_x_values
        .iter()
        .map(|target| interpolate_time_series_value(x_values, y_values, *target))
        .collect()
}

pub fn densify_activity(
    trimmed: &TrimmedActivity,
    fps: f64,
    requirements: &RenderDataRequirements,
) -> DenseActivityReport {
    let duration = trimmed
        .sample_elapsed_seconds
        .last()
        .copied()
        .unwrap_or_default();
    let frame_elapsed_seconds = build_target_x_values(duration, fps);
    let frame_distance_progress = if !requirements.distance_progress
        || trimmed.sample_distance_progress.is_empty()
    {
        Vec::new()
    } else {
        interpolate_numeric_series(
            &trimmed.sample_elapsed_seconds,
            &trimmed.sample_distance_progress,
            &frame_elapsed_seconds,
        )
    };
    let (course_lat, course_lon) = if requirements.course && !trimmed.course.is_empty() {
        interpolate_course_series(
            &trimmed.sample_elapsed_seconds,
            &trimmed.course,
            &frame_elapsed_seconds,
        )
    } else {
        (Vec::new(), Vec::new())
    };
    let time = if requirements.time && !trimmed.time.is_empty() {
        interpolate_time_series(
            trimmed.source_start_time.as_deref(),
            &trimmed.sample_elapsed_seconds,
            &trimmed.time,
            &frame_elapsed_seconds,
        )
    } else {
        Vec::new()
    };

    DenseActivityReport {
        frame_count: frame_elapsed_seconds.len(),
        frame_elapsed_seconds: frame_elapsed_seconds.clone(),
        frame_distance_progress,
        series: DenseSeriesReport {
            speed: densify_optional_numeric_series(
                &trimmed.sample_elapsed_seconds,
                &trimmed.speed,
                &frame_elapsed_seconds,
                requirements.speed,
            ),
            elevation: densify_optional_numeric_series(
                &trimmed.sample_elapsed_seconds,
                &trimmed.elevation,
                &frame_elapsed_seconds,
                requirements.elevation,
            ),
            gradient: densify_optional_numeric_series(
                &trimmed.sample_elapsed_seconds,
                &trimmed.gradient,
                &frame_elapsed_seconds,
                requirements.gradient,
            ),
            heartrate: densify_optional_numeric_series(
                &trimmed.sample_elapsed_seconds,
                &trimmed.heartrate,
                &frame_elapsed_seconds,
                requirements.heartrate,
            ),
            cadence: densify_optional_numeric_series(
                &trimmed.sample_elapsed_seconds,
                &trimmed.cadence,
                &frame_elapsed_seconds,
                requirements.cadence,
            ),
            power: densify_optional_numeric_series(
                &trimmed.sample_elapsed_seconds,
                &trimmed.power,
                &frame_elapsed_seconds,
                requirements.power,
            ),
            temperature: densify_optional_numeric_series(
                &trimmed.sample_elapsed_seconds,
                &trimmed.temperature,
                &frame_elapsed_seconds,
                requirements.temperature,
            ),
            course_lat,
            course_lon,
            time,
        },
    }
}
