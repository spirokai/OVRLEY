use chrono::{
    Duration, Local, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, Offset, SecondsFormat,
    TimeZone, Utc,
};
use ovrley_core::activity::csv::{parse_csv_activity_path, parse_csv_activity_reader};
use ovrley_core::activity::interpolate::densify_activity;
use ovrley_core::activity::trim::trim_activity;
use ovrley_core::commands::backend_parse_csv_activity;
use ovrley_core::normalize::RenderDataRequirements;
use std::io::Cursor;

#[test]
fn trackaddict_gps_updates_preserve_sparse_gps_and_dense_acceleration() {
    let csv = "Time,GPS_Update,Latitude,Longitude,Altitude (m),Speed (Km/h),Heading,Accel X\n\
0.0,1,10.0,20.0,100.0,36.0,90.0,0.1\n\
0.1,0,10.0,20.0,100.0,36.0,90.0,0.2\n\
1.0,1,10.1,20.1,110.0,72.0,100.0,0.3\n";

    let activity = parse_csv_activity_reader(Cursor::new(csv), "trackaddict.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.sample_elapsed_seconds, vec![0.0, 0.1, 1.0]);
    assert_eq!(
        activity.course,
        vec![
            (Some(10.0), Some(20.0)),
            (None, None),
            (Some(10.1), Some(20.1)),
        ]
    );
    assert_eq!(activity.elevation, vec![Some(100.0), None, Some(110.0)]);
    assert_eq!(activity.speed, vec![Some(10.0), None, Some(20.0)]);
    assert_eq!(activity.heading, vec![Some(90.0), None, Some(90.498)]);
    assert_eq!(activity.g_force_x, vec![Some(0.1), Some(0.2), Some(0.3)]);
    assert_eq!(activity.sample_distance_progress, vec![0.0, 0.1, 1.0]);

    let requirements = RenderDataRequirements {
        speed: true,
        elevation: true,
        heading: true,
        distance_progress: true,
        course: true,
        ..RenderDataRequirements::default()
    };
    let trimmed = trim_activity(&activity, 0.0, 1.0, &requirements).unwrap();
    let dense = densify_activity(
        &trimmed,
        ovrley_core::activity::interpolate::frame_timeline_for_fps(1.0, 2.0).unwrap(),
        &requirements,
    );

    assert_eq!(dense.series.speed[1], Some(15.0));
    assert_eq!(dense.series.elevation[1], Some(105.0));
    assert_eq!(dense.series.heading[1], Some(90.249));
    assert_eq!(dense.series.course_lat[1], Some(10.05));
    assert_eq!(dense.series.course_lon[1], Some(20.05));
    assert_eq!(dense.frame_distance_progress[1], Some(0.5));
}

#[test]
fn trackaddict_rejects_malformed_gps_update_values() {
    let csv = "Time,GPS_Update,Latitude,Longitude\n\
0.0,1,10.0,20.0\n\
0.1,true,10.0,20.0\n";

    let error = parse_csv_activity_reader(Cursor::new(csv), "trackaddict.csv").unwrap_err();

    assert!(
        error
            .to_string()
            .contains("CSV row 3 GPS_Update must be 0 or 1"),
        "{error}"
    );
}

#[test]
fn gps_update_does_not_mask_independently_sampled_vehicle_speed() {
    let csv = "Time,GPS_Update,Vehicle Speed (m/s),Accel X\n\
0.0,1,10.0,0.1\n\
0.1,0,11.0,0.2\n";

    let activity = parse_csv_activity_reader(Cursor::new(csv), "mixed-rate.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.speed, vec![Some(10.0), Some(11.0)]);
    assert_eq!(activity.g_force_x.len(), 2);
    assert!(activity.g_force_x[0].is_some());
    assert!(activity.g_force_x[1].is_some());
}

#[test]
fn amozoc_trackaddict_fixture_uses_gps_update_cadence() {
    let activity = parse_fixture("Amozoc - TrackAddict.csv");
    let sample_count = activity.sample_elapsed_seconds.len();
    let gps_count = activity
        .course
        .iter()
        .filter(|(latitude, longitude)| latitude.is_some() && longitude.is_some())
        .count();

    assert_eq!(sample_count, 10_675);
    assert_eq!(gps_count, 531);
    assert_eq!(
        activity
            .speed
            .iter()
            .filter(|value| value.is_some())
            .count(),
        531
    );
    assert_eq!(
        activity
            .heading
            .iter()
            .filter(|value| value.is_some())
            .count(),
        531
    );
    assert!(
        activity
            .g_force_x
            .iter()
            .filter(|value| value.is_some())
            .count()
            > 6_000
    );
    assert!(activity
        .sample_distance_progress
        .windows(2)
        .all(|pair| pair[0] <= pair[1]));
    assert_eq!(activity.sample_distance_progress.last(), Some(&1.0));
}

#[test]
fn racebox_reader_produces_canonical_activity_columns() {
    let csv = "Record,Time,Latitude,Longitude,Altitude,KPH\n\
1,10.000,34.4889022,-80.5974243,147.8,5.46\n\
2,10.040,34.4889018,-80.5974241,147.9,7.20\n";

    let response = parse_csv_activity_reader(Cursor::new(csv), "racebox.csv").unwrap();
    let activity = response.parsed_activity;

    assert_eq!(activity.file_format.as_deref(), Some("csv"));
    assert_eq!(activity.sample_elapsed_seconds, vec![0.0, 0.04]);
    assert!(activity
        .sample_elapsed_seconds
        .windows(2)
        .all(|pair| pair[0] < pair[1]));
    assert_eq!(
        activity.course,
        vec![
            (Some(34.4889022), Some(-80.5974243)),
            (Some(34.4889018), Some(-80.5974241)),
        ]
    );
    assert_eq!(activity.elevation, vec![Some(147.8), Some(147.9)]);
    assert_eq!(activity.speed, vec![Some(1.5166666666666666), Some(2.0)]);
}

#[test]
fn katana_fixture_reconstructs_time_gps_and_telemetry() {
    let activity = parse_fixture("Katana-2026-07-25-111811.csv");

    assert_eq!(activity.sample_elapsed_seconds.len(), 366);
    assert_eq!(activity.sample_elapsed_seconds[0], 0.0);
    assert_eq!(
        activity.sample_elapsed_seconds.last().copied(),
        Some(365.0)
    );
    assert!(activity
        .sample_elapsed_seconds
        .windows(2)
        .all(|pair| pair[0] < pair[1]));

    assert_eq!(
        activity.time.first().and_then(|value| value.as_deref()),
        Some("2026-07-25T11:18:11.160Z")
    );
    assert_eq!(
        activity.time.last().and_then(|value| value.as_deref()),
        Some("2026-07-25T11:24:16.160Z")
    );
    assert_eq!(
        activity.sync_time.as_deref(),
        Some("2026-07-25T11:18:11.160Z")
    );

    assert_eq!(
        activity.course[0],
        (Some(51.178278), Some(0.301071))
    );
    assert!(activity.course.iter().any(|(lat, lon)| lat.is_some() && lon.is_some()));

    assert_eq!(activity.speed[0], Some(0.0));
    assert!(activity.speed.iter().any(Option::is_some));

    assert_eq!(activity.heading[0], Some(0.0));
    assert!(activity.heading.iter().any(Option::is_some));

    assert_eq!(activity.elevation[0], Some(149.0));
    assert!(activity.elevation.iter().any(Option::is_some));
}

#[test]
fn maps_explicit_utc_and_paired_elapsed_absolute_timing_semantically() {
    let utc_csv = "UTC Time,Speed\n\
1700000000.250,36\n\
1700000001.750,72\n";
    let utc_activity = parse_csv_activity_reader(Cursor::new(utc_csv), "utc.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(utc_activity.sample_elapsed_seconds, vec![0.0, 1.5]);
    assert_eq!(
        utc_activity.sync_time.as_deref(),
        Some("2023-11-14T22:13:20.250Z")
    );
    assert_eq!(
        utc_activity.time,
        vec![
            Some("2023-11-14T22:13:20.250Z".to_string()),
            Some("2023-11-14T22:13:21.750Z".to_string()),
        ]
    );

    let paired_csv = "Time (s),Elapsed time (s),Speed\n\
1700000000.250,42.5,36\n\
1700000001.750,44.0,72\n";
    let paired_activity = parse_csv_activity_reader(Cursor::new(paired_csv), "paired.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(paired_activity.sample_elapsed_seconds, vec![0.0, 1.5]);
    assert_eq!(
        paired_activity.sync_time.as_deref(),
        Some("2023-11-14T22:13:20.250Z")
    );
}

#[test]
fn parses_explicit_offset_timestamps_without_numeric_guessing() {
    let csv = "Timestamp,Speed\n\
2026-07-13T18:58:59.125+02:00,36\n\
2026-07-13T18:59:00.625+02:00,72\n";

    let activity = parse_csv_activity_reader(Cursor::new(csv), "rfc3339.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.sample_elapsed_seconds, vec![0.0, 1.5]);
    assert_eq!(
        activity.sync_time.as_deref(),
        Some("2026-07-13T16:58:59.125Z")
    );
}

#[test]
fn keeps_bare_numeric_timestamps_elapsed_and_invalid_local_metadata_optional() {
    let bare_csv = "Timestamp,Speed\n\
1700000000,36\n\
1700000001,72\n";
    let bare_activity = parse_csv_activity_reader(Cursor::new(bare_csv), "bare.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(bare_activity.sample_elapsed_seconds, vec![0.0, 1.0]);
    assert!(bare_activity.sync_time.is_none());

    let explicit_csv = "Date,not-a-date\n\
Time,not-a-time\n\
Elapsed time (ms),Speed\n\
5000,36\n\
6500,72\n";
    let explicit_activity =
        parse_csv_activity_reader(Cursor::new(explicit_csv), "explicit-elapsed.csv")
            .unwrap()
            .parsed_activity;

    assert_eq!(explicit_activity.sample_elapsed_seconds, vec![0.0, 1.5]);
    assert!(explicit_activity.sync_time.is_none());

    let malformed_utc_csv = "Date,\"Saturday, February 14, 2026\"\n\
Time,8:30 AM\n\
Time,UTC Time,Speed\n\
0,invalid,36\n\
1,invalid,72\n";
    let malformed_utc_activity =
        parse_csv_activity_reader(Cursor::new(malformed_utc_csv), "malformed-utc.csv")
            .unwrap()
            .parsed_activity;

    let malformed_utc_local = NaiveDate::from_ymd_opt(2026, 2, 14)
        .unwrap()
        .and_time(NaiveTime::from_hms_opt(8, 30, 0).unwrap());
    assert_eq!(
        malformed_utc_activity.sync_time.as_deref(),
        local_rfc3339(malformed_utc_local).as_deref()
    );
}

#[test]
fn consumes_aim_and_racechrono_local_preambles_transiently() {
    let aim_csv = "\u{feff}Date,\"Saturday, February 14, 2026\"\n\
Time,8:30 AM\n\
Session,CMP Full\n\
Time,Speed\n\
0,36\n\
1,72\n";
    let aim_activity = parse_csv_activity_reader(Cursor::new(aim_csv), "aim.csv")
        .unwrap()
        .parsed_activity;
    let aim_local = NaiveDate::from_ymd_opt(2026, 2, 14)
        .unwrap()
        .and_time(NaiveTime::from_hms_opt(8, 30, 0).unwrap());

    assert_eq!(
        aim_activity.sync_time.as_deref(),
        local_rfc3339(aim_local).as_deref()
    );
    assert!(aim_activity.metadata.get("Date").is_none());
    assert!(aim_activity.metadata.get("Time").is_none());
    assert!(aim_activity.metadata.get("Session").is_none());

    let racechrono_csv = "Created,13/07/2026,16:58\n\
Timestamp (s),Speed\n\
61141,36\n\
61142,72\n";
    let racechrono_activity =
        parse_csv_activity_reader(Cursor::new(racechrono_csv), "racechrono-v1.csv")
            .unwrap()
            .parsed_activity;
    let racechrono_local = NaiveDate::from_ymd_opt(2026, 7, 13)
        .unwrap()
        .and_time(NaiveTime::from_hms_opt(16, 59, 1).unwrap());

    assert_eq!(
        racechrono_activity.sync_time.as_deref(),
        local_rfc3339(racechrono_local).as_deref()
    );
    assert_eq!(racechrono_activity.sample_elapsed_seconds, vec![0.0, 1.0]);
}

#[test]
fn local_preamble_timestamps_respect_dst_transitions_and_reject_non_unique_starts() {
    let (has_transition, examples) = local_transition_examples();
    if !has_transition {
        assert!(examples.is_empty());
        return;
    }
    assert!(examples.iter().any(|example| example.ambiguous));
    assert!(examples.iter().any(|example| !example.ambiguous));

    for example in examples {
        let csv = format!(
            "Date,\"{}\"\nTime,{}\nTime,Speed\n0,36\n14400,72\n",
            example.start.format("%A, %B %d, %Y"),
            example.start.format("%I:%M %p"),
        );
        let activity = parse_csv_activity_reader(Cursor::new(csv), "dst-crossing.csv")
            .unwrap()
            .parsed_activity;
        let expected_end = (example.start.with_timezone(&Utc) + Duration::hours(4))
            .to_rfc3339_opts(SecondsFormat::Millis, true);

        assert_eq!(activity.time[1].as_deref(), Some(expected_end.as_str()));

        let invalid_csv = format!(
            "Date,\"{}\"\nTime,{}\nTime,Speed\n0,36\n1,72\n",
            example.non_unique.format("%A, %B %d, %Y"),
            example.non_unique.format("%I:%M %p"),
        );
        let invalid_activity =
            parse_csv_activity_reader(Cursor::new(invalid_csv), "dst-invalid.csv")
                .unwrap()
                .parsed_activity;

        assert!(invalid_activity.time.iter().all(Option::is_none));
        assert!(invalid_activity.sync_time.is_none());
    }
}

#[test]
fn coalesces_equal_time_rows_before_rebasing_distance() {
    let csv = "Time,Speed (m/s),mileage (m),Latitude\n\
10,5,100,34.0\n\
10,,110,\n\
11,7,120,34.2\n";

    let activity = parse_csv_activity_reader(Cursor::new(csv), "duplicates.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.sample_elapsed_seconds, vec![0.0, 1.0]);
    assert_eq!(activity.speed, vec![Some(5.0), Some(7.0)]);
    assert_eq!(activity.distance, vec![Some(0.0), Some(10.0)]);
    assert_eq!(
        activity.course,
        vec![(Some(34.0), None), (Some(34.2), None)]
    );
    assert!(activity
        .sample_elapsed_seconds
        .windows(2)
        .all(|pair| pair[0] < pair[1]));
    assert!(activity.metadata.get("coalesced_row_count").is_none());
}

#[test]
fn generic_distance_headers_remain_cumulative_distance() {
    let csv = "Time,Distance (m),Latitude,Longitude\n\
0,3,0.0,0.0\n\
1,11,0.0,0.0001\n";

    let activity = parse_csv_activity_reader(Cursor::new(csv), "generic-distance.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.distance, vec![Some(0.0), Some(8.0)]);
    assert_eq!(activity.distance_to_home.first(), Some(&Some(0.0)));
    assert!(activity.distance_to_home[1].is_some());
}

#[test]
fn airdata_distance_pair_maps_home_distance_and_mileage_consistently() {
    let csv = "time(millisecond),latitude,longitude,altitude_above_seaLevel(feet),speed(mph),distance(feet),mileage(feet),compass_heading(degrees)\n\
0,53.4881644,-1.2102215,100,0,10,100,90\n\
1000,53.4881645,-1.2102216,101,1,20,130,100\n";

    let activity = parse_csv_activity_reader(Cursor::new(csv), "airdata.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.distance, vec![Some(0.0), Some(9.144)]);
    assert_eq!(activity.distance_to_home, vec![Some(3.048), Some(6.096)]);
    assert_eq!(activity.elevation, vec![Some(30.48), Some(30.7848)]);
    assert!(activity.heading.iter().any(Option::is_some));
}

#[test]
fn unprofiled_distance_and_mileage_headers_are_rejected_as_ambiguous() {
    let csv = "Time,Distance,Mileage,Latitude\n\
0,1,2,0.0\n\
1,3,4,0.1\n";

    let error = parse_csv_activity_reader(Cursor::new(csv), "ambiguous-distance.csv").unwrap_err();

    assert!(error
        .to_string()
        .contains("ambiguous Distance and Mileage headers"));
}

#[test]
fn explicit_distance_to_home_rejects_negative_observations() {
    let csv = "Time,Distance to home (m),Latitude\n\
0,-1,0.0\n\
1,2,0.0\n";

    let activity = parse_csv_activity_reader(Cursor::new(csv), "distance-to-home.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.distance_to_home, vec![None, Some(2.0)]);
}

#[test]
fn rejects_unplaced_and_decreasing_rows_with_source_context() {
    let missing_csv = "Time,UTC Time,Speed\n\
0,1700000000,36\n\
,,72\n\
2,1700000002,90\n";
    let missing_error =
        parse_csv_activity_reader(Cursor::new(missing_csv), "missing-time.csv").unwrap_err();

    assert!(missing_error.to_string().contains("missing-time.csv"));
    assert!(missing_error.to_string().contains("CSV row 3"));
    assert!(missing_error
        .to_string()
        .contains("neither usable elapsed time nor usable absolute timestamp"));

    let decreasing_csv = "Time,Speed\n\
5,36\n\
4,72\n";
    let decreasing_error =
        parse_csv_activity_reader(Cursor::new(decreasing_csv), "decreasing.csv").unwrap_err();

    assert!(decreasing_error.to_string().contains("decreasing.csv"));
    assert!(decreasing_error.to_string().contains("CSV row 3"));
    assert!(decreasing_error.to_string().contains("must not decrease"));
}

#[test]
fn aligns_absolute_fallback_rows_to_the_selected_elapsed_basis() {
    let csv = "Elapsed time (s),UTC Time,Speed\n\
42.5,1700000000.250,36\n\
,1700000001.750,72\n\
45.5,1700000003.250,90\n";

    let activity = parse_csv_activity_reader(Cursor::new(csv), "mixed-timing.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.sample_elapsed_seconds, vec![0.0, 1.5, 3.0]);
}

#[test]
fn duplicate_elapsed_candidates_select_the_first_source_with_valid_values() {
    let csv = "Time,Time,Latitude,Longitude,Altitude,KPH\n\
invalid,10.000,34.4,-80.5,147.8,5.46\n\
invalid,10.040,34.4,-80.5,147.8,5.61\n";

    let activity = parse_csv_activity_reader(Cursor::new(csv), "duplicate-time.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.sample_elapsed_seconds, vec![0.0, 0.04]);
}

#[test]
fn malformed_metric_observations_become_missing_without_repairing_bounded_values() {
    let csv = "Time,Latitude,Longitude,Distance,RPM,Throttle Position (%),Brake position (%),Heading,Speed\n\
0,91,-181,-1,-1,101,-1,-1,N/A\n\
1,45,90,5,1000,50,25,361,36\n";

    let activity = parse_csv_activity_reader(Cursor::new(csv), "missing-observations.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(
        activity.course,
        vec![(None, None), (Some(45.0), Some(90.0))]
    );
    assert_eq!(activity.distance, vec![Some(0.0), Some(0.0)]);
    assert_eq!(activity.rpm, vec![None, Some(1000.0)]);
    assert_eq!(activity.throttle_position, vec![None, Some(50.0)]);
    assert_eq!(activity.brake_position, vec![None, Some(25.0)]);
    assert_eq!(activity.heading, vec![Some(359.0), Some(359.1)]);
    assert_eq!(activity.speed, vec![None, Some(10.0)]);
}

#[test]
fn unusable_headers_and_repeated_header_rows_report_file_and_record_context() {
    let unusable = "Exporter,Example\nTime,8:30 AM\n";
    let unusable_error =
        parse_csv_activity_reader(Cursor::new(unusable), "unusable.csv").unwrap_err();

    assert!(unusable_error.to_string().contains("unusable.csv"));
    assert!(unusable_error
        .to_string()
        .contains("no usable telemetry header in 2 records"));

    let ambiguous = "Time,Speed\n0,36\nTime,Speed\n1,72\n";
    let ambiguous_error =
        parse_csv_activity_reader(Cursor::new(ambiguous), "two-headers.csv").unwrap_err();

    assert!(ambiguous_error.to_string().contains("two-headers.csv"));
    assert!(ambiguous_error
        .to_string()
        .contains("CSV row 3 has neither usable elapsed time nor usable absolute timestamp"));
}

#[test]
fn racebox_fixture_imports_through_the_path_entry_point() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/activity/sample Racebox.csv");

    let response = parse_csv_activity_path(&fixture).unwrap();
    let activity = response.parsed_activity;

    assert_eq!(activity.file_name.as_deref(), Some("sample Racebox.csv"));
    assert_eq!(activity.sample_elapsed_seconds.first(), Some(&0.0));
    assert!(activity.sample_elapsed_seconds.len() > 2);
    assert!(activity
        .sample_elapsed_seconds
        .windows(2)
        .all(|pair| pair[0] < pair[1]));
    assert_eq!(activity.course.len(), activity.sample_elapsed_seconds.len());
    assert_eq!(activity.speed.len(), activity.sample_elapsed_seconds.len());
    assert_eq!(
        activity.elevation.len(),
        activity.sample_elapsed_seconds.len()
    );
    assert_eq!(
        activity.course.first(),
        Some(&(Some(34.4889022), Some(-80.5974243)))
    );
    assert_eq!(activity.speed.first(), Some(&Some(1.5166666666666666)));
    assert_eq!(activity.elevation.first(), Some(&Some(147.8)));
    let closing_lap_index = activity
        .sample_elapsed_seconds
        .iter()
        .position(|elapsed| (*elapsed - 536.24).abs() < 1e-9)
        .unwrap();
    assert_eq!(activity.lap_number[closing_lap_index - 1], 4);
    assert_eq!(activity.lap_number[closing_lap_index], -1);
    assert_eq!(activity.lap_time_seconds[closing_lap_index], None);
    assert_eq!(activity.lap_durations_seconds.len(), 4);
}

#[test]
fn csv_lap_values_preserve_positive_integer_labels_and_reject_other_values() {
    let csv = "Time,Speed,Lap\n\
0,10,4\n\
1,10,4\n\
2,10,3\n\
3,10,3\n\
4,10,2\n\
5,10,0\n\
6,10,-1\n\
7,10,1.5\n\
8,10,-\n\
9,10,null\n";

    let activity = parse_csv_activity_reader(Cursor::new(csv), "laps.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.lap_number, vec![4, 4, 3, 3, 2, -1, -1, -1, -1, -1]);
    assert_eq!(activity.lap_durations_seconds, vec![2.0, 2.0, 1.0]);
}

#[test]
fn csv_command_returns_the_native_path_response() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/activity/sample Racebox.csv");

    let response = backend_parse_csv_activity(fixture.to_str().unwrap()).unwrap();
    // debug_payload is Some only in debug builds
    if cfg!(not(debug_assertions)) {
        assert!(response.debug_payload.is_none());
    } else {
        assert!(response.debug_payload.is_some());
    }
    let activity = response.parsed_activity;

    assert_eq!(activity.file_name.as_deref(), Some("sample Racebox.csv"));
    assert_eq!(activity.file_format.as_deref(), Some("csv"));
    assert_eq!(activity.sample_elapsed_seconds[0], 0.0);
    assert_eq!(activity.speed[0], Some(1.5166666666666666));
}

#[test]
fn discovers_headers_after_a_preamble_and_converts_explicit_units() {
    let csv = "Exporter,Example\n\
Session,Practice\n\
Time,GPS Latitude (deg),GPS Longitude (deg),GPS Speed (mph),GPS Altitude (ft),GPS Heading (deg)\n\
5,34.0,-80.0,10,100,361\n\
6,34.1,-80.1,20,110,-1\n";

    let activity = parse_csv_activity_reader(Cursor::new(csv), "preamble.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.sample_elapsed_seconds, vec![0.0, 1.0]);
    assert_eq!(activity.speed, vec![Some(4.4704), Some(8.9408)]);
    assert_eq!(activity.elevation, vec![Some(30.48), Some(33.528)]);
    assert_eq!(activity.heading, vec![Some(1.0), Some(0.9)]);
}

#[test]
fn recognizes_a_separate_units_row_by_header_compatibility() {
    let csv = "Time,GPS Speed,GPS Distance 2D,CalculatedGear\n\
s,km/h,m,#\n\
0,36,12,2\n\
1,72,15,3\n";

    let activity = parse_csv_activity_reader(Cursor::new(csv), "units.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.sample_elapsed_seconds, vec![0.0, 1.0]);
    assert_eq!(activity.speed, vec![Some(10.0), Some(20.0)]);
    assert_eq!(activity.distance, vec![Some(0.0), Some(3.0)]);
    assert_eq!(activity.gear_position, gear_numbers(&[2.0, 3.0]));
}

#[test]
fn ignores_bad_units_and_uses_metric_source_priority() {
    let csv = "Time,GPS Speed (knots),GPS Speed (m/s),Speed (m/s) *calc,Vehicle Speed (mph),Distance on GPS Speed (km),Speed Estimate\n\
0,20,-1,3,30,1,900\n\
1,21,-2,4,31,1.5,901\n";

    let activity = parse_csv_activity_reader(Cursor::new(csv), "sources.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.speed, vec![Some(3.0), Some(4.0)]);
    assert_eq!(activity.distance, vec![Some(0.0), Some(500.0)]);
}

#[test]
fn unsupported_and_conflicting_units_disqualify_only_their_sources() {
    let csv = "Time,GPS Speed,Speed (mph),Speed (m/s) *calc\n\
s,knots,km/h,m/s\n\
0,20,36,5\n\
1,21,72,6\n";

    let activity = parse_csv_activity_reader(Cursor::new(csv), "conflict.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.speed, vec![Some(5.0), Some(6.0)]);
}

#[test]
fn an_unsupported_unit_must_match_the_header_dimension_to_form_a_units_row() {
    let csv = "Time,Speed\n\
knots,knots\n\
0,10\n\
1,11\n";

    let error = parse_csv_activity_reader(Cursor::new(csv), "not-units.csv").unwrap_err();

    assert!(error
        .to_string()
        .contains("neither usable elapsed time nor usable absolute timestamp"));
}

#[test]
fn vehicle_gear_outranks_calculated_gear() {
    let csv = "Time,CalculatedGear,Gear *obd\n\
0,1,52-34\n\
1,2,50-34\n";

    let activity = parse_csv_activity_reader(Cursor::new(csv), "gear.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(
        activity.gear_position,
        vec![Some("52-34".to_string()), Some("50-34".to_string()),]
    );
}

#[test]
fn gps_sources_outrank_unqualified_calculated_and_vehicle_sources() {
    let csv = "Time,Latitude,GPS Latitude,Longitude,GPS Longitude,Distance,GPS Distance 2D,Speed (m/s) *calc,GPS Speed (m/s),Vehicle Speed (m/s),Speed (m/s) *estimate\n\
0,10,20,30,40,100,200,3,4,5,999\n\
1,11,21,31,41,110,220,6,7,8,999\n";

    let activity = parse_csv_activity_reader(Cursor::new(csv), "priorities.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(
        activity.course,
        vec![(Some(20.0), Some(40.0)), (Some(21.0), Some(41.0))]
    );
    assert_eq!(activity.distance, vec![Some(0.0), Some(20.0)]);
    assert_eq!(activity.speed, vec![Some(4.0), Some(7.0)]);
}

#[test]
fn vehicle_sources_and_accelerator_pedals_win_and_controls_normalize_by_column() {
    let csv = "Time,RPM,CAN RPM,RPM *obd,Throttle Position (%),Accelerator Pedal Position (%) *can,Brake pressure (MPa),Braking,Gear,Gear *logger,Lean angle (deg)\n\
0,1000,3000,2000,80,25,10,0,2,3,-12.5\n\
1,1100,3100,2100,90,50,20,1,3,4,8.25\n";

    let activity = parse_csv_activity_reader(Cursor::new(csv), "vehicle.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.rpm, vec![Some(3000.0), Some(3100.0)]);
    assert!(activity.cadence.is_empty());
    assert_eq!(activity.throttle_position, vec![Some(25.0), Some(50.0)]);
    assert_eq!(activity.brake_position, vec![Some(0.0), Some(100.0)]);
    assert_eq!(activity.gear_position, gear_numbers(&[3.0, 4.0]));
    assert_eq!(activity.lean_angle, vec![Some(-12.5), Some(8.25)]);
    assert_eq!(activity.extra["metric_units"]["rpm"], "rpm");
    assert_eq!(
        activity.extra["metric_units"]["throttle_position"],
        "percent"
    );
    assert_eq!(activity.extra["coverage"]["rpm"]["availableCount"], 2);
    let extended_attributes = activity.extra["extended_attributes"].as_array().unwrap();
    for metric in ["rpm", "throttle_position", "brake_position", "lean_angle"] {
        assert!(extended_attributes
            .iter()
            .any(|attribute| attribute == metric));
    }

    let ambiguous = "Time,Throttle,Brake position\n\
0,0,0\n\
1,1,1\n\
2,0,1\n";
    let activity = parse_csv_activity_reader(Cursor::new(ambiguous), "binary.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(
        activity.throttle_position,
        vec![Some(0.0), Some(100.0), Some(0.0)]
    );
    assert_eq!(
        activity.brake_position,
        vec![Some(0.0), Some(1.0), Some(1.0)]
    );

    let declared = "Time,Throttle %,Brake on/off\n\
0,1,0\n\
1,2,1\n";
    let activity = parse_csv_activity_reader(Cursor::new(declared), "declared-controls.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.throttle_position, vec![Some(1.0), Some(2.0)]);
    assert_eq!(activity.brake_position, vec![Some(0.0), Some(100.0)]);
}

#[test]
fn acceleration_axes_preserve_signs_and_scalar_uses_the_agreed_precedence() {
    let semantic_csv =
        "Time,Lateral acceleration (G),Longitudinal acceleration (G),Lean angle (deg)\n\
0,-3,4,-20\n\
1,5,-12,15\n";
    let semantic = parse_csv_activity_reader(Cursor::new(semantic_csv), "semantic.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(semantic.g_force_x, vec![Some(-3.0), Some(5.0)]);
    assert_eq!(semantic.g_force_y, vec![Some(4.0), Some(-12.0)]);
    assert_eq!(semantic.g_force, vec![Some(5.0), Some(13.0)]);
    assert_eq!(semantic.lean_angle, vec![Some(-20.0), Some(15.0)]);

    let literal_csv = "Time,Accel X (G),Accel Y (G),Accel Z (G)\n\
0,0,0,1\n\
1,-1,2,-2\n";
    let literal = parse_csv_activity_reader(Cursor::new(literal_csv), "literal.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(literal.g_force_x, vec![Some(0.0), Some(-1.0)]);
    assert_eq!(literal.g_force_y, vec![Some(0.0), Some(2.0)]);
    assert_eq!(literal.g_force_z, vec![Some(1.0), Some(-2.0)]);
    assert_close(literal.g_force[0], 0.0);
    assert_close(literal.g_force[1], 8.0_f64.sqrt());

    let direct_csv =
        "Time,Combined acceleration (G),Lateral acceleration (G),Longitudinal acceleration (G)\n\
0,0.25,3,4\n\
1,0.5,5,12\n";
    let direct = parse_csv_activity_reader(Cursor::new(direct_csv), "direct.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(direct.g_force, vec![Some(0.25), Some(0.5)]);

    let priority_csv = "Time,Lateral acceleration (G),Longitudinal acceleration (G),GPS X acceleration (G),GPS Y acceleration (G),GPS Z acceleration (G),X acceleration (G) *acc,Y acceleration (G) *acc,Z acceleration (G) *acc\n\
0,3,4,10,20,30,-1,-2,-3\n\
1,5,12,40,50,60,-4,-5,-6\n";
    let priority = parse_csv_activity_reader(Cursor::new(priority_csv), "axis-priority.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(priority.g_force_x, vec![Some(-1.0), Some(-4.0)]);
    assert_eq!(priority.g_force_y, vec![Some(-2.0), Some(-5.0)]);
    assert_eq!(priority.g_force_z, vec![Some(-3.0), Some(-6.0)]);
    assert_eq!(priority.g_force, vec![Some(5.0), Some(13.0)]);
}

#[test]
fn trackaddict_aim_lap_legend_and_racebox_fixtures_import_existing_metrics() {
    let fixture_names = [
        "Amozoc - TrackAddict.csv",
        "sample AiM.csv",
        "sample LapLegend.csv",
        "sample Racebox.csv",
    ];

    for name in fixture_names {
        let activity = parse_fixture(name);

        assert!(activity.sample_elapsed_seconds.len() > 2, "{name}");
        assert!(
            activity.course.iter().any(|point| point.0.is_some()),
            "{name}"
        );
        assert!(activity.speed.iter().any(Option::is_some), "{name}");
        assert!(activity.elevation.iter().any(Option::is_some), "{name}");
        assert!(activity.distance.iter().any(Option::is_some), "{name}");
    }

    let trackaddict = parse_fixture("Amozoc - TrackAddict.csv");
    assert_close(trackaddict.speed[0], 24.9 / 3.6);
    assert_close(trackaddict.elevation[0], 2297.6);
    assert_close(trackaddict.barometric_altitude[0], 0.0);
    assert_close(trackaddict.heading[0], 213.8);
    assert!(trackaddict.sync_time.is_some());

    let aim = parse_fixture("sample AiM.csv");
    assert_close(aim.speed[0], 0.01);
    assert_close(aim.elevation[0], 122.8322);
    assert_close(aim.distance[0], 0.0);
    assert_eq!(aim.gear_position[0].as_deref(), Some("0"));

    let lap_legend = parse_fixture("sample LapLegend.csv");
    assert_close(lap_legend.speed[0], 16.0 / 3.6);
    assert_close(lap_legend.elevation[0], 1482.9);
    assert_close(lap_legend.heading[0], 170.6);
    assert_close(lap_legend.distance[0], 0.0);
    assert_close(lap_legend.g_force[0], 0.14);
}

#[test]
fn racechrono_v1_and_v2_fixtures_import_with_strict_rebased_timelines() {
    let fixture_names = [
        "session_20260713_185859_v1.csv",
        "session_20260713_185859_v2.csv",
        "sample RaceChrono.csv",
    ];

    for name in fixture_names {
        let activity = parse_fixture(name);

        assert_eq!(
            activity.sample_elapsed_seconds.first(),
            Some(&0.0),
            "{name}"
        );
        assert!(
            activity
                .sample_elapsed_seconds
                .windows(2)
                .all(|pair| pair[0] < pair[1]),
            "{name}",
        );
        assert_eq!(activity.distance.first(), Some(&Some(0.0)), "{name}");
        assert!(activity.sync_time.is_some(), "{name}");
        assert!(
            activity.metadata.get("coalesced_row_count").is_none(),
            "{name}"
        );
    }

    let v2 = parse_fixture("session_20260713_185859_v2.csv");
    let sample_count = v2.metadata["sample_count"].as_u64().unwrap();
    assert_eq!(sample_count, v2.sample_elapsed_seconds.len() as u64);
    assert!(v2.metadata.get("original_sample_count").is_none());

    let racechrono = parse_fixture("sample RaceChrono.csv");
    assert_eq!(racechrono.course[0], (Some(32.0854405), Some(-81.0744080)));
    assert_close(racechrono.speed[0], 0.032);
    assert_close(racechrono.elevation[0], 8.962);
    assert_close(racechrono.heading[0], 81.941);
    assert_close(racechrono.rpm[0], 747.0);
    assert_close(racechrono.throttle_position[0], 15.3);
    assert_close(racechrono.brake_position[0], 0.0);
    assert_eq!(racechrono.gear_position[0].as_deref(), Some("0"));
    assert_close(racechrono.g_force[0], 0.0);
    assert_close(racechrono.g_force_x[0], 1.002332480590445);
    assert_close(racechrono.g_force_y[0], -0.02031022736508418);
    assert_close(racechrono.g_force_z[0], 0.027631797583651307);

    let v1 = parse_fixture("session_20260713_185859_v1.csv");
    assert_eq!(v1.course[0], (Some(47.3820367), Some(18.2202700)));
    assert_close(v1.speed[0], 0.81);
    assert_close(v1.elevation[0], 225.10);
    assert_close(v1.heading[0], 223.21);
    assert!(v1.rpm.is_empty());
    assert!(v1.throttle_position.is_empty());

    assert_eq!(v2.course[0], (Some(47.3820034), Some(18.2202550)));
    assert_close(v2.speed[0], 0.975);
    assert_close(v2.elevation[0], 219.611);
    assert_close(v2.heading[0], 246.450);
    assert_close(v2.g_force_x[0], 0.02173085923890625);
    assert_close(v2.g_force_y[0], 0.9001238351501731);
    assert_close(v2.g_force_z[0], 0.48696693630865423);
    assert!(v2.lean_angle.contains(&Some(-1.909)));
    assert!(v2.g_force.contains(&Some(0.038)));
}

#[test]
fn motorsport_fixtures_retain_available_new_metrics_as_finite_optional_series() {
    let fixture_names = [
        "Amozoc - TrackAddict.csv",
        "sample AiM.csv",
        "sample LapLegend.csv",
        "sample Racebox.csv",
        "sample RaceChrono.csv",
        "session_20260713_185859_v1.csv",
        "session_20260713_185859_v2.csv",
    ];

    for name in fixture_names {
        let activity = parse_fixture(name);
        let sample_count = activity.sample_elapsed_seconds.len();

        assert_eq!(activity.file_name.as_deref(), Some(name));
        assert_eq!(activity.file_format.as_deref(), Some("csv"));
        assert_eq!(
            activity.sample_elapsed_seconds.first(),
            Some(&0.0),
            "{name}"
        );
        assert!(
            activity
                .sample_elapsed_seconds
                .iter()
                .all(|value| value.is_finite()),
            "{name}",
        );
        assert!(
            activity
                .sample_elapsed_seconds
                .windows(2)
                .all(|pair| pair[0] < pair[1]),
            "{name}",
        );
        assert_eq!(activity.course.len(), sample_count, "{name}");
        assert!(activity.course.iter().all(|(latitude, longitude)| {
            latitude.is_none_or(|value| value.is_finite())
                && longitude.is_none_or(|value| value.is_finite())
        }));
        for series in [
            &activity.elevation,
            &activity.barometric_altitude,
            &activity.speed,
            &activity.heading,
            &activity.distance,
            &activity.g_force,
            &activity.g_force_x,
            &activity.g_force_y,
            &activity.g_force_z,
            &activity.rpm,
            &activity.throttle_position,
            &activity.brake_position,
            &activity.lean_angle,
        ] {
            assert!(
                series.iter().flatten().all(|value| value.is_finite()),
                "{name}"
            );
            assert!(
                series.is_empty() || series.len() == activity.sample_elapsed_seconds.len(),
                "{name}"
            );
        }
        assert!(
            activity.gear_position.is_empty()
                || activity.gear_position.len() == activity.sample_elapsed_seconds.len(),
            "{name}"
        );
    }

    let trackaddict = parse_fixture("Amozoc - TrackAddict.csv");
    assert_close(trackaddict.g_force_x[0], -0.29005555555555557);
    assert_close(trackaddict.g_force_y[0], 0.6419444444444445);
    assert_close(trackaddict.g_force_z[0], -0.7612777777777778);
    assert_close(trackaddict.brake_position[0], 0.0);

    let aim = parse_fixture("sample AiM.csv");
    assert_close(aim.g_force_x[0], 0.0020);
    assert_close(aim.g_force_y[0], 0.0034);
    assert_close(aim.g_force_z[0], -0.9766);
    assert_close(aim.rpm[0], 0.0);
    assert_close(aim.throttle_position[0], 0.0);

    let lap_legend = parse_fixture("sample LapLegend.csv");
    assert!(lap_legend.g_force_x.iter().any(Option::is_some));
    assert!(lap_legend.g_force_y.iter().any(Option::is_some));
    assert!(lap_legend.g_force_z.iter().any(Option::is_some));
    assert!(lap_legend.rpm.iter().any(Option::is_some));
    assert!(lap_legend.throttle_position.iter().any(Option::is_some));
    assert!(lap_legend.brake_position.iter().any(Option::is_some));

    let racebox = parse_fixture("sample Racebox.csv");
    assert!(racebox.g_force_x.iter().any(Option::is_some));
    assert!(racebox.g_force_y.iter().any(Option::is_some));
    assert!(racebox.g_force_z.iter().any(Option::is_some));

    let racechrono = parse_fixture("sample RaceChrono.csv");
    assert!(racechrono.rpm.iter().any(Option::is_some));
    assert!(racechrono.throttle_position.iter().any(Option::is_some));
    assert!(racechrono.brake_position.iter().any(Option::is_some));
    assert!(racechrono.lean_angle.iter().any(Option::is_some));
    assert!(racechrono.g_force.iter().any(Option::is_some));

    let v1 = parse_fixture("session_20260713_185859_v1.csv");
    assert!(v1.rpm.is_empty());
    assert!(v1.throttle_position.is_empty());

    let v2 = parse_fixture("session_20260713_185859_v2.csv");
    assert!(v2.g_force_x.iter().any(Option::is_some));
    assert!(v2.g_force_y.iter().any(Option::is_some));
    assert!(v2.g_force_z.iter().any(Option::is_some));
    assert!(v2.lean_angle.iter().any(Option::is_some));
}

fn parse_fixture(name: &str) -> ovrley_core::activity::schema::ParsedActivity {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/activity")
        .join(name);
    parse_csv_activity_path(&path).unwrap().parsed_activity
}

fn assert_close(actual: Option<f64>, expected: f64) {
    let actual = actual.expect("expected a canonical metric value");
    assert!(
        (actual - expected).abs() < 1e-9,
        "expected {expected}, got {actual}"
    );
}

fn gear_numbers(values: &[f64]) -> Vec<Option<String>> {
    values.iter().map(|value| Some(value.to_string())).collect()
}

fn local_rfc3339(value: chrono::NaiveDateTime) -> Option<String> {
    Local.from_local_datetime(&value).single().map(|value| {
        value
            .with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::Millis, true)
    })
}

struct LocalTransitionExample {
    start: chrono::DateTime<Local>,
    non_unique: NaiveDateTime,
    ambiguous: bool,
}

fn local_transition_examples() -> (bool, Vec<LocalTransitionExample>) {
    let mut instant = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).single().unwrap();
    let end = Utc.with_ymd_and_hms(2031, 1, 1, 0, 0, 0).single().unwrap();
    let mut previous_offset = instant
        .with_timezone(&Local)
        .offset()
        .fix()
        .local_minus_utc();
    let mut has_transition = false;
    let mut examples = Vec::new();
    while instant < end {
        instant += Duration::hours(1);
        let offset = instant
            .with_timezone(&Local)
            .offset()
            .fix()
            .local_minus_utc();
        if offset != previous_offset {
            has_transition = true;
            let start = (instant - Duration::hours(2)).with_timezone(&Local);
            let transition_local = instant.with_timezone(&Local).naive_local();
            for minutes in -180..=180 {
                let candidate = transition_local + Duration::minutes(minutes);
                let ambiguous = match Local.from_local_datetime(&candidate) {
                    LocalResult::None => false,
                    LocalResult::Ambiguous(_, _) => true,
                    LocalResult::Single(_) => continue,
                };
                if !examples
                    .iter()
                    .any(|example: &LocalTransitionExample| example.ambiguous == ambiguous)
                {
                    examples.push(LocalTransitionExample {
                        start,
                        non_unique: candidate,
                        ambiguous,
                    });
                }
                break;
            }
        }
        previous_offset = offset;
    }
    (has_transition, examples)
}

#[test]
fn airdata_datetime_utc_column_is_parsed_as_absolute_timestamps() {
    let csv = "time(millisecond),datetime(utc),latitude,longitude,altitude_above_seaLevel(feet),speed(mph),distance(feet),mileage(feet),compass_heading(degrees)\n\
0,,53.4881644,-1.2102215,100,0,10,100,90\n\
100,2026-07-14 11:45:06,53.4881645,-1.2102216,101,1,20,130,100\n\
200,2026-07-14 11:45:06,53.4881646,-1.2102217,102,2,30,160,110\n";

    let activity = parse_csv_activity_reader(Cursor::new(csv), "airdata-datetime.csv")
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.time.len(), 3);
    assert_eq!(activity.time[0], None);
    assert_eq!(activity.time[1].as_deref(), Some("2026-07-14T11:45:06.000Z"));
    assert_eq!(activity.time[2].as_deref(), Some("2026-07-14T11:45:06.000Z"));
}

mod lap_timing_fixture_tests {
    use super::*;
    use ovrley_core::activity::vbo::parse_vbo_activity_path;

    struct LapExpectations {
        fixture: &'static str,
        min_laps: usize,
        extraction: FixtureKind,
    }

    enum FixtureKind {
        Csv,
        Vbo,
    }

    fn parse(expectations: &LapExpectations) -> ovrley_core::activity::schema::ParsedActivity {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/activity")
            .join(expectations.fixture);
        match expectations.extraction {
            FixtureKind::Csv => parse_csv_activity_path(&path).unwrap().parsed_activity,
            FixtureKind::Vbo => parse_vbo_activity_path(&path, None).unwrap().parsed_activity,
        }
    }

    fn lap_timing_fixtures() -> Vec<LapExpectations> {
        vec![
            LapExpectations {
                fixture: "Amozoc - TrackAddict.csv",
                min_laps: 5,
                extraction: FixtureKind::Csv,
            },
            LapExpectations {
                fixture: "sample AiM.csv",
                min_laps: 5,
                extraction: FixtureKind::Csv,
            },
            LapExpectations {
                fixture: "sample LapLegend.csv",
                min_laps: 0,
                extraction: FixtureKind::Csv,
            },
            LapExpectations {
                fixture: "sample Racebox.csv",
                min_laps: 0,
                extraction: FixtureKind::Csv,
            },
            LapExpectations {
                fixture: "sample RaceChrono.csv",
                min_laps: 0,
                extraction: FixtureKind::Csv,
            },
            LapExpectations {
                fixture: "session_20260713_185859_v1.csv",
                min_laps: 0,
                extraction: FixtureKind::Csv,
            },
            LapExpectations {
                fixture: "session_20260713_185859_v2.csv",
                min_laps: 0,
                extraction: FixtureKind::Csv,
            },
            LapExpectations {
                fixture: "VBO-test.vbo",
                min_laps: 0,
                extraction: FixtureKind::Vbo,
            },
        ]
    }

    #[test]
    fn csv_fixtures_produce_lap_timing_series_aligned_with_elapsed_samples() {
        for expectations in lap_timing_fixtures() {
            let activity = parse(&expectations);
            let n = activity.sample_elapsed_seconds.len();

            assert!(
                n > 0,
                "{} must have samples",
                expectations.fixture
            );
            assert_eq!(
                activity.lap_number.len(),
                n,
                "{} lap_number length mismatch",
                expectations.fixture
            );
            assert_eq!(
                activity.lap_time_seconds.len(),
                n,
                "{} lap_time_seconds length mismatch",
                expectations.fixture
            );
            assert_eq!(
                activity.delta_to_best_lap_seconds.len(),
                n,
                "{} delta_to_best_lap_seconds length mismatch",
                expectations.fixture
            );
        }
    }

    #[test]
    fn csv_fixtures_populate_lap_duration_metadata_when_laps_are_detected() {
        for expectations in lap_timing_fixtures() {
            let activity = parse(&expectations);

            if expectations.min_laps > 0 {
                assert!(
                    !activity.lap_durations_seconds.is_empty(),
                    "{} must have lap durations",
                    expectations.fixture
                );
                assert_eq!(
                    activity.lap_durations_seconds.len(),
                    activity.lap_durations_best_so_far_seconds.len(),
                    "{} best_so_far length must match durations",
                    expectations.fixture
                );
                assert!(
                    activity.best_lap_time_seconds.is_some(),
                    "{} must have best_lap_time_seconds",
                    expectations.fixture
                );
                assert!(
                    activity.lap_durations_seconds.len() >= expectations.min_laps,
                    "{} must have at least {} laps, got {}",
                    expectations.fixture,
                    expectations.min_laps,
                    activity.lap_durations_seconds.len()
                );
            } else {
                assert!(
                    !activity.lap_durations_seconds.is_empty()
                        || activity.best_lap_time_seconds.is_none(),
                    "{} must not have best lap when no durations exist",
                    expectations.fixture
                );
            }
        }
    }

    #[test]
    fn csv_fixtures_with_explicit_lap_columns_have_non_negative_lap_numbers() {
        let fixtures_with_lap_column = [
            "Amozoc - TrackAddict.csv",
            "sample LapLegend.csv",
            "sample Racebox.csv",
            "sample RaceChrono.csv",
            "session_20260713_185859_v1.csv",
            "session_20260713_185859_v2.csv",
        ];

        for name in fixtures_with_lap_column {
            let activity = parse(&LapExpectations {
                fixture: name,
                min_laps: 0,
                extraction: FixtureKind::Csv,
            });

            let has_lap_numbers = activity.lap_number.iter().any(|&v| v >= 0);
            assert!(
                has_lap_numbers,
                "{} must have at least one non-negative lap number",
                name
            );
        }
    }

    #[test]
    fn aim_fixture_beacon_markers_produce_lap_boundaries() {
        let activity = parse(&LapExpectations {
            fixture: "sample AiM.csv",
            min_laps: 0,
            extraction: FixtureKind::Csv,
        });

        assert!(
            activity.lap_number.iter().any(|&v| v >= 0),
            "AiM must produce lap numbers from Beacon Markers"
        );

        let lap_count = activity.lap_durations_seconds.len();
        assert!(
            lap_count > 0,
            "AiM must produce completed laps from Beacon Markers"
        );

        assert!(
            activity.best_lap_time_seconds.is_some(),
            "AiM must produce best lap from Beacon Markers"
        );

        let expected_beacons = 7;
        assert!(
            lap_count <= expected_beacons,
            "AiM Beacon Markers have {} markers, got {} laps",
            expected_beacons,
            lap_count
        );
    }

    #[test]
    fn vbo_fixture_produces_lap_timing_via_crossing_detection() {
        let activity = parse(&LapExpectations {
            fixture: "VBO-test.vbo",
            min_laps: 0,
            extraction: FixtureKind::Vbo,
        });

        assert_eq!(
            activity.lap_number.len(),
            activity.sample_elapsed_seconds.len(),
            "VBO lap_number length must match elapsed"
        );

        let has_lap_data = activity.lap_number.iter().any(|&v| v >= 0);

        if has_lap_data {
            assert!(
                !activity.lap_durations_seconds.is_empty(),
                "VBO must produce lap durations when laps are detected"
            );
            assert!(
                activity.best_lap_time_seconds.is_some(),
                "VBO must produce best lap when laps are detected"
            );
            for (lap, &time) in activity.lap_time_seconds.iter().enumerate() {
                if activity.lap_number[lap] >= 0 {
                    assert!(
                        time.is_some(),
                        "VBO sample {} in lap {} must have lap_time",
                        lap,
                        activity.lap_number[lap]
                    );
                }
            }
        }
    }

    #[test]
    fn trackaddict_fixture_produces_multi_lap_delta_to_best() {
        let activity = parse(&LapExpectations {
            fixture: "Amozoc - TrackAddict.csv",
            min_laps: 0,
            extraction: FixtureKind::Csv,
        });

        assert!(
            activity.lap_durations_seconds.len() >= 2,
            "TrackAddict must have at least 2 laps for delta comparison"
        );

        let deltas_after_first_lap = activity
            .lap_number
            .iter()
            .enumerate()
            .filter(|&(_, &lap)| lap >= 1)
            .filter_map(|(i, _)| activity.delta_to_best_lap_seconds[i])
            .collect::<Vec<f64>>();

        assert!(
            !deltas_after_first_lap.is_empty(),
            "TrackAddict must produce delta values from lap 1 onward"
        );
    }

    #[test]
    fn out_lap_fixtures_carry_lap_number_minus_one() {
        let out_lap_fixtures = [
            "session_20260713_185859_v1.csv",
            "session_20260713_185859_v2.csv",
        ];

        for name in out_lap_fixtures {
            let activity = parse(&LapExpectations {
                fixture: name,
                min_laps: 0,
                extraction: FixtureKind::Csv,
            });

            assert!(
                activity.lap_number.iter().any(|&v| v == -1),
                "{} must have out-lap samples with lap_number == -1",
                name
            );

            let any_out_lap_with_null = activity
                .lap_number
                .iter()
                .zip(activity.lap_time_seconds.iter())
                .any(|(&lap, &time)| lap == -1 && time.is_none());

            assert!(
                any_out_lap_with_null,
                "{} out-lap samples must have null lap_time_seconds",
                name
            );
        }
    }

    #[test]
    fn lap_time_seconds_increases_within_each_lap() {
        for expectations in lap_timing_fixtures() {
            let activity = parse(&expectations);
            if activity.lap_durations_seconds.is_empty() {
                continue;
            }

            let mut current_lap: i64 = -1;
            let mut last_time: Option<f64> = None;

            for (i, &lap) in activity.lap_number.iter().enumerate() {
                if lap != current_lap {
                    last_time = None;
                    current_lap = lap;
                }
                if lap >= 0 {
                    if let Some(time) = activity.lap_time_seconds[i] {
                        if let Some(prev) = last_time {
                            assert!(
                                time >= prev - 1e-9,
                                "{} lap_time_seconds must be non-decreasing within lap {} at sample {}: {} < {}",
                                expectations.fixture,
                                lap,
                                i,
                                time,
                                prev
                            );
                        }
                        last_time = Some(time);
                    }
                }
            }
        }
    }
}
