//! Native IGC flight-log extraction through the shared raw-activity finalizer.
//!
//! The record grammar and manufacturer table are based on igc-parser and
//! flight-recorder-manufacturers, Copyright (c) 2017 Tobias Bieniek (MIT):
//! https://github.com/Turbo87/igc-parser
//!
//! IGC is an external-system boundary and parsing is lenient: a malformed line is
//! recorded in `metadata.parse_errors` and the remaining fixes still import.
//!
//! - B-record GPS fixes become one RawSample each.
//! - GPS altitude maps to `elevation`; pressure altitude is not emitted.
//! - Void (`V`) fixes keep their timestamp but clear position and altitude.
//! - GSP centi-km/h becomes `speed` m/s, TRT degrees `heading`, VAT centi-m/s
//!   `vertical_speed` m/s, and OAT `temperature` with a file-level scale.

use crate::activity::finalize::{finalize_raw_activity, FinalizeActivityResponse};
use crate::activity::parsers::raw_values::{is_js_whitespace, js_trim, safe_number};
use crate::activity::parsers::{decode_file_text, read_activity_file};
use crate::activity::schema::{RawActivity, RawActivityOptions, RawSample, SmoothingOption};
use crate::error::{CoreError, CoreResult};
use chrono::{DateTime, Days, NaiveDate, SecondsFormat};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

const ONE_HOUR_MS: i64 = 60 * 60 * 1000;
const ONE_DAY_MS: i64 = 24 * ONE_HOUR_MS;
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
const INVALID_TIME: &str = "Invalid time value";

const MANUFACTURERS: [(&str, &str); 35] = [
    ("ACT", "Aircotec"),
    ("CAM", "Cambridge Aero Instruments"),
    ("CNI", "ClearNav Instruments"),
    ("DSX", "Data Swan/DSX"),
    ("EWA", "EW Avionics"),
    ("FIL", "Filser"),
    ("FLA", "Flarm"),
    ("FLY", "Flytech"),
    ("GCS", "Garrecht"),
    ("IMI", "IMI Gliding Equipment"),
    ("LGS", "Logstream"),
    ("LXN", "LX Navigation"),
    ("LXV", "LXNAV"),
    ("NAV", "Naviter"),
    ("NTE", "New Technologies"),
    ("NKL", "Nielsen Kellerman"),
    ("PES", "Peschges"),
    ("PFE", "PressFinish Electronics"),
    ("PRT", "Print Technik"),
    ("SCH", "Scheffel"),
    ("SDI", "Streamline Data Instruments"),
    ("TRI", "Triadis Engineering GmbH"),
    ("ZAN", "Zander"),
    ("XCS", "XCSoar"),
    ("XLK", "LK8000"),
    ("XGD", "GpsDump"),
    ("XCM", "SeeYou Recorder"),
    ("XFH", "Flyskyhy"),
    ("XCT", "XCTrack"),
    ("XFM", "Flymaster Live"),
    ("XTR", "XCTracer"),
    ("XSB", "SkyBean"),
    ("XSD", "leGPSBip"),
    ("XLF", "Logfly"),
    ("XLM", "Loctome"),
];

/// Text headers whose values are unused but whose malformed lines are still reported,
/// paired with the record name used in that report.
const VALIDATED_HEADERS: [(&str, &str); 10] = [
    ("CM2", "CM2"),
    ("GID", "GID"),
    ("CID", "GTY"),
    ("CCL", "GID"),
    ("SIT", "SIT"),
    ("RFW", "RFW"),
    ("RHW", "RHW"),
    ("DTM", "FDT"),
    ("ALG", "ALG"),
    ("ALP", "ALP"),
];

/// One I-record entry. Unreadable column numbers leave the field empty on every fix.
struct FixExtension {
    start: Option<i64>,
    end: Option<i64>,
    code: String,
}

struct Fix {
    /// `None` when the date header and record time do not form a real instant.
    timestamp: Option<i64>,
    valid: bool,
    latitude: f64,
    longitude: f64,
    gps_altitude: Option<f64>,
    extensions: HashMap<String, String>,
}

