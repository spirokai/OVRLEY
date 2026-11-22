//! Public façade for CSV header resolution.
//!
//! Header data types live in [`types`], while exact alias parsing and
//! units-row recognition live in [`parser`]. This façade keeps the
//! existing `csv::headers` API local to the CSV importer while allowing those
//! concerns to evolve independently.

pub(super) use super::parser::{is_compatible_units_row, parse_header_candidate};
pub(super) use super::types::{
    AccelerationKind, ControlKind, HeaderColumn, HeaderLayout, SourcePriority, TimingKind,
};
