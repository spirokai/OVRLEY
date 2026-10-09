//! TCX XML is an external-system boundary. Missing or invalid sensor readings
//! remain absent; each Trackpoint still produces one canonical RawSample.
//! Workout summaries and derived metrics belong to the shared finalizer.

use crate::activity::schema::{
    LapMarkers, RawActivity, RawActivityOptions, RawSample, SmoothingOption,
};
use crate::error::{CoreError, CoreResult};
use chrono::{DateTime, FixedOffset};
use roxmltree::{Document, Node};
use serde_json::json;
use std::collections::BTreeMap;

const TCX_NAMESPACE: &str = "http://www.garmin.com/xmlschemas/TrainingCenterDatabase/v2";
const EXTENSION_NAMESPACE: &str = "http://www.garmin.com/xmlschemas/ActivityExtension/v2";

/// Extracts one TCX activity's laps and tracks into the canonical raw contract.
/// Multiple activities are rejected instead of merging independent timelines.
/// Both namespace-qualified TCX v2 and unqualified exports are accepted.
pub fn extract_tcx_activity(text: &str, file_name: &str) -> CoreResult<RawActivity> {
    let document = Document::parse(text).map_err(|error| {
        CoreError::Activity(format!("The TCX file could not be parsed: {error}"))
    })?;
    let root = document.root_element();
    let namespace = root.tag_name().namespace();
    if root.tag_name().name() != "TrainingCenterDatabase"
        || !matches!(namespace, None | Some(TCX_NAMESPACE))
    {
        return Err(CoreError::Activity(
            "The TCX file must contain a TrainingCenterDatabase v2 root".into(),
        ));
    }
    let mut activities = children(root, "Activities", namespace).flat_map(|container| {
        container.descendants().filter(|node| {
            node.is_element()
                && node.tag_name().name() == "Activity"
                && node.tag_name().namespace() == namespace
        })
    });
    let activity = activities.next().ok_or_else(|| {
        CoreError::Activity("The TCX file does not contain any activities".into())
    })?;
    if activities.next().is_some() {
        return Err(CoreError::Activity(
            "The TCX file contains multiple activities; export a file containing a single activity"
                .into(),
        ));
    }
    let activity_id = child_text(activity, "Id", namespace)
        .ok_or_else(|| CoreError::Activity("TCX activity is missing Id".into()))?;
    let origin = DateTime::parse_from_rfc3339(activity_id).map_err(|error| {
        CoreError::Activity(format!("TCX activity has an invalid Id timestamp: {error}"))
    })?;
    let laps: Vec<_> = children(activity, "Lap", namespace).collect();
    let lap_markers = extract_lap_markers(&laps, origin)?;
    let raw_samples: Vec<_> = laps
        .iter()
        .flat_map(|lap| {
            children(*lap, "Track", namespace)
                .flat_map(move |track| children(track, "Trackpoint", namespace))
                .map(move |point| extract_trackpoint(point, namespace, origin))
        })
        .collect();
    if raw_samples.is_empty() {
        return Err(CoreError::Activity(
            "The TCX file does not contain any track points".into(),
        ));
    }
    if !raw_samples
        .iter()
        .any(|sample| sample.elapsed_seconds.is_some())
    {
        return Err(CoreError::Activity(
            "The TCX file does not contain any valid trackpoint timestamps".into(),
        ));
    }
    if raw_samples
        .iter()
        .any(|sample| sample.elapsed_seconds.is_some_and(|elapsed| elapsed < 0.0))
    {
        return Err(CoreError::Activity(
            "TCX trackpoint timestamps must not precede the activity Id".into(),
        ));
    }
    let creator = children(activity, "Creator", namespace)
        .next()
        .and_then(|creator| child_text(creator, "Name", namespace));
    let sport = activity.attribute("Sport").map(str::to_lowercase);
    let total_timer_time = laps.iter().try_fold(0.0, |total, lap| {
        number(*lap, "TotalTimeSeconds", namespace).map(|duration| total + duration)
    });
    Ok(RawActivity {
        file_name: file_name.to_string(),
        file_format: "tcx".to_string(),
        metadata: json!({
            "creator": creator,
            "sport": sport,
            "total_timer_time": total_timer_time,
        }),
        sync_time: Some(origin.to_rfc3339()),
        lap_markers,
        raw_samples,
        options: RawActivityOptions {
            skip_idle_gap_fill: false,
            smoothing: BTreeMap::from([
                (
                    "pace".to_string(),
                    SmoothingOption {
                        enabled: true,
                        method: "zero_phase_ma".to_string(),
                        window_seconds: 5.0,
                    },
                ),
                (
                    "heading".to_string(),
                    SmoothingOption {
                        enabled: true,
                        method: "circular_ema".to_string(),
                        window_seconds: 3.0,
                    },
                ),
            ]),
        },
    })
}

