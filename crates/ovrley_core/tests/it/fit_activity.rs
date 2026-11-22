use ovrley_core::activity::parsers::fit::{extract_fit_activity, parse_fit_activity_path};
use ovrley_core::activity::schema::{LapMarkers, RawActivity};
use std::path::Path;

fn fixture(name: &str) -> RawActivity {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/activity")
        .join(name);
    extract_fit_activity(&std::fs::read(path).unwrap(), name).unwrap()
}

#[test]
fn imports_lap_boundaries_and_decimal_degree_positions() {
    let activity = fixture("test-ride.fit");

    assert_eq!(activity.file_format, "fit");
    assert_eq!(activity.metadata["sport"], "cycling");
    assert_eq!(activity.metadata["creator"], "Intervals.icu");
    assert_eq!(activity.metadata["source_device_product"], "Intervals.icu");
    assert_eq!(
        activity.metadata["source_device_manufacturer"],
        "development"
    );
    assert_eq!(
        activity.metadata["file_created_at"],
        "2026-05-02T18:40:03.000Z"
    );
    assert_eq!(
        activity.sync_time.as_deref(),
        Some("2026-05-02T17:00:18.000Z")
    );
    let LapMarkers::BeaconMarkers(markers) = &activity.lap_markers else {
        panic!("expected lap markers");
    };
    assert_eq!(markers.len(), 15);
    assert_eq!(markers[..3], [0.0, 1248.0, 1298.0]);
    assert!(!activity.options.skip_idle_gap_fill);
    let sample = activity
        .raw_samples
        .iter()
        .find(|sample| sample.latitude.is_some())
        .unwrap();
    assert!(sample.latitude.unwrap().abs() <= 90.0);
    assert!(sample.longitude.unwrap().abs() <= 180.0);
}

#[test]
fn syncs_to_the_first_record_time_instead_of_the_file_creation_time() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/activity/test-ride.fit");
    let activity = parse_fit_activity_path(&path, None)
        .unwrap()
        .parsed_activity;

    assert_eq!(
        activity.metadata["file_created_at"],
        "2026-05-02T18:40:03.000Z"
    );
    assert_eq!(
        activity.sync_time.as_deref(),
        Some("2026-05-02T17:00:18.000Z")
    );
}

/// This fixture's records also carry a developer field without a description.
#[test]
fn imports_dive_units_gas_switches_and_water_temperature() {
    let activity = fixture("test-dive.fit");
    let samples = &activity.raw_samples;

    assert!(activity.options.skip_idle_gap_fill);
    assert_eq!(
        samples
            .iter()
            .map(|sample| sample.depth.unwrap())
            .collect::<Vec<_>>(),
        [1.0, 5.0, 10.0, 15.0, 20.0, 12.0, 2.0]
    );
    assert_eq!(samples[0].fraction_o2, Some(0.21));
    assert_eq!(samples[0].temperature, Some(19.0));
    assert_eq!(samples[0].water_temperature, Some(19.0));
    assert_eq!(samples[4].first_stop_depth, Some(3.0));
    assert_eq!(samples[4].first_stop_time, Some(2.0));
    assert_eq!(samples[4].time_to_surface, Some(5.0));
    assert_eq!(samples[4].fraction_o2, Some(0.5));
    assert_eq!(samples[4].fraction_he, Some(0.0));
}

#[test]
fn rejects_unreadable_files_and_files_without_records() {
    let error = |bytes: &[u8]| {
        extract_fit_activity(bytes, "error.fit")
            .unwrap_err()
            .to_string()
    };
    let header_only = b"\x0c\x10\x00\x00\x00\x00\x00\x00.FIT";

    assert!(error(b"not a fit file").contains("could not be parsed"));
    assert!(error(header_only).contains("does not contain any record messages"));
}
