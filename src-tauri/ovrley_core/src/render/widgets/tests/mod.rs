//! Widget-specific test module root.
//!
//! Declares sub-modules for internal geometry and reduction behavior that is
//! not part of the crate's public API. The parent `mod.rs` wires this module via
//! `#[cfg(test)] mod tests;` because the tested functions are `pub(crate)`
//! or private and require module-local access.
//!
//! Gauge modules here cover normalized fill, continuous reveal transitions,
//! and segmented wedge geometry. Elevation and route modules cover frame
//! state, projection, reduction, and RDP behavior.

mod elevation_frame_state_tests;
mod elevation_geometry_tests;
mod elevation_reduction_tests;
mod g_force_frame_state_tests;
mod gauge_metric_tests;
mod rdp_elevation_tests;
