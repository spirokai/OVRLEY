//! The timeline in Sync Doctor: the telemetry graph and its event bands, and the landmark handles
//! with their drag.

use ovrley_core::media::sync::LandmarkKind;
use crate::features::player::lanes::seconds_to_view_px;
use crate::features::sync::detection::{DETECTED_LOCATION_ID, Detection, GraphPoint, GraphSeries, TurnDirection};
use crate::features::sync::manual::{self, find_video_sync_target};
use crate::features::sync::matching::LOCATION_TIMING_TOLERANCE_SECONDS;
use crate::store::Store;
use crate::store::editor::TimelineViewport;

const GRAPH_ROBUST_PERCENTILE: f64 = 0.99;
const GRAPH_MIN_SCALE_VALUE: f64 = 1.0;
/// The graph row's height in rem (`h-16`). Its paths are built at its height in logical pixels,
/// so they scale 1:1 with the bands.
pub const GRAPH_HEIGHT_REM: f64 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphScale {
    pub min: f64,
    pub max: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphScales {
    pub speed: GraphScale,
    pub turning: GraphScale,
}

fn percentile(values: &mut [f64], percentile: f64) -> f64 {
    if values.is_empty() {
        return GRAPH_MIN_SCALE_VALUE;
    }
    values.sort_by(f64::total_cmp);
    let position = (values.len() - 1) as f64 * percentile;
    let (lower, upper) = (position.floor() as usize, position.ceil() as usize);
    values[lower] + (values[upper] - values[lower]) * (position - lower as f64)
}

/// Speed from 0 to its 99th percentile, turning symmetric about 0.
pub fn build_graph_scales(series: &GraphSeries) -> GraphScales {
    let mut speed: Vec<f64> = series
        .speed
        .iter()
        .filter_map(|point| point.value)
        .filter(|value| value.is_finite())
        .collect();
    let mut turning: Vec<f64> = series
        .turning
        .iter()
        .filter_map(|point| point.value)
        .filter(|value| value.is_finite())
        .map(f64::abs)
        .collect();
    let speed_max = percentile(&mut speed, GRAPH_ROBUST_PERCENTILE).max(GRAPH_MIN_SCALE_VALUE);
    let turning_max = percentile(&mut turning, GRAPH_ROBUST_PERCENTILE).max(GRAPH_MIN_SCALE_VALUE);
    GraphScales {
        speed: GraphScale {
            min: 0.0,
            max: speed_max,
        },
        turning: GraphScale {
            min: -turning_max,
            max: turning_max,
        },
    }
}

fn visible_samples(series: &[GraphPoint], view: TimelineViewport) -> &[GraphPoint] {
    if series.is_empty() || view.view_end < view.view_start {
        return &[];
    }
    let start = series.partition_point(|point| point.time < view.view_start);
    let end = series.partition_point(|point| point.time <= view.view_end);
    &series[start..end.max(start)]
}

fn bucket_extrema(
    run: &[GraphPoint],
    view: TimelineViewport,
    width_px: f64,
    output: &mut Vec<GraphPoint>,
) {
    let columns = width_px.floor().max(1.0) as i64;
    let mut buckets: std::collections::BTreeMap<i64, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (index, point) in run.iter().enumerate() {
        let x = seconds_to_view_px(point.time, view, width_px);
        let bucket = (x.floor() as i64).clamp(0, columns - 1);
        buckets.entry(bucket).or_default().push(index);
    }
    for indices in buckets.values() {
        let value = |index: usize| run[index].value.expect("a valid run");
        let (mut minimum, mut maximum) = (indices[0], indices[0]);
        for &candidate in &indices[1..] {
            if value(candidate) < value(minimum) {
                minimum = candidate;
            }
            if value(candidate) > value(maximum) {
                maximum = candidate;
            }
        }
        if minimum == maximum {
            output.push(run[minimum]);
        } else if minimum < maximum {
            output.extend([run[minimum], run[maximum]]);
        } else {
            output.extend([run[maximum], run[minimum]]);
        }
    }
}

/// Per pixel the run's minimum and maximum in order; gaps stay separators.
pub fn decimate_graph_samples(
    samples: &[GraphPoint],
    view: TimelineViewport,
    width_px: f64,
) -> Vec<GraphPoint> {
    if samples.is_empty() || width_px <= 0.0 {
        return Vec::new();
    }
    let mut output = Vec::new();
    let mut run: Vec<GraphPoint> = Vec::new();
    for sample in samples {
        if sample.value.is_none() {
            bucket_extrema(&run, view, width_px, &mut output);
            run.clear();
            output.push(*sample);
        } else {
            run.push(*sample);
        }
    }
    bucket_extrema(&run, view, width_px, &mut output);
    output
}

fn value_to_y(value: f64, scale: GraphScale, height_px: f64) -> f64 {
    let ratio = ((value - scale.min) / (scale.max - scale.min)).clamp(0.0, 1.0);
    height_px - ratio * height_px
}

/// Gaps start new subpaths.
pub fn build_svg_path(
    series: &[GraphPoint],
    scale: GraphScale,
    view: TimelineViewport,
    width_px: f64,
    height_px: f64,
) -> String {
    if width_px <= 0.0 || view.view_end <= view.view_start || series.is_empty() {
        return String::new();
    }
    let visible = visible_samples(series, view);
    let decimated;
    let samples = if visible.len() as f64 > width_px {
        decimated = decimate_graph_samples(visible, view, width_px);
        &decimated[..]
    } else {
        visible
    };
    let mut commands = Vec::with_capacity(samples.len());
    let mut starts_subpath = true;
    for sample in samples {
        let Some(value) = sample.value else {
            starts_subpath = true;
            continue;
        };
        let x = seconds_to_view_px(sample.time, view, width_px);
        let y = value_to_y(value, scale, height_px);
        commands.push(format!(
            "{} {x} {y}",
            if starts_subpath { 'M' } else { 'L' }
        ));
        starts_subpath = false;
    }
    commands.join(" ")
}

/// What an event band marks: its tone and label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BandTone {
    Stop,
    Turn,
    Location,
}

