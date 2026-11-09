//! Shared time handling for source media and embedded telemetry.
//!
//! This module owns conversions between camera/vendor timestamps,
//! media-relative milliseconds, and RFC 3339 strings. Keeping these rules in
//! one place avoids subtle drift between GPS row expansion, sync-time
//! derivation, and vendor fallback parsers.

use chrono::{DateTime, Days, NaiveDateTime, TimeDelta, TimeZone, Utc};
use chrono_tz::Tz;
use telemetry_parser::util::SampleInfo;

pub(crate) const DJI_TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

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

/// Converts a DJI AC004 timestamp to UTC RFC 3339 text.
///
/// Depending on camera model and firmware, DJI AC004 metadata stores timestamps
/// either in the camera's local timezone (sometimes with the `UTC+8` calendar
/// date, rolling the date by `+1 day` in negative-UTC zones) or directly in
/// China Standard Time (`Asia/Shanghai`). When a reference UTC instant from the
/// container metadata is supplied, candidate dates within `±1 day` are evaluated
/// in both the GPS-derived timezone and `Asia/Shanghai`, selecting the instant
/// closest to the container reference while preserving the GPS clock's exact
/// minutes and seconds.
pub(crate) fn dji_naive_timestamp_to_rfc3339(
    naive: NaiveDateTime,
    tz: Tz,
    reference_utc: Option<DateTime<Utc>>,
) -> Option<String> {
    let resolve_in = |zone: Tz, candidate: NaiveDateTime| {
        zone.from_local_datetime(&candidate)
            .single()
            .map(|local_dt| local_dt.with_timezone(&Utc))
    };
    let base_utc = resolve_in(tz, naive)?;
    let utc_dt = match reference_utc {
        Some(reference) => {
            let day_candidates = [
                Some(naive),
                naive.checked_sub_days(Days::new(1)),
                naive.checked_add_days(Days::new(1)),
            ];
            [tz, Tz::Asia__Shanghai]
                .into_iter()
                .flat_map(|zone| {
                    day_candidates
                        .into_iter()
                        .flatten()
                        .filter_map(move |candidate| resolve_in(zone, candidate))
                })
                .min_by_key(|candidate| (*candidate - reference).num_seconds().abs())
                .unwrap_or(base_utc)
        }
        None => base_utc,
    };
    Some(utc_dt.to_rfc3339())
}

#[cfg(test)]
pub(crate) fn dji_timestamp_to_rfc3339(
    timestamp_text: &str,
    tz_name: &str,
    reference_utc: Option<DateTime<Utc>>,
) -> Option<String> {
    let naive = NaiveDateTime::parse_from_str(timestamp_text, DJI_TIMESTAMP_FORMAT).ok()?;
    let tz: Tz = tz_name.parse().ok()?;
    dji_naive_timestamp_to_rfc3339(naive, tz, reference_utc)
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

    #[test]
    fn dji_timestamp_parser_converts_camera_local_to_utc_via_timezone() {
        assert_eq!(
            dji_timestamp_to_rfc3339("2026-03-15 23:58:14", "Europe/Prague", None).as_deref(),
            Some("2026-03-15T22:58:14+00:00")
        );
    }

    #[test]
    fn dji_timestamp_parser_corrects_plus_one_day_roll_against_reference() {
        let reference = DateTime::parse_from_rfc3339("2026-11-07T19:10:33Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            dji_timestamp_to_rfc3339(
                "2026-11-08 10:33:56",
                "America/Los_Angeles",
                Some(reference),
            )
            .as_deref(),
            Some("2026-11-07T18:33:56+00:00")
        );
    }

    #[test]
    fn dji_timestamp_parser_supports_shanghai_timestamps_against_reference() {
        let reference = DateTime::parse_from_rfc3339("2026-03-15T15:58:14Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            dji_timestamp_to_rfc3339(
                "2026-03-15 23:58:14",
                "Europe/Prague",
                Some(reference),
            )
            .as_deref(),
            Some("2026-03-15T15:58:14+00:00")
        );
    }

    #[test]
    fn dji_timestamp_parser_rejects_invalid_timezone() {
        assert_eq!(
            dji_timestamp_to_rfc3339("2026-03-15 23:58:14", "Not/A_Tz", None),
            None
        );
    }

    #[test]
    fn dji_timestamp_parser_rejects_malformed_timestamp() {
        assert_eq!(
            dji_timestamp_to_rfc3339("2026-13-01 99:99:99", "Europe/Prague", None),
            None
        );
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
