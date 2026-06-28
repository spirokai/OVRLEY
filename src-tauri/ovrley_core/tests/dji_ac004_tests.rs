use ovrley_core::media::dji_ac004::{extract_from_video, parse_raw_metadata};
use ovrley_core::media::mp4_telemetry;
use std::path::Path;

#[test]
fn extract_from_video_reads_dji_fixture() {
    let (repo_root, fixture) = dji_fixture();

    let telemetry = extract_from_video(repo_root, &fixture)
        .expect("expected extraction to succeed")
        .expect("expected DJI AC004 telemetry");

    assert_eq!(telemetry.device_name.as_deref(), Some("DJI AC004"));
    assert_eq!(
        telemetry.sync_time.as_deref(),
        Some("2026-03-15T23:58:14+00:00")
    );
    assert!(!telemetry.samples.is_empty());
    assert!(telemetry.samples[0].latitude.is_finite());
    assert!(telemetry.samples[0].longitude.is_finite());
    assert!(telemetry.samples[0].heading.is_some());
    assert!(telemetry.samples[0].g_force.is_some());
}

#[test]
fn mp4_telemetry_uses_ac004_fallback_for_dji_fixture() {
    let (repo_root, fixture) = dji_fixture();

    let metadata = mp4_telemetry::probe_video_metadata(fixture.to_str().unwrap())
        .expect("expected video metadata probe to succeed");
    let activity = mp4_telemetry::extract_activity(
        repo_root,
        fixture.to_str().unwrap(),
        metadata.fps.unwrap_or(30.0),
        metadata.duration.unwrap_or(0.0),
    )
    .expect("expected extraction to succeed")
    .expect("expected MP4 telemetry activity");

    assert_eq!(activity.file_format.as_deref(), Some("mp4_telemetry"));
    assert_eq!(
        activity.source_start_time.as_deref(),
        Some("2026-03-15T23:58:14+00:00")
    );
    assert_eq!(
        activity
            .metadata
            .get("camera_model")
            .and_then(|value| value.as_str()),
        Some("DJI AC004")
    );
    assert!(
        activity.sample_elapsed_seconds.len() > 100,
        "expected GPS-cadence samples"
    );
    assert!(
        activity.heading.iter().any(|value| value.is_some()),
        "expected heading values"
    );
    assert!(
        activity
            .g_force
            .iter()
            .filter(|value| value.is_some())
            .count()
            > 100,
        "expected IMU g-force values"
    );
    assert_eq!(
        activity
            .metadata
            .get("telemetry_source")
            .and_then(|value| value.as_str()),
        Some("dji_ac004_fallback")
    );
}

#[test]
fn parse_raw_metadata_extracts_ac004_gps_points() {
    let raw = top_level_sample(sample_payload(
        "DJI AC004",
        25.0,
        1,
        50.087_465,
        14.421_254,
        312_400,
        "2026-03-15 23:58:14",
        3.0,
        4.0,
        Some((0.0, 0.0, 1.5)),
    ));

    let telemetry = parse_raw_metadata(&raw).expect("expected AC004 telemetry");

    assert_eq!(telemetry.device_name.as_deref(), Some("DJI AC004"));
    assert_eq!(telemetry.sample_rate_hz, Some(25.0));
    assert_eq!(
        telemetry.sync_time.as_deref(),
        Some("2026-03-15T23:58:14+00:00")
    );
    assert_eq!(telemetry.samples.len(), 1);

    let sample = &telemetry.samples[0];
    assert_eq!(sample.frame_index, 0);
    assert_eq!(sample.timestamp_ms, 0.0);
    assert!((sample.latitude - 50.087_465).abs() < 1e-9);
    assert!((sample.longitude - 14.421_254).abs() < 1e-9);
    assert!((sample.altitude - 312.4).abs() < 1e-9);
    assert!((sample.speed - 5.0).abs() < 1e-9);
    assert!((sample.heading.unwrap() - 36.869_897_645_844_02).abs() < 1e-9);
    assert!((sample.g_force.unwrap() - 0.5).abs() < 1e-9);
    assert_eq!(sample.timestamp, "2026-03-15T23:58:14+00:00");
}