/// A detected event's band over the graph.
#[derive(Clone, Debug, PartialEq)]
pub struct EventBand {
    pub id: String,
    pub tone: BandTone,
    pub label_key: &'static str,
    pub start_second: f64,
    pub end_second: f64,
    pub left: f64,
    pub width: f64,
}

/// Stops over their low-speed interval, turns over their interval, the
/// location over ±10 s; only their visible parts, ordered by start then id.
pub fn build_event_bands(
    detection: Option<&Detection>,
    view: TimelineViewport,
    width_px: f64,
) -> Vec<EventBand> {
    let Some(detection) = detection else {
        return Vec::new();
    };
    if width_px <= 0.0 || view.view_end <= view.view_start {
        return Vec::new();
    }
    let band = |id: &str, tone: BandTone, label_key: &'static str, start: f64, end: f64| {
        let (visible_start, visible_end) = (start.max(view.view_start), end.min(view.view_end));
        if visible_end <= visible_start {
            return None;
        }
        let left = seconds_to_view_px(visible_start, view, width_px);
        let right = seconds_to_view_px(visible_end, view, width_px);
        Some(EventBand {
            id: id.to_string(),
            tone,
            label_key,
            start_second: start,
            end_second: end,
            left,
            width: right - left,
        })
    };
    let mut bands: Vec<EventBand> = detection
        .stops
        .iter()
        .filter_map(|stop| {
            band(
                &stop.id,
                BandTone::Stop,
                "videoSync.stop",
                stop.low_speed_interval.start,
                stop.low_speed_interval.end,
            )
        })
        .chain(detection.location.and_then(|location| {
            band(
                DETECTED_LOCATION_ID,
                BandTone::Location,
                "videoSync.location",
                location.time - LOCATION_TIMING_TOLERANCE_SECONDS,
                location.time + LOCATION_TIMING_TOLERANCE_SECONDS,
            )
        }))
        .chain(detection.turns.iter().filter_map(|turn| {
            let key = if turn.direction == TurnDirection::Left {
                "videoSync.leftTurn"
            } else {
                "videoSync.rightTurn"
            };
            band(&turn.id, BandTone::Turn, key, turn.start, turn.end)
        }))
        .collect();
    bands.sort_by(|left, right| {
        left.start_second
            .total_cmp(&right.start_second)
            .then_with(|| crate::format::compare_text(&left.id, &right.id))
    });
    bands
}

/// The graph row's geometry: both paths and the bands, rebuilt only when the detection, the view
/// or the width changes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphView {
    pub width_px: f64,
    pub height_px: f64,
    pub speed_path: String,
    pub turning_path: String,
    pub bands: Vec<EventBand>,
}

