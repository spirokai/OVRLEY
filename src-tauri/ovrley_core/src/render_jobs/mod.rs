//! Render contracts, inspection freshness and shared native execution ownership.
//! The batch runner owns queue transitions; sinks only observe snapshots.

mod activity;
pub mod batch;
mod batch_state;
pub mod planning;
mod submission;

pub mod contracts;
pub mod execution;
pub mod inspection;
