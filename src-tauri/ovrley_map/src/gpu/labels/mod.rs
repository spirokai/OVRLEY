//! Map labels: the text side. Which face draws a character, SDF glyph bitmaps, and the layout that
//! turns a label's text into positioned glyphs.
//!
//! The glyph service is native only; layout takes glyph metrics from a lookup, so it also runs where
//! there are no font files.
//!
//! Names along roads and rivers lie flat on the ground (`line`): their glyphs are walked along the line for
//! each camera.
//!
//! Label tiles are laid out with the tile geometry (`layout`), kept on the CPU, and placed for each
//! camera (`placement`). Only the quads placement chose reach the GPU (`draw`).

mod draw;
#[cfg(not(target_arch = "wasm32"))]
mod glyphs;
mod layer;
mod layout;
mod line;
mod placement;
#[cfg(not(target_arch = "wasm32"))]
mod service;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
mod text;

pub(crate) use draw::LabelLayers;
pub(crate) use layer::{is_label_source, parse_label_layer, LabelLayer};
pub(crate) use layout::{lay_out, LabelSymbol};
pub use layout::{GlyphId, GlyphLookup};

#[cfg(not(target_arch = "wasm32"))]
pub use service::{GlyphKey, GlyphService, ServedGlyph};
pub use text::{
    font_chain, shape, ChainLink, Face, FontGlyph, GlyphBitmap, GlyphMetrics, PositionedGlyph,
    ShapeError, ShapeStyle, Shaping, TextAnchor, TextJustify, ONE_EM,
};
