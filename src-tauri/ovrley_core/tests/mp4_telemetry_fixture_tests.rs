use std::fs;
use std::path::Path;

use ovrley_core::media::mp4_telemetry;

#[test]
fn extracts_supported_mp4_telemetry_fixtures_with_provenance() {
    let fixtures = discover_telemetry_fixtures();
    assert!(!fixtures.is_empty(), "no telemetry fixtures found");

    let repo_root = repo_root();
    for filename in &fixtures {
        let fixture = video_fixture(filename);
        let stem = Path::new(filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("video");

        let metadata = mp4_telemetry::probe_video_metadata(fixture.to_str().unwrap())
            .unwrap_or_else(|err| panic!("{stem}: probe failed: {err}"));
        let activity = mp4_telemetry::extract_activity(
            repo_root,
            fixture.to_str().unwrap(),
            metadata.fps.unwrap_or(30.0),
            metadata.duration.unwrap_or(0.0),
        )
        .unwrap_or_else(|err| panic!("{stem}: extraction failed: {err}"))
        .unwrap_or_else(|| panic!("{stem}: expected telemetry activity"));

        assert_eq!(
            activity.file_format.as_deref(),
            Some("mp4_telemetry"),
            "{stem}: file_format"
        );

        let expected_source = if filename == "DJI-telemetry.MP4" {
            "dji_ac004_fallback"
        } else {
            "telemetry_parser"
        };
        assert_eq!(
            activity
                .metadata
                .get("telemetry_source")
                .and_then(|value| value.as_str()),
            Some(expected_source),
            "{stem}: telemetry_source"
        );

        let has_usable_gps = fixture_has_usable_gps(filename);
        let expected_timeline = if has_usable_gps {
            "gps_anchored"
        } else {
            "video_derived"
        };
        assert_eq!(
            activity
                .metadata
                .get("timeline_kind")
                .and_then(|value| value.as_str()),
            Some(expected_timeline),
            "{stem}: timeline_kind"
        );

        let gps_count = metadata_count(&activity.metadata, "gps_sample_count", stem);
        let imu_count = metadata_count(&activity.metadata, "imu_sample_count", stem);
        let camera_count = metadata_count(&activity.metadata, "camera_sample_count", stem);
        let total_count = metadata_count(&activity.metadata, "telemetry_sample_count", stem);
        assert_eq!(
            total_count,
            gps_count + imu_count + camera_count,
            "{stem}: telemetry_sample_count"
        );

        if has_usable_gps {
            assert!(gps_count > 0, "{stem}: expected GPS samples");
        } else {
            assert_eq!(gps_count, 0, "{stem}: invalid GPS must be filtered");
            assert!(
                activity
                    .sample_course_points
                    .iter()
                    .all(|(lat, lon)| lat.is_none() && lon.is_none()),
                "{stem}: invalid GPS must not produce course points"
            );
        }

        if filename == "GoPro-telemetry.MP4" {
            assert!(
                (activity.sample_elapsed_seconds[0] - 0.110_097).abs() < 1e-9,
                "{stem}: GPS timeline must remain video-relative"
            );
            assert!(camera_count > 0, "{stem}: expected camera telemetry");
        }

        if filename == "DJI-telemetry.MP4" {
            assert_eq!(
                activity.sync_time.as_deref(),
                Some("2026-03-15T23:58:14+00:00"),
                "{stem}: sync_time"
            );
            assert!(imu_count > 0, "{stem}: expected IMU telemetry");
        }
    }
}

fn discover_telemetry_fixtures() -> Vec<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("video");
    let mut fixtures: Vec<String> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read fixture dir {dir:?}: {e}"))
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().to_string_lossy().to_string();
            let lower = name.to_lowercase();
            lower.contains("telemetry").then_some(name)
        })
        .collect();
    fixtures.sort();
    fixtures
}

fn metadata_count(metadata: &serde_json::Value, field: &str, stem: &str) -> u64 {
    metadata
        .get(field)
        .and_then(|value| value.as_u64())
        .unwrap_or_else(|| panic!("{stem}: metadata.{field} must be a count"))
}

fn fixture_has_usable_gps(filename: &str) -> bool {
    filename != "Hero8-telemetry.mp4"
}

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
}

fn video_fixture(filename: &str) -> std::path::PathBuf {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("video")
        .join(filename);

    assert!(
        fixture.is_file(),
        "expected video fixture at {}",
        fixture.display()
    );
    fixture
}
