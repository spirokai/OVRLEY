//! Colour of the composite source video, which the raw overlay must match.
//!
//! FFmpeg converts the RGBA overlay into the main video's colour space. A
//! source without a matrix tag is converted with BT.601, while players display
//! untagged HD video as BT.709. Untagged HD sources are therefore treated as
//! BT.709 and the main video is tagged accordingly (see
//! [`YuvColor::untagged_hd_as_bt709`]), so the overlay colours match on screen.

use crate::render::FrameSize;

/// The `-colorspace` tag of the source stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorSpaceTag {
    /// Untagged; FFmpeg converts with BT.601 coefficients. HD sources are
    /// retagged BT.709 before use (see [`YuvColor::untagged_hd_as_bt709`]).
    Unspecified,
    Bt470bg,
    Smpte170m,
    Bt709,
    Bt2020nc,
}

/// The `-color_range` tag of the source stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorRangeTag {
    /// Untagged; FFmpeg treats it as limited range.
    Unspecified,
    Limited,
    Full,
}

/// Colour of the source video at the overlay, from its probed stream tags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct YuvColor {
    pub space: ColorSpaceTag,
    pub range: ColorRangeTag,
}

impl YuvColor {
    /// Maps ffprobe's `color_space` and `color_range`. Returns None for a
    /// colour space without a supported matrix.
    pub fn from_probe(color_space: Option<&str>, color_range: Option<&str>) -> Option<Self> {
        let space = match color_space {
            None | Some("unknown") => ColorSpaceTag::Unspecified,
            Some("bt470bg") => ColorSpaceTag::Bt470bg,
            Some("smpte170m") => ColorSpaceTag::Smpte170m,
            Some("bt709") => ColorSpaceTag::Bt709,
            Some("bt2020nc") => ColorSpaceTag::Bt2020nc,
            Some(_) => return None,
        };
        let range = match color_range {
            Some("pc") => ColorRangeTag::Full,
            Some("tv") => ColorRangeTag::Limited,
            _ => ColorRangeTag::Unspecified,
        };
        Some(Self { space, range })
    }

    /// For an untagged matrix on an HD or larger frame (width at least 1280 or
    /// height above 576, the rule players use to pick BT.709), returns the
    /// colour as BT.709. The caller must then tag the main video `bt709` too.
    /// Returns None when the source's own tag applies.
    pub fn untagged_hd_as_bt709(self, size: FrameSize) -> Option<Self> {
        let hd = size.width >= 1280 || size.height > 576;
        (self.space == ColorSpaceTag::Unspecified && hd).then_some(Self {
            space: ColorSpaceTag::Bt709,
            range: self.range,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untagged_hd_sources_are_treated_as_bt709() {
        let untagged = YuvColor::from_probe(None, Some("pc")).unwrap();
        let size = |width, height| FrameSize { width, height };
        let bt709_full = YuvColor {
            space: ColorSpaceTag::Bt709,
            range: ColorRangeTag::Full,
        };
        assert_eq!(
            untagged.untagged_hd_as_bt709(size(3840, 2160)),
            Some(bt709_full)
        );
        assert_eq!(
            untagged.untagged_hd_as_bt709(size(1080, 1920)),
            Some(bt709_full)
        );
        assert_eq!(untagged.untagged_hd_as_bt709(size(720, 576)), None);
        let bt709_limited = YuvColor::from_probe(Some("bt709"), Some("tv")).unwrap();
        assert_eq!(bt709_limited.untagged_hd_as_bt709(size(3840, 2160)), None);
        let smpte = YuvColor::from_probe(Some("smpte170m"), None).unwrap();
        assert_eq!(smpte.untagged_hd_as_bt709(size(1920, 1080)), None);
    }

    #[test]
    fn probe_tags_map_to_colour_tags() {
        assert_eq!(
            YuvColor::from_probe(Some("unknown"), Some("pc")),
            Some(YuvColor {
                space: ColorSpaceTag::Unspecified,
                range: ColorRangeTag::Full,
            })
        );
        assert_eq!(
            YuvColor::from_probe(None, None).unwrap().range,
            ColorRangeTag::Unspecified
        );
        assert_eq!(YuvColor::from_probe(Some("smpte240m"), Some("tv")), None);
    }
}
