use ovrley_core::activity::tcx::parse_tcx_activity_reader;

#[test]
fn tcx_trackpoints_finalize_into_existing_activity_fields() {
    let tcx = r#"<TrainingCenterDatabase xmlns="http://www.garmin.com/xmlschemas/TrainingCenterDatabase/v2"
      xmlns:ns3="http://www.garmin.com/xmlschemas/ActivityExtension/v2">
      <Activities><Activity Sport="Biking">
        <Id>2026-06-07T13:01:49+02:00</Id>
        <Lap StartTime="2026-06-07T13:01:49+02:00"><TotalTimeSeconds>2</TotalTimeSeconds><Track>
          <Trackpoint><Time>2026-06-07T13:01:49+02:00</Time>
            <Position><LatitudeDegrees>51.380845</LatitudeDegrees><LongitudeDegrees>20.288191</LongitudeDegrees></Position>
            <AltitudeMeters>189.1</AltitudeMeters><DistanceMeters>0</DistanceMeters>
            <HeartRateBpm><Value>91</Value></HeartRateBpm><Cadence>0</Cadence>
            <Extensions><ns3:TPX><ns3:Speed>0</ns3:Speed><ns3:Watts>0</ns3:Watts></ns3:TPX></Extensions>
          </Trackpoint>
          <Trackpoint><Time>2026-06-07T11:01:50Z</Time><DistanceMeters>10</DistanceMeters>
            <Extensions><ns3:TPX><ns3:Speed>10</ns3:Speed><ns3:Watts>250</ns3:Watts></ns3:TPX></Extensions>
          </Trackpoint>
        </Track></Lap>
        <Lap StartTime="2026-06-07T11:01:51Z"><TotalTimeSeconds>1</TotalTimeSeconds><Track>
          <Trackpoint><Time>2026-06-07T11:01:51Z</Time><DistanceMeters>20</DistanceMeters></Trackpoint>
        </Track></Lap>
        <Creator><Name>Sample Device</Name></Creator>
      </Activity></Activities>
    </TrainingCenterDatabase>"#;
    let activity = parse_tcx_activity_reader(tcx.as_bytes(), "ride.tcx", None)
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.file_format.as_deref(), Some("tcx"));
    assert_eq!(activity.sample_elapsed_seconds, vec![0.0, 1.0, 2.0]);
    assert_eq!(
        activity.sync_time.as_deref(),
        Some("2026-06-07T11:01:49.000Z")
    );
    assert_eq!(activity.course[0], (Some(51.380845), Some(20.288191)));
    assert_eq!(activity.elevation[0], Some(189.1));
    assert_eq!(activity.heartrate, vec![Some(91.0), None, None]);
    assert_eq!(activity.cadence, vec![Some(0.0), None, None]);
    assert_eq!(activity.power, vec![Some(0.0), Some(250.0), None]);
    assert_eq!(activity.speed[1], Some(10.0));
    assert_eq!(activity.distance, vec![Some(0.0), Some(10.0), Some(20.0)]);
    assert_eq!(activity.lap_number, vec![0, 0, 1]);
    assert_eq!(activity.metadata["duration_seconds"], 2.0);
    assert_eq!(activity.metadata["total_timer_time"], 3.0);
    assert_eq!(activity.metadata["creator"], "Sample Device");
}

#[test]
fn tcx_rejects_multiple_activities() {
    let tcx = r#"<TrainingCenterDatabase><Activities>
      <Activity Sport="Biking"/><Activity Sport="Running"/>
    </Activities></TrainingCenterDatabase>"#;
    let error = parse_tcx_activity_reader(tcx.as_bytes(), "multiple.tcx", None).unwrap_err();

    assert!(error.to_string().contains("multiple.tcx"));
    assert!(error.to_string().contains("multiple activities"));
}

