//! Native FIT extraction through the shared raw-activity finalizer.
//!
//! The `fitparser` crate decodes the binary format and applies the FIT profile. This
//! module only adapts decoded messages to [`RawActivity`].
//!
//! FIT is an external-system boundary and decoding is lenient where recording devices
//! are known to be imperfect:
//!
//! - File and header checksums are not validated.
//! - Bytes after the first FIT file's data section are ignored, including a missing CRC.
//! - Developer fields overlay native fields of the same name, which is how sensors
//!   supply metrics that the FIT profile has no record field for. Developer fields
//!   without a description are named `unknown_dev_field_<index>_<number>`, which
//!   no lookup reads.
//! - Non-numeric `altitude` and `calories` values fail the import. Other unreadable
//!   sensor values stay absent.
//!
//! Diving extraction is based on Unabara by Arnaud Dupuis (GPL-2.0):
//! <https://github.com/arnauddupuis/unabara/blob/main/src/core/format_parsers/fit_parser.cpp>

use crate::activity::finalize::FinalizeActivityResponse;
use crate::activity::parsers::helpers::{js_number_to_string, safe_gear_value, safe_number};
use crate::activity::parsers::{
    format_timestamp, parse_bytes_path, seconds_between, smoothing_options, BAR_PER_ATMOSPHERE,
    TRACK_SMOOTHING,
};
use crate::activity::schema::{LapMarkers, RawActivity, RawActivityOptions, RawSample};
use crate::error::{CoreError, CoreResult};
use chrono::{DateTime, Utc};
use fitparser::de::{DecodeOption, FitObject, FitStreamProcessor};
use fitparser::profile::MesgNum;
use fitparser::{FitDataRecord, Value};
use serde_json::json;
use std::path::Path;

const DIVE_SUB_SPORTS: [&str; 6] = [
    "single_gas_diving",
    "multi_gas_diving",
    "gauge_diving",
    "apnea_diving",
    "apnea_hunting",
    "ccr_diving",
];
const CCR_DILUENT: &str = "closed_circuit_diluent";
/// FIT stores positions as signed 32-bit fractions of a half circle.
const DEGREES_PER_SEMICIRCLE: f64 = 180.0 / 2_147_483_648.0;
/// Bit-flag fields hold their numeric payload below these masks.
const LEFT_RIGHT_BALANCE_MASK: i64 = 0x7F;
const MESSAGE_INDEX_MASK: i64 = 0x0FFF;
/// Producers that declare the signed temperature field as `uint8` share only this range with it.
const UNSIGNED_TEMPERATURE_LIMIT: u8 = 127;
type SampleField = fn(&mut RawSample) -> &mut Option<f64>;

/// Carried-forward dive readings: record field, divisor, and raw sample field.
/// FIT reports one current ppO2 value in bar. Setpoints are never substituted.
const DIVE_READINGS: [(&str, f64, SampleField); 6] = [
    ("depth", 1.0, |sample| &mut sample.depth),
    ("next_stop_depth", 1.0, |sample| {
        &mut sample.first_stop_depth
    }),
    ("next_stop_time", 60.0, |sample| &mut sample.first_stop_time),
    ("time_to_surface", 60.0, |sample| {
        &mut sample.time_to_surface
    }),
    ("po2", BAR_PER_ATMOSPHERE, |sample| &mut sample.average_ppo2),
    ("temperature", 1.0, |sample| &mut sample.water_temperature),
];

/// Opens a native FIT file and returns the shared finalized activity response.
pub fn parse_fit_activity_path(
    path: &Path,
    repo_root: Option<&Path>,
) -> CoreResult<FinalizeActivityResponse> {
    parse_bytes_path(path, repo_root, "FIT", extract_fit_activity)
}

