//! Central fixture-path resolver for all tests.
//!
//! Every test imports paths from here. No test file should contain
//! `repo_root()`, `fixture_path()`, or `parent().unwrap()` chains —
//! all path resolution lives in this single file.
//!
//! To redirect all tests to a different fixture directory (e.g., CI mirror),
//! change the `fixtures()` function. No other test file needs modification.
//!
//! Some functions may appear unused depending on which test crates are
//! compiled — this is expected because different test files use different
//! subsets of the registry. All entries are intentionally retained as
//! the canonical fixture catalogue.
//!
//! ## Thread Safety
//!
//! All functions are pure path constructors — no shared state, no mutex,
//! safe to call from any thread.
//!
//! ## Role
//!
//! This is test infrastructure code, not production code. Production code
//! must never import from the test configuration layer.

use std::path::PathBuf;

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn fixtures() -> PathBuf {
    repo_root().join("tests").join("fixtures")
}

#[allow(dead_code)]
pub fn parsed_activity_path() -> PathBuf {
    fixtures().join("activity").join("gpx-parse-debug.json")
}

#[allow(dead_code)]
pub fn fit_activity_path() -> PathBuf {
    fixtures().join("activity").join("fit-parse-debug.json")
}

#[allow(dead_code)]
pub fn simple_config_path() -> PathBuf {
    fixtures().join("config").join("simple.json")
}

#[allow(dead_code)]
pub fn composite_config_path() -> PathBuf {
    fixtures().join("config").join("composite.json")
}

#[allow(dead_code)]
pub fn ffprobe_1080p_path() -> PathBuf {
    fixtures().join("ffprobe").join("1080p.json")
}

#[allow(dead_code)]
pub fn sample_video_path() -> PathBuf {
    fixtures().join("video").join("test-1080p.mp4")
}

#[allow(dead_code)]
pub fn workspace_root() -> PathBuf {
    repo_root().parent().unwrap().to_path_buf()
}

#[allow(dead_code)]
pub fn repo_git_root() -> PathBuf {
    repo_root()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}