pub fn graph_view(
    detection: Option<&Detection>,
    view: TimelineViewport,
    width_px: f64,
    height_px: f64,
) -> GraphView {
    let empty = GraphSeries::default();
    let series = detection.map_or(&empty, |detection| &detection.graph_series);
    let scales = build_graph_scales(series);
    GraphView {
        width_px,
        height_px,
        speed_path: build_svg_path(&series.speed, scales.speed, view, width_px, height_px),
        turning_path: build_svg_path(&series.turning, scales.turning, view, width_px, height_px),
        bands: build_event_bands(detection, view, width_px),
    }
}

// --- The landmark handles ---------------------------------------------------------------------

/// A landmark handle being dragged: the preview of its video second.
#[derive(Clone, Debug, PartialEq)]
pub struct LandmarkDrag {
    pub id: String,
    pub video_second: f64,
    pub moved: bool,
}

/// One landmark on the timeline.
#[derive(Clone, Debug, PartialEq)]
pub struct LandmarkModel {
    pub id: String,
    pub kind: LandmarkKind,
    /// Its handle and line, in timeline pixels; an off-view landmark sits at the edge it is past.
    pub left: f64,
    pub clamped: bool,
    /// In view, or being dragged: the handle takes the pointer.
    pub interactive: bool,
    pub video_second: f64,
}

/// The landmark models: each at the target's displayed offset plus its
/// video second, so a clip drag moves it.
pub fn landmark_models(store: &Store) -> Vec<LandmarkModel> {
    let view = store.editor.timeline_viewport;
    let width = store.player.viewport.metrics.width_px;
    let Some(target) = find_video_sync_target(store) else {
        return Vec::new();
    };
    if width <= 0.0 || view.view_end <= view.view_start {
        return Vec::new();
    }
    let displayed_offset = store
        .player
        .timeline
        .timeline()
        .displayed
        .clips
        .iter()
        .find(|clip| clip.id() == target.id())
        .map_or(target.offset_seconds(), |clip| clip.offset_seconds());
    let drag = store.video_sync.landmark_drag.as_ref();
    store
        .manual_video_sync
        .landmarks
        .iter()
        .map(|landmark| {
            let dragged = drag.filter(|drag| drag.id == landmark.id());
            let video_second = dragged.map_or(landmark.video_second(), |drag| drag.video_second);
            let second = displayed_offset + video_second;
            let in_view = second >= view.view_start && second <= view.view_end;
            let left = if in_view {
                seconds_to_view_px(second, view, width)
            } else if second > view.view_end {
                width
            } else {
                0.0
            };
            LandmarkModel {
                id: landmark.id().to_string(),
                kind: landmark.kind(),
                left,
                clamped: !in_view,
                interactive: in_view || dragged.is_some(),
                video_second,
            }
        })
        .collect()
}

/// A press on a handle: the drag starts at the landmark's second.
pub fn landmark_down(store: &mut Store, id: &str) {
    if find_video_sync_target(store).is_none() {
        return;
    }
    let Some(landmark) = store
        .manual_video_sync
        .landmarks
        .iter()
        .find(|landmark| landmark.id() == id)
    else {
        return;
    };
    store.video_sync.landmark_drag = Some(LandmarkDrag {
        id: id.to_string(),
        video_second: landmark.video_second(),
        moved: false,
    });
}

/// The pointer moved to timeline `x`: the view follows it past the edges, and the preview takes
/// its video second, kept inside the video. The offset never changes.
pub fn landmark_move(store: &mut Store, x: f64) {
    let Some(target) = find_video_sync_target(store) else {
        return;
    };
    if store.video_sync.landmark_drag.is_none() {
        return;
    }
    let width = store.player.viewport.metrics.width_px;
    if width > 0.0 && !(0.0..=width).contains(&x) {
        let second = crate::features::player::gestures::pointer_to_second(store, x);
        let minimum = crate::features::player::playback::player_timing(store).timeline_minimum;
        crate::features::player::viewport::follow_second(store, second, minimum);
    }
    let second = crate::features::player::gestures::pointer_to_second(store, x);
    let drag = store
        .video_sync
        .landmark_drag
        .as_mut()
        .expect("checked above");
    drag.moved = true;
    drag.video_second = (second - target.offset_seconds()).max(0.0).min(target.duration_seconds());
}

