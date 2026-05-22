use serde_json::json;

use ovrley_core::encode::video_probe::read_video_stream_duration;

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
    let duration: f64 = format["duration"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

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