#[derive(Default)]
struct IgcLog {
    logger_manufacturer: Option<String>,
    date: Option<String>,
    date_epoch_ms: Option<i64>,
    pilot: Option<String>,
    glider_type: Option<String>,
    logger_type: Option<String>,
    timezone: Option<f64>,
    has_task: bool,
    fix_extensions: Vec<FixExtension>,
    previous_timestamp: Option<i64>,
    line_number: usize,
    fixes: Vec<Fix>,
    errors: Vec<String>,
}

/// Opens a native IGC file and returns the shared finalized activity response.
pub fn parse_igc_activity_path(
    path: &Path,
    repo_root: Option<&Path>,
) -> CoreResult<FinalizeActivityResponse> {
    let (bytes, file_name) = read_activity_file(path, "IGC")?;
    finalize_raw_activity(&extract_igc_activity(&bytes, file_name)?, repo_root)
}

/// Extracts one canonical raw sample per B-record fix from IGC file bytes.
pub fn extract_igc_activity(bytes: &[u8], file_name: &str) -> CoreResult<RawActivity> {
    let text = decode_file_text(bytes);
    let mut log = IgcLog::default();
    for line in text.split('\n') {
        log.line_number += 1;
        if let Err(error) = log.process_line(js_trim(line)) {
            log.errors.push(error);
        }
    }

    let logger_manufacturer = log
        .logger_manufacturer
        .take()
        .ok_or_else(|| CoreError::Activity("Missing A record".into()))?;
    let date = log
        .date
        .take()
        .ok_or_else(|| CoreError::Activity("Missing HFDTE record".into()))?;
    let first_fix = log.fixes.first().ok_or_else(|| {
        CoreError::Activity("The IGC file does not contain any fix records.".into())
    })?;
    let first_timestamp = first_fix.timestamp.ok_or_else(invalid_time)?;
    let oat_divisor = infer_oat_divisor(&log.fixes);
    let raw_samples = log
        .fixes
        .iter()
        .map(|fix| raw_sample_from_fix(fix, first_timestamp, oat_divisor))
        .collect::<CoreResult<Vec<_>>>()?;

    let pilot = log.pilot.filter(|pilot| !pilot.is_empty());
    let activity_name = match pilot {
        Some(pilot) => format!("{date} - {pilot}"),
        None => date.clone(),
    };
    Ok(RawActivity {
        file_name: file_name.to_string(),
        file_format: "igc".to_string(),
        metadata: json!({
            "activity_name": activity_name,
            "date": date,
            "glider_type": log.glider_type.filter(|value| !value.is_empty()),
            "timezone": timezone_metadata(log.timezone),
            "logger_manufacturer": logger_manufacturer,
            "logger_type": log.logger_type.filter(|value| !value.is_empty()),
            "parse_errors": (!log.errors.is_empty()).then_some(log.errors),
        }),
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
                        window_seconds: 0.5,
                    },
                ),
            ]),
        },
    })
}

fn invalid_time() -> CoreError {
    CoreError::Activity(INVALID_TIME.into())
}

/// A zero offset carries no information. Whole hours stay JSON integers.
fn timezone_metadata(timezone: Option<f64>) -> Value {
    match timezone.filter(|hours| *hours != 0.0 && hours.is_finite()) {
        Some(hours) if hours.fract() == 0.0 && hours.abs() <= MAX_SAFE_INTEGER => {
            json!(hours as i64)
        }
        Some(hours) => json!(hours),
        None => Value::Null,
    }
}

/// Logger vendors encode OAT inconsistently, so the scale is decided per file:
/// `24` is 24 C, `0244` is 24.4 C, and `2400` is 24.00 C.
fn infer_oat_divisor(fixes: &[Fix]) -> f64 {
    let largest = fixes
        .iter()
        .filter_map(|fix| extension_number(fix, "OAT"))
        .fold(0.0_f64, |largest, value| largest.max(value.abs()));
    if largest >= 1000.0 {
        100.0
    } else if largest >= 100.0 {
        10.0
    } else {
        1.0
    }
}

fn extension_number(fix: &Fix, code: &str) -> Option<f64> {
    fix.extensions
        .get(code)
        .or_else(|| fix.extensions.get(&code.to_ascii_lowercase()))
        .and_then(|value| safe_number(value))
}

