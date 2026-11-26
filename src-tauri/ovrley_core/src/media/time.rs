//! Shared time handling for source media and embedded telemetry.
//!
//! This module owns conversions between camera/vendor timestamps,
//! media-relative milliseconds, and RFC 3339 strings. Keeping these rules in
//! one place avoids subtle drift between GPS row expansion, sync-time
//! derivation, and vendor fallback parsers.

use chrono::{NaiveDateTime, TimeDelta, TimeZone, Utc};
use chrono_tz::Tz;
use telemetry_parser::util::SampleInfo;

const DJI_TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// DJI's factory clock zone, which some camera models write GPS time in.
const DJI_UTC_PLUS_8: TimeDelta = TimeDelta::hours(8);

/// Distributes a sub-frame telemetry row within its enclosing parser sample.
///
/// GoPro GPS5 and camera metadata can store multiple rows inside a single
/// [`SampleInfo`] envelope with no per-row timestamps. This assumes rows are
/// uniformly spaced within the parser-reported sample duration.
pub fn sub_sample_timestamp_ms(sample: &SampleInfo, index: usize, row_count: usize) -> f64 {
    if index == 0 || row_count <= 1 || !sample.duration_ms.is_finite() || sample.duration_ms <= 0.0
    {
        return sample.timestamp_ms;
    }

    sample.timestamp_ms + sample.duration_ms * index as f64 / row_count as f64
}

/// Converts GPS epoch seconds into RFC 3339 text.
pub(crate) fn unix_seconds_to_rfc3339(unix_ts: f64) -> String {
    if !unix_ts.is_finite() {
        return String::new();
    }

    let secs = unix_ts.floor() as i64;
    let nanos = ((unix_ts - secs as f64) * 1_000_000_000.0)
        .round()
        .clamp(0.0, 999_999_999.0) as u32;

    Utc.timestamp_opt(secs, nanos)
        .single()
        .map(|datetime| datetime.to_rfc3339())
        .unwrap_or_default()
}

/// Converts an integer millisecond epoch plus a media-time offset into RFC 3339.
pub(crate) fn unix_millis_plus_offset_ms_to_rfc3339(unix_ms: u64, offset_ms: f64) -> String {
    if !offset_ms.is_finite() {
        return String::new();
    }

    let total_ns = unix_ms as i128 * 1_000_000 + (offset_ms * 1_000_000.0).round() as i128;
    let secs = total_ns.div_euclid(1_000_000_000) as i64;
    let nanos = total_ns.rem_euclid(1_000_000_000) as u32;

    Utc.timestamp_opt(secs, nanos)
        .single()
        .map(|datetime| datetime.to_rfc3339())
        .unwrap_or_default()
}

/// Derives video start time from an absolute GPS timestamp and media time.
pub(crate) fn gps_unix_seconds_to_video_start_rfc3339(
    gps_unix_seconds: f64,
    media_time_ms: f64,
) -> String {
    unix_seconds_to_rfc3339(gps_unix_seconds - media_time_ms / 1000.0)
}

/// Derives video start time from GoPro GPSU and the first GPS row's media time.
pub(crate) fn gpsu_millis_to_video_start_rfc3339(
    gpsu_unix_ms: u64,
    first_row_media_time_ms: f64,
) -> String {
    unix_millis_plus_offset_ms_to_rfc3339(gpsu_unix_ms, -first_row_media_time_ms)
}

/// Parses DJI GPS clock text (`YYYY-MM-DD HH:MM:SS`, no zone designator).
/// What zone the text is in depends on the camera model; see [`DjiGpsClock`].
pub(crate) fn parse_dji_timestamp(text: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(text, DJI_TIMESTAMP_FORMAT).ok()
}

/// How a DJI camera model writes its zone-less GPS clock text.
///
/// GPS convention is UTC, but DJI firmware formats the text itself and the
/// zone varies by model. The text carries no zone marker, so the model's
/// device string (`DJI AC00x`) decides how to read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DjiGpsClock {
    /// UTC. Osmo Action 4 (`DJI AC003`) and every model not listed below.
    Utc,
    /// UTC+8 wall-clock time wherever the clip was recorded. Osmo Action 6
    /// (`DJI AC006`).
    UtcPlus8,
    /// Local time of day at the GPS position, carrying the UTC+8 calendar
    /// date. Osmo Action 5 Pro (`DJI AC004`).
    LocalTimeUtcPlus8Date,
}

impl DjiGpsClock {
    pub(crate) fn for_device(device_name: Option<&str>) -> Self {
        match device_name {
            Some("DJI AC006") => Self::UtcPlus8,
            Some("DJI AC004") => Self::LocalTimeUtcPlus8Date,
            _ => Self::Utc,
        }
    }

