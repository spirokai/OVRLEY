//! Map labels: the text side. Which face draws a character, SDF glyph bitmaps, and the layout that
//! turns a label's text into positioned glyphs.
//!
//! The glyph service is native only; layout takes glyph metrics from a lookup, so it also runs where
//! there are no font files.

mod glyphs;
#[cfg(not(target_arch = "wasm32"))]
mod service;
mod text;

pub use glyphs::GlyphBitmap;
#[cfg(not(target_arch = "wasm32"))]
pub use service::{GlyphKey, GlyphService, ServedGlyph};
pub use text::{
    font_chain, shape, ChainLink, Face, FontGlyph, GlyphMetrics, PositionedGlyph, ShapeError,
    ShapeStyle, Shaping, TextAnchor, TextJustify, ONE_EM,
};