/// Extracts one canonical raw sample per `record` message from FIT file bytes.
pub fn extract_fit_activity(bytes: &[u8], file_name: &str) -> CoreResult<RawActivity> {
    let messages = decode_messages(bytes)?;
    let records = of_kind(&messages, MesgNum::Record).collect::<Vec<_>>();
    if records.is_empty() {
        return Err(CoreError::Activity(
            "The FIT file does not contain any record messages.".into(),
        ));
    }

    let session = of_kind(&messages, MesgNum::Session).next();
    let session_field = |name| session.and_then(|session| field(session, name));
    let file_id = of_kind(&messages, MesgNum::FileId).next();
    let device_product = file_id.and_then(|file_id| {
        ["product_name", "garmin_product", "favero_product"]
            .iter()
            .find_map(|name| field(file_id, name).and_then(text))
    });
    let timestamps = records
        .iter()
        .map(|record| timestamp_field(record, "timestamp"))
        .collect::<Vec<_>>();
    let (sync_time, lap_markers) = extract_lap_timing(&messages, timestamps[0])?;

    let sub_sport = session_field("sub_sport")
        .or_else(|| {
            of_kind(&messages, MesgNum::Sport)
                .next()
                .and_then(|sport| field(sport, "sub_sport"))
        })
        .and_then(text);
    let is_dive = sub_sport.is_some_and(|sub_sport| DIVE_SUB_SPORTS.contains(&sub_sport))
        || records
            .iter()
            .any(|record| field(record, "depth").is_some());

    let mut first_timestamp = None;
    let mut raw_samples = records
        .iter()
        .zip(&timestamps)
        .map(|(record, timestamp)| {
            let elapsed = elapsed_seconds(&mut first_timestamp, *timestamp);
            extract_record(record, *timestamp, elapsed)
        })
        .collect::<CoreResult<Vec<_>>>()?;
    if is_dive {
        apply_dive_samples(
            &mut raw_samples,
            &records,
            &timestamps,
            &messages,
            sub_sport,
        );
    }

    Ok(RawActivity {
        file_name: file_name.to_string(),
        file_format: "fit".to_string(),
        metadata: json!({
            "creator": device_product,
            "file_created_at": file_id
                .and_then(|file_id| timestamp_field(file_id, "time_created"))
                .map(format_timestamp),
            "source_device_manufacturer": file_id
                .and_then(|file_id| field(file_id, "manufacturer"))
                .and_then(enum_json),
            "source_device_product": device_product,
            "sport": session_field("sport").and_then(enum_json),
            "sub_sport": session_field("sub_sport").and_then(enum_json),
            "total_elapsed_time": session_field("total_elapsed_time").and_then(number),
            "total_timer_time": session_field("total_timer_time").and_then(number),
        }),
        sync_time,
        lap_markers,
        raw_samples,
        options: RawActivityOptions {
            skip_idle_gap_fill: is_dive,
            smoothing: smoothing_options(&TRACK_SMOOTHING),
        },
    })
}

/// Decodes the data section of the first FIT file, never reading its trailing checksum.
fn decode_messages(bytes: &[u8]) -> CoreResult<Vec<FitDataRecord>> {
    let parse_error = |error: fitparser::Error| {
        CoreError::Activity(format!("The FIT file could not be parsed: {error}"))
    };
    let mut processor = FitStreamProcessor::new();
    processor.add_option(DecodeOption::SkipHeaderCrcValidation);
    let (mut input, header) = processor.deserialize_next(bytes).map_err(parse_error)?;
    let FitObject::Header(header) = header else {
        return Err(CoreError::Activity(
            "The FIT file does not start with a header.".into(),
        ));
    };
    let data_size = header.data_size() as usize;
    let trailing_bytes = input
        .len()
        .checked_sub(data_size)
        .ok_or_else(|| CoreError::Activity("The FIT file data exceeds the file length.".into()))?;

    let mut messages = Vec::new();
    while input.len() > trailing_bytes {
        let (remaining, object) = processor.deserialize_next(input).map_err(parse_error)?;
        input = remaining;
        if let FitObject::DataMessage(message) = object {
            messages.push(processor.decode_message(message).map_err(parse_error)?);
        }
    }
    Ok(messages)
}

