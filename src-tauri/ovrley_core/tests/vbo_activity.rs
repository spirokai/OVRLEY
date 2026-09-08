use ovrley_core::activity::vbo::{parse_vbo_activity_path, parse_vbo_activity_reader};
use ovrley_core::commands::backend_parse_vbo_activity;
use ovrley_core::paths::AppPaths;
use std::io::Cursor;

#[test]
fn vbo_reader_builds_and_finalizes_canonical_activity_columns() {
    let vbo = "[column names]\n\
time lat long velocity heading height vert-vel\n\
[data]\n\
120000.000 2820.000 -4920.000 36 180 100 1.5\n\
120000.500 2820.600 -4920.600 72 181 101 2.0\n\
120001.500 2821.200 -4921.200 90 182 102 2.5\n";

    let activity = parse_vbo_activity_reader(Cursor::new(vbo), "session.vbo", None)
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.file_name.as_deref(), Some("session.vbo"));
    assert_eq!(activity.file_format.as_deref(), Some("vbo"));
    assert_eq!(activity.sample_elapsed_seconds, vec![0.0, 0.5, 1.5]);
    assert_eq!(activity.course[0], (Some(47.0), Some(82.0)));
    assert_eq!(activity.speed, vec![Some(10.0), Some(20.0), Some(25.0)]);
    assert_eq!(activity.heading[1], Some(180.05));
    assert_eq!(activity.elevation[2], Some(102.0));
    assert_eq!(activity.vertical_speed[0], Some(1.5 / 3.6));
    assert!(activity.distance.last().copied().flatten().unwrap() > 0.0);
}

#[test]
fn vbo_reader_uses_default_columns_and_handles_midnight_rollover() {
    let vbo = "File created on 17/07/2026 at 23:59:59\n\
[header]\n\
satellites\n\
time\n\
latitude\n\
longitude\n\
velocity kmh\n\
[data]\n\
12 235959.500 2820 -4920 36\n\
12 000000.000 2820.1 -4920.1 37\n\
12 000000.500 2820.2 -4920.2 38\n";

    let activity = parse_vbo_activity_reader(Cursor::new(vbo), "rollover.vbo", None)
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.sample_elapsed_seconds, vec![0.0, 0.5, 1.0]);
    assert_eq!(
        activity.time,
        vec![
            Some("2026-07-17T23:59:59.500Z".to_string()),
            Some("2026-07-18T00:00:00.000Z".to_string()),
            Some("2026-07-18T00:00:00.500Z".to_string()),
        ]
    );
    assert_eq!(
        activity.sync_time.as_deref(),
        Some("2026-07-17T23:59:59.500Z")
    );
    assert_eq!(activity.course[0], (Some(47.0), Some(82.0)));
}

#[test]
fn vbo_reader_preserves_absolute_time_and_explicit_elapsed_time() {
    let vbo = "File created on 17/07/2026 at 12:00:02\n\
[column names]\n\
time elapsed_time speed_mph\n\
[data]\n\
120000.000 5.00 10\n\
120001.000 6.25 20\n";

    let activity = parse_vbo_activity_reader(Cursor::new(vbo), "dual-time.vbo", None)
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.sample_elapsed_seconds, vec![0.0, 1.25]);
    assert_eq!(
        activity.time,
        vec![
            Some("2026-07-17T12:00:00.000Z".to_string()),
            Some("2026-07-17T12:00:01.000Z".to_string()),
        ]
    );
    assert_eq!(activity.speed, vec![Some(4.4704), Some(8.9408)]);
}

#[test]
fn vbo_reader_accepts_elapsed_or_unix_timestamp_as_the_timeline() {
    let elapsed = "[column names]\nelapsed_time velocity\n[data]\n50 36\n51.5 72\n";
    let activity = parse_vbo_activity_reader(Cursor::new(elapsed), "elapsed.vbo", None)
        .unwrap()
        .parsed_activity;
    assert_eq!(activity.sample_elapsed_seconds, vec![0.0, 1.5]);
    assert!(activity.time.iter().all(Option::is_none));

    let timestamp = "[column names]\ntimestamp velocity\n[data]\n1700000000 36\n1700000001.25 72\n";
    let activity = parse_vbo_activity_reader(Cursor::new(timestamp), "timestamp.vbo", None)
        .unwrap()
        .parsed_activity;
    assert_eq!(activity.sample_elapsed_seconds, vec![0.0, 1.25]);
    assert_eq!(
        activity.time,
        vec![
            Some("2023-11-14T22:13:20.000Z".to_string()),
            Some("2023-11-14T22:13:21.250Z".to_string()),
        ]
    );
}

