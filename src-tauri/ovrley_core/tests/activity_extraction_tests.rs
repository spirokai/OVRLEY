use std::collections::HashMap;
use std::fs;
use std::path::Path;

use ovrley_core::media::mp4_telemetry;

#[test]
fn extract_activity_from_telemetry_fixtures() {
    let known_sync_times: HashMap<&str, &str> = [
        ("DJI-telemetry.MP4", "2026-03-15T23:58:14+00:00"),
        ("GoPro-telemetry.MP4", "2024-08-05T12:28:30.063903+00:00"),
    ]
    .iter()
    .cloned()
    .collect();

    let fixtures = discover_telemetry_fixtures();
    assert!(!fixtures.is_empty(), "no telemetry fixtures found");

    let output_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("activity");
    fs::create_dir_all(&output_dir).unwrap();

    let repo_root = repo_root();
    for filename in &fixtures {
        let fixture = video_fixture(filename);
        let stem = Path::new(filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("video");

        let metadata = mp4_telemetry::probe_video_metadata(fixture.to_str().unwrap())
            .unwrap_or_else(|err| panic!("{stem}: probe failed: {err}"));

        let fps = metadata.fps.unwrap_or(30.0);
        let duration_s = metadata.duration.unwrap_or(0.0);

        let activity =
            mp4_telemetry::extract_activity(repo_root, fixture.to_str().unwrap(), fps, duration_s)
                .unwrap_or_else(|err| panic!("{stem}: extraction failed: {err}"))
                .unwrap_or_else(|| panic!("{stem}: expected activity, got None"));

        // Write artifact
        fs::write(
            output_dir.join(format!("{stem}-activity.json")),
            serde_json::to_string_pretty(&activity).unwrap(),
        )
        .unwrap();

        // ── Quality assertions ──────────────────────────────────

        let n = activity.sample_elapsed_seconds.len();
        assert!(n > 0, "{stem}: expected at least one sample");

        if filename == "GoPro-telemetry.MP4" {
            assert!(
                (activity.sample_elapsed_seconds[0] - 0.110_097).abs() < 1e-9,
                "{stem}: embedded GPS timeline must stay video-relative"
            );
        }

        // All metric series must match the GPS-cadence length
        assert_eq!(
            n,
            activity.sample_distance_progress.len(),
            "{stem}: sample_distance_progress"
        );
        assert_eq!(
            n,
            activity.sample_course_points.len(),
            "{stem}: sample_course_points"
        );
        assert_eq!(n, activity.course.len(), "{stem}: course");
        assert_eq!(n, activity.elevation.len(), "{stem}: elevation");
        assert_eq!(n, activity.speed.len(), "{stem}: speed");
        assert_eq!(n, activity.distance.len(), "{stem}: distance");
        assert_eq!(n, activity.heading.len(), "{stem}: heading");
        assert_eq!(n, activity.time.len(), "{stem}: time");
        assert_eq!(n, activity.g_force.len(), "{stem}: g_force");
        assert_eq!(n, activity.iso.len(), "{stem}: iso");
        assert_eq!(n, activity.aperture.len(), "{stem}: aperture");
        assert_eq!(n, activity.shutter_speed.len(), "{stem}: shutter_speed");
        assert_eq!(n, activity.focal_length.len(), "{stem}: focal_length");
        assert_eq!(n, activity.ev.len(), "{stem}: ev");
        assert_eq!(
            n,
            activity.color_temperature.len(),
            "{stem}: color_temperature"
        );

        let has_usable_gps = fixture_has_usable_gps(filename);

        // Course points must be populated only when the source has a usable GPS fix.
        let non_null_course = activity
            .sample_course_points
            .iter()
            .filter(|(lat, lon)| lat.is_some() && lon.is_some())
            .count();
        if has_usable_gps {
            assert_eq!(
                non_null_course, n,
                "{stem}: expected all {n} course points non-null, got {non_null_course}"
            );
        } else {
            assert_eq!(
                non_null_course, 0,
                "{stem}: invalid GPS must not produce course points"
            );
        }

        // Speed should be non-null for most samples only when GPS is usable.
        let non_null_speed = activity.speed.iter().filter(|v| v.is_some()).count();
        if has_usable_gps {
            assert!(
                non_null_speed >= n / 2,
                "{stem}: expected >= half ({n2}) speed non-null, got {non_null_speed}",
                n2 = n / 2
            );
        } else {
            assert_eq!(
                non_null_speed, 0,
                "{stem}: invalid GPS must not produce speed"
            );
        }

        // IMU presence check
        let has_any_g_force = activity.g_force.iter().any(|v| v.is_some());
        if has_any_g_force && has_usable_gps {
            assert!(
                non_null_course > 0,
                "{stem}: g_force present but no course points?"
            );
        }

        // Non-MP4 fields must all be empty
        assert!(
            activity.heartrate.is_empty(),
            "{stem}: heartrate must be empty"
        );
        assert!(activity.cadence.is_empty(), "{stem}: cadence must be empty");
        assert!(activity.power.is_empty(), "{stem}: power must be empty");
        assert!(
            activity.temperature.is_empty(),
            "{stem}: temperature must be empty"
        );
        assert!(activity.pace.is_empty(), "{stem}: pace must be empty");
        assert!(
            activity.air_pressure.is_empty(),
            "{stem}: air_pressure must be empty"
        );
        assert!(
            activity.ground_contact_time.is_empty(),
            "{stem}: ground_contact_time must be empty"
        );
        assert!(
            activity.stride_length.is_empty(),
            "{stem}: stride_length must be empty"
        );
        assert!(
            activity.stroke_rate.is_empty(),
            "{stem}: stroke_rate must be empty"
        );
        assert!(activity.torque.is_empty(), "{stem}: torque must be empty");
        assert!(
            activity.gear_position.is_empty(),
            "{stem}: gear_position must be empty"
        );

        // Sync time: assert only for known fixtures
        if let Some(expected) = known_sync_times.get(filename.as_str()) {
            assert_eq!(
                activity.source_start_time.as_deref(),
                Some(*expected),
                "{stem}: sync time mismatch"
            );
        }

        // File format / metadata structure
        assert_eq!(
            activity.file_format.as_deref(),
            Some("mp4_telemetry"),
            "{stem}: file_format"
        );
        assert!(
            activity.trim_end_seconds > 0.0,
            "{stem}: trim_end_seconds > 0"
        );
        assert_eq!(
            activity.trim_start_seconds, 0.0,
            "{stem}: trim_start_seconds must be 0"
        );
        assert!(
            activity.trim_end_seconds >= duration_s - 0.1,
            "{stem}: trim_end {end} must cover probe duration {duration_s}",
            end = activity.trim_end_seconds
        );

        // Distance progress must be non-decreasing and end near 1.0
        let last_progress = activity
            .sample_distance_progress
            .last()
            .copied()
            .unwrap_or(0.0);
        if has_usable_gps {
            assert!(
                (last_progress - 1.0).abs() < 1e-6,
                "{stem}: expected distance_progress[-1] ≈ 1.0, got {last_progress}"
            );
        } else {
            assert_eq!(
                last_progress, 0.0,
                "{stem}: no usable GPS should leave distance progress at 0"
            );
        }
        for window in activity.sample_distance_progress.windows(2) {
            assert!(
                window[0] <= window[1] + 1e-9,
                "{stem}: distance_progress non-decreasing ({a} > {b})",
                a = window[0],
                b = window[1]
            );
        }

        // Metadata duration should match trim_end
        let metadata_dur = activity
            .metadata
            .get("duration_seconds")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        assert!(
            (metadata_dur - activity.trim_end_seconds).abs() < 0.1,
            "{stem}: metadata duration {metadata_dur} vs trim_end {te} mismatch",
            te = activity.trim_end_seconds
        );
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

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
}

fn fixture_has_usable_gps(filename: &str) -> bool {
    filename != "Hero8-telemetry.mp4"
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
