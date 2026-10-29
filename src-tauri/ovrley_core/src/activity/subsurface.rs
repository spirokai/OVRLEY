//! Subsurface parsing is based on Unabara by Arnaud Dupuis (GPL-2.0):
//! https://github.com/arnauddupuis/unabara/blob/main/src/core/format_parsers/subsurface_parser.cpp
//! Rewritten in Rust for OVRLEY's canonical raw activity and diving metrics.

use super::dive_xml::{
    self, children, gas_fractions, mean, number, single_dive, start_time, timestamp,
};
use super::finalize::FinalizeActivityResponse;
use super::schema::{LapMarkers, RawActivity, RawActivityOptions, RawSample};
use crate::error::{CoreError, CoreResult};
use roxmltree::{Document, Node};
use serde_json::json;
use std::{collections::BTreeMap, io::Read, path::Path};

/// Opens and finalizes a single Subsurface XML/SSRF dive.
pub fn parse_subsurface_activity_path(
    path: &Path,
    repo_root: Option<&Path>,
) -> CoreResult<FinalizeActivityResponse> {
    dive_xml::parse_path(path, repo_root, "Subsurface", extract_subsurface_activity)
}

/// Reads a Subsurface dive using the shared activity finalizer.
pub fn parse_subsurface_activity_reader<R: Read>(
    reader: R,
    file_name: &str,
    repo_root: Option<&Path>,
) -> CoreResult<FinalizeActivityResponse> {
    dive_xml::parse_reader(
        reader,
        file_name,
        repo_root,
        "Subsurface",
        extract_subsurface_activity,
    )
}

fn quantity(text: &str, unit: &str) -> Option<f64> {
    number(text.trim().strip_suffix(unit).unwrap_or(text).trim())
}

fn duration_seconds(text: &str) -> Option<f64> {
    let text = text.trim();
    let value = text.strip_suffix("min").unwrap_or(text).trim();
    if let Some((minutes, seconds)) = value.split_once(':') {
        let minutes =
            number(minutes).filter(|minutes| *minutes >= 0.0 && minutes.fract() == 0.0)?;
        let seconds = number(seconds).filter(|seconds| (0.0..60.0).contains(seconds))?;
        Some(minutes * 60.0 + seconds)
    } else if text.ends_with("min") {
        number(value).map(|minutes| minutes * 60.0)
    } else {
        quantity(text, "s")
    }
}

fn fractions(cylinder: Node<'_, '_>) -> (Option<f64>, Option<f64>) {
    // Subsurface omits the helium attribute for zero-helium gas mixes.
    gas_fractions(
        cylinder
            .attribute("o2")
            .and_then(|value| quantity(value, "%"))
            .map(|value| value / 100.0),
        cylinder.attribute("he").map_or(Some(0.0), |value| {
            quantity(value, "%").map(|value| value / 100.0)
        }),
    )
}

fn prediction_minutes(text: &str) -> Option<f64> {
    let minutes =
        if text.contains(':') || text.trim().ends_with("min") || text.trim().ends_with('s') {
            duration_seconds(text).map(|seconds| seconds / 60.0)
        } else {
            // Unabara also accepts unqualified decompression predictions in minutes.
            number(text)
        };
    minutes.filter(|minutes| *minutes >= 0.0)
}