#[test]
fn vbo_reader_resolves_reordered_header_only_layout_by_title() {
    let vbo = "[header]\n\
time\n\
latitude\n\
longitude\n\
velocity kmh\n\
[data]\n\
120000 2820 -4920 36\n\
120001 2821 -4921 72\n";

    let activity = parse_vbo_activity_reader(Cursor::new(vbo), "header-only.vbo", None)
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.sample_elapsed_seconds, vec![0.0, 1.0]);
    assert_eq!(activity.course[0], (Some(47.0), Some(82.0)));
    assert_eq!(activity.speed, vec![Some(10.0), Some(20.0)]);
}

#[test]
fn vbo_reader_maps_supported_racechrono_custom_channels() {
    let vbo = "[column names]\n\
 time lat long rc_lateral_acc_1-canbus longitudinal_acc_1_canbus combined-acc-calculated z-acc_1-acc gear-position-1-canbus engine-rpm-1-obd throttle-position_1-canbus brake-pos_1-canbus engine-load-1-canbus lean-angle-calc\n\
[data]\n\
120000 2820 -4920 0.3 0.4 0.7 0.1 3 4200 55 20 65 -12.5\n\
120001 2821 -4921 0.6 0.8 1.1 0.2 4 4300 60 25 70 8.0\n";

    let activity = parse_vbo_activity_reader(Cursor::new(vbo), "racechrono.vbo", None)
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.g_force_x, vec![Some(0.3), Some(0.6)]);
    assert_eq!(activity.g_force_y, vec![Some(0.4), Some(0.8)]);
    assert_eq!(activity.g_force_z, vec![Some(0.1), Some(0.2)]);
    assert_eq!(activity.g_force, vec![Some(0.7), Some(1.1)]);
    assert_eq!(
        activity.gear_position,
        vec![Some("3".to_string()), Some("4".to_string())]
    );
    assert_eq!(activity.rpm, vec![Some(4200.0), Some(4300.0)]);
    assert_eq!(activity.throttle_position, vec![Some(55.0), Some(60.0)]);
    assert_eq!(activity.brake_position, vec![Some(20.0), Some(25.0)]);
    assert_eq!(activity.engine_load, vec![Some(65.0), Some(70.0)]);
    assert_eq!(activity.lean_angle, vec![Some(-12.5), Some(8.0)]);
}

#[test]
fn vbo_reader_resolves_header_aliases_and_declared_units() {
    let vbo = "[header]\n\
UTC time\n\
latitude\n\
longitude\n\
speed mph\n\
bearing degrees\n\
altitude metres\n\
vertical speed km/h\n\
distance traveled m\n\
[column names]\n\
time rc-latitude-1-gps rc_longitude_1_gps speed-2-canbus bearing-calc altitude-gps vertvel distance-travelled_2-gps\n\
[data]\n\
120000 2820 4920 10 180 100 36 500\n\
120001 2821 4921 20 181 101 72 510\n";

    let activity = parse_vbo_activity_reader(Cursor::new(vbo), "aliases.vbo", None)
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.course[0], (Some(47.0), Some(-82.0)));
    assert_eq!(activity.speed, vec![Some(4.4704), Some(8.9408)]);
    assert_eq!(activity.heading, vec![Some(180.0), Some(180.05)]);
    assert_eq!(activity.elevation, vec![Some(100.0), Some(101.0)]);
    assert_eq!(activity.vertical_speed, vec![Some(10.0), Some(20.0)]);
    assert_eq!(activity.distance, vec![Some(0.0), Some(10.0)]);
}

#[test]
fn vbo_reader_applies_explicit_source_priority() {
    let vbo = "[column names]\n\
time velocity speed_1-canbus latacc-calc lateral_acc_1-canbus\n\
[data]\n\
120000 36 72 1 2\n\
120001 54 90 3 4\n";

    let activity = parse_vbo_activity_reader(Cursor::new(vbo), "priority.vbo", None)
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.speed, vec![Some(10.0), Some(15.0)]);
    assert_eq!(activity.g_force_x, vec![Some(2.0), Some(4.0)]);
}

