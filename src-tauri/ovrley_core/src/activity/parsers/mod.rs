//! Native activity file extraction into the canonical raw activity contract.

mod dive_xml;
/// Native GPX extraction.
pub mod gpx;
/// Lenient readers for sensor values in external activity files.
pub mod raw_values;
/// Native Subsurface XML dive extraction.
pub mod subsurface;
/// Native Garmin TCX extraction into canonical raw activity samples.
pub mod tcx;
/// Native Universal Dive Data Format extraction.
pub mod uddf;
