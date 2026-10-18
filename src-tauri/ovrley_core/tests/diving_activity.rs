use ovrley_core::activity::{
    schema::ParsedActivity, subsurface::parse_subsurface_activity_reader,
    uddf::parse_uddf_activity_reader,
};

fn uddf(text: &str) -> ParsedActivity {
    parse_uddf_activity_reader(text.as_bytes(), "dive.uddf", None)
        .unwrap()
        .parsed_activity
}

fn subsurface(text: &str) -> ParsedActivity {
    parse_subsurface_activity_reader(text.as_bytes(), "dive.ssrf", None)
        .unwrap()
        .parsed_activity
}

fn close(value: Option<f64>, expected: f64) {
    assert!(
        (value.unwrap() - expected).abs() < 1e-9,
        "{value:?} != {expected}"
    );
}

#[test]
fn uddf_33_fixture_imports_all_seven_metrics_and_loads_into_preview() {
    let activity = uddf(include_str!("fixtures/activity/test-uddf.uddf"));
    assert_eq!(activity.sample_elapsed_seconds.len(), 688);
    assert_eq!(activity.trim_end_seconds, 6240.0);
    let index = activity
        .sample_elapsed_seconds
        .iter()
        .position(|time| *time == 380.0)
        .unwrap();
    close(activity.depth[index], 45.0);
    close(activity.first_stop_depth[index], 0.1);
    close(activity.first_stop_time[index], 10.0 / 60.0);
    close(activity.time_to_surface[index], 1.0);
    close(activity.average_ppo2[index], 787_000.0 / 6.0 / 101_325.0);
    close(activity.fraction_o2[index], 0.16);
    close(activity.fraction_he[index], 0.48);
    close(activity.water_temperature[index], 20.35);
    // This fixture has duplicate and out-of-order waypoints; preview requires a strict timeline.
    ovrley_widgets::activity::ActivitySampler::from_json(&serde_json::to_vec(&activity).unwrap())
        .unwrap();
}

#[test]
fn uddf_32_fixture_converts_units_and_preserves_sparse_and_missing_values() {
    let activity = uddf(include_str!("fixtures/activity/test-uddf-3.2.uddf"));
    assert_eq!(activity.sample_elapsed_seconds, [0.0, 60.0, 120.0]);
    assert_eq!(activity.depth, [Some(0.0), Some(18.0), Some(20.5)]);
    assert_eq!(activity.water_temperature, [Some(24.0); 3]);
    assert_eq!(activity.first_stop_depth, [None, Some(0.0), Some(6.0)]);
    assert_eq!(activity.first_stop_time, [None, Some(0.0), Some(3.0)]);
    assert!(activity.time_to_surface.iter().all(Option::is_none));
}

#[test]
fn uddf_32_reads_measured_oxygen_without_substituting_a_setpoint() {
    let activity = uddf(
        r#"<uddf version="3.2"><profiledata><dive><samples>
          <waypoint><divetime>0</divetime><setpo2>130000</setpo2></waypoint>
          <waypoint><divetime>60</divetime><measuredpo2>101325</measuredpo2></waypoint>
        </samples></dive></profiledata></uddf>"#,
    );
    assert_eq!(activity.average_ppo2, [None, Some(1.0)]);
}

#[test]
fn subsurface_fixture_converts_predictions_and_zero_helium_gases() {
    let activity = subsurface(include_str!("fixtures/activity/test-subsurface.ssrf"));
    assert_eq!(
        activity.sample_elapsed_seconds,
        [0.0, 60.0, 120.0, 180.0, 240.0]
    );
    assert_eq!(
        activity.depth,
        [Some(0.0), Some(18.0), None, Some(20.0), Some(19.0)]
    );
    assert_eq!(
        activity.water_temperature,
        [Some(24.0), Some(24.0), None, Some(22.0), Some(22.0)]
    );
    close(activity.first_stop_depth[3], 3.0);
    close(activity.first_stop_time[3], 2.0);
    close(activity.time_to_surface[4], 4.5);
    assert_eq!(
        activity.fraction_o2,
        [Some(0.32), Some(0.32), Some(0.5), Some(0.5), Some(0.5)]
    );
    assert_eq!(activity.fraction_he, [Some(0.0); 5]);
}

#[test]
fn subsurface_keeps_measured_oxygen_and_gas_switches_at_their_recorded_times() {
    let activity = subsurface(
        r#"<divelog><dive>
          <cylinder o2="21%" he="0%"/><cylinder o2="50%" he="0%"/>
          <divecomputer>
            <sample time="0" depth="20 m" po2="1.3 bar"/>
            <event time="30" name="gaschange" cylinder="1"/>
            <sample time="60" depth="21 m" sensor1="1.01325 bar" sensor2="1.01325 bar"/>
            <sample time="90" dc_supplied_ppo2="2.0265 bar"/>
            <sample time="120" dc_supplied_ppo2="0 bar"/>
          </divecomputer>
        </dive></divelog>"#,
    );
    assert_eq!(
        activity.sample_elapsed_seconds,
        [0.0, 30.0, 60.0, 90.0, 120.0]
    );
    assert_eq!(
        activity.average_ppo2,
        [None, None, Some(1.0), Some(2.0), Some(0.0)]
    );
    assert_eq!(
        activity.fraction_o2,
        [Some(0.21), Some(0.5), Some(0.5), Some(0.5), Some(0.5)]
    );
    assert_eq!(activity.depth[1], None);
}
