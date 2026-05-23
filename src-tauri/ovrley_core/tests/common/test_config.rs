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
//! ## Available fixtures
//!
//! | Function | Path relative to `tests/fixtures/` |
//! |----------|-------------------------------------|
//! | `parsed_activity_path()` | `activity/gpx-parse-debug.json` |
//! | `fit_activity_path()` | `activity/fit-parse-debug.json` |
//! | `simple_config_path()` | `config/simple.json` |
//! | `composite_config_path()` | `config/composite.json` |
//! | `ffprobe_1080p_path()` | `ffprobe/1080p.json` |
//! | `sample_video_path()` | `video/test-1080p.mp4` (alias: `test_1080p_video_path()`) |
//! | `workspace_root()` | `CARGO_MANIFEST_DIR.parent()` — Tauri workspace root |
//! | `repo_git_root()` | Git repository root (two levels up from the crate) |
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

// Each test crate compiles independently and uses a different subset of
// fixture paths. All functions in this file are intentional registry entries
// and must be kept even if some are not referenced by every test crate.
#![allow(dead_code)]

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
    repo_root().parent().unwrap().to_path_buf()
}

pub fn repo_git_root() -> PathBuf {
    repo_root()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}
