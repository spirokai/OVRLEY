//! Activity ingestion and frame-density preparation.
//!
//! The frontend supplies parsed GPX/FIT activity data as JSON. This module
//! accepts both the plain `ParsedActivity` shape and the debug wrapper shape,
//! validates the render window against the activity duration, and returns a
//! dense per-frame report containing only the telemetry series required by the
//! selected template.

/// Interpolation helpers used for numeric, coordinate, and timestamp series.
pub mod interpolate;
/// Serializable activity payloads and internal dense/trimmed report types.
pub mod schema;
/// Scene-window trimming for parsed activity samples.
pub mod trim;

use crate::activity::interpolate::densify_activity;
use crate::activity::schema::{DebugPayload, DenseActivityReport, ParsedActivity};
use crate::activity::trim::trim_activity;
use crate::config::RenderConfig;
use serde_json::Value;

/// Parses frontend activity JSON from either production or debug payload shapes.
pub fn parse_activity_json(input: &str) -> Result<ParsedActivity, String> {
    // Debug exports wrap the real payload under `parsed_activity`; production
    // calls pass the payload directly. Accept both to keep diagnostics reusable
    // in tests and local tooling.
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

/// Trims and densifies parsed activity data for the provided render config.
pub fn build_dense_activity_report(
    activity: &ParsedActivity,
    config: &RenderConfig,
) -> Result<DenseActivityReport, String> {
    // Data requirements are derived from the template before trimming so unused
    // high-cardinality series never get copied or densified.
    let requirements = config.render_data_requirements()?;
    let trimmed = trim_activity(
        activity,
        config.scene.start,
        config.scene.end,
        &requirements,
    )?;
    Ok(densify_activity(&trimmed, config.scene.fps, &requirements))
}

#[cfg(test)]
mod tests {
    use super::{build_dense_activity_report, parse_activity_json};
    use crate::config::parse_config_json;
    use std::fs;
    use std::path::PathBuf;

    // Resolves the repository root from the crate manifest directory.
    fn repo_root() -> PathBuf {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    // Loads an activity parser debug fixture by filename.
    fn fixture(name: &str) -> String {
        let path = repo_root().join("debug").join("activities").join(name);
        fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("Failed to read {}: {error}", path.display()))
    }

    #[test]
    // Verifies that a complete GPX fixture densifies to the expected frame count.
    fn builds_dense_report_for_full_fixture() {
        let activity = parse_activity_json(&fixture("Test_GPX-parse-debug.json")).unwrap();
        let config = parse_config_json(
            r#"{
                "scene":{"fps":30,"start":0,"end":4912,"width":3840,"height":2160},
                "values":[{"value":"speed","x":0,"y":0}]
            }"#,
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
        assert!(report.series.course_lat.is_empty());
    }

    #[test]
    // Verifies non-integer trim windows produce stable frame counts at common FPS values.
    fn trims_non_integer_window_across_multiple_fps() {
        let activity = parse_activity_json(&fixture("Test_FIT-parse-debug.json")).unwrap();

        for (fps, expected_frames) in [(24.0, 708usize), (30.0, 885usize), (60.0, 1770usize)] {
            let config = parse_config_json(&format!(
                r#"{{
                    "scene":{{"fps":{fps},"start":600.25,"end":629.75}},
                    "values":[{{"value":"time","x":0,"y":0}}]
                }}"#
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
            assert!(report.series.time.is_empty());
        }
    }

    #[test]
    // Verifies only telemetry required by values/plots is densified.
    fn only_densifies_series_requested_by_template() {
        let activity = parse_activity_json(&fixture("Test_FIT-parse-debug.json")).unwrap();
        let config = parse_config_json(
            r#"{
                "scene":{"fps":30,"start":600,"end":630},
                "values":[{"value":"speed","x":0,"y":0}],
                "plots":{"course":{"value":"course","x":0,"y":0,"width":200,"height":100}}
            }"#,
        )
        .unwrap();

        let report = build_dense_activity_report(&activity, &config).unwrap();

        assert_eq!(report.series.speed.len(), report.frame_count);
        assert!(report.series.elevation.is_empty());
        assert!(report.series.gradient.is_empty());
        assert!(report.series.time.is_empty());
        assert!(report.series.course_lat.is_empty());
        assert!(report.series.course_lon.is_empty());
        assert_eq!(report.frame_distance_progress.len(), report.frame_count);
    }

    #[test]
    // Verifies trimmed plot progress remains absolute to the full activity.
    fn trimmed_exports_keep_absolute_distance_progress() {
        let activity = parse_activity_json(&fixture("Test_FIT-parse-debug.json")).unwrap();
        let config = parse_config_json(
            r#"{
                "scene":{"fps":30,"start":600,"end":630},
                "plots":{"course":{"value":"course","x":0,"y":0,"width":200,"height":100}}
            }"#,
        )
        .unwrap();

        let report = build_dense_activity_report(&activity, &config).unwrap();

        let first_progress = report
            .frame_distance_progress
            .first()
            .and_then(|value| *value)
            .unwrap_or_default();
        let last_progress = report
            .frame_distance_progress
            .last()
            .and_then(|value| *value)
            .unwrap_or_default();

        assert!(first_progress > 0.0);
        assert!(last_progress > first_progress);
        assert!(last_progress < 1.0);
    }
}
