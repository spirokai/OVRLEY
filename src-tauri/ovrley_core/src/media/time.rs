//! Shared time handling for source media and embedded telemetry.
//!
//! This module owns conversions between camera/vendor timestamps,
//! media-relative milliseconds, and RFC 3339 strings. Keeping these rules in
//! one place avoids subtle drift between GPS row expansion, sync-time
//! derivation, and vendor fallback parsers.

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
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

/// Parses a DJI AC004 local timestamp as UTC-formatted RFC 3339 text.
pub(crate) fn dji_timestamp_to_rfc3339(timestamp_text: &str) -> Option<String> {
    let naive = NaiveDateTime::parse_from_str(timestamp_text, DJI_TIMESTAMP_FORMAT).ok()?;
    let datetime: DateTime<Utc> = DateTime::from_naive_utc_and_offset(naive, Utc);
    Some(datetime.to_rfc3339())
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
    fn dji_timestamp_parser_promotes_camera_text_to_utc_rfc3339() {
        assert_eq!(
            dji_timestamp_to_rfc3339("2026-03-15 23:58:14").as_deref(),
            Some("2026-03-15T23:58:14+00:00")
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