#[test]
fn parse_raw_metadata_uses_sample_rate_for_relative_timing() {
    let mut raw = top_level_sample(sample_payload(
        "DJI AC004",
        50.0,
        1,
        50.0,
        14.0,
        100_000,
        "2026-03-15 23:58:14",
        0.0,
        0.0,
        None,
    ));
    raw.extend(top_level_sample(sample_payload(
        "DJI AC004",
        50.0,
        1,
        50.1,
        14.1,
        101_000,
        "2026-03-15 23:58:14",
        0.0,
        0.0,
        None,
    )));

    let telemetry = parse_raw_metadata(&raw).expect("expected AC004 telemetry");

    assert_eq!(telemetry.samples.len(), 2);
    assert_eq!(telemetry.samples[0].timestamp_ms, 0.0);
    assert!((telemetry.samples[1].timestamp_ms - 20.0).abs() < 1e-9);
}

#[test]
fn parse_raw_metadata_rejects_samples_without_gps_fix() {
    let raw = top_level_sample(sample_payload(
        "DJI AC004",
        25.0,
        0,
        50.087_465,
        14.421_254,
        312_400,
        "2026-03-15 23:58:14",
        3.0,
        4.0,
        None,
    ));

    assert!(parse_raw_metadata(&raw).is_none());
}

fn sample_payload(
    device_name: &str,
    sample_rate_hz: f32,
    fix_type: u64,
    latitude: f64,
    longitude: f64,
    altitude_mm: u64,
    timestamp: &str,
    vx: f32,
    vy: f32,
    acceleration: Option<(f32, f32, f32)>,
) -> Vec<u8> {
    let device = message(vec![
        length_delimited(4, device_name.as_bytes()),
        fixed32(5, sample_rate_hz),
    ]);
    let coords = message(vec![
        varint(1, fix_type),
        fixed64(2, latitude),
        fixed64(3, longitude),
    ]);
    let timestamp_msg = message(vec![length_delimited(1, timestamp.as_bytes())]);
    let fix = message(vec![
        length_delimited(1, &coords),
        varint(2, altitude_mm),
        length_delimited(6, &timestamp_msg),
    ]);
    let velocity = message(vec![fixed32(1, vx), fixed32(2, vy)]);
    let acceleration = acceleration.map(|(x, y, z)| {
        length_delimited(
            10,
            &message(vec![fixed32(2, x), fixed32(3, y), fixed32(4, z)]),
        )
    });
    let mut sensor_fields = Vec::new();
    if let Some(acceleration) = acceleration {
        sensor_fields.push(acceleration);
    }
    message(vec![length_delimited(
        4,
        &message(vec![
            length_delimited(1, &device),
            length_delimited(2, &fix),
            length_delimited(3, &velocity),
        ]),
    )])
    .into_iter()
    .chain(length_delimited(2, &message(sensor_fields)))
    .collect()
}

fn top_level_sample(sample: Vec<u8>) -> Vec<u8> {
    length_delimited(3, &sample)
}

fn message(fields: Vec<Vec<u8>>) -> Vec<u8> {
    fields.into_iter().flatten().collect()
}

fn varint(field: u64, value: u64) -> Vec<u8> {
    let mut out = encode_varint(field << 3);
    out.extend(encode_varint(value));
    out
}

fn fixed64(field: u64, value: f64) -> Vec<u8> {
    let mut out = encode_varint((field << 3) | 1);
    out.extend(value.to_le_bytes());
    out
}

fn length_delimited(field: u64, value: &[u8]) -> Vec<u8> {
    let mut out = encode_varint((field << 3) | 2);
    out.extend(encode_varint(value.len() as u64));
    out.extend(value);
    out
}

fn fixed32(field: u64, value: f32) -> Vec<u8> {
    let mut out = encode_varint((field << 3) | 5);
    out.extend(value.to_le_bytes());
    out
}

fn encode_varint(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            return out;
        }
    }
}

fn dji_fixture() -> (&'static Path, std::path::PathBuf) {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("video")
        .join("DJI-telemetry.MP4");

    assert!(
        fixture.is_file(),
        "expected DJI fixture at {}",
        fixture.display()
    );

    (repo_root, fixture)
}
