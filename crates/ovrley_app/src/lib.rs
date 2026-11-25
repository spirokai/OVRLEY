//! OVRLEY's application logic, independent of the UI toolkit: the [`store`] and its dispatcher,
//! the [`features`] that act on it, and the domain they share (the [`video`] collection and
//! program, the [`activity`] window, [`controls`], [`format`], [`i18n`]). `ovrley_ui` draws the
//! window and projects the store into it; `ovrley_mcp` drives the same actions for agents.

pub mod activity;
pub mod controls;
pub mod dialogs;
pub mod features;
pub mod format;
pub mod fps;
pub mod i18n;
pub mod preferences;
pub mod shortcuts;
pub mod store;
pub mod video;
