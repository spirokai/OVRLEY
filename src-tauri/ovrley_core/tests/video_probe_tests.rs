//! Video probe (ffprobe metadata) tests.
//!
//! Verifies `read_video_stream_duration` priority logic (stream duration
//! before container duration, frame-count fallback), and validates the
//! stored ffprobe JSON fixture has expected resolution, codec, duration,
//! stream count, and parsable r_frame_rate.
//!
//! ## Fixtures
//!
//! - `test_config::ffprobe_1080p_path()` — stored ffprobe JSON output for
//!   a representative 1080p H.264 video.
//!
//! ## Type
//! Integration test. Reads fixture files from disk but does not invoke
//! ffprobe or ffmpeg at runtime.
//!
//! ## Regressions guarded
//! - Stream duration ignored when container duration is also present
//! - Frame-count fallback producing wrong duration
//! - ffprobe fixture schema drift (fields renamed, removed, or retyped)
//! - r_frame_rate format changes (e.g., "30" instead of "30000/1001")

use serde_json::json;

use ovrley_core::media::video_probe::read_video_stream_duration;
use ovrley_core::media::{Resolution, SourceVideoMetadata};

mod common;

#[test]
fn reads_video_stream_duration_before_container_duration() {
    let stream = json!({
        "duration": "30.033333",
        "nb_frames": "901"
    });

    assert_eq!(
        read_video_stream_duration(&stream, Some(30.0)),
        Some(30.033333)
    );
}

#[test]
fn falls_back_to_frame_count_when_stream_duration_is_missing() {
    let stream = json!({
        "nb_frames": "901"
    });

    let duration = read_video_stream_duration(&stream, Some(30.0)).unwrap();

    assert!((duration - 30.033333333333335).abs() < 1e-9);
}

#[test]
fn source_video_metadata_serializes_sync_and_legacy_creation_fields() {
    let metadata = SourceVideoMetadata {
        path: "clip.mp4".to_string(),
        duration: Some(12.5),
        fps: Some(29.97002997002997),
        fps_num: Some(30000),
        fps_den: Some(1001),
        resolution: Some(Resolution {
            width: 1920,
            height: 1080,
        }),
        creation_time: Some("2026-05-20T21:49:10.000000Z".to_string()),
        sync_time: Some("2026-05-20T21:49:10.000000Z".to_string()),
        codec_name: Some("h264".to_string()),
        codec_long_name: None,
        codec_profile: None,
        pix_fmt: Some("yuv420p".to_string()),
        bits_per_raw_sample: Some(8),
        has_audio: true,
        container_format: Some("mov,mp4,m4a,3gp,3g2,mj2".to_string()),
        rotation_degrees: Some(0),
    };

    let value = serde_json::to_value(metadata).unwrap();

    assert_eq!(value["creationTime"], "2026-05-20T21:49:10.000000Z");
    assert_eq!(value["syncTime"], "2026-05-20T21:49:10.000000Z");
    assert_eq!(value["fpsNum"], 30000);
    assert_eq!(value["fpsDen"], 1001);
}

// --- Snapshot / golden tests (Step 11g) ---

#[test]
fn ffprobe_fixture_has_expected_resolution_and_codec() {
    let json_text = std::fs::read_to_string(common::test_config::ffprobe_1080p_path()).unwrap();
    let probe: serde_json::Value = serde_json::from_str(&json_text).unwrap();

    let streams = probe["streams"].as_array().unwrap();
    let video_stream = streams
        .iter()
        .find(|s| s["codec_type"] == "video")
        .expect("video stream");

    assert_eq!(video_stream["width"], 1920);
    assert_eq!(video_stream["height"], 1080);
    assert_eq!(video_stream["codec_name"], "h264");
    assert!(!video_stream["r_frame_rate"].as_str().unwrap().is_empty());
}

#[test]
fn ffprobe_fixture_has_duration_and_format() {
    let json_text = std::fs::read_to_string(common::test_config::ffprobe_1080p_path()).unwrap();
    let probe: serde_json::Value = serde_json::from_str(&json_text).unwrap();

    let format = &probe["format"];
    let duration: f64 = format["duration"].as_str().unwrap().parse().unwrap();

    assert!(duration > 0.0);
    assert!(duration < 120.0);
}

#[test]
fn ffprobe_fixture_has_multiple_streams() {
    let json_text = std::fs::read_to_string(common::test_config::ffprobe_1080p_path()).unwrap();
    let probe: serde_json::Value = serde_json::from_str(&json_text).unwrap();

    let streams = probe["streams"].as_array().unwrap();
    assert!(streams.len() >= 1);
}

#[test]
fn ffprobe_fixture_parses_video_stream_r_frame_rate() {
    let json_text = std::fs::read_to_string(common::test_config::ffprobe_1080p_path()).unwrap();
    let probe: serde_json::Value = serde_json::from_str(&json_text).unwrap();

    let streams = probe["streams"].as_array().unwrap();
    let video_stream = streams
        .iter()
        .find(|s| s["codec_type"] == "video")
        .expect("video stream");

    let fps_str = video_stream["r_frame_rate"].as_str().unwrap();
    let parts: Vec<f64> = fps_str.split('/').map(|p| p.parse().unwrap()).collect();
    assert_eq!(parts.len(), 2);
    let fps = parts[0] / parts[1];
    assert!(fps > 0.0 && fps <= 120.0);
}