/// Reads one record into a sample, leaving dive fields for [`apply_dive_samples`].
fn extract_record(
    record: &FitDataRecord,
    timestamp: Option<DateTime<Utc>>,
    elapsed_seconds: Option<f64>,
) -> CoreResult<RawSample> {
    let metric = |name: &str| field(record, name).and_then(number);
    let first_metric = |names: &[&str]| {
        names
            .iter()
            .find_map(|name| field(record, name))
            .and_then(number)
    };
    let altitude = ["enhanced_altitude", "altitude"]
        .iter()
        .find_map(|name| Some((*name, field(record, name)?)));

    Ok(RawSample {
        air_pressure: metric("absolute_pressure"),
        barometric_altitude: altitude
            .map(|(name, value)| required_number(name, value))
            .transpose()?,
        calories: field(record, "calories")
            .map(|value| required_number("calories", value))
            .transpose()?,
        cadence: metric("cadence"),
        core_temperature: metric("core_temperature"),
        distance: metric("distance"),
        elapsed_seconds,
        elevation: altitude.and_then(|(_, value)| number(value)),
        engine_power: metric("motor_power"),
        g_force: first_metric(&["g_force", "gforce"]),
        gear_position: ["gear_ratio", "gear", "front_gear"]
            .iter()
            .find_map(|name| field(record, name))
            .and_then(gear_value),
        gradient: metric("grade"),
        ground_contact_time: first_metric(&["ground_contact_time", "stance_time"]),
        heading: first_metric(&[
            "gps_heading",
            "compass_heading",
            "heading",
            "course_heading",
            "navigation_heading",
        ]),
        heartrate: metric("heart_rate"),
        latitude: metric("position_lat").map(|value| value * DEGREES_PER_SEMICIRCLE),
        left_right_balance: field(record, "left_right_balance")
            .and_then(|value| masked_flag_value(value, LEFT_RIGHT_BALANCE_MASK))
            .map(|value| value as f64),
        longitude: metric("position_long").map(|value| value * DEGREES_PER_SEMICIRCLE),
        pace: metric("pace"),
        power: metric("power"),
        speed: field(record, "enhanced_speed")
            .or_else(|| field(record, "speed"))
            .and_then(number),
        stride_length: first_metric(&["stride_length", "step_length"]),
        stroke_rate: first_metric(&["stroke_rate", "running_cadence"]),
        temperature: record_field(record, "temperature").and_then(number),
        timestamp: timestamp.map(format_timestamp),
        torque: metric("torque"),
        vertical_oscillation: metric("vertical_oscillation"),
        vertical_speed: metric("vertical_speed"),
        ..RawSample::default()
    })
}

/// Rebases a record to the first record with a timestamp. Records before it read as time zero.
fn elapsed_seconds(
    first_timestamp: &mut Option<DateTime<Utc>>,
    timestamp: Option<DateTime<Utc>>,
) -> Option<f64> {
    match timestamp {
        Some(timestamp) => Some(seconds_between(
            *first_timestamp.get_or_insert(timestamp),
            timestamp,
        )),
        None => first_timestamp.is_none().then_some(0.0),
    }
}

/// Missing timestamps leave lap timing unavailable rather than merging separate laps.
fn extract_lap_timing(
    messages: &[FitDataRecord],
    first_record_time: Option<DateTime<Utc>>,
) -> CoreResult<(Option<String>, LapMarkers)> {
    let starts = of_kind(messages, MesgNum::Lap)
        .map(|lap| timestamp_field(lap, "start_time"))
        .collect::<Option<Vec<_>>>();
    let (Some(starts), Some(first_record_time)) = (starts, first_record_time) else {
        return Ok((None, LapMarkers::None));
    };
    if starts.is_empty() {
        return Ok((None, LapMarkers::None));
    }
    if starts.windows(2).any(|pair| pair[1] <= pair[0]) {
        return Err(CoreError::Activity(
            "FIT lap start_time values must be in strictly increasing order.".into(),
        ));
    }

    // Keep the recording at t=0 and shorten the lap in progress at the first record.
    // Earlier laps have no recorded samples and do not extend the activity.
    let first_observed_lap = starts
        .iter()
        .rposition(|start| *start <= first_record_time)
        .unwrap_or(0);
    let markers = starts[first_observed_lap..]
        .iter()
        .map(|start| seconds_between(first_record_time, *start).max(0.0))
        .collect();
    Ok((
        Some(format_timestamp(first_record_time)),
        LapMarkers::BeaconMarkers(markers),
    ))
}