/// A missing lap timestamp leaves lap timing unavailable for the whole activity.
/// Skipping an individual boundary would merge laps and produce incorrect durations.
fn extract_lap_markers(
    laps: &[Node<'_, '_>],
    origin: DateTime<FixedOffset>,
) -> CoreResult<LapMarkers> {
    let lap_start_times = laps
        .iter()
        .enumerate()
        .map(|(index, lap)| {
            lap.attribute("StartTime")
                .map(|start| {
                    DateTime::parse_from_rfc3339(start.trim()).map_err(|error| {
                        CoreError::Activity(format!(
                            "TCX lap {} has an invalid StartTime: {error}",
                            index + 1
                        ))
                    })
                })
                .transpose()
        })
        .collect::<CoreResult<Vec<_>>>()?;
    let Some(lap_start_times) = lap_start_times.into_iter().collect::<Option<Vec<_>>>() else {
        return Ok(LapMarkers::None);
    };
    let lap_start_elapsed_seconds: Vec<_> = lap_start_times
        .into_iter()
        .map(|start| (start - origin).num_milliseconds() as f64 / 1000.0)
        .collect();
    if lap_start_elapsed_seconds.iter().any(|start| *start < 0.0)
        || lap_start_elapsed_seconds
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        return Err(CoreError::Activity(
            "TCX lap StartTime values must not precede the activity Id and must be strictly increasing"
                .into(),
        ));
    }
    Ok(LapMarkers::BeaconMarkers(lap_start_elapsed_seconds))
}

fn extract_trackpoint(
    point: Node<'_, '_>,
    namespace: Option<&str>,
    origin: DateTime<FixedOffset>,
) -> RawSample {
    let position = children(point, "Position", namespace).next();
    let heart_rate = children(point, "HeartRateBpm", namespace).next();
    let extension = children(point, "Extensions", namespace)
        .flat_map(|extensions| children(extensions, "TPX", Some(EXTENSION_NAMESPACE)))
        .next();
    let timestamp = child_text(point, "Time", namespace).map(str::to_string);
    let elapsed_seconds = timestamp
        .as_deref()
        .and_then(|time| DateTime::parse_from_rfc3339(time).ok())
        .map(|time| (time - origin).num_milliseconds() as f64 / 1000.0);
    RawSample {
        timestamp,
        elapsed_seconds,
        latitude: position.and_then(|position| number(position, "LatitudeDegrees", namespace)),
        longitude: position.and_then(|position| number(position, "LongitudeDegrees", namespace)),
        elevation: number(point, "AltitudeMeters", namespace),
        distance: number(point, "DistanceMeters", namespace),
        heartrate: heart_rate.and_then(|heart_rate| number(heart_rate, "Value", namespace)),
        cadence: number(point, "Cadence", namespace).or_else(|| {
            extension
                .and_then(|extension| number(extension, "RunCadence", Some(EXTENSION_NAMESPACE)))
        }),
        speed: extension
            .and_then(|extension| number(extension, "Speed", Some(EXTENSION_NAMESPACE))),
        power: extension
            .and_then(|extension| number(extension, "Watts", Some(EXTENSION_NAMESPACE))),
        ..RawSample::default()
    }
}

fn children<'a, 'input>(
    node: Node<'a, 'input>,
    name: &'a str,
    namespace: Option<&'a str>,
) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(move |child| {
        child.is_element()
            && child.tag_name().name() == name
            && child.tag_name().namespace() == namespace
    })
}

fn child_text<'a, 'input>(
    node: Node<'a, 'input>,
    name: &'a str,
    namespace: Option<&'a str>,
) -> Option<&'a str> {
    children(node, name, namespace)
        .next()
        .and_then(|child| child.text())
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn number(node: Node<'_, '_>, name: &str, namespace: Option<&str>) -> Option<f64> {
    child_text(node, name, namespace)?
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}
