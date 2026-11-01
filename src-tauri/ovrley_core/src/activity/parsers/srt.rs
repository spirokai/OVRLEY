//! Native DJI SRT telemetry extraction through the shared raw-activity finalizer.
//!
//! The line grammar and per-model corrections are based on DJI_SRT_Parser,
//! Copyright (c) 2018 Juan Irache (MIT): https://github.com/JuanIrache/DJI_SRT_Parser
//!
//! SRT is an external-system boundary and parsing is lenient: unreadable values stay
//! `None`, and only a file without any telemetry is rejected.
//!
//! - Camera clocks have no offset, so timestamps stay naive for the finalizer to read in
//!   the GPS-derived timezone. Only a `Z` suffix marks a timestamp as absolute.
//! - `elapsed_seconds` is the cue start rebased to the first cue, because the first timestamp
//!   is the activity's time zero. Files without cue times leave it empty and the finalizer
//!   derives it from timestamps.
//! - Most formats write GPS tuples longitude first. Matrice 300 writes latitude first and
//!   marks the third value as `0.0M`.
//! - Coordinates get a short zero-phase moving average, because cameras repeat each GPS
//!   reading across many video-rate cues. A 1 Hz file is left as written.
//! - Elevation prefers absolute altitude, then height above home, then barometer. `D` is
//!   the distance from home, not distance travelled, so it is not read.

use crate::activity::finalize::smoothing::{smoothing_window_for_seconds, zero_phase_smooth};
use crate::activity::finalize::FinalizeActivityResponse;
use crate::activity::parsers::helpers::{all_digits, js_trim, safe_number};
use crate::activity::parsers::{decode_file_text, parse_bytes_path, smoothing_options};
use crate::activity::schema::{RawActivity, RawActivityOptions, RawSample};
use crate::error::{CoreError, CoreResult};
use chrono::NaiveDate;
use regex::{Captures, Regex};
use serde_json::json;
use std::collections::HashMap;
use std::f64::consts::{FRAC_PI_2, PI};
use std::path::Path;
use std::sync::LazyLock;

/// Bracketed `[shutter: x]` is in seconds, unlike the reciprocal `Shutter`, `TV`, and `SS`.
const BRACKETED_SHUTTER: &str = "shutter";
/// Moving-average window for coordinates, run forward and backward so nothing shifts in time.
const GPS_SMOOTHING_SECONDS: f64 = 0.3;
/// Radian coordinates spanning more than this are not a plausible drone flight.
const MAX_RADIAN_SPAN: f64 = 0.001;

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("SRT patterns are valid")
}

static TIMECODE: LazyLock<Regex> =
    LazyLock::new(|| regex(r"([0-9]{2}):([0-9]{2}):([0-9]{2}),([0-9]{3})\s*-->"));
/// `KEY (a, b, c)` groups, or a bare `(a, b, c)` such as the second group of `LeCurve(64) (...)`.
static TUPLE: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?:\b([A-Za-z_][A-Za-z_.]*)\s?)?\(([^)]*)\)"));
/// `[iso : 100]`, `ISO:100`, `ISO 100`, `F/2.8`, `IR:F2.8`, and `H 85.80m` style fields.
static FIELD: LazyLock<Regex> = LazyLock::new(|| {
    regex(r"\b([A-Za-z_]+(?:\.[A-Za-z_]+)?)(?:\s*:\s*[A-Za-z]?|\s+|/)([-+]?[0-9./]+)\w{0,3}\b")
});
/// Dates use `-` or `.`, may be unpadded, and carry fractions as `.373`, `,745,078`, or glued digits.
static DATE: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r"([0-9]{4})[-.]([0-9]{1,2})[-.]([0-9]{1,2})[ T]([0-9]{1,2}):([0-9]{2}):([0-9]{2})(?:[.,]?([0-9]+))?(Z)?",
    )
});
static FPV_DATE: LazyLock<Regex> = LazyLock::new(|| {
    regex(r"[0-9]{4}-[0-9]{1,2}-[0-9]{1,2} [0-9]{1,2}:[0-9]{2}:[0-9]{2}.[0-9]{3}")
});
static FPV_ALTITUDE: LazyLock<Regex> = LazyLock::new(|| regex(r"\[altitude: -?[0-9].*\]"));

#[derive(Clone, Copy, Default)]
struct Position {
    latitude: Option<f64>,
    longitude: Option<f64>,
    altitude: Option<f64>,
}

#[derive(Default)]
struct Packet {
    elapsed_seconds: Option<f64>,
    timestamp: Option<String>,
    fields: HashMap<String, String>,
    gps: Option<Position>,
    rtk: Option<Position>,
}

