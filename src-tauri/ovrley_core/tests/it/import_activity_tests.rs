//! `import_activity_path`: the one activity import entry (Phase 3, D4).

use ovrley_core::activity::{import_activity_path, ACTIVITY_EXTENSIONS};
use ovrley_core::paths::AppPaths;
use std::path::{Path, PathBuf};

const TCX: &str = r#"<TrainingCenterDatabase xmlns="http://www.garmin.com/xmlschemas/TrainingCenterDatabase/v2">
  <Activities><Activity Sport="Biking">
    <Id>2026-06-07T11:01:49Z</Id>
    <Lap StartTime="2026-06-07T11:01:49Z"><TotalTimeSeconds>2</TotalTimeSeconds><Track>
      <Trackpoint><Time>2026-06-07T11:01:49Z</Time><DistanceMeters>0</DistanceMeters></Trackpoint>
      <Trackpoint><Time>2026-06-07T11:01:50Z</Time><DistanceMeters>10</DistanceMeters></Trackpoint>
      <Trackpoint><Time>2026-06-07T11:01:51Z</Time><DistanceMeters>20</DistanceMeters></Trackpoint>
    </Track></Lap>
  </Activity></Activities>
</TrainingCenterDatabase>"#;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/activity")
        .join(name)
}

fn paths() -> AppPaths {
    AppPaths::from_repo_root(PathBuf::from("."))
}

/// A copy of one sample per extension, named with the extension in lower or upper case.
fn sample(dir: &Path, extension: &str, upper: bool) -> String {
    let name = format!(
        "ride.{}",
        if upper {
            extension.to_uppercase()
        } else {
            extension.to_string()
        }
    );
    let target = dir.join(name);
    match extension {
        "tcx" => std::fs::write(&target, TCX).unwrap(),
        other => {
            let source = match other {
                "gpx" => "test.gpx",
                "fit" => "test-ride.fit",
                "srt" => "DJI-sample2.SRT",
                "igc" => "20180427.igc",
                "csv" => "Shearwater.csv",
                "vbo" => "VBO-test.vbo",
                "uddf" => "test-uddf.uddf",
                "ssrf" | "xml" => "test-subsurface.ssrf",
                _ => unreachable!("{other} has no sample"),
            };
            std::fs::copy(fixture(source), &target).unwrap();
        }
    }
    target.to_string_lossy().into_owned()
}

#[test]
fn every_extension_imports_in_both_cases_without_a_debug_payload() {
    let dir = std::env::temp_dir().join(format!("ovrley-import-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for upper in [false, true] {
        for extension in ACTIVITY_EXTENSIONS {
            let case_dir = dir
                .join(if upper { "upper" } else { "lower" })
                .join(extension);
            std::fs::create_dir_all(&case_dir).unwrap();
            let path = sample(&case_dir, extension, upper);
            let response = import_activity_path(&paths(), &path)
                .unwrap_or_else(|error| panic!("{path}: {error}"));
            assert!(response.debug_payload.is_none(), "{path}");
            assert!(
                !response.parsed_activity.sample_elapsed_seconds.is_empty(),
                "{path}"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unsupported_extension_fails_with_the_file_name() {
    let error = import_activity_path(&paths(), "/a/notes.txt").unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Unsupported activity file format: notes.txt"),
        "{error}"
    );
}

#[test]
fn a_path_without_a_file_name_fails() {
    let error = import_activity_path(&paths(), "/a/").unwrap_err();
    assert!(
        error.to_string().contains("Invalid activity path: /a/"),
        "{error}"
    );
}
