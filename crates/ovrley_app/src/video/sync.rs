//! Automatic placement of the videos: a video's start against the external activity, or between
//! videos without one, and the recording-time helpers.
//!
//! Recording times are external data. A GPS time is an absolute instant, read as the wall clock of
//! the activity's timezone. A camera clock (`ffprobe`) or filename time is a wall clock whose zone
//! is unknown: it is compared as it reads, or, read as UTC, converted like a GPS time. Wall clocks
//! are compared as milliseconds of the naive UTC date `{clock}Z`.

use std::sync::LazyLock;

use chrono::{DateTime, NaiveDateTime, Utc};
use regex::Regex;
use ovrley_core::media::edits::TimezoneMode;
use crate::i18n::{catalog, Locale};

/// Why a video could not be placed against the activity, or that it lies outside it. Translated
/// when it is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncWarning {
    CouldNotDetermineCreationTime,
    TimezoneRequired,
    InvalidTimestamps,
    OutsideActivity,
}

impl SyncWarning {
    pub fn message(self, locale: Locale) -> String {
        let (key, default) = match self {
            Self::CouldNotDetermineCreationTime => (
                "store.couldNotDetermineVideoCreationTime",
                "Could not determine video creation time",
            ),
            Self::TimezoneRequired => (
                "store.timezoneIsRequiredForVideoSync",
                "timezone is required for video sync",
            ),
            Self::InvalidTimestamps => {
                ("store.invalidTimestampFormats", "Invalid timestamp formats")
            }
            Self::OutsideActivity => (
                "store.videoOutsideActivity",
                "Video is outside the activity",
            ),
        };
        catalog().t_or(locale, key, default).to_string()
    }
}

/// The activity videos are placed against: its start, end and recording timezone.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncActivity {
    pub sync_time: Option<String>,
    pub end_time: Option<String>,
    pub timezone: Option<String>,
}

/// A video's recording time as placement reads it.
#[derive(Clone, Copy, Debug)]
pub struct RecordingTime<'a> {
    pub creation_time: Option<&'a str>,
    pub time_source: Option<&'a str>,
    pub duration_seconds: f64,
}

/// Where a video goes. `determined` is false when its time could not be resolved;
/// `timezone_mode` is the reading it settles on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub offset_seconds: f64,
    pub determined: bool,
    pub sync_warning: Option<SyncWarning>,
    pub timezone_mode: Option<TimezoneMode>,
}

/// How far from the activity (or from the earliest video, without one) a detected start may lie:
/// a start further away is silently clamped to this distance, so videos recorded far from the
/// activity stay workable on the timeline. Starts set by hand are not limited.
pub const MAX_SYNC_DISTANCE_SECONDS: f64 = 2.0 * 3600.0;

/// The half-open intervals overlap.
pub fn video_overlaps_activity(video_start: f64, video_duration: f64, activity_end: f64) -> bool {
    video_start < activity_end && video_start + video_duration > 0.0
}

/// The camera-clock text that timestamps are parsed and shown from: a fraction
/// before the zone, then the zone (`Z`, `±hh:mm`, `±hhmm` or ` UTC`), then surrounding spaces;
/// the first space becomes `T`.
pub fn strip_camera_clock(timestamp: &str) -> String {
    let (body, _zone) = split_zone(timestamp);
    let body = match body.rfind('.') {
        Some(dot)
            if dot + 1 < body.len()
                && body[dot + 1..].bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            &body[..dot]
        }
        _ => body,
    };
    body.trim().replacen(' ', "T", 1)
}

/// Splits a trailing zone designator off, as the strip's regular expression matches it.
fn split_zone(timestamp: &str) -> (&str, &str) {
    if let Some(body) = timestamp.strip_suffix(" UTC") {
        return (body, " UTC");
    }
    if let Some(body) = timestamp.strip_suffix('Z') {
        return (body, "Z");
    }
    let bytes = timestamp.as_bytes();
    for length in [6, 5] {
        if bytes.len() < length {
            continue;
        }
        let zone = &timestamp[bytes.len() - length..];
        let zone_bytes = zone.as_bytes();
        let signed = matches!(zone_bytes[0], b'+' | b'-');
        let digits_ok = if length == 6 {
            zone_bytes[1..3].iter().all(u8::is_ascii_digit)
                && zone_bytes[3] == b':'
                && zone_bytes[4..].iter().all(u8::is_ascii_digit)
        } else {
            zone_bytes[1..].iter().all(u8::is_ascii_digit)
        };
        if signed && digits_ok {
            return (&timestamp[..bytes.len() - length], zone);
        }
    }
    (timestamp, "")
}

