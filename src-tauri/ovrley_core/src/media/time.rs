//! Shared time handling for source media and embedded telemetry.
//!
//! This module owns conversions between camera/vendor timestamps,
//! media-relative milliseconds, and RFC 3339 strings. Keeping these rules in
//! one place avoids subtle drift between GPS row expansion, sync-time
//! derivation, and vendor fallback parsers.

use chrono::{DateTime, NaiveDateTime, TimeDelta, TimeZone, Utc};
use chrono_tz::Tz;
use telemetry_parser::util::SampleInfo;

const DJI_TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// How far DJI GPS clock text runs ahead of UTC unless the container clock
/// says otherwise: `UTC+8` wall-clock time, wherever the clip was recorded.
const DJI_DEFAULT_CLOCK_OFFSET: TimeDelta = TimeDelta::hours(8);

/// Largest gap between the container clock and a reading of the GPS clock
/// text for the container clock to be trusted to choose that reading.
const DJI_MAX_REFERENCE_ERROR: TimeDelta = TimeDelta::hours(2);

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
pub(crate) fn parse_dji_timestamp(text: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(text, DJI_TIMESTAMP_FORMAT).ok()
}

/// Decides how far a clip's DJI GPS clock text runs ahead of UTC.
///
/// DJI firmware disagrees on what the zone-less text means. Most clips store
/// `UTC+8` wall-clock time wherever they were recorded; some store local time
/// of day at the GPS position with a calendar date that can be a day off. The
/// text alone cannot tell these apart, so `UTC+8` stays the default.
///
/// `reference` is the container `creation_time` advanced to the first fix.
/// When it lies within [`DJI_MAX_REFERENCE_ERROR`] of a reading, the closest
/// reading wins: `UTC+8`, or local time at `gps_tz` on the text's date `±1`
/// day. A missing or unreliable camera clock never moves the result away from
/// `UTC+8`. Decided once per clip from `first_fix`, the offset is applied to
/// every sample, keeping the GPS clock's exact seconds.
pub(crate) fn dji_clock_offset(
    first_fix: NaiveDateTime,
    gps_tz: Option<Tz>,
    reference: Option<DateTime<Utc>>,
) -> TimeDelta {
    let Some(reference) = reference else {
        return DJI_DEFAULT_CLOCK_OFFSET;
    };
    let error = |offset: TimeDelta| ((first_fix - offset).and_utc() - reference).abs();
    // Both readings of a repeated DST hour stay candidates.
    let local_offsets = gps_tz.into_iter().flat_map(|tz| {
        [-1, 0, 1]
            .into_iter()
            .filter_map(move |days| first_fix.checked_add_signed(TimeDelta::days(days)))
            .flat_map(move |shifted| {
                let local = tz.from_local_datetime(&shifted);
                [local.earliest(), local.latest()]
            })
            .flatten()
            .map(move |local| first_fix - local.naive_utc())
    });
    let closest = std::iter::once(DJI_DEFAULT_CLOCK_OFFSET)
        .chain(local_offsets)
        .min_by_key(|offset| error(*offset))
        .unwrap_or(DJI_DEFAULT_CLOCK_OFFSET);
    if error(closest) <= DJI_MAX_REFERENCE_ERROR {
        closest
    } else {
        DJI_DEFAULT_CLOCK_OFFSET
    }
}

/// Converts DJI GPS clock text to UTC RFC 3339 text using the clip's offset
/// from [`dji_clock_offset`].
pub(crate) fn dji_timestamp_to_rfc3339(
    timestamp: NaiveDateTime,
    clock_offset: TimeDelta,
) -> String {
    (timestamp - clock_offset).and_utc().to_rfc3339()
}

pub(crate) fn advance_utc_by_ms(base: DateTime<Utc>, offset_ms: f64) -> DateTime<Utc> {
    if !offset_ms.is_finite() {
        return base;
    }
    base + TimeDelta::milliseconds(offset_ms.round() as i64)
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

    fn utc(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn dji_clock_offset_keeps_utc_plus_8_when_container_clock_agrees() {
        let first_fix = parse_dji_timestamp("2026-07-28 01:46:15").unwrap();
        let reference = Some(utc("2026-07-27T17:46:07Z"));
        assert_eq!(
            dji_clock_offset(first_fix, Some(Tz::America__Los_Angeles), reference),
            TimeDelta::hours(8)
        );
    }

    #[test]
    fn dji_clock_offset_ignores_unreliable_container_clock() {
        let first_fix = parse_dji_timestamp("2026-11-08 10:33:56").unwrap();
        let reference = Some(utc("2020-01-01T00:00:00Z"));
        assert_eq!(
            dji_clock_offset(first_fix, Some(Tz::America__Los_Angeles), reference),
            TimeDelta::hours(8)
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
