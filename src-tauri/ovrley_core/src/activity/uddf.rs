//! UDDF parsing is based on Unabara by Arnaud Dupuis (GPL-2.0):
//! https://github.com/arnauddupuis/unabara/blob/main/src/core/format_parsers/uddf_parser.cpp
//! Rewritten in Rust for OVRLEY's canonical raw activity and diving metrics.

use super::dive_xml::{
    self, child_text, children, gas_fractions, mean, number, single_dive, start_time, timestamp,
};
use super::finalize::FinalizeActivityResponse;
use super::schema::{LapMarkers, RawActivity, RawActivityOptions, RawSample};
use crate::error::{CoreError, CoreResult};
use roxmltree::Document;
use serde_json::json;
use std::{collections::BTreeMap, io::Read, path::Path};

/// Opens and finalizes a single native UDDF dive.
pub fn parse_uddf_activity_path(
    path: &Path,
    repo_root: Option<&Path>,
) -> CoreResult<FinalizeActivityResponse> {
    dive_xml::parse_path(path, repo_root, "UDDF", extract_uddf_activity)
}

/// Reads UDDF XML and finalizes it through the shared activity pipeline.
pub fn parse_uddf_activity_reader<R: Read>(
    reader: R,
    file_name: &str,
    repo_root: Option<&Path>,
) -> CoreResult<FinalizeActivityResponse> {
    dive_xml::parse_reader(reader, file_name, repo_root, "UDDF", extract_uddf_activity)
}

/// Extracts SI-unit waypoint observations, gas switches, and measured oxygen sensors.
pub fn extract_uddf_activity(text: &str, file_name: &str) -> CoreResult<RawActivity> {
    let document = Document::parse(text).map_err(|error| {
        CoreError::Activity(format!("The UDDF file could not be parsed: {error}"))
    })?;
    let root = document.root_element();
    if root.tag_name().name() != "uddf" {
        return Err(CoreError::Activity(
            "The UDDF file must contain an uddf root".into(),
        ));
    }
    let dive = single_dive(root, "UDDF")?;
    let oxygen_element = if root
        .attribute("version")
        .is_some_and(|version| version.starts_with("3.3"))
    {
        "ppo2"
    } else {
        "measuredpo2"
    };
    let info = children(dive, "informationbeforedive").next();
    let start = start_time(info.and_then(|info| child_text(info, "datetime")), "UDDF")?;
    let mixes: BTreeMap<_, _> = children(root, "gasdefinitions")
        .flat_map(|definitions| children(definitions, "mix"))
        .filter_map(|mix| {
            Some((
                mix.attribute("id")?,
                gas_fractions(
                    child_text(mix, "o2").and_then(number),
                    child_text(mix, "he").and_then(number),
                ),
            ))
        })
        .collect();
    let mut current_mix = None;
    let mut sensors = BTreeMap::new();
    let mut state = RawSample::default();
    let mut raw_samples: Vec<RawSample> = Vec::new();
    let mut waypoints: Vec<_> = children(dive, "samples")
        .flat_map(|samples| children(samples, "waypoint"))
        .filter_map(|waypoint| {
            Some((
                child_text(waypoint, "divetime")
                    .and_then(number)
                    .filter(|time| *time >= 0.0)?,
                waypoint,
            ))
        })
        .collect();
    // Some AP DiveSight exports place samples slightly out of time order.
    // Sort before carrying delta observations so the source clock stays intact.
    waypoints.sort_by(|(left, _), (right, _)| left.total_cmp(right));
    for (elapsed, waypoint) in waypoints {
        state.elapsed_seconds = Some(elapsed);
        state.timestamp = timestamp(start, elapsed);
        if let Some(text) = child_text(waypoint, "depth") {
            state.depth = number(text).filter(|depth| *depth >= 0.0);
        }
        if let Some(text) = child_text(waypoint, "temperature") {
            state.water_temperature = number(text)
                .filter(|kelvin| *kelvin >= 0.0)
                .map(|kelvin| kelvin - 273.15);
        }
        for switch in children(waypoint, "switchmix") {
            current_mix = switch
                .attribute("ref")
                .and_then(|reference| mixes.get(reference))
                .copied();
        }
        (state.fraction_o2, state.fraction_he) = current_mix.unwrap_or((None, None));
        for (index, sensor) in children(waypoint, oxygen_element).enumerate() {
            let key = sensor
                .attribute("ref")
                .map(str::to_string)
                .unwrap_or_else(|| format!("#{index}"));
            // UDDF oxygen pressures are Pa; the widget's canonical unit is atm.
            if let Some(value) = sensor.text().and_then(number).filter(|value| *value >= 0.0) {
                sensors.insert(key, value / 101_325.0);
            } else {
                sensors.remove(&key);
            }
        }
        state.average_ppo2 = mean(sensors.values().copied());
        let deepest_stop = children(waypoint, "decostop")
            .filter(|stop| matches!(stop.attribute("kind"), None | Some("mandatory")))
            .filter_map(|stop| {
                Some((
                    number(stop.attribute("decodepth")?).filter(|depth| *depth >= 0.0)?,
                    stop,
                ))
            })
            .max_by(|(left, _), (right, _)| left.total_cmp(right));
        if let Some((depth, stop)) = deepest_stop {
            state.first_stop_depth = Some(depth);
            state.first_stop_time = stop
                .attribute("duration")
                .and_then(number)
                .filter(|seconds| *seconds >= 0.0)
                .map(|seconds| seconds / 60.0);
        } else if child_text(waypoint, "nodecotime")
            .and_then(number)
            .is_some_and(|seconds| seconds > 0.0)
        {
            state.first_stop_depth = Some(0.0);
            state.first_stop_time = Some(0.0);
        }
        // UDDF 3.3 can report TTS explicitly. Summing stop durations would omit ascent travel.
        if let Some(value) = child_text(waypoint, "timetosurface") {
            state.time_to_surface = number(value)
                .filter(|seconds| *seconds >= 0.0)
                .map(|seconds| seconds / 60.0);
        }
        // Apply every delta, but emit one combined observation per timestamp.
        match raw_samples.last_mut() {
            Some(previous) if previous.elapsed_seconds == Some(elapsed) => {
                *previous = state.clone();
            }
            _ => raw_samples.push(state.clone()),
        }
    }
    if raw_samples.is_empty() {
        return Err(CoreError::Activity(
            "The UDDF file does not contain any timed waypoints".into(),
        ));
    }
    Ok(RawActivity {
        file_name: file_name.into(),
        file_format: "uddf".into(),
        metadata: json!({ "sport": "diving", "dive_number": info.and_then(|info| child_text(info, "divenumber")), "creator": children(root, "generator").next().and_then(|generator| child_text(generator, "name")) }),
        sync_time: start.map(|time| time.to_rfc3339()),
        lap_markers: LapMarkers::None,
        raw_samples,
        options: RawActivityOptions {
            skip_idle_gap_fill: true,
            ..Default::default()
        },
    })
}