/// `Date.parse(`${clock}Z`)` of an ISO wall clock: milliseconds of the naive time read as UTC.
fn parse_wall_clock_ms(clock: &str) -> Option<i64> {
    [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
    ]
    .iter()
    .find_map(|format| NaiveDateTime::parse_from_str(clock, format).ok())
    .map(|naive| naive.and_utc().timestamp_millis())
}

/// The wall clock of an instant in `timezone`, in milliseconds read as UTC
/// `None` for an unreadable instant or zone.
fn zoned_wall_clock_ms(timestamp: &str, timezone: &str) -> Option<i64> {
    let zone: chrono_tz::Tz = timezone.parse().ok()?;
    // A clock without a zone is read as UTC; `new Date()` would read it in the machine's zone.
    let instant = match DateTime::parse_from_rfc3339(timestamp) {
        Ok(instant) => instant.with_timezone(&Utc),
        Err(_) => DateTime::from_timestamp_millis(parse_wall_clock_ms(&strip_camera_clock(timestamp))?)?,
    };
    // Whole seconds, as the formatted parts carry no fraction.
    Some(instant.with_timezone(&zone).naive_local().and_utc().timestamp() * 1000)
}

/// GPS times as the wall clock of `timezone`, other sources as their own
/// clock text.
fn parse_sync_timestamp(timestamp: Option<&str>, gps: bool, timezone: Option<&str>) -> Option<i64> {
    let timestamp = timestamp.filter(|timestamp| !timestamp.trim().is_empty())?;
    if gps {
        zoned_wall_clock_ms(timestamp, timezone?)
    } else {
        parse_wall_clock_ms(&strip_camera_clock(timestamp))
    }
}

fn is_ambiguous(time_source: Option<&str>) -> bool {
    matches!(time_source, Some("ffprobe" | "filename"))
}

/// The signed start against the activity and its duration.
fn automatic_offset(
    video: &RecordingTime,
    activity: &SyncActivity,
    mode: TimezoneMode,
) -> Result<(f64, f64), SyncWarning> {
    if video.creation_time.is_none_or(str::is_empty) {
        return Err(SyncWarning::CouldNotDetermineCreationTime);
    }
    let timezone = activity
        .timezone
        .as_deref()
        .filter(|timezone| !timezone.is_empty())
        .ok_or(SyncWarning::TimezoneRequired)?;
    let gps = !(is_ambiguous(video.time_source) && mode == TimezoneMode::Local);
    let video_start = parse_sync_timestamp(video.creation_time, gps, Some(timezone));
    let activity_start = parse_sync_timestamp(activity.sync_time.as_deref(), true, Some(timezone));
    let activity_end = parse_sync_timestamp(activity.end_time.as_deref(), true, Some(timezone));
    match (video_start, activity_start, activity_end) {
        (Some(video_start), Some(activity_start), Some(activity_end))
            if activity_end > activity_start =>
        {
            Ok((
                (video_start - activity_start) as f64 / 1000.0,
                (activity_end - activity_start) as f64 / 1000.0,
            ))
        }
        _ => Err(SyncWarning::InvalidTimestamps),
    }
}