#[test]
fn vbo_reader_rejects_malformed_present_data_and_timeline_repairs() {
    let bad_number = "[column names]\ntime velocity\n[data]\n120000 10\n120001 nope\n";
    let error = parse_vbo_activity_reader(Cursor::new(bad_number), "bad-number.vbo", None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("VBO line 5 contains invalid numeric value 'nope'"));

    let bad_width = "[column names]\ntime velocity\n[data]\n120000 10\n120001\n";
    let error = parse_vbo_activity_reader(Cursor::new(bad_width), "bad-width.vbo", None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("has 1 values but the column layout declares 2"));

    let decreasing = "[column names]\ntime velocity\n[data]\n120000 10\n115959 11\n";
    let error = parse_vbo_activity_reader(Cursor::new(decreasing), "decreasing.vbo", None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("time must be strictly increasing"));

    let no_time = "[column names]\nvelocity\n[data]\n10\n11\n";
    let error = parse_vbo_activity_reader(Cursor::new(no_time), "no-time.vbo", None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("has no time, timestamp, or elapsed_time column"));

    let misaligned_header = "[header]\ntime\nvelocity kmh\n[column names]\ntime velocity heading\n[data]\n120000 10 90\n120001 11 91\n";
    let error = parse_vbo_activity_reader(
        Cursor::new(misaligned_header),
        "misaligned-header.vbo",
        None,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("header declares 2 channels but the column layout declares 3"));

    let conflicting_names =
        "[header]\ntime\nspeed kmh\n[column names]\ntime rpm\n[data]\n120000 10\n120001 11\n";
    let error = parse_vbo_activity_reader(
        Cursor::new(conflicting_names),
        "conflicting-names.vbo",
        None,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("resolves to conflicting metrics speed and rpm"));

    let decreasing_distance = "[column names]\ntime distance\n[data]\n120000 100\n120001 99\n";
    let error = parse_vbo_activity_reader(
        Cursor::new(decreasing_distance),
        "decreasing-distance.vbo",
        None,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("distance must be non-decreasing"));

    let invalid_creation_date =
        "File created on nonsense\n[column names]\ntime velocity\n[data]\n120000 10\n120001 11\n";
    let error =
        parse_vbo_activity_reader(Cursor::new(invalid_creation_date), "bad-created.vbo", None)
            .unwrap_err()
            .to_string();
    assert!(error.contains("invalid file creation timestamp"));

    let invalid_utf8 = [
        b'[', b'c', b'o', b'l', b'u', b'm', b'n', b' ', b'n', b'a', b'm', b'e', b's', b']', b'\n',
        0xff,
    ];
    let error = parse_vbo_activity_reader(Cursor::new(invalid_utf8), "bad-encoding.vbo", None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("not valid UTF-8"));
}

#[test]
fn racechrono_vbo_fixture_resolves_standard_and_suffixed_channels() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/activity/VBO-test.vbo");
    let activity = parse_vbo_activity_path(&fixture, None)
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.file_format.as_deref(), Some("vbo"));
    assert_close(activity.course[0].0, 1141.673_250 / 60.0);
    assert_close(activity.course[0].1, -5879.371_750 / 60.0);
    assert_close(activity.speed[0], 2.401 / 3.6);
    assert_close(activity.elevation[0], 2292.7);
    assert_close(activity.g_force[0], 0.020);
    assert_close(activity.g_force_x[0], 0.130);
    assert_close(activity.g_force_y[0], 0.003);
    assert_close(activity.rpm[0], 646.0);
    assert_close(activity.throttle_position[0], 0.0);
    assert_close(activity.engine_load[0], 47.0);
    assert_eq!(activity.gear_position[0].as_deref(), Some("1"));
    assert_close(activity.lean_angle[0], -0.351);
}

#[test]
fn vbo_path_and_command_use_the_native_columnar_pipeline() {
    let debug_root = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("ovrley-vbo-command-{}", std::process::id()));
    std::fs::create_dir_all(&debug_root).unwrap();
    let fixture = debug_root.join("command-test.vbo");
    std::fs::write(
        &fixture,
        "[column names]\ntime lat long velocity height\n[data]\n120000 2820 -4920 36 100\n120001 2821 -4921 72 101\n",
    )
    .unwrap();

    let response = parse_vbo_activity_path(&fixture, None).unwrap();
    assert_eq!(response.parsed_activity.file_format.as_deref(), Some("vbo"));
    assert_eq!(response.parsed_activity.sample_elapsed_seconds[0], 0.0);
    assert_eq!(response.parsed_activity.course[0].0, Some(47.0));
    assert_eq!(response.parsed_activity.speed[0], Some(10.0));
    assert_eq!(response.parsed_activity.elevation[0], Some(100.0));

    let paths = AppPaths::from_repo_root(debug_root.clone());
    let response = backend_parse_vbo_activity(&paths, fixture.to_str().unwrap()).unwrap();
    assert!(response.debug_payload.is_none());
    assert_eq!(
        response.parsed_activity.file_name.as_deref(),
        fixture.file_name().unwrap().to_str()
    );
    assert_eq!(response.parsed_activity.file_format.as_deref(), Some("vbo"));
    assert!(debug_root
        .join("debug/activities/command-test-parse-debug.json")
        .is_file());

    std::fs::remove_dir_all(debug_root).unwrap();
}

fn assert_close(actual: Option<f64>, expected: f64) {
    let actual = actual.expect("expected a canonical VBO metric value");
    assert!(
        (actual - expected).abs() < 1e-9,
        "expected {expected}, got {actual}"
    );
}
