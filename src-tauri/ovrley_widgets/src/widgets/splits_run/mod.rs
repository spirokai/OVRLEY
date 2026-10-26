//! Running splits table: split number, pace-proportional bar and pace per split.
//! `ovrley_core` derives the split table; this family only samples and draws it.
pub mod format;
pub mod frame;
pub mod interaction;
pub mod layout;
pub mod preview;
pub mod table;
pub mod validation;

#[cfg(test)]
mod tests;

/// Running bar length resolution in the frame key.
const BAR_TICKS: u32 = 4096;
/// Gap between columns relative to the value font size.
const COLUMN_GAP_RATIO: f32 = 0.6;
/// The split number sits half a column gap further from its bar.
const NUMBER_BAR_GAP_RATIO: f32 = COLUMN_GAP_RATIO * 1.5;
const PLACEHOLDER: &str = "-:--";
/// A running split without a pace draws a full-length bar at this share of its opacity.
const PLACEHOLDER_BAR_OPACITY: f32 = 0.35;