/// An ambiguous camera clock tries both readings until one is chosen,
/// preferring local when both overlap; a sole local match stays inferred (`None`) for later
/// activity changes. A video outside the activity keeps its signed offset and its warning.
pub fn resolve_video_sync(
    video: &RecordingTime,
    activity: &SyncActivity,
    timezone_mode: Option<TimezoneMode>,
) -> Placement {
    let ambiguous = is_ambiguous(video.time_source);
    let infer = ambiguous && timezone_mode.is_none();
    let modes = if infer {
        vec![TimezoneMode::Local, TimezoneMode::Utc]
    } else {
        vec![timezone_mode.unwrap_or(TimezoneMode::Local)]
    };
    let candidates: Result<Vec<(TimezoneMode, f64, bool, f64)>, SyncWarning> = modes
        .iter()
        .map(|mode| {
            automatic_offset(video, activity, *mode).map(|(offset, activity_duration)| {
                (
                    *mode,
                    offset,
                    video_overlaps_activity(offset, video.duration_seconds, activity_duration),
                    activity_duration,
                )
            })
        })
        .collect();
    match candidates {
        Err(warning) => Placement {
            offset_seconds: 0.0,
            determined: false,
            sync_warning: Some(warning),
            timezone_mode: None,
        },
        Ok(candidates) => {
            let matches: Vec<_> = candidates
                .iter()
                .filter(|(_, _, overlaps, _)| *overlaps)
                .collect();
            let selected = matches.first().copied().unwrap_or(&candidates[0]);
            let inferred_local_only =
                infer && matches.len() == 1 && selected.0 == TimezoneMode::Local;
            Placement {
                // A start far from the activity is kept within reach.
                offset_seconds: selected.1.clamp(-MAX_SYNC_DISTANCE_SECONDS, selected.3 + MAX_SYNC_DISTANCE_SECONDS),
                determined: true,
                sync_warning: (!selected.2).then_some(SyncWarning::OutsideActivity),
                timezone_mode: (ambiguous && !inferred_local_only).then_some(selected.0),
            }
        }
    }
}

/// GPS times as instants, other sources as their clock text.
fn creation_time_ms(video: &RecordingTime) -> Option<i64> {
    let timestamp = video
        .creation_time
        .filter(|timestamp| !timestamp.trim().is_empty())?;
    if video.time_source == Some("gps") {
        return DateTime::parse_from_rfc3339(timestamp)
            .ok()
            .map(|instant| instant.with_timezone(&Utc).timestamp_millis());
    }
    parse_sync_timestamp(Some(timestamp), false, None)
}

/// Without an activity, videos keep the gaps between their recording
/// times, the earliest at 0. Camera clocks are compared as wall clocks.
pub fn place_videos_by_creation_time(videos: &[RecordingTime]) -> Vec<Placement> {
    let starts: Vec<Option<i64>> = videos.iter().map(creation_time_ms).collect();
    let origin = starts.iter().flatten().min().copied();
    starts
        .into_iter()
        .map(|start| match (start, origin) {
            (Some(start), Some(origin)) => Placement {
                // Kept within reach of the earliest video.
                offset_seconds: ((start - origin) as f64 / 1000.0).min(MAX_SYNC_DISTANCE_SECONDS),
                determined: true,
                sync_warning: None,
                timezone_mode: None,
            },
            _ => Placement {
                offset_seconds: 0.0,
                determined: false,
                sync_warning: Some(SyncWarning::CouldNotDetermineCreationTime),
                timezone_mode: None,
            },
        })
        .collect()
}

