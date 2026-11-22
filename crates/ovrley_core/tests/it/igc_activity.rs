use ovrley_core::activity::parsers::igc::extract_igc_activity;
use std::path::Path;

const HEADER: &str = "AXCSABC\nHFDTE020720\nHFPLTPILOT:Foo_Bar\n";

#[test]
fn normalizes_logger_extensions_into_canonical_units() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/activity/1G_77fv6m71.igc");
    let activity = extract_igc_activity(&std::fs::read(path).unwrap(), "flight.igc").unwrap();

    assert_eq!(activity.file_format, "igc");
    assert_eq!(activity.metadata["logger_manufacturer"], "LXNAV");
    assert_eq!(
        activity.metadata["activity_name"],
        "2017-07-15 - Florian Graf"
    );
    // Raw B101903 extension fields: GSP=09437, TRT=331, VAT=00346, OAT=0244.
    let sample = &activity.raw_samples[31];
    assert_eq!(
        sample.timestamp.as_deref(),
        Some("2017-07-15T10:19:03.000Z")
    );
    assert_eq!(sample.speed, Some(9437.0 / 360.0));
    assert_eq!(sample.heading, Some(331.0));
    assert_eq!(sample.vertical_speed, Some(3.46));
    assert_eq!(sample.temperature, Some(24.4));
}

#[test]
fn keeps_void_fixes_on_a_timeline_that_crosses_midnight() {
    let igc = format!(
        "{HEADER}B2359595206343N00006198WA0058700558\nB0000015206343S00006198EV0058700558\n"
    );
    let activity = extract_igc_activity(igc.as_bytes(), "midnight.igc").unwrap();
    let (first, second) = (&activity.raw_samples[0], &activity.raw_samples[1]);

    assert_eq!(first.elapsed_seconds, Some(0.0));
    assert_eq!(first.elevation, Some(558.0));
    assert!(first.longitude.is_some_and(|longitude| longitude < 0.0));
    assert_eq!(
        second.timestamp.as_deref(),
        Some("2020-07-03T00:00:01.000Z")
    );
    assert_eq!(second.elapsed_seconds, Some(2.0));
    assert_eq!(second.latitude, None);
    assert_eq!(second.elevation, None);
}

#[test]
fn malformed_lines_are_reported_without_failing_the_import() {
    let igc = format!("{HEADER}Bbad\nH-GTY:x\nB1101355206343N00006198WA0058700558\n");
    let activity = extract_igc_activity(igc.as_bytes(), "lenient.igc").unwrap();

    assert_eq!(activity.raw_samples.len(), 1);
    assert_eq!(
        activity.metadata["parse_errors"],
        serde_json::json!([
            "Invalid B record at line 4: Bbad",
            "Invalid GTY header at line 5: H-GTY:x",
        ])
    );
}

#[test]
fn rejects_logs_missing_required_records() {
    let error = |igc: &str| {
        extract_igc_activity(igc.as_bytes(), "error.igc")
            .unwrap_err()
            .to_string()
    };

    assert!(error("HFDTE020720\n").contains("Missing A record"));
    assert!(error("AXCSABC\n").contains("Missing HFDTE record"));
    assert!(error(HEADER).contains("The IGC file does not contain any fix records."));
}