/// The release: a moved drag commits once; a press without a move scrubs to the landmark.
pub fn landmark_up(store: &mut Store, commit: bool) -> Result<(), String> {
    let Some(drag) = store.video_sync.landmark_drag.take() else {
        return Ok(());
    };
    if !commit {
        return Ok(());
    }
    if drag.moved {
        return manual::move_landmark(store, &drag.id, drag.video_second);
    }
    if let Some(model) = landmark_models(store)
        .into_iter()
        .find(|model| model.id == drag.id)
    {
        let target = find_video_sync_target(store).expect("a landmark has a target");
        let displayed = store
            .player
            .timeline
            .timeline()
            .displayed
            .clips
            .iter()
            .find(|clip| clip.id() == target.id())
            .map_or(target.offset_seconds(), |clip| clip.offset_seconds());
        crate::features::player::playback::commit_scrub(store, displayed + model.video_second);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::sync::detection::{Interval, LocationEvent, StopEvent, TurnEvent};

    fn points(values: &[f64], start: f64) -> Vec<GraphPoint> {
        values
            .iter()
            .enumerate()
            .map(|(index, &value)| GraphPoint {
                time: start + index as f64,
                value: Some(value),
            })
            .collect()
    }

    fn view(start: f64, end: f64) -> TimelineViewport {
        TimelineViewport {
            view_start: start,
            view_end: end,
        }
    }

    #[test]
    fn uses_fixed_zero_based_speed_and_symmetric_turning_scales() {
        let scales = build_graph_scales(&GraphSeries {
            speed: points(&[2.0, 4.0, 6.0, 100.0], 0.0),
            turning: points(&[-8.0, -2.0, 3.0, 12.0], 0.0),
        });
        assert_eq!(scales.speed.min, 0.0);
        assert!(scales.speed.max > 6.0);
        assert_eq!(scales.turning.min, -scales.turning.max);
        assert!(scales.turning.max > 8.0);
    }

    #[test]
    fn keeps_a_brief_minimum_and_maximum_in_one_pixel() {
        let decimated = decimate_graph_samples(
            &points(&[4.0, 4.0, 0.5, 4.0, 9.0, 4.0], 0.0),
            view(0.0, 6.0),
            1.0,
        );
        assert_eq!(
            decimated
                .iter()
                .map(|sample| sample.value.unwrap())
                .collect::<Vec<_>>(),
            [0.5, 9.0]
        );
    }

    #[test]
    fn uses_the_shared_time_transform_and_preserves_gaps() {
        let speed = vec![
            GraphPoint {
                time: 0.0,
                value: Some(0.0),
            },
            GraphPoint {
                time: 5.0,
                value: Some(5.0),
            },
            GraphPoint {
                time: 10.0,
                value: None,
            },
            GraphPoint {
                time: 15.0,
                value: Some(10.0),
            },
        ];
        let detection = Detection {
            graph_series: GraphSeries {
                speed,
                turning: Vec::new(),
            },
            ..Detection::default()
        };
        let graph = graph_view(Some(&detection), view(0.0, 20.0), 400.0, 64.0);
        assert!(graph.speed_path.contains("M 0 64"));
        assert!(graph.speed_path.contains("L 100"));
        assert!(graph.speed_path.contains("M 300"));
        assert!(!graph.speed_path.contains("L 300"));
    }

    fn bands(detection: Detection) -> Vec<EventBand> {
        build_event_bands(Some(&detection), view(0.0, 100.0), 500.0)
    }

    #[test]
    fn renders_a_location_as_a_ten_second_tolerance_band() {
        let found = bands(Detection {
            location: Some(LocationEvent { time: 50.0 }),
            ..Detection::default()
        });
        assert_eq!(found.len(), 1);
        assert_eq!(
            (found[0].tone, found[0].start_second, found[0].end_second),
            (BandTone::Location, 40.0, 60.0)
        );
    }

    #[test]
    fn renders_a_stop_over_its_low_speed_interval() {
        let found = bands(Detection {
            stops: vec![StopEvent {
                id: "stop-0".into(),
                time: 20.0,
                low_speed_interval: Interval {
                    start: 20.0,
                    end: 37.0,
                },
            }],
            ..Detection::default()
        });
        assert_eq!(
            (
                found[0].id.as_str(),
                found[0].start_second,
                found[0].end_second
            ),
            ("stop-0", 20.0, 37.0)
        );
    }

    #[test]
    fn renders_a_turn_over_its_interval() {
        let turn = TurnEvent {
            id: "turn-0".into(),
            direction: TurnDirection::Right,
            start: 20.0,
            end: 24.0,
            signed_change: 90.0,
            representative_time: 22.0,
        };
        let found = bands(Detection {
            turns: vec![turn],
            ..Detection::default()
        });
        assert_eq!(
            (
                found[0].id.as_str(),
                found[0].start_second,
                found[0].end_second,
                found[0].label_key
            ),
            ("turn-0", 20.0, 24.0, "videoSync.rightTurn")
        );
    }
}