fn raw_sample_from_fix(fix: &Fix, first_timestamp: i64, oat_divisor: f64) -> CoreResult<RawSample> {
    let timestamp = fix.timestamp.ok_or_else(invalid_time)?;
    let timestamp_iso = DateTime::from_timestamp_millis(timestamp)
        .ok_or_else(invalid_time)?
        .to_rfc3339_opts(SecondsFormat::Millis, true);
    Ok(RawSample {
        timestamp: Some(timestamp_iso),
        elapsed_seconds: Some((timestamp - first_timestamp) as f64 / 1000.0),
        latitude: fix.valid.then_some(fix.latitude),
        longitude: fix.valid.then_some(fix.longitude),
        elevation: fix.gps_altitude.filter(|_| fix.valid),
        speed: extension_number(fix, "GSP").map(|value| value / 360.0),
        heading: extension_number(fix, "TRT"),
        vertical_speed: extension_number(fix, "VAT").map(|value| value / 100.0),
        temperature: extension_number(fix, "OAT").map(|value| value / oat_divisor),
        ..RawSample::default()
    })
}

impl IgcLog {
    fn process_line(&mut self, line: &str) -> Result<(), String> {
        match line.chars().next() {
            Some('B') => {
                let fix = self.parse_b_record(line)?;
                self.previous_timestamp = fix.timestamp;
                self.fixes.push(fix);
            }
            Some('K') => {
                self.previous_timestamp = self.parse_k_record(line)?;
            }
            Some('H') => self.process_header(line)?,
            Some('C') => self.process_task_line(line)?,
            Some('A') => self.logger_manufacturer = Some(self.parse_a_record(line)?),
            Some('I') => self.fix_extensions = self.parse_ij_record(line)?,
            Some('J') => {
                self.parse_ij_record(line)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn invalid(&self, record: &str, line: &str) -> String {
        format!("Invalid {record} at line {}: {line}", self.line_number)
    }

    fn process_header(&mut self, line: &str) -> Result<(), String> {
        match js_slice(line, 2, 5).as_str() {
            "DTE" => self.parse_date_header(line)?,
            "TZN" => self.timezone = Some(self.parse_timezone(line)?),
            "PLT" => self.pilot = Some(self.text_header("PLT", line)?),
            "GTY" => self.glider_type = Some(self.text_header("GTY", line)?),
            "FTY" => self.logger_type = Some(self.text_header("FTY", line)?),
            code => {
                if let Some((_, label)) = VALIDATED_HEADERS.iter().find(|(known, _)| *known == code)
                {
                    self.text_header(label, line)?;
                }
            }
        }
        Ok(())
    }

    fn parse_a_record(&self, line: &str) -> Result<String, String> {
        let id = line
            .get(1..4)
            .filter(|id| id.bytes().all(is_word_byte) && !has_line_terminator(&line[4..]))
            .ok_or_else(|| self.invalid("A record", line))?
            .to_ascii_uppercase();
        let name = MANUFACTURERS
            .iter()
            .find(|(code, _)| *code == id)
            .map(|(_, name)| name.to_string());
        Ok(name.unwrap_or(id))
    }

    fn parse_date_header(&mut self, line: &str) -> Result<(), String> {
        let digits = line
            .strip_prefix("HFDTE")
            .map(|rest| rest.strip_prefix("DATE:").unwrap_or(rest))
            .and_then(|rest| rest.get(..6))
            .filter(|digits| all_digits(digits))
            .ok_or_else(|| self.invalid("DTE header", line))?;
        let (day, month, year) = (&digits[..2], &digits[2..4], &digits[4..6]);
        let century = if matches!(&year[..1], "8" | "9") {
            "19"
        } else {
            "20"
        };
        let date = format!("{century}{year}-{month}-{day}");
        self.date_epoch_ms = date_epoch_ms(&date);
        self.date = Some(date);
        Ok(())
    }

    /// Reads `H<source><code>[label:]value`. The value follows the first colon.
    fn text_header(&self, label: &str, line: &str) -> Result<String, String> {
        let invalid = || self.invalid(&format!("{label} header"), line);
        let rest = header_rest(line).ok_or_else(invalid)?;
        let value = rest.split_once(':').map_or(rest, |(_, value)| value);
        Ok(js_trim(&value.replace('_', " ")).to_string())
    }

    /// Reads `H<source>TZN[label]:<hours>`, keeping the leading number of the value.
    fn parse_timezone(&self, line: &str) -> Result<f64, String> {
        let invalid = || self.invalid("TZN header", line);
        let value = header_rest(line)
            .and_then(|rest| rest.rsplit_once(':'))
            .map(|(_, value)| value)
            .filter(|value| {
                let unsigned = value.strip_prefix(['+', '-']).unwrap_or(value);
                !unsigned.is_empty()
                    && unsigned
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || byte == b'.')
            })
            .ok_or_else(invalid)?;
        let end = value
            .match_indices('.')
            .nth(1)
            .map_or(value.len(), |(index, _)| index);
        value[..end].parse().map_err(|_| invalid())
    }

    fn process_task_line(&mut self, line: &str) -> Result<(), String> {
        let bytes = line.as_bytes();
        if !self.has_task {
            let declaration_valid = bytes.len() >= 25
                && all_digit_bytes(&bytes[1..23])
                && bytes[23..25]
                    .iter()
                    .all(|byte| byte.is_ascii_digit() || *byte == b'-');
            if !declaration_valid {
                return Err(self.invalid("task declaration", line));
            }
            self.has_task = true;
            return Ok(());
        }
        let point_valid = bytes.len() >= 18
            && all_digit_bytes(&bytes[1..8])
            && matches!(bytes[8], b'N' | b'S')
            && all_digit_bytes(&bytes[9..17])
            && matches!(bytes[17], b'E' | b'W');
        if !point_valid {
            return Err(self.invalid("task point declaration", line));
        }
        Ok(())
    }

    fn parse_b_record(&self, line: &str) -> Result<Fix, String> {
        if self.date.is_none() {
            return Err("Missing HFDTE record before first B record".to_string());
        }
        if !is_b_record(line.as_bytes()) {
            return Err(self.invalid("B record", line));
        }
        let extensions: HashMap<String, String> = self
            .fix_extensions
            .iter()
            .map(|extension| {
                let value = match (extension.start, extension.end) {
                    (Some(start), Some(end)) => js_slice(line, start - 1, end),
                    _ => String::new(),
                };
                (extension.code.clone(), value)
            })
            .collect();
        let extra_digits = |code: &str| {
            extensions
                .get(code)
                .filter(|value| all_digits(value))
                .map_or("", String::as_str)
        };
        let latitude = coordinate(
            &line[7..9],
            &line[9..11],
            &line[11..14],
            extra_digits("LAD"),
            &line[14..15] == "S",
        );
        let longitude = coordinate(
            &line[15..18],
            &line[18..20],
            &line[20..23],
            extra_digits("LOD"),
            &line[23..24] == "W",
        );
        let gps_altitude = Some(&line[30..35])
            .filter(|altitude| *altitude != "00000")
            .and_then(safe_number);
        Ok(Fix {
            timestamp: self.timestamp(&line[1..7]),
            valid: &line[24..25] == "A",
            latitude,
            longitude,
            gps_altitude,
            extensions,
        })
    }

    fn parse_k_record(&self, line: &str) -> Result<Option<i64>, String> {
        if self.date.is_none() {
            return Err("Missing HFDTE record before first K record".to_string());
        }
        let time = line
            .get(1..7)
            .filter(|time| all_digits(time))
            .ok_or_else(|| self.invalid("K record", line))?;
        Ok(self.timestamp(time))
    }

    fn parse_ij_record(&self, line: &str) -> Result<Vec<FixExtension>, String> {
        let record = &line[..1];
        let invalid = || self.invalid(&format!("{record} record"), line);
        let bytes = line.as_bytes();
        let shape_valid = bytes.len() >= 10
            && all_digit_bytes(&bytes[1..7])
            && bytes[7..10].iter().all(u8::is_ascii_uppercase);
        if !shape_valid {
            return Err(invalid());
        }
        let count: i64 = line[1..3].parse().map_err(|_| invalid())?;
        if (line.encode_utf16().count() as i64) < 3 + count * 7 {
            return Err(invalid());
        }
        Ok((0..count)
            .map(|index| {
                let offset = 3 + index * 7;
                FixExtension {
                    start: js_parse_int(&js_slice(line, offset, offset + 2)),
                    end: js_parse_int(&js_slice(line, offset + 2, offset + 4)),
                    code: js_slice(line, offset + 4, offset + 7),
                }
            })
            .collect())
    }

    /// Combines the date header with a record time. A time more than an hour before
    /// the previous record is taken to belong to a following day.
    fn timestamp(&self, time: &str) -> Option<i64> {
        let hour: i64 = time[..2].parse().ok()?;
        let minute: i64 = time[2..4].parse().ok()?;
        let second: i64 = time[4..6].parse().ok()?;
        let in_range = hour < 24 && minute < 60 && second < 60;
        if !in_range && (hour, minute, second) != (24, 0, 0) {
            return None;
        }
        let mut timestamp = self.date_epoch_ms? + (hour * 3600 + minute * 60 + second) * 1000;
        if let Some(previous) = self.previous_timestamp {
            while timestamp < previous - ONE_HOUR_MS {
                timestamp += ONE_DAY_MS;
            }
        }
        Some(timestamp)
    }
}

/// Reads a `YYYY-MM-DD` date. Days 1 through 31 are accepted in every month and roll
/// into the next one.
fn date_epoch_ms(date: &str) -> Option<i64> {
    let year = date[..4].parse().ok()?;
    let month = date[5..7].parse().ok()?;
    let day: u32 = date[8..10].parse().ok()?;
    if !(1..=31).contains(&day) {
        return None;
    }
    let date =
        NaiveDate::from_ymd_opt(year, month, 1)?.checked_add_days(Days::new(u64::from(day - 1)))?;
    Some(date.and_hms_opt(0, 0, 0)?.and_utc().timestamp_millis())
}

/// Returns the text after `H<source><code>` when the source is a word character and
/// the text stays on one logical line.
fn header_rest(line: &str) -> Option<&str> {
    let source = *line.as_bytes().get(1)?;
    let rest = line.get(5..).filter(|_| is_word_byte(source))?;
    (!has_line_terminator(rest)).then_some(rest)
}

fn is_b_record(bytes: &[u8]) -> bool {
    let altitude =
        |field: &[u8]| all_digit_bytes(field) || (field[0] == b'-' && all_digit_bytes(&field[1..]));
    bytes.len() >= 35
        && all_digit_bytes(&bytes[1..14])
        && matches!(bytes[14], b'N' | b'S')
        && all_digit_bytes(&bytes[15..23])
        && matches!(bytes[23], b'E' | b'W')
        && matches!(bytes[24], b'A' | b'V')
        && altitude(&bytes[25..30])
        && altitude(&bytes[30..35])
}

fn coordinate(whole: &str, minutes: &str, thousandths: &str, extra: &str, negative: bool) -> f64 {
    let whole: f64 = whole.parse().expect("B record digits were validated");
    let minutes: f64 = format!("{minutes}.{thousandths}{extra}")
        .parse()
        .expect("B record digits were validated");
    let degrees = whole + minutes / 60.0;
    if negative && degrees != 0.0 {
        -degrees
    } else {
        degrees
    }
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn all_digit_bytes(bytes: &[u8]) -> bool {
    bytes.iter().all(u8::is_ascii_digit)
}

fn all_digits(text: &str) -> bool {
    all_digit_bytes(text.as_bytes())
}

fn has_line_terminator(text: &str) -> bool {
    text.contains(['\n', '\r', '\u{2028}', '\u{2029}'])
}

/// Slices by UTF-16 column, with negative indices counting from the end, as ECMAScript does.
fn js_slice(text: &str, start: i64, end: i64) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    let resolve = |index: i64| {
        let length = units.len() as i64;
        let column = if index < 0 { length + index } else { index };
        column.clamp(0, length) as usize
    };
    let (start, end) = (resolve(start), resolve(end));
    if start < end {
        String::from_utf16_lossy(&units[start..end])
    } else {
        String::new()
    }
}

/// Reads the leading integer of a short column field, as ECMAScript `parseInt` does.
fn js_parse_int(text: &str) -> Option<i64> {
    let text = text.trim_start_matches(is_js_whitespace);
    let (negative, unsigned) = match text.strip_prefix('-') {
        Some(unsigned) => (true, unsigned),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let end = unsigned
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(unsigned.len());
    let value: i64 = unsigned[..end].parse().ok()?;
    Some(if negative { -value } else { value })
}
