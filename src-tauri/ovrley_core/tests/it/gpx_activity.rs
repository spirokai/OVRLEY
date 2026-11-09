use ovrley_core::activity::parsers::gpx::extract_gpx_activity;

const EXPORTER_GPX: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="SpringBoot Export" xmlns="http://www.topografix.com/GPX/1/1" xmlns:ext="http://example.com/location/ext">
  <trk>
    <name>Session 4221372</name>
    <trkseg>
      <trkpt lat="47.6662884" lon="21.6567532">
        <ele>194.70001220703125</ele>
        <time>2026-09-20T13:02:09.923Z</time>
        <extensions>
          <ext:speed>0.2771809161487156</ext:speed>
          <ext:speedMps></ext:speedMps>
          <ext:lean>0.0</ext:lean>
          <ext:travelledDistance>0.0</ext:travelledDistance>
          <ext:lapNumber>1</ext:lapNumber>
          <ext:xAccelerationG>0.0074643223</ext:xAccelerationG>
          <ext:yAccelerationG>-0.036097955</ext:yAccelerationG>
          <ext:zAccelerationG>0.07249162</ext:zAccelerationG>
        </extensions>
      </trkpt>
      <trkpt lat="47.66627569158134" lon="21.656767991765975">
        <ele>194.70001220703125</ele>
        <time>2026-09-20T13:02:15.960Z</time>
        <extensions>
          <ext:speedMps>0.22938074038800563</ext:speedMps>
          <ext:lean>-0.25</ext:lean>
          <ext:travelledDistance>1.797462868698045</ext:travelledDistance>
          <ext:lapNumber>2</ext:lapNumber>
          <ext:xAccelerationG>-0.11160793</ext:xAccelerationG>
          <ext:yAccelerationG>-0.0765297</ext:yAccelerationG>
          <ext:zAccelerationG>0.0855134</ext:zAccelerationG>
        </extensions>
      </trkpt>
    </trkseg>
  </trk>
</gpx>"#;

fn error_message(gpx: &str) -> String {
    extract_gpx_activity(gpx.as_bytes(), "error.gpx")
        .unwrap_err()
        .to_string()
}

#[test]
fn maps_exporter_extensions_into_canonical_fields() {
    let activity = extract_gpx_activity(EXPORTER_GPX.as_bytes(), "session.gpx").unwrap();

    assert_eq!(activity.file_name, "session.gpx");
    assert_eq!(activity.file_format, "gpx");
    assert_eq!(activity.metadata["activity_name"], "Session 4221372");
    assert_eq!(activity.metadata["creator"], "SpringBoot Export");
    assert_eq!(activity.raw_samples.len(), 2);

    let first = &activity.raw_samples[0];
    assert_eq!(first.latitude, Some(47.6662884));
    assert_eq!(first.elevation, Some(194.70001220703125));
    assert_eq!(first.timestamp.as_deref(), Some("2026-09-20T13:02:09.923Z"));
    assert_eq!(first.speed, Some(0.2771809161487156));
    assert_eq!(first.distance, Some(0.0));
    assert_eq!(first.lap_number, Some(1));
    assert_eq!(first.lean_angle, Some(0.0));
    assert_eq!(first.g_force_x, Some(0.0074643223));
    assert_eq!(first.g_force_y, Some(-0.036097955));
    assert_eq!(first.g_force_z, Some(0.07249162));

    let second = &activity.raw_samples[1];
    assert_eq!(second.speed, Some(0.22938074038800563));
    assert_eq!(second.distance, Some(1.797462868698045));
    assert_eq!(second.lap_number, Some(2));
    assert_eq!(second.lean_angle, Some(-0.25));
}

#[test]
fn rejects_unusable_documents() {
    assert!(error_message("not xml").contains("The GPX file could not be parsed."));
    assert!(error_message("<gpx><parsererror/><trkpt/></gpx>")
        .contains("The GPX file could not be parsed."));
    assert!(error_message("<gpx><trk/></gpx>")
        .contains("The GPX file does not contain any track points."));
}

#[test]
fn strict_extensions_fail_when_unreadable() {
    let point =
        |extension: &str| format!("<gpx><trkpt><extensions>{extension}</extensions></trkpt></gpx>");

    assert!(error_message(&point("<kcal>x</kcal>"))
        .contains("GPX extension kcal must be numeric when present"));
    assert!(
        error_message(&point("<pressure_altitude>x</pressure_altitude>"))
            .contains("GPX extension pressure_altitude must be numeric when present")
    );
    assert!(error_message(&point("<lapNumber>1.5</lapNumber>"))
        .contains("GPX extension lap_number must be an integer when present"));
}

#[test]
fn comment_2d_speed_backs_up_missing_extension_speed() {
    let gpx = "<gpx><trkpt><cmt>alt: 1; 2dSpeed: 4.5</cmt></trkpt>\
        <trkpt><cmt>2dSpeed: 4.5</cmt><extensions><speed>9</speed></extensions></trkpt></gpx>";
    let activity = extract_gpx_activity(gpx.as_bytes(), "speed.gpx").unwrap();

    assert_eq!(activity.raw_samples[0].speed, Some(4.5));
    assert_eq!(activity.raw_samples[1].speed, Some(9.0));
}
