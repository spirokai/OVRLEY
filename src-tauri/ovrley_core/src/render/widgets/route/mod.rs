//! Native route preparation and composition. The shared route family owns
//! projection, simplification, geometry, sampling and drawing.

mod draw;
pub(crate) mod prepare;

pub(crate) use draw::draw_route_widget;
pub(crate) use prepare::prepare_route_cache;