/// Extracts the first sampled dive computer without merging independent profiles.
pub fn extract_subsurface_activity(text: &str, file_name: &str) -> CoreResult<RawActivity> {
    let document = Document::parse(text).map_err(|error| {
        CoreError::Activity(format!("The Subsurface file could not be parsed: {error}"))
    })?;
    let root = document.root_element();
    if root.tag_name().name() != "divelog" {
        return Err(CoreError::Activity(
            "The Subsurface file must contain a divelog root".into(),
        ));
    }
    let dive = single_dive(root, "Subsurface")?;
    let computer = children(dive, "divecomputer")
        .find(|computer| children(*computer, "sample").next().is_some())
        .ok_or_else(|| {
            CoreError::Activity(
                "The Subsurface file does not contain any dive computer samples".into(),
            )
        })?;
    let start_text = computer
        .attribute("date")
        .or_else(|| dive.attribute("date"))
        .zip(
            computer
                .attribute("time")
                .or_else(|| dive.attribute("time")),
        )
        .map(|(date, time)| format!("{date}T{time}"));
    let start = start_time(start_text.as_deref(), "Subsurface")?;
    let cylinders: Vec<_> = children(dive, "cylinder").collect();
    let initial = if matches!(computer.attribute("dctype"), Some("CCR" | "PSCR")) {
        cylinders
            .iter()
            .position(|cylinder| cylinder.attribute("use") == Some("diluent"))
    } else {
        cylinders.iter().position(|cylinder| {
            !matches!(
                cylinder.attribute("use"),
                Some("oxygen" | "diluent" | "not used")
            )
        })
    };
    let gas_mixes: Vec<_> = cylinders.iter().copied().map(fractions).collect();
    let mut current_mix = initial.map(|index| gas_mixes[index]);
    let mut switches: Vec<_> = children(computer, "event")
        .filter(|event| event.attribute("name") == Some("gaschange"))
        .filter_map(|event| Some((duration_seconds(event.attribute("time")?)?, event)))
        .collect();
    switches.sort_by(|(left, _), (right, _)| left.total_cmp(right));
    let mut switches = switches.into_iter().peekable();
    let mut sensors = BTreeMap::new();
    let mut supplied_ppo2 = None;
    let mut state = RawSample::default();
    let mut raw_samples = Vec::new();
    for sample in children(computer, "sample") {
        let Some(elapsed) = sample
            .attribute("time")
            .and_then(duration_seconds)
            .filter(|time| *time >= 0.0)
        else {
            continue;
        };
        while switches.peek().is_some_and(|(time, _)| *time <= elapsed) {
            let (event_time, event) = switches.next().unwrap();
            current_mix = if let Some(index) = event.attribute("cylinder") {
                index
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| gas_mixes.get(index))
                    .copied()
            } else {
                Some(fractions(event))
            };
            // A recorded switch between samples needs its own observation so
            // hold interpolation changes gas at the event's actual time.
            if event_time >= 0.0 && event_time < elapsed {
                if let Some((Some(fraction_o2), Some(fraction_he))) = current_mix {
                    raw_samples.push(RawSample {
                        elapsed_seconds: Some(event_time),
                        timestamp: timestamp(start, event_time),
                        fraction_o2: Some(fraction_o2),
                        fraction_he: Some(fraction_he),
                        ..RawSample::default()
                    });
                }
            }
        }
        state.elapsed_seconds = Some(elapsed);
        state.timestamp = timestamp(start, elapsed);
        for (attribute, unit, target) in [
            ("depth", "m", &mut state.depth),
            ("temp", "C", &mut state.water_temperature),
            ("stopdepth", "m", &mut state.first_stop_depth),
        ] {
            if let Some(value) = sample.attribute(attribute) {
                *target =
                    quantity(value, unit).filter(|value| attribute == "temp" || *value >= 0.0);
            }
        }
        for (attribute, target) in [
            ("stoptime", &mut state.first_stop_time),
            ("tts", &mut state.time_to_surface),
        ] {
            if let Some(value) = sample.attribute(attribute) {
                *target = prediction_minutes(value);
            }
        }
        for attribute in sample.attributes().filter(|attribute| {
            attribute
                .name()
                .strip_prefix("sensor")
                .is_some_and(|index| index.parse::<usize>().is_ok_and(|index| index > 0))
        }) {
            if let Some(value) = quantity(attribute.value(), "bar").filter(|value| *value >= 0.0) {
                sensors.insert(attribute.name(), value / 1.01325);
            } else {
                sensors.remove(attribute.name());
            }
        }
        if let Some(value) = sample.attribute("dc_supplied_ppo2") {
            supplied_ppo2 = quantity(value, "bar")
                .filter(|value| *value >= 0.0)
                .map(|value| value / 1.01325);
        }
        state.average_ppo2 = supplied_ppo2.or_else(|| mean(sensors.values().copied()));
        // `po2` is a CCR setpoint in Subsurface, not a sensor observation.
        (state.fraction_o2, state.fraction_he) = current_mix.unwrap_or((None, None));
        if let Some(value) = sample.attribute("heartbeat") {
            state.heartrate = number(value);
        }
        raw_samples.push(state.clone());
    }
    if raw_samples.is_empty() {
        return Err(CoreError::Activity(
            "The Subsurface file does not contain any timed samples".into(),
        ));
    }
    Ok(RawActivity {
        file_name: file_name.into(),
        file_format: "subsurface".into(),
        metadata: json!({ "sport": "diving", "dive_number": dive.attribute("number"), "creator": root.attribute("program"), "source_device_product": computer.attribute("model") }),
        sync_time: start.map(|time| time.to_rfc3339()),
        lap_markers: LapMarkers::None,
        raw_samples,
        options: RawActivityOptions {
            skip_idle_gap_fill: true,
            ..Default::default()
        },
    })
}
