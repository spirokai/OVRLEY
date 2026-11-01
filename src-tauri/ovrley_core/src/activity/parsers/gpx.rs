//! Native GPX extraction through the shared raw-activity finalizer.
//!
//! GPX is an external-system boundary. Elements are matched by local name in any
//! namespace and anywhere below their parent, so vendor extensions need no schema.
//! Most unreadable sensor values stay absent; unreadable `barometric_altitude`,
//! `calories`, and `lap_number` extensions fail the import.

use crate::activity::finalize::{finalize_raw_activity, FinalizeActivityResponse};
use crate::activity::parsers::raw_values::{
    is_js_whitespace, js_trim, safe_gear_value, safe_number,
};
use crate::activity::parsers::{decode_file_text, read_activity_file};
use crate::activity::schema::{RawActivity, RawActivityOptions, RawSample, SmoothingOption};
use crate::error::{CoreError, CoreResult};
use roxmltree::{Document, Node, ParsingOptions};
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

type ExtensionValues = HashMap<String, String>;

/// Opens a native GPX file and returns the shared finalized activity response.
pub fn parse_gpx_activity_path(
    path: &Path,
    repo_root: Option<&Path>,
) -> CoreResult<FinalizeActivityResponse> {
    let (bytes, file_name) = read_activity_file(path, "GPX")?;
    finalize_raw_activity(&extract_gpx_activity(&bytes, file_name)?, repo_root)
}