#[derive(Clone, Copy)]
struct DiveGas<'a> {
    mode: Option<&'a str>,
    fraction_o2: Option<f64>,
    fraction_he: Option<f64>,
}

/// Collects enabled gases by message index in first-seen order. A gas without an index uses
/// its position among all gas messages, and a repeated index replaces the earlier gas.
fn extract_dive_gases(messages: &[FitDataRecord]) -> Vec<(i64, DiveGas<'_>)> {
    let mut gases: Vec<(i64, DiveGas<'_>)> = Vec::new();
    for (position, message) in of_kind(messages, MesgNum::DiveGas).enumerate() {
        if field(message, "status").and_then(text) == Some("disabled") {
            continue;
        }
        let content = |name| field(message, name).and_then(number);
        let oxygen = content("oxygen_content");
        let helium = content("helium_content");
        let mix = oxygen.zip(helium).filter(|(oxygen, helium)| {
            *oxygen >= 0.0 && *helium >= 0.0 && oxygen + helium <= 100.0
        });
        let gas = DiveGas {
            mode: field(message, "mode").and_then(text),
            fraction_o2: mix.map(|(oxygen, _)| oxygen / 100.0),
            fraction_he: mix.map(|(_, helium)| helium / 100.0),
        };
        let index = field(message, "message_index")
            .and_then(|value| masked_flag_value(value, MESSAGE_INDEX_MASK))
            .unwrap_or(position as i64);
        match gases.iter_mut().find(|(existing, _)| *existing == index) {
            Some((_, existing)) => *existing = gas,
            None => gases.push((index, gas)),
        }
    }
    gases
}

/// Carries sparse dive readings and the active gas forward across records, and inserts a
/// sample at each gas switch that falls strictly between two records.
fn apply_dive_samples(
    samples: &mut Vec<RawSample>,
    records: &[&FitDataRecord],
    timestamps: &[Option<DateTime<Utc>>],
    messages: &[FitDataRecord],
    sub_sport: Option<&str>,
) {
    let gases = extract_dive_gases(messages);
    let mut current_gas = gases
        .iter()
        .map(|(_, gas)| *gas)
        .find(|gas| (sub_sport == Some("ccr_diving")) == (gas.mode == Some(CCR_DILUENT)));
    let mut switches = of_kind(messages, MesgNum::Event)
        .filter(|event| field(event, "event").and_then(text) == Some("dive_gas_switched"))
        .filter_map(|event| Some((timestamp_field(event, "timestamp")?, event)))
        .collect::<Vec<_>>();
    switches.sort_by_key(|(timestamp, _)| *timestamp);
    let mut pending_switches = switches.into_iter().peekable();
    let mut carried = RawSample::default();
    let mut switch_samples = Vec::new();

    for ((sample, record), timestamp) in samples.iter_mut().zip(records).zip(timestamps) {
        while let Some((switch_time, event)) = pending_switches
            .next_if(|(switch_time, _)| timestamp.is_some_and(|time| *switch_time <= time))
        {
            current_gas = field(event, "data").and_then(number).and_then(|data| {
                gases
                    .iter()
                    .find(|(index, _)| *index as f64 == data)
                    .map(|(_, gas)| *gas)
            });
            let is_between_records = timestamps[0].is_some_and(|first| switch_time > first)
                && timestamp.is_some_and(|time| switch_time < time);
            if let (Some(gas), true) = (current_gas, is_between_records) {
                switch_samples.push(RawSample {
                    timestamp: Some(format_timestamp(switch_time)),
                    elapsed_seconds: timestamps[0].map(|first| seconds_between(first, switch_time)),
                    fraction_o2: gas.fraction_o2,
                    fraction_he: gas.fraction_he,
                    ..RawSample::default()
                });
            }
        }
        for (name, divisor, reading) in DIVE_READINGS {
            if let Some(value) = record_field(record, name) {
                *reading(&mut carried) = number(value).map(|value| value / divisor);
            }
            *reading(sample) = *reading(&mut carried);
        }
        sample.fraction_o2 = current_gas.and_then(|gas| gas.fraction_o2);
        sample.fraction_he = current_gas.and_then(|gas| gas.fraction_he);
    }

    samples.append(&mut switch_samples);
    samples.sort_by(|left, right| {
        left.elapsed_seconds
            .unwrap_or(0.0)
            .total_cmp(&right.elapsed_seconds.unwrap_or(0.0))
    });
}

fn of_kind(messages: &[FitDataRecord], kind: MesgNum) -> impl Iterator<Item = &FitDataRecord> + '_ {
    messages
        .iter()
        .filter(move |message| message.kind() == kind)
}

