//! OVRLEY's widgets: their configuration and validation, activity sampling, layout and Skia
//! drawing, shared by export and the editor's preview. Hosts acquire fonts, images and map frames;
//! this crate performs no filesystem or encoding work.

// The activity column macros the build script generates are used before `activity` declares them.
#[macro_use]
pub mod model;
pub mod activity;
pub mod preview;
pub mod render;
pub mod series;
pub mod widgets;

/// Why a widget could not be validated or drawn.
#[derive(Debug, thiserror::Error)]
pub enum WidgetError {
    /// A configuration field is missing or out of range; the message names it.
    #[error("Invalid configuration: {0}")]
    Config(String),
    /// A host resource or Skia surface failed.
    #[error("Widget rendering failed: {0}")]
    Render(String),
}

pub type WidgetResult<T> = Result<T, WidgetError>;