/// Extracts one canonical raw sample per `trkpt` from GPX file bytes.
///
/// Bytes are always decoded as UTF-8, whatever the XML declaration says.
pub fn extract_gpx_activity(bytes: &[u8], file_name: &str) -> CoreResult<RawActivity> {
    let text = decode_file_text(bytes);
    let options = ParsingOptions {
        allow_dtd: true,
        ..ParsingOptions::default()
    };
    let document = Document::parse_with_options(&text, options)
        .map_err(|_| CoreError::Activity("The GPX file could not be parsed.".into()))?;
    let document_node = document.root();
    if first_descendant(document_node, "parsererror").is_some() {
        return Err(CoreError::Activity(
            "The GPX file could not be parsed.".into(),
        ));
    }

    let raw_samples = descendants_named(document_node, "trkpt")
        .map(extract_track_point)
        .collect::<CoreResult<Vec<_>>>()?;
    if raw_samples.is_empty() {
        return Err(CoreError::Activity(
            "The GPX file does not contain any track points.".into(),
        ));
    }

    let first_name = |container: Option<Node<'_, '_>>| {
        container
            .and_then(|node| first_descendant(node, "name"))
            .and_then(trimmed_text)
    };
    let activity_name = first_name(first_descendant(document_node, "metadata"))
        .or_else(|| first_name(first_descendant(document_node, "trk")));
    let creator = unprefixed_attribute(document.root_element(), "creator")
        .filter(|creator| !creator.is_empty());

    Ok(RawActivity {
        file_name: file_name.to_string(),
        file_format: "gpx".to_string(),
        metadata: json!({ "activity_name": activity_name, "creator": creator }),
        sync_time: None,
        lap_markers: Default::default(),
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

fn extract_track_point(point: Node<'_, '_>) -> CoreResult<RawSample> {
    let mut values = ExtensionValues::new();
    if let Some(extensions) = first_descendant(point, "extensions") {
        for child in extensions.children().filter(Node::is_element) {
            collect_leaf_extension_values(child, &mut values);
        }
    }
    let speed_2d = first_descendant(point, "cmt")
        .map(text_content)
        .and_then(|comment| comment_2d_speed(&comment).and_then(safe_number));
    let metric = |aliases: &[&str]| read_metric(&values, aliases);

    Ok(RawSample {
        air_pressure: metric(&["air_pressure", "absolute_pressure", "pressure"]),
        barometric_altitude: read_strict_metric(
            &values,
            &[
                "barometric_altitude",
                "barometricaltitude",
                "pressure_altitude",
            ],
        )?,
        calories: read_strict_metric(&values, &["calories", "kcal", "energy"])?,
        cadence: metric(&["cad", "cadence"]),
        core_temperature: metric(&["core_temperature", "coretemp", "core_temp"]),
        distance: metric(&[
            "distance",
            "distance_m",
            "distancemeters",
            "travelled_distance",
            "travelleddistance",
        ]),
        elevation: first_descendant(point, "ele")
            .map(text_content)
            .and_then(|text| safe_number(&text)),
        g_force: metric(&["g_force", "gforce"]),
        g_force_x: metric(&[
            "g_force_x",
            "gforce_x",
            "x_acceleration_g",
            "xaccelerationg",
        ]),
        g_force_y: metric(&[
            "g_force_y",
            "gforce_y",
            "y_acceleration_g",
            "yaccelerationg",
        ]),
        g_force_z: metric(&[
            "g_force_z",
            "gforce_z",
            "z_acceleration_g",
            "zaccelerationg",
        ]),
        gear_position: ["gear_position", "gear", "gear_ratio"]
            .iter()
            .find_map(|alias| safe_gear_value(extension_value(&values, alias)?)),
        gradient: metric(&["gradient", "grade", "slope"]),
        ground_contact_time: metric(&["ground_contact_time", "groundcontacttime", "stance_time"]),
        heading: metric(&["heading", "course", "bearing", "gps_heading"]),
        heartrate: metric(&["hr", "heartrate", "heart_rate"]),
        latitude: unprefixed_attribute(point, "lat").and_then(safe_number),
        left_right_balance: metric(&["left_right_balance", "leftrightbalance", "balance"]),
        lap_number: read_strict_lap_number(&values, &["lap_number", "lapnumber"])?,
        lean_angle: metric(&["lean_angle", "leanangle", "lean"]),
        longitude: unprefixed_attribute(point, "lon").and_then(safe_number),
        pace: metric(&["pace"]),
        power: metric(&["power", "powerinwatts", "watts"]),
        speed: metric(&["speed", "enhanced_speed", "speed_mps", "speedmps"]).or(speed_2d),
        stride_length: metric(&["stride_length", "stridelength", "step_length"]),
        stroke_rate: metric(&["stroke_rate", "strokerate"]),
        temperature: metric(&["atemp", "temperature", "temp"]),
        timestamp: first_descendant(point, "time").and_then(trimmed_text),
        torque: metric(&["torque"]),
        vertical_oscillation: metric(&["vertical_oscillation", "verticaloscillation"]),
        vertical_speed: metric(&["vertical_speed", "verticalspeed", "vam"]),
        ..RawSample::default()
    })
}

fn collect_leaf_extension_values(element: Node<'_, '_>, target: &mut ExtensionValues) {
    let mut children = element.children().filter(Node::is_element).peekable();
    if children.peek().is_some() {
        for child in children {
            collect_leaf_extension_values(child, target);
        }
        return;
    }
    let key = normalize_extension_key(element.tag_name().name());
    if let (false, Some(value)) = (key.is_empty(), trimmed_text(element)) {
        target.insert(key, value);
    }
}

fn normalize_extension_key(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .filter(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        .collect()
}

fn extension_value<'a>(values: &'a ExtensionValues, alias: &str) -> Option<&'a str> {
    values
        .get(&normalize_extension_key(alias))
        .map(String::as_str)
}

/// Returns the first alias holding a readable number; unreadable aliases fall through.
fn read_metric(values: &ExtensionValues, aliases: &[&str]) -> Option<f64> {
    aliases
        .iter()
        .find_map(|alias| safe_number(extension_value(values, alias)?))
}

fn read_strict_metric(values: &ExtensionValues, aliases: &[&str]) -> CoreResult<Option<f64>> {
    let Some((alias, value)) = first_present(values, aliases) else {
        return Ok(None);
    };
    safe_number(value).map(Some).ok_or_else(|| {
        CoreError::Activity(format!(
            "GPX extension {alias} must be numeric when present"
        ))
    })
}

fn read_strict_lap_number(values: &ExtensionValues, aliases: &[&str]) -> CoreResult<Option<i64>> {
    let Some((alias, value)) = first_present(values, aliases) else {
        return Ok(None);
    };
    safe_number(value)
        .filter(|number| number.fract() == 0.0 && number.abs() <= MAX_SAFE_INTEGER)
        .map(|number| Some(number as i64))
        .ok_or_else(|| {
            CoreError::Activity(format!(
                "GPX extension {alias} must be an integer when present"
            ))
        })
}

fn first_present<'a, 'b>(
    values: &'a ExtensionValues,
    aliases: &[&'b str],
) -> Option<(&'b str, &'a str)> {
    aliases
        .iter()
        .find_map(|alias| Some((*alias, extension_value(values, alias)?)))
}

/// Finds the raw `2dSpeed` field in a `key: value; key: value` track point comment.
/// A field is only recognized at the start of the comment or right after a `;`, and
/// an empty field is skipped in favor of a later one.
fn comment_2d_speed(comment: &str) -> Option<&str> {
    const KEY: &str = "2dspeed:";
    std::iter::once(0)
        .chain(comment.match_indices(';').map(|(index, _)| index + 1))
        .find_map(|start| {
            let field = comment[start..].trim_start_matches(is_js_whitespace);
            let key = field.get(..KEY.len())?;
            if !key.eq_ignore_ascii_case(KEY) {
                return None;
            }
            let value = &field[KEY.len()..];
            let value = &value[..value.find(';').unwrap_or(value.len())];
            (!value.is_empty()).then_some(value)
        })
}

/// Reads an attribute written without a namespace prefix. `Node::attribute` would also
/// match a prefixed attribute with the same local name.
fn unprefixed_attribute<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|attribute| attribute.namespace().is_none() && attribute.name() == name)
        .map(|attribute| attribute.value())
}

fn descendants_named<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    name: &'a str,
) -> impl Iterator<Item = Node<'a, 'input>> {
    node.descendants()
        .skip(1)
        .filter(move |descendant| descendant.is_element() && descendant.tag_name().name() == name)
}

fn first_descendant<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    name: &'a str,
) -> Option<Node<'a, 'input>> {
    descendants_named(node, name).next()
}

/// All text below a node in document order, ignoring comments and processing instructions.
fn text_content(node: Node<'_, '_>) -> String {
    node.descendants()
        .filter(Node::is_text)
        .filter_map(|descendant| descendant.text())
        .collect()
}

fn trimmed_text(node: Node<'_, '_>) -> Option<String> {
    let text = text_content(node);
    let trimmed = js_trim(&text);
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}
