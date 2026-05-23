//! Route/course plot widget.
//!
//! The route widget projects latitude/longitude samples into a local 2D plane,
//! fits and simplifies the path inside the configured widget bounds, caches the
//! remaining route as a static layer, and draws the completed route plus marker
//! for each frame.
//!
//! Module ownership:
//! - `normalize` — plot option normalization and default resolution.
//! - `prepare` — sample building, geometry fitting, static-layer caching.
//! - `frame_state` — per-frame marker position and polyline prefix computation.
//! - `simplify` — RDP and LTTB point decimation helpers.
//! - `draw` — per-frame canvas composition of layers, polyline, and marker.

mod normalize;
mod prepare;
mod frame_state;
mod simplify;
mod draw;

pub(crate) use draw::draw_route_widget;
pub(crate) use prepare::prepare_route_cache;
#[allow(unused_imports)]
pub(crate) use simplify::simplify_route_samples;
