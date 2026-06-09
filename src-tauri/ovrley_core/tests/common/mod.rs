//! Shared test support - module root.
//!
//! Re-exports `test_config` and any shared helper utilities used across
//! multiple integration test files. Keep this file minimal: it declares
//! submodules only. Real test-support logic lives in `test_config.rs`.
//!
//! ## Role
//! This is test infrastructure code, not production code. It provides the
//! shared fixture-path resolver and any common test utilities needed by
//! the crate-level integration test suite under `ovrley_core/tests/`.

pub mod builders;
pub mod composite;
pub mod seam;
pub mod test_config;
