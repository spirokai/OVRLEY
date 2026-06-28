use ovrley_core::media::mp4_telemetry;
use serde_json::Value;
use std::fs;
use std::path::Path;

#[test]
fn extracts_supported_mp4_telemetry_fixtures_through_one_pipeline() {
    let cases = [
        FixtureCase {
            name: "DJI AC004",
            filename: "DJI-telemetry.MP4",
            expected_sync_time: "2026-03-15T23:58:14+00:00",
            min_samples: 100,
            min_gps_samples: 100,
        },
        FixtureCase {
            name: "GoPro GPS5",
            filename: "GoPro-telemetry.MP4",
            expected_sync_time: "2024-08-05T12:28:30.174+00:00",
            min_samples: 20,
            min_gps_samples: 20,
        },
        FixtureCase {
            name: "Hero8 GPS5",
            filename: "Hero8-telemetry.mp4",
            expected_sync_time: "2019-11-18T23:42:08.645+00:00",
            min_samples: 100,
            min_gps_samples: 100,
        },
    ];

    let repo_root = repo_root();
    for case in cases {
        let fixture = video_fixture(case.filename);
        let activity = mp4_telemetry::extract_telemetry(repo_root, fixture.to_str().unwrap())
            .unwrap_or_else(|error| panic!("{}: extraction failed: {error}", case.name))
            .unwrap_or_else(|| panic!("{}: expected telemetry activity", case.name));
        write_normalized_debug_output(repo_root, case.filename, &activity);

        assert_eq!(
            activity["fileFormat"].as_str(),
            Some("mp4_telemetry"),
            "{}: unexpected file format",
            case.name
        );
        assert_eq!(
            activity["syncTime"].as_str(),
            Some(case.expected_sync_time),
            "{}: unexpected syncTime",
            case.name
        );

        assert!(
            activity.get("rawSamples").is_none(),
            "{}: normalized payload should be columnar and must not include rawSamples",
            case.name
        );

        let series = activity
            .get("series")
            .unwrap_or_else(|| panic!("{}: normalized payload must include series", case.name));
        let gps = series
            .get("gps")
            .unwrap_or_else(|| panic!("{}: normalized payload must include GPS series", case.name));
        let gps_time_ms = series_array(gps, "timeMs", case.name);
        assert!(
            activity["metadata"]["telemetry_sample_count"]
                .as_u64()
                .unwrap_or_default()
                >= case.min_samples as u64,
            "{}: expected at least {} telemetry samples, got {}",
            case.name,
            case.min_samples,
            activity["metadata"]["telemetry_sample_count"]
        );
        assert!(
            gps_time_ms.len() >= case.min_gps_samples,
            "{}: expected at least {} GPS samples, got {}",
            case.name,
            case.min_gps_samples,
            gps_time_ms.len()
        );

        assert_equal_series_lengths(
            gps,
            &[
                "timestamp",
                "latitude",
                "longitude",
                "altitude",
                "elevation",
                "speed",
                "heading",
            ],
            case.name,
        );
        assert_equal_series_lengths(
            series.get("imu").expect("IMU series"),
            &["gForce"],
            case.name,
        );
        assert_equal_series_lengths(
            series.get("camera").expect("camera series"),
            &[
                "iso",
                "aperture",
                "shutterSpeed",
                "focalLength",
                "ev",
                "colorTemperature",
            ],
            case.name,
        );

        assert!(
            gps.get("gForce").is_none(),
            "{}: GPS series must not carry g-force from another cadence",
            case.name
        );
        assert!(
            !gps_has_camera_scalar(gps),
            "{}: GPS series must not carry camera scalars from another cadence",
            case.name
        );

        assert_eq!(
            activity["metadata"]["gps_sample_count"].as_u64(),
            Some(gps_time_ms.len() as u64),
            "{}: gps_sample_count must match GPS timeMs length",
            case.name,
        );
    }
}

struct FixtureCase {
    name: &'static str,
    filename: &'static str,
    expected_sync_time: &'static str,
    min_samples: usize,
    min_gps_samples: usize,
}

fn series_array<'a>(series: &'a Value, field: &str, case_name: &str) -> &'a Vec<Value> {
    series
        .get(field)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{case_name}: series field {field} must be an array"))
}

fn assert_equal_series_lengths(series: &Value, fields: &[&str], case_name: &str) {
    let time_ms_len = series_array(series, "timeMs", case_name).len();
    for field in fields {
        let field_len = series_array(series, field, case_name).len();
        assert_eq!(
            field_len, time_ms_len,
            "{case_name}: series field {field} length must match timeMs length"
        );
    }
}

fn gps_has_camera_scalar(gps: &Value) -> bool {
    [
        "iso",
        "aperture",
        "shutterSpeed",
        "focalLength",
        "ev",
        "colorTemperature",
    ]
    .iter()
    .any(|field| gps.get(*field).is_some())
}

fn write_normalized_debug_output(repo_root: &Path, fixture_filename: &str, activity: &Value) {
    let debug_dir = repo_root.join("debug").join("mp4telemetry");
    fs::create_dir_all(&debug_dir).unwrap_or_else(|error| {
        panic!(
            "failed to create MP4 telemetry debug directory {}: {error}",
            debug_dir.display()
        )
    });

    let stem = Path::new(fixture_filename)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("video");
    let output_path = debug_dir.join(format!("{stem}-normalized.json"));
    let json = serde_json::to_string_pretty(activity)
        .unwrap_or_else(|error| panic!("failed to serialize normalized telemetry JSON: {error}"));

    fs::write(&output_path, json).unwrap_or_else(|error| {
        panic!(
            "failed to write normalized telemetry debug output {}: {error}",
            output_path.display()
        )
    });
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
