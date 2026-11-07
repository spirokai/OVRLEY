use ovrley_core::activity::finalize::finalize_raw_activity;
use ovrley_core::activity::parsers::srt::extract_srt_activity;
use ovrley_core::activity::schema::{RawActivity, RawSample};
use std::path::Path;

fn extract(srt: &str) -> RawActivity {
    extract_srt_activity(srt.as_bytes(), "flight.srt").unwrap()
}

fn first_sample(srt: &str) -> RawSample {
    extract(srt).raw_samples.remove(0)
}

#[test]
fn reads_bracketed_telemetry_with_a_naive_camera_timestamp() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/activity/DJI_20260714124521_0235_D.SRT");
    let activity = extract_srt_activity(&std::fs::read(path).unwrap(), "flight.SRT").unwrap();
    let sample = &activity.raw_samples[2];

    assert_eq!(activity.file_format, "srt");
    assert_eq!(activity.raw_samples.len(), 6344);
    assert!(activity.options.skip_idle_gap_fill);
    assert_eq!(sample.elapsed_seconds, Some(0.032));
    assert_eq!(sample.timestamp.as_deref(), Some("2026-07-14T12:45:22.064"));
    assert!((sample.latitude.unwrap() - 53.488534).abs() < 1e-5);
    assert!((sample.longitude.unwrap() + 1.209534).abs() < 1e-5);
    assert_eq!(sample.elevation, Some(-44.824));
    assert_eq!(sample.iso, Some(640.0));
    assert_eq!(sample.aperture, Some(1.9));
    assert_eq!(sample.shutter_speed, Some(1.0 / 8000.0));
    assert_eq!(sample.ev, Some(0.0));
    assert_eq!(sample.color_temperature, Some(5696.0));
}

#[test]
fn smooths_coordinates_at_video_cadence_and_leaves_one_hertz_files_as_written() {
    let latitudes = |step_ms: u32| {
        let srt: String = (0..6u32)
            .map(|index| {
                let millis = index * step_ms;
                let latitude = if index < 3 { 10.0 } else { 10.6 };
                format!(
                    "{}\n00:00:{:02},{:03} --> 00:00:59,000\n[latitude: {latitude}] [longitude: 20.0]\n\n",
                    index + 1,
                    millis / 1000,
                    millis % 1000
                )
            })
            .collect();
        extract(&srt)
            .raw_samples
            .iter()
            .map(|sample| sample.latitude.unwrap())
            .collect::<Vec<_>>()
    };

    let smoothed = latitudes(100);
    assert_eq!(smoothed[0], 10.0);
    assert!(smoothed[2] > 10.0 && smoothed[3] < 10.6);
    assert!(((smoothed[2] - 10.0) - (10.6 - smoothed[3])).abs() < 1e-9);

    assert_eq!(latitudes(1000), [10.0, 10.0, 10.0, 10.6, 10.6, 10.6]);
}

#[test]
fn reads_camera_time_in_the_gps_timezone_and_keeps_zulu_time_absolute() {
    let time = |date: &str| {
        let srt = format!(
            "1\n00:00:00,000 --> 00:00:00,033\n{date}\n[latitude: 41.424724] [longitude: 2.234156]\n"
        );
        let response = finalize_raw_activity(&extract(&srt), None).unwrap();
        response.parsed_activity.time[0].clone().unwrap()
    };

    assert_eq!(
        time("2022-08-07 13:40:40,774,808"),
        "2022-08-07T11:40:40.774Z"
    );
    assert_eq!(time("2022-08-07T13:40:40.774Z"), "2022-08-07T13:40:40.774Z");
}

#[test]
fn reads_legacy_lines_as_longitude_first_with_barometer_elevation() {
    let sample = first_sample(
        "1\n00:00:01,000 --> 00:00:02,000\n\
         HOME(149.0251,-20.2532) 2017.8.5 14:11:51\n\
         GPS(149.0251,-20.2533,16) BAROMETER:1.9\n\
         ISO:100 Shutter:400 EV:-1/2 Fnum:F2.8\n",
    );

    assert_eq!(sample.timestamp.as_deref(), Some("2017-08-05T14:11:51.000"));
    assert_eq!(sample.longitude, Some(149.0251));
    assert_eq!(sample.latitude, Some(-20.2533));
    assert_eq!(sample.elevation, Some(1.9));
    assert_eq!(sample.shutter_speed, Some(1.0 / 400.0));
    assert_eq!(sample.ev, Some(-0.5));
    assert_eq!(sample.aperture, Some(2.8));
}

