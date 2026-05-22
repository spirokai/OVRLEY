//! Central fixture-path resolver for all tests.
//!
//! Every test imports paths from here. No test file should contain
//! `repo_root()`, `fixture_path()`, or `parent().unwrap()` chains.

use std::path::PathBuf;

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn fixtures() -> PathBuf {
    repo_root().join("tests").join("fixtures")
}

pub fn parsed_activity_path() -> PathBuf {
    fixtures().join("activity").join("gpx-parse-debug.json")
}

pub fn fit_activity_path() -> PathBuf {
    fixtures().join("activity").join("fit-parse-debug.json")
}

pub fn simple_config_path() -> PathBuf {
    fixtures().join("config").join("simple.json")
}

pub fn composite_config_path() -> PathBuf {
    fixtures().join("config").join("composite.json")
}

pub fn ffprobe_1080p_path() -> PathBuf {
    fixtures().join("ffprobe").join("1080p.json")
}

pub fn sample_video_path() -> PathBuf {
    fixtures().join("video").join("test-1080p.mp4")
}

pub fn test_1080p_video_path() -> PathBuf {
    fixtures().join("video").join("test-1080p.mp4")
}

pub fn workspace_root() -> PathBuf {
    repo_root()
        .parent()
        .unwrap()
        .to_path_buf()
}

pub fn repo_git_root() -> PathBuf {
    repo_root()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}