    /// How far the clock text runs ahead of UTC, decided once per clip from
    /// its first fix so every sample keeps the GPS clock's exact seconds.
    ///
    /// [`Self::LocalTimeUtcPlus8Date`] needs the timezone at the fix; when the
    /// coordinates resolve to none (e.g. open sea), the text is read as UTC.
    pub(crate) fn offset(self, first_fix: NaiveDateTime, gps_tz: Option<Tz>) -> TimeDelta {
        match self {
            Self::Utc => TimeDelta::zero(),
            Self::UtcPlus8 => DJI_UTC_PLUS_8,
            Self::LocalTimeUtcPlus8Date => gps_tz
                .and_then(|tz| local_time_utc_plus_8_date_offset(first_fix, tz))
                .unwrap_or_else(TimeDelta::zero),
        }
    }
}

/// Reads `first_fix` as local time of day at `tz` on whichever calendar day
/// (the written date or the day before) falls on the written date in UTC+8.
fn local_time_utc_plus_8_date_offset(first_fix: NaiveDateTime, tz: Tz) -> Option<TimeDelta> {
    [0, -1]
        .into_iter()
        .filter_map(|days| first_fix.checked_add_signed(TimeDelta::days(days)))
        .flat_map(|local| {
            let local = tz.from_local_datetime(&local);
            // Both readings of a repeated DST hour stay candidates.
            [local.earliest(), local.latest()]
        })
        .flatten()
        .map(|local| local.naive_utc())
        .find(|utc| (*utc + DJI_UTC_PLUS_8).date() == first_fix.date())
        .map(|utc| first_fix - utc)
}

/// Converts DJI GPS clock text to UTC RFC 3339 text using the clip's offset
/// from [`DjiGpsClock::offset`].
pub(crate) fn dji_timestamp_to_rfc3339(
    timestamp: NaiveDateTime,
    clock_offset: TimeDelta,
) -> String {
    (timestamp - clock_offset).and_utc().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gopro_gpsu_sync_time_subtracts_first_row_media_time() {
        assert_eq!(
            gpsu_millis_to_video_start_rfc3339(1_722_860_910_174, 110.097),
            "2024-08-05T12:28:30.063903+00:00"
        );
    }

    #[test]
    fn gps_row_timestamps_add_offsets_without_floating_epoch_drift() {
        assert_eq!(
            unix_millis_plus_offset_ms_to_rfc3339(1_722_860_910_174, 54.73684210526316),
            "2024-08-05T12:28:30.228736842+00:00"
        );
    }

    fn dji_utc(device_name: &str, text: &str, gps_tz: Option<Tz>) -> String {
        let timestamp = parse_dji_timestamp(text).unwrap();
        let offset = DjiGpsClock::for_device(Some(device_name)).offset(timestamp, gps_tz);
        dji_timestamp_to_rfc3339(timestamp, offset)
    }

    #[test]
    fn dji_osmo_action_4_gps_clock_is_utc() {
        assert_eq!(
            dji_utc("DJI AC003", "2026-08-31 07:13:53", Some(Tz::Europe__Warsaw)),
            "2026-08-31T07:13:53+00:00"
        );
    }

    #[test]
    fn dji_osmo_action_6_gps_clock_is_utc_plus_8() {
        assert_eq!(
            dji_utc(
                "DJI AC006",
                "2026-07-28 01:46:15",
                Some(Tz::America__Los_Angeles)
            ),
            "2026-07-27T17:46:15+00:00"
        );
    }

    #[test]
    fn dji_osmo_action_5_pro_gps_clock_is_local_time_with_utc_plus_8_date() {
        assert_eq!(
            dji_utc(
                "DJI AC004",
                "2026-11-08 10:33:56",
                Some(Tz::America__Los_Angeles)
            ),
            "2026-11-07T18:33:56+00:00"
        );
    }

    #[test]
    fn dji_unknown_model_gps_clock_is_utc() {
        assert_eq!(
            dji_utc(
                "DJI AC009",
                "2026-07-28 01:46:15",
                Some(Tz::America__Los_Angeles)
            ),
            "2026-07-28T01:46:15+00:00"
        );
    }

    #[test]
    fn dji_timestamp_parser_rejects_malformed_timestamp() {
        assert_eq!(parse_dji_timestamp("2026-13-01 99:99:99"), None);
    }

    #[test]
    fn sub_sample_timestamp_distributes_rows_inside_sample_duration() {
        let sample = SampleInfo {
            sample_index: 0,
            track_index: 0,
            timestamp_ms: 100.0,
            duration_ms: 40.0,
            video_rotation: None,
            tag_map: None,
        };

        assert_eq!(sub_sample_timestamp_ms(&sample, 0, 4), 100.0);
        assert_eq!(sub_sample_timestamp_ms(&sample, 2, 4), 120.0);
    }
}
