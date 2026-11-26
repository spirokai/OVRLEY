//! Shared diving-XML extraction (Subsurface and UDDF).
//!
//! Subsurface and UDDF parsing are based on Unabara by Arnaud Dupuis (GPL-2.0):
//! <https://github.com/arnauddupuis/unabara/tree/main/src/core/format_parsers>,
//! rewritten for the canonical raw activity and diving metrics.

use super::{parse_number, parse_document, parse_text_path, parse_text_reader, BAR_PER_ATMOSPHERE};
use crate::activity::schema::{LapMarkers, ParsedActivity, RawActivity, RawActivityOptions, RawSample};
use crate::error::{CoreError, CoreResult};
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeDelta};
use roxmltree::Node;
use serde_json::json;
use std::{collections::BTreeMap, io::Read, path::Path};

/// Finds child elements in the parent's namespace.
pub(super) fn children<'a, 'input>(
    node: Node<'a, 'input>,
    name: &'a str,
) -> impl Iterator<Item = Node<'a, 'input>> {
    super::children(node, name, node.tag_name().namespace())
}

pub(super) fn child_text<'a>(node: Node<'a, '_>, name: &'a str) -> Option<&'a str> {
    super::child_text(node, name, node.tag_name().namespace())
}

/// Reads a number, accepting a decimal comma.
pub(super) fn number(text: &str) -> Option<f64> {
    parse_number(&text.trim().replace(',', "."))
}

fn single_dive<'a, 'input>(
    root: Node<'a, 'input>,
    format: &str,
) -> CoreResult<Node<'a, 'input>> {
    let mut dives = root.descendants().filter(|node| {
        node.is_element()
            && node.tag_name().name() == "dive"
            && node.tag_name().namespace() == root.tag_name().namespace()
            // UDDF can include header-only previous-dive references.
            && (children(*node, "samples").next().is_some()
                || children(*node, "divecomputer").next().is_some())
    });
    let dive = dives.next().ok_or_else(|| {
        CoreError::Activity(format!("The {format} file does not contain any dives"))
    })?;
    if dives.next().is_some() {
        return Err(CoreError::Activity(format!(
            "The {format} file contains multiple dives; export a file containing a single dive"
        )));
    }
    Ok(dive)
}

pub(super) fn start_time(
    text: Option<&str>,
    format: &str,
) -> CoreResult<Option<DateTime<FixedOffset>>> {
    text.map(|text| {
        DateTime::parse_from_rfc3339(text.trim())
            .or_else(|_| {
                // These exports can omit the offset. With no GPS timeline, retain
                // the shared finalizer's convention of interpreting naive time as UTC.
                NaiveDateTime::parse_from_str(text.trim(), "%Y-%m-%dT%H:%M:%S%.f")
                    .map(|time| time.and_utc().fixed_offset())
            })
            .map_err(|error| {
                CoreError::Activity(format!("Invalid {format} dive start time: {error}"))
            })
    })
    .transpose()
}

pub(super) fn timestamp(start: Option<DateTime<FixedOffset>>, elapsed: f64) -> Option<String> {
    start?
        .checked_add_signed(TimeDelta::try_milliseconds(
            (elapsed * 1000.0).round() as i64
        )?)
        .map(|time| time.to_rfc3339())
}

pub(super) fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (sum, count) = values.fold((0.0, 0), |(sum, count), value| (sum + value, count + 1));
    (count > 0).then(|| sum / count as f64)
}

/// A gas observation is atomic; an omitted constituent is not zero helium.
fn gas_fractions(
    oxygen: Option<f64>,
    helium: Option<f64>,
) -> (Option<f64>, Option<f64>) {
    match (oxygen, helium) {
        (Some(oxygen), Some(helium))
            if (0.0..=1.0).contains(&oxygen)
                && (0.0..=1.0).contains(&helium)
                && oxygen + helium <= 1.0 =>
        {
            (Some(oxygen), Some(helium))
        }
        _ => (None, None),
    }
}

/// Opens and finalizes a single Subsurface XML/SSRF dive.
pub fn parse_subsurface_path(
    path: &Path,
    repo_root: Option<&Path>,
) -> CoreResult<ParsedActivity> {
    parse_text_path(path, repo_root, "Subsurface", extract_subsurface)
}

/// Reads a Subsurface dive using the shared activity finalizer.
pub fn parse_subsurface_reader<R: Read>(
    reader: R,
    file_name: &str,
    repo_root: Option<&Path>,
) -> CoreResult<ParsedActivity> {
    parse_text_reader(
        reader,
        file_name,
        repo_root,
        "Subsurface",
        extract_subsurface,
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
pub fn extract_subsurface(text: &str, file_name: &str) -> CoreResult<RawActivity> {
    let document = parse_document(text, "Subsurface")?;
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
                sensors.insert(attribute.name(), value / BAR_PER_ATMOSPHERE);
            } else {
                sensors.remove(attribute.name());
            }
        }
        if let Some(value) = sample.attribute("dc_supplied_ppo2") {
            supplied_ppo2 = quantity(value, "bar")
                .filter(|value| *value >= 0.0)
                .map(|value| value / BAR_PER_ATMOSPHERE);
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

/// Opens and finalizes a single native UDDF dive.
pub fn parse_uddf_path(
    path: &Path,
    repo_root: Option<&Path>,
) -> CoreResult<ParsedActivity> {
    parse_text_path(path, repo_root, "UDDF", extract_uddf)
}

/// Reads UDDF XML and finalizes it through the shared activity pipeline.
pub fn parse_uddf_reader<R: Read>(
    reader: R,
    file_name: &str,
    repo_root: Option<&Path>,
) -> CoreResult<ParsedActivity> {
    parse_text_reader(reader, file_name, repo_root, "UDDF", extract_uddf)
}

/// Extracts SI-unit waypoint observations, gas switches, and measured oxygen sensors.
pub fn extract_uddf(text: &str, file_name: &str) -> CoreResult<RawActivity> {
    let document = parse_document(text, "UDDF")?;
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
