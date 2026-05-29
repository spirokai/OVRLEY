//! Heading compass tape widget.
//!
//! The heading widget renders a horizontal compass tape that scrolls with the
//! heading value. The tape is a 360-degree wrapped strip with configurable
//! major/minor ticks, numeric/cardinal labels, and a center indicator.
//!
//! Module ownership:
//! - `geometry` — Pure tick position, label placement, and scroll offset math.
//! - `prepare` — Cached tape image rendering and per-frame offset precomputation.
//! - `draw` — Per-frame tiled tape draw + indicator rendering.

mod draw;
pub(crate) mod geometry;
mod prepare;

pub(crate) use draw::draw_heading_widget;
pub(crate) use prepare::prepare_heading_cache;
