//! Native MISB ST 0601 KLV telemetry extraction for STANAG 4609 drone video.
//!
//! Professional, ISR, and enterprise drone platforms carry their telemetry as
//! MISB KLV embedded in an MPEG-TS stream, not as the consumer `.srt`/`.gpx`/`.fit`
//! formats the other parsers read. This parser demuxes the transport stream and
//! reads ST 0601 sensor position and platform heading into the same canonical raw
//! activity samples as every other source, so a MISB feed becomes an overlay source.
//!
//! KLV is a boundary like the others: an undecodable packet is skipped, and only a
//! stream with no ST 0601 sensor position at all is rejected.
//!
//! - Sample time is the TS presentation time stamp, rebased to the first packet, so
//!   the overlay lines up with the video frame the telemetry describes.
//! - Elevation prefers the sensor true altitude (MSL), then the ellipsoid height.

use crate::activity::finalize::FinalizeActivityResponse;
use crate::activity::parsers::{parse_bytes_path, smoothing_options};
use crate::activity::schema::{RawActivity, RawActivityOptions, RawSample};
use crate::error::{CoreError, CoreResult};
use chrono::{SecondsFormat, TimeZone, Utc};
use misb::st0601::Packet;
use misb::{klv, st0601, ts};
use serde_json::json;
use std::path::Path;

/// Opens a native MISB KLV transport stream and returns the finalized activity.
pub fn parse_misb_activity_path(
    path: &Path,
    repo_root: Option<&Path>,
) -> CoreResult<FinalizeActivityResponse> {
    parse_bytes_path(path, repo_root, "MISB", extract_misb_activity)
}

/// Converts Precision Time Stamp microseconds since 1970 UTC to an RFC 3339 string.
fn micros_to_rfc3339(micros: u64) -> Option<String> {
    match Utc.timestamp_micros(micros as i64) {
        chrono::LocalResult::Single(time) => {
            Some(time.to_rfc3339_opts(SecondsFormat::Millis, true))
        }
        _ => None,
    }
}

/// Rebases presentation time stamps so the first sample is the activity's time zero.
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

/// Extracts one canonical raw sample per ST 0601 packet from an MPEG-TS KLV feed.
pub fn extract_misb_activity(bytes: &[u8], file_name: &str) -> CoreResult<RawActivity> {
    let mut raw_samples: Vec<RawSample> = Vec::new();
    for unit in ts::klv_units(bytes) {
        let elapsed_seconds = unit.pts_seconds();
        for item in klv::split(&unit.data) {
            if !klv::same_key(item.key, &st0601::KEY) {
                continue;
            }
            let Ok(packet) = Packet::from_unit(&item) else {
                continue;
            };
            let Some(position) = packet.sensor_position() else {
                continue;
            };
            raw_samples.push(RawSample {
                elapsed_seconds,
                timestamp: packet.timestamp().and_then(micros_to_rfc3339),
                latitude: Some(position.lat),
                longitude: Some(position.lon),
                elevation: position.height.or_else(|| packet.sensor_hae()),
                heading: packet.platform_heading(),
                ..RawSample::default()
            });
        }
    }
    if raw_samples.is_empty() {
        return Err(CoreError::Activity(
            "The file contains no MISB ST 0601 sensor positions.".into(),
        ));
    }
    rebase_cue_times(&mut raw_samples);
    Ok(RawActivity {
        file_name: file_name.to_string(),
        file_format: "misb".to_string(),
        metadata: json!({}),
        sync_time: None,
        lap_markers: Default::default(),
        raw_samples,
        options: RawActivityOptions {
            skip_idle_gap_fill: true,
            smoothing: smoothing_options(&[
                ("speed", "zero_phase_ma", 0.5),
                ("vertical_speed", "zero_phase_ma", 1.0),
                ("heading", "circular_ema", 0.5),
            ]),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use misb::st0601::{Builder, Tag};

    /// Builds one ST 0601 packet carrying sensor position and platform heading.
    fn st0601_packet(lat: f64, lon: f64, alt: f64, heading: f64) -> Vec<u8> {
        let mut builder = Builder::new(1_700_000_000_000_000);
        builder.float(Tag::SensorLatitude, lat).unwrap();
        builder.float(Tag::SensorLongitude, lon).unwrap();
        builder.float(Tag::SensorTrueAltitude, alt).unwrap();
        builder.float(Tag::PlatformHeadingAngle, heading).unwrap();
        builder.finish()
    }

    /// Muxes ST 0601 packets into a STANAG 4609 transport stream with PTS.
    fn transport_stream(packets: &[(Vec<u8>, u64)]) -> Vec<u8> {
        let mut muxer = ts::Muxer::new(0x0100);
        let mut out = muxer.tables();
        for (klv, pts) in packets {
            out.extend(muxer.write(klv, Some(*pts)));
        }
        out
    }

    #[test]
    fn reads_sensor_track_from_transport_stream() {
        // Two ST 0601 packets one second apart (90 kHz PTS ticks).
        let stream = transport_stream(&[
            (st0601_packet(34.120000, -118.300000, 500.0, 90.0), 90_000),
            (st0601_packet(34.120100, -118.300100, 510.0, 92.0), 180_000),
        ]);
        assert!(ts::is_transport_stream(&stream));

        let activity = extract_misb_activity(&stream, "flight.ts").expect("parses MISB stream");
        assert_eq!(activity.file_format, "misb");
        assert_eq!(activity.raw_samples.len(), 2);

        let first = &activity.raw_samples[0];
        assert!((first.latitude.unwrap() - 34.120000).abs() < 1e-3);
        assert!((first.longitude.unwrap() + 118.300000).abs() < 1e-3);
        assert!((first.elevation.unwrap() - 500.0).abs() < 1.0);
        assert!((first.heading.unwrap() - 90.0).abs() < 1e-2);
        // PTS rebased so the first packet is time zero and the next is one second later.
        assert_eq!(first.elapsed_seconds, Some(0.0));
        assert!((activity.raw_samples[1].elapsed_seconds.unwrap() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn rejects_a_stream_without_misb_telemetry() {
        let empty = ts::Muxer::new(0x0100).tables();
        assert!(extract_misb_activity(&empty, "empty.ts").is_err());
    }
}