/// Opens a native SRT file and returns the shared finalized activity response.
pub fn parse_srt_activity_path(
    path: &Path,
    repo_root: Option<&Path>,
) -> CoreResult<FinalizeActivityResponse> {
    parse_bytes_path(path, repo_root, "SRT", extract_srt_activity)
}

/// Extracts one canonical raw sample per telemetry cue from SRT file bytes.
pub fn extract_srt_activity(bytes: &[u8], file_name: &str) -> CoreResult<RawActivity> {
    let text = decode_file_text(bytes);
    let packets = parse_packets(&text);
    if packets.is_empty() {
        return Err(CoreError::Activity(
            "The SRT file does not contain any telemetry.".into(),
        ));
    }
    let coordinates_in_radians = coordinates_seem_radians(&packets);
    let altitude_scale = if altitude_is_divided_by_ten(&text) {
        10.0
    } else {
        1.0
    };
    let mut raw_samples: Vec<RawSample> = packets
        .iter()
        .map(|packet| packet.raw_sample(coordinates_in_radians, altitude_scale))
        .collect();
    rebase_cue_times(&mut raw_samples);
    smooth_coordinates(&mut raw_samples);
    Ok(RawActivity {
        file_name: file_name.to_string(),
        file_format: "srt".to_string(),
        metadata: json!({}),
        sync_time: None,
        lap_markers: Default::default(),
        raw_samples,
        options: RawActivityOptions {
            skip_idle_gap_fill: true,
            smoothing: smoothing_options(&[
                ("pace", "zero_phase_ma", 5.0),
                ("speed", "zero_phase_ma", 0.5),
                ("vertical_speed", "zero_phase_ma", 1.0),
                ("heading", "circular_ema", 0.5),
            ]),
        },
    })
}

fn rebase_cue_times(samples: &mut [RawSample]) {
    if let Some(first) = samples.iter().find_map(|sample| sample.elapsed_seconds) {
        for elapsed in samples
            .iter_mut()
            .filter_map(|sample| sample.elapsed_seconds.as_mut())
        {
            *elapsed -= first;
        }
    }
}

/// Smooths each coordinate over the cue cadence. Files without cue times have no cadence
/// here and stay as written.
fn smooth_coordinates(samples: &mut [RawSample]) {
    let cue_times_ms: Vec<f64> = samples
        .iter()
        .filter_map(|sample| sample.elapsed_seconds)
        .map(|seconds| seconds * 1000.0)
        .collect();
    let window = smoothing_window_for_seconds(&cue_times_ms, GPS_SMOOTHING_SECONDS);
    let smooth = |coordinate: fn(&RawSample) -> Option<f64>| {
        let series: Vec<_> = samples.iter().map(coordinate).collect();
        zero_phase_smooth(&series, window)
    };
    let latitudes = smooth(|sample| sample.latitude);
    let longitudes = smooth(|sample| sample.longitude);
    for ((sample, latitude), longitude) in samples.iter_mut().zip(latitudes).zip(longitudes) {
        sample.latitude = latitude;
        sample.longitude = longitude;
    }
}

/// Reads cues line by line, because some formats omit the cue time or the blank separators.
/// Cues without any telemetry are dropped, which DJI files often end with.
fn parse_packets(text: &str) -> Vec<Packet> {
    let mut packets: Vec<Packet> = Vec::new();
    for line in text.split(['\n', '\r']).map(js_trim) {
        if !line.is_empty() && all_digits(line) {
            packets.push(Packet::default());
        } else if let Some(packet) = packets.last_mut() {
            if line.contains("-->") {
                packet.elapsed_seconds = parse_timecode(line);
            } else {
                packet.read_telemetry_line(line);
            }
        }
    }
    packets.retain(Packet::has_telemetry);
    packets
}

fn parse_timecode(line: &str) -> Option<f64> {
    let captures = TIMECODE.captures(line)?;
    let part = |index: usize| captures[index].parse::<f64>().ok();
    Some(part(1)? * 3600.0 + part(2)? * 60.0 + part(3)? + part(4)? / 1000.0)
}

/// Reads a timestamp without moving it to any timezone: see the module notes.
fn parse_timestamp(captures: &Captures) -> Option<String> {
    let part = |index: usize| captures[index].parse::<u32>().ok();
    let millis = captures.get(7).map_or(Some(0), |fraction| {
        format!("{:0<3}", fraction.as_str())[..3].parse().ok()
    })?;
    let timestamp = NaiveDate::from_ymd_opt(captures[1].parse().ok()?, part(2)?, part(3)?)?
        .and_hms_milli_opt(part(4)?, part(5)?, part(6)?, millis)?
        .format("%Y-%m-%dT%H:%M:%S%.3f");
    let suffix = captures.get(8).map_or("", |zulu| zulu.as_str());
    Some(format!("{timestamp}{suffix}"))
}