/// A date-then-time timestamp in a file name (`20240601103000`, `VID_20240601_103000`,
/// `Screenshot_2024-06-01-10-30-00`, `Screenshot 2024-06-01 at 10.30.00`, `PXL_20240601_103000123`),
/// as local clock text `YYYY-MM-DDTHH:MM:SS`, or `None` without one or for an impossible date.
pub fn parse_video_filename_creation_time(path: &str) -> Option<String> {
    // YYYY MM DD, optional date/time separator, HH MM SS, optional milliseconds; each part may
    // be split by a separator, and the whole is not embedded in a longer run of digits.
    static TIMESTAMP: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?:^|\D)(\d{4})[-_.]?(\d{2})[-_.]?(\d{2})(?:[-_. T]| at )?(\d{2})[-_.:]?(\d{2})[-_.:]?(\d{2})(?:[.,]?\d{3})?(?:\D|$)",
        )
        .expect("valid filename timestamp pattern")
    });
    let filename = path.rsplit(['/', '\\']).next()?;
    TIMESTAMP.captures_iter(filename).find_map(|parts| {
        let timestamp = format!(
            "{}-{}-{}T{}:{}:{}",
            &parts[1], &parts[2], &parts[3], &parts[4], &parts[5], &parts[6]
        );
        NaiveDateTime::parse_from_str(&timestamp, "%Y-%m-%dT%H:%M:%S")
            .ok()
            .map(|_| timestamp)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video<'a>(
        creation_time: &'a str,
        time_source: &'a str,
        duration_seconds: f64,
    ) -> RecordingTime<'a> {
        RecordingTime {
            creation_time: Some(creation_time),
            time_source: Some(time_source),
            duration_seconds,
        }
    }

    fn activity(timezone: &str) -> SyncActivity {
        SyncActivity {
            sync_time: Some("2024-06-01T08:00:00Z".into()),
            end_time: Some("2024-06-01T10:00:00Z".into()),
            timezone: Some(timezone.into()),
        }
    }

    #[test]
    fn camera_clocks_are_stripped_of_fractions_and_zones() {
        for (text, clock) in [
            ("2024-06-01T10:00:00.123Z", "2024-06-01T10:00:00"),
            ("2024-06-01 10:00:00+02:00", "2024-06-01T10:00:00"),
            ("2024-06-01T10:00:00.5-0500", "2024-06-01T10:00:00"),
            ("2024-06-01 10:00:00 UTC", "2024-06-01T10:00:00"),
            ("2024-06-01T10:00:00", "2024-06-01T10:00:00"),
        ] {
            assert_eq!(strip_camera_clock(text), clock, "{text}");
        }
    }

    #[test]
    fn gps_times_read_as_the_wall_clock_of_the_zone_across_a_dst_change() {
        // Berlin is UTC+1 before 31 March 2024 02:00 and UTC+2 after.
        let before = zoned_wall_clock_ms("2024-03-31T00:30:00Z", "Europe/Berlin").unwrap();
        let after = zoned_wall_clock_ms("2024-03-31T01:30:00Z", "Europe/Berlin").unwrap();
        assert_eq!(before, parse_wall_clock_ms("2024-03-31T01:30:00").unwrap());
        assert_eq!(after, parse_wall_clock_ms("2024-03-31T03:30:00").unwrap());
        assert_eq!(
            zoned_wall_clock_ms("2024-03-31T00:30:00Z", "Mars/Olympus"),
            None
        );
    }

    #[test]
    fn resolves_against_the_activity() {
        let placement = resolve_video_sync(
            &video("2024-06-01T08:30:00Z", "gps", 60.0),
            &activity("UTC"),
            None,
        );
        assert_eq!(
            placement,
            Placement {
                offset_seconds: 1800.0,
                determined: true,
                sync_warning: None,
                timezone_mode: None
            }
        );
        let outside = resolve_video_sync(
            &video("2024-06-01T12:00:00Z", "gps", 60.0),
            &activity("UTC"),
            None,
        );
        assert_eq!(
            (outside.offset_seconds, outside.sync_warning),
            (14400.0, Some(SyncWarning::OutsideActivity))
        );
    }

    #[test]
    fn an_ambiguous_clock_prefers_local_and_stays_inferred_as_the_sole_match() {
        // In Berlin (UTC+2 in June) the local reading of 10:30 is 08:30 UTC, inside the activity;
        // read as UTC it is 12:30 local, outside.
        let local = resolve_video_sync(
            &video("2024-06-01T10:30:00", "ffprobe", 60.0),
            &activity("Europe/Berlin"),
            None,
        );
        assert_eq!(
            (local.offset_seconds, local.timezone_mode, local.determined),
            (1800.0, None, true)
        );
        // 08:30 read as UTC is 10:30 local, inside; read locally it is 06:30 UTC, before the activity.
        let utc = resolve_video_sync(
            &video("2024-06-01T08:30:00", "ffprobe", 60.0),
            &activity("Europe/Berlin"),
            None,
        );
        assert_eq!(
            (utc.offset_seconds, utc.timezone_mode),
            (1800.0, Some(TimezoneMode::Utc))
        );
        let chosen = resolve_video_sync(
            &video("2024-06-01T10:30:00", "ffprobe", 60.0),
            &activity("Europe/Berlin"),
            Some(TimezoneMode::Local),
        );
        assert_eq!(
            chosen.timezone_mode,
            Some(TimezoneMode::Local),
            "a chosen reading is kept"
        );
    }

    #[test]
    fn unresolvable_times_report_why() {
        let missing = RecordingTime {
            creation_time: None,
            time_source: None,
            duration_seconds: 1.0,
        };
        assert_eq!(
            resolve_video_sync(&missing, &activity("UTC"), None).sync_warning,
            Some(SyncWarning::CouldNotDetermineCreationTime)
        );
        let no_zone = SyncActivity {
            timezone: None,
            ..activity("UTC")
        };
        assert_eq!(
            resolve_video_sync(&video("2024-06-01T08:30:00Z", "gps", 1.0), &no_zone, None)
                .sync_warning,
            Some(SyncWarning::TimezoneRequired)
        );
        let garbage = resolve_video_sync(&video("yesterday", "gps", 1.0), &activity("UTC"), None);
        assert_eq!(
            (
                garbage.determined,
                garbage.offset_seconds,
                garbage.sync_warning
            ),
            (false, 0.0, Some(SyncWarning::InvalidTimestamps))
        );
    }

    #[test]
    fn places_videos_relative_to_the_earliest_without_an_activity() {
        let placements = place_videos_by_creation_time(&[
            video("2024-06-01T10:00:30", "ffprobe", 10.0),
            video("2024-06-01T08:00:00Z", "gps", 10.0),
            RecordingTime {
                creation_time: None,
                time_source: None,
                duration_seconds: 10.0,
            },
            video("2024-06-01T10:00:00", "filename", 10.0),
        ]);
        let offsets: Vec<f64> = placements
            .iter()
            .map(|placement| placement.offset_seconds)
            .collect();
        // The first is 7230 s after the earliest, clamped to two hours.
        assert_eq!(offsets, [MAX_SYNC_DISTANCE_SECONDS, 0.0, 0.0, 7200.0]);
        assert!(!placements[2].determined);
        assert_eq!(
            placements[2].sync_warning,
            Some(SyncWarning::CouldNotDetermineCreationTime)
        );
    }

    #[test]
    fn filename_times_need_a_real_date_and_digit_bounds() {
        assert_eq!(
            parse_video_filename_creation_time("/v/VID_20240601_103000.mp4").as_deref(),
            Some("2024-06-01T10:30:00")
        );
        assert_eq!(
            parse_video_filename_creation_time("C:\\v\\20240601_103000.mov").as_deref(),
            Some("2024-06-01T10:30:00")
        );
        for name in [
            "DJI_20240601103000_0001_D_O001.MP4",
            "Screenshot_2024-06-01-10-30-00.png",
            "Screenshot from 2024-06-01 10-30-00.png",
            "Screenshot 2024-06-01 at 10.30.00.png",
            "PXL_20240601_103000123.mp4",
        ] {
            assert_eq!(
                parse_video_filename_creation_time(name).as_deref(),
                Some("2024-06-01T10:30:00"),
                "{name}"
            );
        }
        assert_eq!(
            parse_video_filename_creation_time("/v/20240230_103000.mp4"),
            None,
            "30 February"
        );
        assert_eq!(
            parse_video_filename_creation_time("/v/120240601_103000.mp4"),
            None,
            "a ninth digit before"
        );
        assert_eq!(
            parse_video_filename_creation_time("/v/20240601_1030001.mp4"),
            None,
            "a seventh digit after"
        );
        assert_eq!(
            parse_video_filename_creation_time("/20240601_103000/clip.mp4"),
            None,
            "only the file name counts"
        );
    }

    #[test]
    fn overlap_is_half_open() {
        assert!(video_overlaps_activity(-5.0, 10.0, 100.0));
        assert!(!video_overlaps_activity(-10.0, 10.0, 100.0));
        assert!(!video_overlaps_activity(100.0, 10.0, 100.0));
    }

    #[test]
    fn detected_starts_are_clamped_to_two_hours_from_the_activity_or_the_earliest_video() {
        let far = resolve_video_sync(&video("2024-06-03T08:00:00Z", "gps", 60.0), &activity("UTC"), None);
        assert_eq!((far.offset_seconds, far.sync_warning), (7200.0 + MAX_SYNC_DISTANCE_SECONDS, Some(SyncWarning::OutsideActivity)));
        let before = resolve_video_sync(&video("2024-05-30T08:00:00Z", "gps", 60.0), &activity("UTC"), None);
        assert_eq!(before.offset_seconds, -MAX_SYNC_DISTANCE_SECONDS);
        let placements = place_videos_by_creation_time(&[video("2024-06-01T08:00:00Z", "gps", 1.0), video("2024-06-05T08:00:00Z", "gps", 1.0)]);
        assert_eq!(placements[1].offset_seconds, MAX_SYNC_DISTANCE_SECONDS);
    }
}
