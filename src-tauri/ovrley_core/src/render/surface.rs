//! Skia surface allocation and PNG output helpers.
//!
//! Owns: Skia raster surface creation (`create_surface`), native pixel-format
//!       info (`native_n32_image_info`), buffer wrapping for video frames
//!       (`wrap_native_surface`), and PNG export (`write_surface_png`).
//! Does not own: frame rendering (see [`crate::render::mod`]), text drawing
//!       (see [`crate::render::text`]), widget rendering (see
//!       [`crate::render::widgets`]).
//!
//! Allowed dependencies: `skia_safe`, `crate::error`.
//! Forbidden dependencies: `config`, `activity`, `encode`, `commands`.
//!
//! ## Performance
//! `create_surface` performs a heap allocation per call (owned Skia surface).
//! `wrap_native_surface` is allocation-free — it borrows a caller-owned RGBA
//! buffer and is safe for the video render hot path. `write_surface_png` is
//! called once per preview render, not in the video hot loop.

use crate::error::{CoreError, CoreResult};
use skia_safe::{surfaces, Borrows, EncodedImageFormat, ImageInfo, Surface};
use std::path::Path;

/// Creates an owned Skia raster surface using Skia's native 32-bit format.
pub fn create_surface(width: u32, height: u32) -> CoreResult<Surface> {
    surfaces::raster_n32_premul((width as i32, height as i32)).ok_or_else(|| {
        CoreError::Render(format!("Failed to create raster surface {width}x{height}"))
    })
}

/// Builds image metadata for the RGBA buffers used by the video pipeline.
pub fn native_n32_image_info(width: u32, height: u32) -> ImageInfo {
    ImageInfo::new(
        (width as i32, height as i32),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Unpremul,
        None,
    )
}

/// Wraps caller-owned RGBA pixels in a Skia surface.
///
/// The returned surface borrows `pixels`, so drawing writes directly into the
/// encoder buffer without an intermediate copy.
pub fn wrap_native_surface<'pixels>(
    width: u32,
    height: u32,
    pixels: &'pixels mut [u8],
) -> CoreResult<Borrows<'pixels, Surface>> {
    let info = native_n32_image_info(width, height);
    let row_bytes = (width as usize) * 4;
    surfaces::wrap_pixels(&info, pixels, row_bytes, None).ok_or_else(|| {
        CoreError::Render(format!(
            "Failed to wrap native n32 surface {width}x{height}"
        ))
    })
}

/// Encodes a Skia surface snapshot as PNG and writes it to `path`.
pub fn write_surface_png(surface: &mut Surface, path: &Path) -> CoreResult<()> {
    let image = surface.image_snapshot();
    let data = image
        .encode(None, EncodedImageFormat::PNG, 100)
        .ok_or_else(|| CoreError::Render("Failed to encode preview PNG".into()))?;
    std::fs::write(path, data.as_bytes()).map_err(|source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}
