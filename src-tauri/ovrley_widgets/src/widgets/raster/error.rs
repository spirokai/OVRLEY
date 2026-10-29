use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize, thiserror::Error)]
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

    pub fn for_widget(self, id: &str) -> crate::error::WidgetError {
        crate::error::WidgetError::Config(format!(
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
