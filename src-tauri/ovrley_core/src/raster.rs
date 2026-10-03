//! Validates a selected bitmap once and prepares its oriented editor preview.

use image::{metadata::Orientation, ImageDecoder, ImageFormat, ImageReader};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::Arc;

pub const MAX_ENCODED_BYTES: u64 = 5 * 1024 * 1024;
const MAX_PIXELS: u64 = 25_000_000;

/// An immutable, validated image. Original bytes belong to persistence; the
/// oriented PNG is shared by editor previews and render jobs without copying.
pub struct SelectedRaster {
    width: u32,
    height: u32,
    preview_png: Vec<u8>,
    content_hash: u64,
    encoded_bytes: Vec<u8>,
    extension: &'static str,
}

impl SelectedRaster {
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn preview_png(&self) -> &[u8] {
        &self.preview_png
    }

    pub fn content_hash(&self) -> u64 {
        self.content_hash
    }

    pub fn encoded_bytes(&self) -> &[u8] {
        &self.encoded_bytes
    }

    pub fn extension(&self) -> &'static str {
        self.extension
    }
}

impl std::fmt::Debug for SelectedRaster {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SelectedRaster")
            .field("dimensions", &self.dimensions())
            .field("extension", &self.extension)
            .field("encoded_size", &self.encoded_bytes.len())
            .finish_non_exhaustive()
    }
}

/// The shell owns session resources; the core only borrows their lookup seam.
pub trait RasterResourceResolver {
    fn resolve(&self, resource_id: &str) -> Option<Arc<SelectedRaster>>;
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum RasterError {
    #[error("Select an absolute image path.")]
    RelativePath,
    #[error("The image file is missing.")]
    Missing,
    #[error("The image file cannot be read.")]
    Unreadable,
    #[error("The image must be a PNG, JPEG, BMP, or TIFF file with a supported extension.")]
    UnsupportedType,
    #[error("The image exceeds the 5 MiB encoded file limit.")]
    EncodedSize,
    #[error("The image exceeds the 25 MP decoded resolution limit.")]
    Resolution,
    #[error("The image could not be decoded.")]
    Decode,
    #[error("The embedded image asset is missing. Select a replacement image.")]
    MissingAsset,
    #[error("The embedded image asset is corrupt. Select a replacement image.")]
    CorruptAsset,
}

impl RasterError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::RelativePath => "relative_path",
            Self::Missing => "missing",
            Self::Unreadable => "unreadable",
            Self::UnsupportedType => "unsupported_type",
            Self::EncodedSize => "encoded_size",
            Self::Resolution => "resolution",
            Self::Decode => "decode",
            Self::MissingAsset => "missing_asset",
            Self::CorruptAsset => "corrupt_asset",
        }
    }

    pub fn for_widget(self, id: &str) -> crate::error::CoreError {
        crate::error::CoreError::Config(format!(
            "Raster {id}: {self} [raster_error:{}:{id}]",
            self.code()
        ))
    }
}

pub fn valid_raster_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// Archive names use canonical extensions, while file selection also accepts jpg.
pub fn is_canonical_extension(extension: &str) -> bool {
    format_for_extension(extension).is_ok_and(|(_, canonical)| canonical == extension)
}

fn format_for_extension(extension: &str) -> Result<(ImageFormat, &'static str), RasterError> {
    match extension.to_ascii_lowercase().as_str() {
        "png" => Ok((ImageFormat::Png, "png")),
        "jpeg" | "jpg" => Ok((ImageFormat::Jpeg, "jpeg")),
        "bmp" => Ok((ImageFormat::Bmp, "bmp")),
        "tiff" => Ok((ImageFormat::Tiff, "tiff")),
        _ => Err(RasterError::UnsupportedType),
    }
}