/// The last field of a name wins: developer fields follow native fields in decoded order.
fn field<'a>(message: &'a FitDataRecord, name: &str) -> Option<&'a Value> {
    message
        .fields()
        .iter()
        .rev()
        .find(|field| field.name() == name)
        .map(|field| field.value())
}

/// Reads a record field, treating temperatures outside the shared unsigned range as absent.
fn record_field<'a>(record: &'a FitDataRecord, name: &str) -> Option<&'a Value> {
    field(record, name).filter(|value| {
        name != "temperature"
            || !matches!(value, Value::UInt8(value) if *value >= UNSIGNED_TEMPERATURE_LIMIT)
    })
}

fn timestamp_field(message: &FitDataRecord, name: &str) -> Option<DateTime<Utc>> {
    match field(message, name)? {
        Value::Timestamp(timestamp) => Some(timestamp.to_utc()),
        _ => None,
    }
}

/// Reads a finite number. A field written with more elements than its profile type
/// reads as its first element, and negative zero reads as zero.
fn number(value: &Value) -> Option<f64> {
    let number = match value {
        Value::String(text) => safe_number(text)?,
        Value::Array(values) => return number(values.first()?),
        value => TryInto::<f64>::try_into(value.clone()).ok()?,
    };
    number
        .is_finite()
        .then_some(if number == 0.0 { 0.0 } else { number })
}

fn required_number(name: &str, value: &Value) -> CoreResult<f64> {
    number(value).ok_or_else(|| {
        CoreError::Activity(format!("FIT record.{name} must be numeric when present"))
    })
}

fn gear_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => safe_gear_value(text),
        value => number(value).map(js_number_to_string),
    }
}

/// Reads the masked payload of a bit-flag field. The decoder names a value only when it
/// equals one flag exactly, and only the mask itself has payload bits set.
fn masked_flag_value(value: &Value, mask: i64) -> Option<i64> {
    match value {
        Value::String(name) => Some(if name == "mask" { mask } else { 0 }),
        value => number(value).map(|number| number as i64 & mask),
    }
}

fn text(value: &Value) -> Option<&str> {
    match value {
        Value::String(text) => Some(text),
        _ => None,
    }
}

/// An enum named by the profile is text. A value the profile does not name stays numeric.
fn enum_json(value: &Value) -> Option<serde_json::Value> {
    match value {
        Value::String(text) => (!text.is_empty()).then(|| json!(text)),
        value => number(value)
            .filter(|number| *number != 0.0)
            .map(|number| json!(number as i64)),
    }
}
