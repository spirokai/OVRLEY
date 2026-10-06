//! Render contracts, inspection freshness and shared native execution ownership.
//! The batch runner owns queue transitions; sinks only observe snapshots.

pub mod batch;
pub mod batch_plan;
pub mod contracts;
pub mod execution;
pub mod inspection;
