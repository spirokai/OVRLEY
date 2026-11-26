//! Imported source videos: their probed metadata, the user's edits, the manual sync state and
//! embedded telemetry. Rendering and output encoding live in [`crate::encode`].

pub mod edits;
pub mod probe;
pub mod sync;
pub mod telemetry;
