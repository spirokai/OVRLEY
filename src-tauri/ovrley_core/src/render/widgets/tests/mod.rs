//! Widget-specific test module root.
//!
//! Declares sub-modules for RDP simplification behavior tests applied
//! to elevation and route data. These tests exercise internal widget
//! types (`ElevationSample`, `RouteSample`) not available to crate-level
//! integration tests. The parent `mod.rs` wires this module via
//! `#[cfg(test)] mod tests;` because the tested functions are `pub(crate)`
//! or private and require module-local access.
//!
//! ## Sub-modules
//!
//! - `rdp_elevation_tests.rs` — elevation-sample RDP simplification
//! - `rdp_route_tests.rs` — route-sample RDP simplification

mod elevation_frame_state_tests;
mod elevation_geometry_tests;
mod elevation_reduction_tests;
mod rdp_elevation_tests;
mod rdp_route_tests;