fn parse_position_tuple(tuple: &str) -> Position {
    let items: Vec<&str> = tuple.split(',').map(js_trim).collect();
    if let Some(position) = parse_hemisphere_position(&items) {
        return position;
    }
    let item = |index: usize| items.get(index).copied().unwrap_or_default();
    let third = item(2);
    let (latitude, longitude) = if third.ends_with('M') {
        (item(0), item(1))
    } else {
        (item(1), item(0))
    };
    Position {
        latitude: safe_number(latitude),
        longitude: safe_number(longitude),
        altitude: third.strip_suffix('m').and_then(safe_number),
    }
}

/// Reads the Autel `W:122.1,N:37.4,100.1m` form.
fn parse_hemisphere_position(items: &[&str]) -> Option<Position> {
    let mut position = Position::default();
    for item in items {
        if let Some((hemisphere, value)) = item.split_once(':') {
            let value = safe_number(value);
            match hemisphere {
                "N" => position.latitude = value,
                "S" => position.latitude = value.map(|value| -value),
                "E" => position.longitude = value,
                "W" => position.longitude = value.map(|value| -value),
                _ => {}
            }
        } else if let Some(altitude) = item.strip_suffix('m') {
            position.altitude = safe_number(altitude);
        }
    }
    (position.latitude.is_some() && position.longitude.is_some()).then_some(position)
}

/// Some Mavic Air 2 firmware writes radians under the misspelled `longtitude` key. A whole
/// flight inside the radian range and a tiny span is taken to be radians, not degrees.
fn coordinates_seem_radians(packets: &[Packet]) -> bool {
    let mut latitudes = (f64::INFINITY, f64::NEG_INFINITY);
    let mut longitudes = latitudes;
    let mut found = false;
    for packet in packets {
        let nonzero = |key| packet.number(key).filter(|value| *value != 0.0);
        if let (Some(latitude), Some(longitude)) = (nonzero("LATITUDE"), nonzero("LONGTITUDE")) {
            latitudes = (latitudes.0.min(latitude), latitudes.1.max(latitude));
            longitudes = (longitudes.0.min(longitude), longitudes.1.max(longitude));
            found = true;
        }
    }
    found
        && longitudes.1.abs() <= PI
        && latitudes.1.abs() <= FRAC_PI_2
        && latitudes.1 - latitudes.0 <= MAX_RADIAN_SPAN
        && longitudes.1 - longitudes.0 <= MAX_RADIAN_SPAN
}

/// DJI FPV is recognizable by this combination and writes a tenth of its real altitude.
fn altitude_is_divided_by_ten(text: &str) -> bool {
    text.contains("font size=\"28\"") && FPV_DATE.is_match(text) && FPV_ALTITUDE.is_match(text)
}

/// Reads `-1/3` and `0.7` style exposure values.
fn parse_ratio(raw: &str) -> Option<f64> {
    match raw.split_once('/') {
        Some((numerator, denominator)) => {
            let denominator = safe_number(denominator).filter(|value| *value != 0.0)?;
            Some(safe_number(numerator)? / denominator)
        }
        None => safe_number(raw),
    }
}

fn parse_reciprocal(raw: &str) -> Option<f64> {
    let denominator = safe_number(raw.strip_prefix("1/").unwrap_or(raw))?;
    (denominator > 0.0).then(|| 1.0 / denominator)
}

fn positive(value: Option<f64>) -> Option<f64> {
    value.filter(|value| *value > 0.0)
}

impl Packet {
    fn has_telemetry(&self) -> bool {
        self.timestamp.is_some()
            || !self.fields.is_empty()
            || self.gps.is_some()
            || self.rtk.is_some()
    }