fn expected_format(path: &Path) -> Result<(ImageFormat, &'static str), RasterError> {
    if !path.is_absolute() {
        return Err(RasterError::RelativePath);
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .ok_or(RasterError::UnsupportedType)?;
    format_for_extension(extension)
}

fn io_error(error: std::io::Error) -> RasterError {
    if error.kind() == std::io::ErrorKind::NotFound {
        RasterError::Missing
    } else {
        RasterError::Unreadable
    }
}

fn oriented_dimensions(
    width: u32,
    height: u32,
    orientation: Orientation,
) -> Result<(u32, u32), RasterError> {
    let swaps_axes = matches!(
        orientation,
        Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH
    );
    let dimensions = if swaps_axes {
        (height, width)
    } else {
        (width, height)
    };
    if dimensions.0 == 0 || dimensions.1 == 0 {
        return Err(RasterError::Decode);
    }
    if u64::from(dimensions.0) * u64::from(dimensions.1) > MAX_PIXELS {
        return Err(RasterError::Resolution);
    }
    Ok(dimensions)
}

pub fn load_selected_raster(path: &Path) -> Result<SelectedRaster, RasterError> {
    let (expected, extension) = expected_format(path)?;
    let file = File::open(path).map_err(io_error)?;
    let metadata = file.metadata().map_err(io_error)?;
    if !metadata.is_file() {
        return Err(RasterError::Unreadable);
    }
    if metadata.len() > MAX_ENCODED_BYTES {
        return Err(RasterError::EncodedSize);
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_ENCODED_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    decode_selected_raster(expected, extension, bytes)
}

pub fn load_embedded_raster(
    extension: &str,
    bytes: Vec<u8>,
) -> Result<SelectedRaster, RasterError> {
    let (expected, extension) = format_for_extension(extension)?;
    decode_selected_raster(expected, extension, bytes)
}

fn decode_selected_raster(
    expected: ImageFormat,
    extension: &'static str,
    bytes: Vec<u8>,
) -> Result<SelectedRaster, RasterError> {
    if bytes.len() as u64 > MAX_ENCODED_BYTES {
        return Err(RasterError::EncodedSize);
    }
    let reader = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|_| RasterError::Decode)?;
    if reader.format() != Some(expected) {
        return Err(RasterError::UnsupportedType);
    }
    let mut decoder = reader.into_decoder().map_err(|_| RasterError::Decode)?;
    let orientation = decoder.orientation().map_err(|_| RasterError::Decode)?;
    let (raw_width, raw_height) = decoder.dimensions();
    let (width, height) = oriented_dimensions(raw_width, raw_height, orientation)?;

    let mut image = image::DynamicImage::from_decoder(decoder).map_err(|_| RasterError::Decode)?;
    image.apply_orientation(orientation);
    let mut preview_png = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut preview_png), ImageFormat::Png)
        .map_err(|_| RasterError::Decode)?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    preview_png.hash(&mut hasher);
    Ok(SelectedRaster {
        width,
        height,
        preview_png,
        content_hash: hasher.finish(),
        encoded_bytes: bytes,
        extension,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_extension_is_strict() {
        let absolute = std::env::current_dir().unwrap();
        assert!(matches!(
            expected_format(Path::new("image.png")),
            Err(RasterError::RelativePath)
        ));
        assert!(matches!(
            expected_format(&absolute.join("image.tif")),
            Err(RasterError::UnsupportedType)
        ));
        assert_eq!(
            expected_format(&absolute.join("image.JPEG")).unwrap(),
            (ImageFormat::Jpeg, "jpeg")
        );
    }

    #[test]
    fn rejects_oversized_encoded_input_before_decode() {
        let bytes = vec![0; MAX_ENCODED_BYTES as usize + 1];
        assert!(matches!(
            decode_selected_raster(ImageFormat::Png, "png", bytes),
            Err(RasterError::EncodedSize)
        ));
    }

    #[test]
    fn enforces_oriented_resolution_limit() {
        assert_eq!(
            oriented_dimensions(5_000, 5_000, Orientation::NoTransforms).unwrap(),
            (5_000, 5_000)
        );
        assert!(matches!(
            oriented_dimensions(5_001, 5_000, Orientation::Rotate90),
            Err(RasterError::Resolution)
        ));
    }

    #[test]
    fn applies_jpeg_orientation_to_dimensions_and_preview() {
        let image = image::DynamicImage::ImageRgb8(image::RgbImage::new(2, 3));
        let mut jpeg = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut jpeg), ImageFormat::Jpeg)
            .unwrap();
        let exif = [
            0xff, 0xe1, 0x00, 0x22, b'E', b'x', b'i', b'f', 0, 0, b'I', b'I', 0x2a, 0, 8, 0, 0, 0,
            1, 0, 0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
        ];
        jpeg.splice(2..2, exif);
        let selected = decode_selected_raster(ImageFormat::Jpeg, "jpeg", jpeg).unwrap();
        assert_eq!((selected.width, selected.height), (3, 2));
        let preview =
            image::load_from_memory_with_format(&selected.preview_png, ImageFormat::Png).unwrap();
        assert_eq!((preview.width(), preview.height()), (3, 2));
    }

    #[test]
    fn decodes_every_supported_image_format() {
        let image = image::DynamicImage::ImageRgb8(image::RgbImage::new(2, 3));
        for format in [
            ImageFormat::Png,
            ImageFormat::Jpeg,
            ImageFormat::Bmp,
            ImageFormat::Tiff,
        ] {
            let mut bytes = Vec::new();
            image
                .write_to(&mut Cursor::new(&mut bytes), format)
                .unwrap();
            let extension = match format {
                ImageFormat::Png => "png",
                ImageFormat::Jpeg => "jpeg",
                ImageFormat::Bmp => "bmp",
                ImageFormat::Tiff => "tiff",
                _ => unreachable!(),
            };
            let selected = decode_selected_raster(format, extension, bytes).unwrap();
            assert_eq!((selected.width, selected.height), (2, 3));
        }
    }
}