#[test]
fn tcx_preserves_lap_start_times_through_idle_gap_filling() {
    let tcx = r#"<TrainingCenterDatabase><Activities><Activity Sport="Biking">
      <Id>2026-06-07T11:00:00Z</Id>
      <Lap StartTime="2026-06-07T13:00:01+02:00"><Track>
        <Trackpoint><Time>2026-06-07T11:00:02Z</Time><DistanceMeters>0</DistanceMeters></Trackpoint>
        <Trackpoint><Time>2026-06-07T11:00:03Z</Time><DistanceMeters>10</DistanceMeters></Trackpoint>
      </Track></Lap>
      <Lap StartTime="2026-06-07T11:00:05Z"><Track>
        <Trackpoint><Time>2026-06-07T11:00:07Z</Time><DistanceMeters>10</DistanceMeters></Trackpoint>
      </Track></Lap>
    </Activity></Activities></TrainingCenterDatabase>"#;
    let activity = parse_tcx_activity_reader(tcx.as_bytes(), "laps.tcx", None)
        .unwrap()
        .parsed_activity;

    assert_eq!(
        activity.sync_time.as_deref(),
        Some("2026-06-07T11:00:00.000Z")
    );
    assert_eq!(
        activity.sample_elapsed_seconds,
        vec![2.0, 3.0, 4.0, 5.0, 6.0, 7.0]
    );
    assert_eq!(
        activity.time,
        (2..=7)
            .map(|second| Some(format!("2026-06-07T11:00:{second:02}.000Z")))
            .collect::<Vec<_>>()
    );
    assert_eq!(&activity.speed[2..5], &[Some(0.0); 3]);
    assert_eq!(&activity.cadence[2..5], &[Some(0.0); 3]);
    assert_eq!(&activity.power[2..5], &[Some(0.0); 3]);
    assert_eq!(activity.lap_start_elapsed_seconds, vec![1.0, 5.0]);
    assert_eq!(activity.lap_number, vec![0, 0, 0, 1, 1, 1]);
    assert_eq!(
        activity.lap_time_seconds,
        vec![
            Some(1.0),
            Some(2.0),
            Some(3.0),
            Some(0.0),
            Some(1.0),
            Some(2.0)
        ]
    );
    assert_eq!(activity.lap_durations_seconds, vec![4.0]);
    assert_eq!(activity.metadata["inserted_idle_sample_count"], 3);
    ovrley_core::activity::normalize_parsed_activity(activity).unwrap();
}

#[test]
fn tcx_imports_running_cadence_without_replacing_missing_readings() {
    let tcx = r#"<TrainingCenterDatabase xmlns="http://www.garmin.com/xmlschemas/TrainingCenterDatabase/v2"
      xmlns:ax="http://www.garmin.com/xmlschemas/ActivityExtension/v2">
      <Activities><Activity Sport="Running"><Id>2026-06-07T11:00:00Z</Id>
      <Lap StartTime="2026-06-07T11:00:00Z"><Track>
        <Trackpoint><Time>2026-06-07T11:00:00Z</Time>
          <Extensions><ax:TPX><ax:RunCadence>86</ax:RunCadence></ax:TPX></Extensions>
        </Trackpoint>
        <Trackpoint><Time>2026-06-07T11:00:01Z</Time>
          <Extensions><ax:TPX><ax:RunCadence>0</ax:RunCadence></ax:TPX></Extensions>
        </Trackpoint>
        <Trackpoint><Time>2026-06-07T11:00:02Z</Time></Trackpoint>
      </Track></Lap></Activity></Activities>
    </TrainingCenterDatabase>"#;
    let activity = parse_tcx_activity_reader(tcx.as_bytes(), "run.tcx", None)
        .unwrap()
        .parsed_activity;

    assert_eq!(activity.cadence, vec![Some(86.0), Some(0.0), None]);
}

#[test]
fn tcx_uses_activity_id_without_lap_timestamps() {
    let tcx = r#"<TrainingCenterDatabase><Activities><Activity Sport="Biking">
      <Id>2026-06-07T11:01:49Z</Id><Lap><Track>
        <Trackpoint><Time>2026-06-07T11:01:50Z</Time></Trackpoint>
        <Trackpoint><Time>2026-06-07T11:01:51Z</Time></Trackpoint>
      </Track></Lap>
    </Activity></Activities></TrainingCenterDatabase>"#;
    let activity = parse_tcx_activity_reader(tcx.as_bytes(), "no-lap-times.tcx", None)
        .unwrap()
        .parsed_activity;

    assert_eq!(
        activity.sync_time.as_deref(),
        Some("2026-06-07T11:01:49.000Z")
    );
    assert_eq!(activity.sample_elapsed_seconds, vec![1.0, 2.0]);
    assert!(activity.lap_start_elapsed_seconds.is_empty());
    assert_eq!(activity.lap_number, vec![-1, -1]);
    assert_eq!(activity.lap_time_seconds, vec![None, None]);
}