    fn read_telemetry_line(&mut self, line: &str) {
        // Tuples leave first so their contents, such as Mini SE's `Index (Ev:8371)`, stay out
        // of the scalar fields.
        let scalars = TUPLE.replace_all(line, |captures: &Captures| {
            match captures.get(1).map(|key| key.as_str()) {
                Some(key) if key.eq_ignore_ascii_case("GPS") => {
                    self.gps = Some(parse_position_tuple(&captures[2]));
                }
                Some(key) if key.eq_ignore_ascii_case("RTK") => {
                    self.rtk = Some(parse_position_tuple(&captures[2]));
                }
                _ => {}
            }
            ""
        });
        for captures in FIELD.captures_iter(&scalars) {
            let key = match &captures[1] {
                BRACKETED_SHUTTER => BRACKETED_SHUTTER.to_string(),
                key => key.replace('.', "_").to_ascii_uppercase(),
            };
            self.fields.insert(key, captures[2].to_string());
        }
        if let Some(captures) = DATE.captures(&scalars) {
            self.timestamp = parse_timestamp(&captures);
        }
    }

    fn field(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(String::as_str)
    }

    fn number(&self, key: &str) -> Option<f64> {
        self.field(key).and_then(safe_number)
    }

    /// Returns the first readable value among alternative names for one measurement.
    fn first_number(&self, keys: &[&str]) -> Option<f64> {
        keys.iter().find_map(|key| self.number(key))
    }

    /// Autel can also write `N:`, `S:`, `E:`, and `W:` fields beside `NUM`.
    fn hemisphere_coordinates(&self) -> Option<(f64, f64)> {
        self.field("NUM")?;
        let signed = |positive: &str, negative: &str| {
            self.number(positive)
                .or_else(|| self.number(negative).map(|value| -value))
        };
        Some((signed("N", "S")?, signed("E", "W")?))
    }

    /// An all-zero fix is what DJI writes before GPS lock, not a position.
    fn coordinates(&self, in_radians: bool) -> (Option<f64>, Option<f64>) {
        let (latitude, longitude) = match self.hemisphere_coordinates() {
            Some((latitude, longitude)) => (Some(latitude), Some(longitude)),
            None => {
                let tuple = self.gps.or(self.rtk).unwrap_or_default();
                (
                    self.number("LATITUDE").or(tuple.latitude),
                    self.first_number(&["LONGITUDE", "LONGTITUDE"])
                        .or(tuple.longitude),
                )
            }
        };
        if latitude == Some(0.0) && longitude == Some(0.0) {
            return (None, None);
        }
        if in_radians {
            return (
                latitude.map(f64::to_degrees),
                longitude.map(f64::to_degrees),
            );
        }
        (latitude, longitude)
    }

    fn elevation(&self, altitude_scale: f64) -> Option<f64> {
        self.number("ABS_ALT")
            .or_else(|| self.number("ALTITUDE").map(|value| value * altitude_scale))
            .or_else(|| self.number("H"))
            .or_else(|| self.gps.or(self.rtk).and_then(|position| position.altitude))
            .or_else(|| self.first_number(&["BAROMETER", "REL_ALT", "HB"]))
    }

    fn shutter_speed(&self) -> Option<f64> {
        if let Some(raw) = self.field(BRACKETED_SHUTTER) {
            return if raw.contains('/') {
                parse_reciprocal(raw)
            } else {
                positive(safe_number(raw))
            };
        }
        ["SHUTTER", "TV", "SS"]
            .iter()
            .find_map(|key| self.field(key))
            .and_then(parse_reciprocal)
    }

    /// Some cameras write f/2.8 as `280`.
    fn aperture(&self) -> Option<f64> {
        let value = self
            .first_number(&["FNUM", "IR", "F"])
            .or_else(|| self.hemisphere_coordinates().and(self.number("NUM")));
        positive(value).map(|value| if value > 50.0 { value / 100.0 } else { value })
    }

    /// Some cameras write 22.4 mm as `224`.
    fn focal_length(&self) -> Option<f64> {
        let raw = self.field("FOCAL_LEN")?;
        let value = positive(safe_number(raw))?;
        let is_scaled = raw.len() >= 3 && all_digits(raw);
        Some(if is_scaled { value / 10.0 } else { value })
    }

    fn raw_sample(&self, coordinates_in_radians: bool, altitude_scale: f64) -> RawSample {
        let (latitude, longitude) = self.coordinates(coordinates_in_radians);
        RawSample {
            elapsed_seconds: self.elapsed_seconds,
            timestamp: self.timestamp.clone(),
            latitude,
            longitude,
            elevation: self.elevation(altitude_scale),
            speed: self.number("H_S"),
            vertical_speed: self.number("V_S"),
            iso: positive(self.number("ISO")),
            aperture: self.aperture(),
            shutter_speed: self.shutter_speed(),
            focal_length: self.focal_length(),
            ev: self.field("EV").and_then(parse_ratio),
            color_temperature: positive(self.first_number(&["CT", "COLORTEMP"])),
            ..RawSample::default()
        }
    }
}
