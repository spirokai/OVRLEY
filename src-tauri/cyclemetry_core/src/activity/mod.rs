pub mod interpolate;
pub mod schema;
pub mod trim;

use crate::activity::interpolate::densify_activity;
use crate::activity::schema::{DebugPayload, DenseActivityReport, ParsedActivity};
use crate::activity::trim::trim_activity;
use crate::config::RenderConfig;
use serde_json::Value;

pub fn parse_activity_json(input: &str) -> Result<ParsedActivity, String> {
    let value: Value = serde_json::from_str(input)
        .map_err(|error| format!("Invalid parsedActivity JSON: {error}"))?;

    if value.get("parsed_activity").is_some() {
        serde_json::from_value::<DebugPayload>(value)
            .map(|payload| payload.parsed_activity)
            .map_err(|error| format!("Invalid parsedActivity debug payload: {error}"))
    } else {
        serde_json::from_value(value)
            .map_err(|error| format!("Invalid parsedActivity payload: {error}"))
    }
}

pub fn build_dense_activity_report(
    activity: &ParsedActivity,
    config: &RenderConfig,
) -> Result<DenseActivityReport, String> {
    let trimmed = trim_activity(activity, config.scene.start, config.scene.end)?;
    Ok(densify_activity(&trimmed, config.scene.fps))
}

#[cfg(test)]
mod tests {
    use super::{build_dense_activity_report, parse_activity_json};
    use crate::config::parse_config_json;
    use std::fs;
    use std::path::PathBuf;

    fn repo_root() -> PathBuf {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    fn fixture(name: &str) -> String {
        let path = repo_root().join("app").join("debug").join(name);
        fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("Failed to read {}: {error}", path.display()))
    }

    #[test]
    fn builds_dense_report_for_full_fixture() {
        let activity = parse_activity_json(&fixture("Test_GPX-parse-debug.json")).unwrap();
        let config = parse_config_json(
            r#"{"scene":{"fps":30,"start":0,"end":4912,"width":3840,"height":2160}}"#,
        )
        .unwrap();
        let report = build_dense_activity_report(&activity, &config).unwrap();

        assert_eq!(report.frame_count, 147360);
        assert_eq!(report.frame_elapsed_seconds.first().copied(), Some(0.0));
        assert!(
            report
                .frame_elapsed_seconds
                .last()
                .copied()
                .unwrap_or_default()
                < 4912.0
        );
        assert_eq!(report.series.speed.len(), report.frame_count);
        assert_eq!(report.series.course_lat.len(), report.frame_count);
    }

    #[test]
    fn trims_non_integer_window_across_multiple_fps() {
        let activity = parse_activity_json(&fixture("Test_FIT-parse-debug.json")).unwrap();

        for (fps, expected_frames) in [(24.0, 708usize), (30.0, 885usize), (60.0, 1770usize)] {
            let config = parse_config_json(&format!(
                r#"{{"scene":{{"fps":{fps},"start":600.25,"end":629.75}}}}"#
            ))
            .unwrap();
            let report = build_dense_activity_report(&activity, &config).unwrap();
            assert_eq!(report.frame_count, expected_frames);
            assert_eq!(report.frame_elapsed_seconds.first().copied(), Some(0.0));
            assert!(
                report
                    .frame_elapsed_seconds
                    .last()
                    .copied()
                    .unwrap_or_default()
                    < 29.5
            );
            assert_eq!(report.series.time.len(), expected_frames);
        }
    }
}