#[test]
fn reads_matrice_tuples_latitude_first() {
    let sample = first_sample(
        "1\n00:00:00,000 --> 00:00:01,000\n2022.06.21 16:06:17\nGPS(36.6146,-6.1120,0.0M) BAROMETER:0.3M\n",
    );

    assert_eq!(sample.latitude, Some(36.6146));
    assert_eq!(sample.longitude, Some(-6.1120));
    assert_eq!(sample.elevation, Some(0.3));
}

#[test]
fn ignores_diagnostic_tuples_on_comma_separated_lines() {
    let sample = first_sample(
        "1\n00:00:00,000 --> 00:00:01,000\n\
         F/2.8, SS 583.06, ISO 110, EV +0.7, Index (Ev:8371,Nf:237), Af Rect(X:0, Y:0, W:0, H:99), \
         ColorTemp 5105, GPS (91.4580, -43.0639, 13), D 0.40m, H 2.80m, H.S 1.50m/s, V.S n/a\n",
    );

    assert_eq!(sample.timestamp, None);
    assert_eq!(sample.aperture, Some(2.8));
    assert_eq!(sample.shutter_speed, Some(1.0 / 583.06));
    assert_eq!(sample.ev, Some(0.7));
    assert_eq!(sample.color_temperature, Some(5105.0));
    assert_eq!(sample.longitude, Some(91.4580));
    assert_eq!(sample.latitude, Some(-43.0639));
    assert_eq!(sample.elevation, Some(2.80));
    assert_eq!(sample.distance, None);
    assert_eq!(sample.speed, Some(1.5));
    assert_eq!(sample.vertical_speed, None);
}

#[test]
fn applies_known_firmware_corrections() {
    let radians =
        first_sample("1\n00:00:00,000 --> 00:00:00,033\n[latitude : 0.7] [longtitude : 0.03]\n");
    assert_eq!(radians.latitude, Some(0.7_f64.to_degrees()));
    assert_eq!(radians.longitude, Some(0.03_f64.to_degrees()));

    let fpv = first_sample(
        "1\n00:00:00,000 --> 00:00:00,033\n<font size=\"28\">\n2021-03-01 10:00:00.123\n[altitude: 12.5] </font>\n",
    );
    assert_eq!(fpv.elevation, Some(125.0));

    let no_lock = first_sample(
        "1\n00:00:00,000 --> 00:00:00,033\n[latitude : 0.000000] [longitude : 0.000000]\n",
    );
    assert_eq!(no_lock.latitude, None);
    assert_eq!(no_lock.longitude, None);
}

#[test]
fn rebases_cue_times_so_the_first_timestamp_is_time_zero() {
    let activity = extract(
        "1\n00:15:04,500 --> 00:15:05,000\n2017.08.05 14:11:51\n[iso: 100]\n\n\
         2\n00:15:05,500 --> 00:15:06,000\n2017.08.05 14:11:52\n[iso: 100]\n",
    );

    assert_eq!(activity.raw_samples[0].elapsed_seconds, Some(0.0));
    assert_eq!(activity.raw_samples[1].elapsed_seconds, Some(1.0));
}

#[test]
fn keeps_cues_without_cue_times_and_drops_cues_without_telemetry() {
    let activity = extract(
        "1\n2017.08.05 14:11:51,393,525,393,525\n[latitude : -20.2533] [longtitude : 149.0251]\n\n\
         2\n00:00:02,000 --> 00:00:03,000\n\n3\n",
    );

    assert_eq!(activity.raw_samples.len(), 1);
    assert_eq!(activity.raw_samples[0].elapsed_seconds, None);
    assert_eq!(
        activity.raw_samples[0].timestamp.as_deref(),
        Some("2017-08-05T14:11:51.393")
    );
}

#[test]
fn rejects_files_without_telemetry() {
    for srt in ["", "1\n"] {
        let error = extract_srt_activity(srt.as_bytes(), "empty.srt").unwrap_err();
        assert!(error.to_string().contains("does not contain any telemetry"));
    }
}
