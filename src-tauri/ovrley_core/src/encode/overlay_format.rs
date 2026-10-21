//! Pixel format of the raw overlay frames piped to FFmpeg.
//!
//! Composite overlays are drawn as straight-alpha RGBA. Overlay graphs that
//! blend in YUV receive the frames already converted to `yuva420p` in the
//! source video's matrix and range, tagged exactly like the source, so FFmpeg
//! neither converts the overlay nor moves 4 bytes per pixel through the pipe.
//! FFmpeg converts an untagged overlay into the main video's colour space the
//! same way, so the composited colours are unchanged.
//!
//! A source without a matrix tag is converted by FFmpeg with BT.601, while
//! players display untagged HD video as BT.709. Untagged HD sources are
//! therefore treated as BT.709 and the main video is tagged accordingly (see
//! [`YuvColor::untagged_hd_as_bt709`]), so the overlay colours match on screen.

use crate::error::{CoreError, CoreResult};
use crate::render::FrameSize;

/// YUV matrix coefficients supported for overlay conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YuvMatrix {
    Bt601,
    Bt709,
    Bt2020,
}

/// The `-colorspace` tag of the source stream, copied onto the overlay input.
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

/// The `-color_range` tag of the source stream, copied onto the overlay input.
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
    /// colour space without a supported matrix; its overlay stays RGBA.
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

    fn matrix(self) -> YuvMatrix {
        match self.space {
            ColorSpaceTag::Unspecified | ColorSpaceTag::Bt470bg | ColorSpaceTag::Smpte170m => {
                YuvMatrix::Bt601
            }
            ColorSpaceTag::Bt709 => YuvMatrix::Bt709,
            ColorSpaceTag::Bt2020nc => YuvMatrix::Bt2020,
        }
    }

    fn full_range(self) -> bool {
        self.range == ColorRangeTag::Full
    }
}

/// Raw frame format written to FFmpeg's overlay input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayPixelFormat {
    Rgba,
    Yuva420p(YuvColor),
}

impl OverlayPixelFormat {
    /// Rawvideo input options that describe one piped frame.
    pub fn ffmpeg_input_args(self) -> Vec<String> {
        match self {
            Self::Rgba => vec!["-pix_fmt".into(), "rgba".into()],
            Self::Yuva420p(color) => {
                let mut args = vec!["-pix_fmt".to_string(), "yuva420p".to_string()];
                let range = match color.range {
                    ColorRangeTag::Unspecified => None,
                    ColorRangeTag::Limited => Some("tv"),
                    ColorRangeTag::Full => Some("pc"),
                };
                let space = match color.space {
                    ColorSpaceTag::Unspecified => None,
                    ColorSpaceTag::Bt470bg => Some("bt470bg"),
                    ColorSpaceTag::Smpte170m => Some("smpte170m"),
                    ColorSpaceTag::Bt709 => Some("bt709"),
                    ColorSpaceTag::Bt2020nc => Some("bt2020nc"),
                };
                if let Some(range) = range {
                    args.extend(["-color_range".into(), range.into()]);
                }
                if let Some(space) = space {
                    args.extend(["-colorspace".into(), space.into()]);
                }
                args
            }
        }
    }

    /// Bytes of one piped frame.
    pub fn frame_len(self, size: FrameSize) -> CoreResult<usize> {
        match self {
            Self::Rgba => size.rgba_len(),
            Self::Yuva420p(_) => {
                let (luma, chroma) = yuva420p_plane_lens(size)?;
                luma.checked_mul(2)
                    .and_then(|planes| planes.checked_add(chroma * 2))
                    .ok_or_else(|| {
                        CoreError::Render(format!(
                            "yuva420p frame byte length overflow: {}x{}",
                            size.width, size.height
                        ))
                    })
            }
        }
    }
}

fn yuva420p_plane_lens(size: FrameSize) -> CoreResult<(usize, usize)> {
    let luma = size.rgba_len()? / 4;
    let chroma = (size.width as usize).div_ceil(2) * (size.height as usize).div_ceil(2);
    Ok((luma, chroma))
}

/// Fixed-point (16.16) conversion coefficients for one matrix and range.
struct Coefficients {
    y: [i64; 3],
    cb: [i64; 3],
    cr: [i64; 3],
    y_offset: i64,
}

const FIXED_ONE: f64 = 65536.0;

impl Coefficients {
    fn new(color: YuvColor) -> Self {
        let (kr, kb) = match color.matrix() {
            YuvMatrix::Bt601 => (0.299, 0.114),
            YuvMatrix::Bt709 => (0.2126, 0.0722),
            YuvMatrix::Bt2020 => (0.2627, 0.0593),
        };
        let kg = 1.0 - kr - kb;
        let (luma_scale, chroma_scale, y_offset) = if color.full_range() {
            (1.0, 1.0, 0.0)
        } else {
            (219.0 / 255.0, 224.0 / 255.0, 16.0)
        };
        let fixed = |value: f64| (value * FIXED_ONE).round() as i64;
        let cb_scale = chroma_scale / (2.0 * (1.0 - kb));
        let cr_scale = chroma_scale / (2.0 * (1.0 - kr));
        Self {
            y: [kr, kg, kb].map(|k| fixed(k * luma_scale)),
            cb: [-kr, -kg, 1.0 - kb].map(|k| fixed(k * cb_scale)),
            cr: [1.0 - kr, -kg, -kb].map(|k| fixed(k * cr_scale)),
            y_offset: fixed(y_offset),
        }
    }
}

fn clamp_byte(value: i64) -> u8 {
    value.clamp(0, 255) as u8
}

/// Converts straight-alpha RGBA into planar `yuva420p` (Y, U, V, A planes).
///
/// Chroma of each 2×2 block averages its pixels weighted by alpha, so
/// transparent pixels around anti-aliased edges do not darken the colour that
/// is blended. Fully transparent pixels carry black luma and neutral chroma.
pub fn convert_rgba_to_yuva420p(rgba: &[u8], size: FrameSize, color: YuvColor, out: &mut [u8]) {
    let width = size.width as usize;
    let height = size.height as usize;
    let chroma_width = width.div_ceil(2);
    let luma_len = width * height;
    let chroma_len = chroma_width * height.div_ceil(2);
    assert_eq!(rgba.len(), luma_len * 4, "RGBA frame length");
    assert_eq!(
        out.len(),
        luma_len * 2 + chroma_len * 2,
        "yuva420p frame length"
    );
    let coefficients = Coefficients::new(color);
    let black_luma = clamp_byte((coefficients.y_offset + 32768) >> 16);
    let (y_plane, rest) = out.split_at_mut(luma_len);
    let (u_plane, rest) = rest.split_at_mut(chroma_len);
    let (v_plane, a_plane) = rest.split_at_mut(chroma_len);

    for (index, [r, g, b, alpha]) in rgba.as_chunks::<4>().0.iter().enumerate() {
        a_plane[index] = *alpha;
        y_plane[index] = if *alpha == 0 {
            black_luma
        } else {
            let [r, g, b] = [*r, *g, *b].map(i64::from);
            let [yr, yg, yb] = coefficients.y;
            clamp_byte((yr * r + yg * g + yb * b + coefficients.y_offset + 32768) >> 16)
        };
    }

    for chroma_row in 0..height.div_ceil(2) {
        let rows = [2 * chroma_row, (2 * chroma_row + 1).min(height - 1)];
        for chroma_column in 0..chroma_width {
            let columns = [2 * chroma_column, (2 * chroma_column + 1).min(width - 1)];
            let (mut sum_alpha, mut sum_r, mut sum_g, mut sum_b) = (0i64, 0i64, 0i64, 0i64);
            for row in rows
                .iter()
                .copied()
                .take(if rows[0] == rows[1] { 1 } else { 2 })
            {
                for column in
                    columns
                        .iter()
                        .copied()
                        .take(if columns[0] == columns[1] { 1 } else { 2 })
                {
                    let offset = (row * width + column) * 4;
                    let alpha = i64::from(rgba[offset + 3]);
                    sum_alpha += alpha;
                    sum_r += alpha * i64::from(rgba[offset]);
                    sum_g += alpha * i64::from(rgba[offset + 1]);
                    sum_b += alpha * i64::from(rgba[offset + 2]);
                }
            }
            let index = chroma_row * chroma_width + chroma_column;
            if sum_alpha == 0 {
                u_plane[index] = 128;
                v_plane[index] = 128;
                continue;
            }
            let chroma = |[kr, kg, kb]: [i64; 3]| {
                let numerator = kr * sum_r + kg * sum_g + kb * sum_b;
                let denominator = sum_alpha << 16;
                // Rounded division of a signed numerator.
                let rounded = if numerator >= 0 {
                    (numerator + denominator / 2) / denominator
                } else {
                    -((-numerator + denominator / 2) / denominator)
                };
                clamp_byte(128 + rounded)
            };
            u_plane[index] = chroma(coefficients.cb);
            v_plane[index] = chroma(coefficients.cr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BT601_FULL: YuvColor = YuvColor {
        space: ColorSpaceTag::Unspecified,
        range: ColorRangeTag::Full,
    };
    const BT709_LIMITED: YuvColor = YuvColor {
        space: ColorSpaceTag::Bt709,
        range: ColorRangeTag::Limited,
    };

    fn convert(rgba: &[u8], width: u32, height: u32, color: YuvColor) -> Vec<u8> {
        let size = FrameSize { width, height };
        let mut out = vec![0; OverlayPixelFormat::Yuva420p(color).frame_len(size).unwrap()];
        convert_rgba_to_yuva420p(rgba, size, color, &mut out);
        out
    }

    #[test]
    fn opaque_colours_match_ffmpeg_for_the_source_colour_space() {
        // Reference values from FFmpeg 8 overlaying RGBA onto sources tagged this way.
        let red = [255, 0, 0, 255].repeat(4);
        assert_eq!(
            convert(&red, 2, 2, BT601_FULL),
            [76, 76, 76, 76, 85, 255, 255, 255, 255, 255]
        );
        assert_eq!(
            convert(&red, 2, 2, BT709_LIMITED),
            [63, 63, 63, 63, 102, 240, 255, 255, 255, 255]
        );
        let white = [255, 255, 255, 255].repeat(4);
        assert_eq!(
            convert(&white, 2, 2, BT709_LIMITED)[..6],
            [235, 235, 235, 235, 128, 128]
        );
        // An untagged 4K GoPro clip (full range) once its main video is tagged bt709.
        let bt709_full = YuvColor {
            space: ColorSpaceTag::Bt709,
            range: ColorRangeTag::Full,
        };
        assert_eq!(
            convert(&red, 2, 2, bt709_full)[..6],
            [54, 54, 54, 54, 99, 255]
        );
    }

    #[test]
    fn chroma_ignores_transparent_pixels_of_its_block() {
        let mut rgba = [0, 0, 0, 0].repeat(4);
        rgba[..4].copy_from_slice(&[255, 0, 0, 255]);
        let out = convert(&rgba, 2, 2, BT601_FULL);
        assert_eq!(out[..4], [76, 0, 0, 0]);
        assert_eq!(out[4..6], [85, 255]);
        assert_eq!(out[6..], [255, 0, 0, 0]);

        let transparent = convert(&[0, 0, 0, 0].repeat(4), 2, 2, BT709_LIMITED);
        assert_eq!(transparent, [16, 16, 16, 16, 128, 128, 0, 0, 0, 0]);
    }

    #[test]
    fn odd_dimensions_round_chroma_planes_up() {
        let size = FrameSize {
            width: 3,
            height: 3,
        };
        let format = OverlayPixelFormat::Yuva420p(BT601_FULL);
        assert_eq!(format.frame_len(size).unwrap(), 9 * 2 + 4 * 2);
        let out = convert(&[255, 0, 0, 255].repeat(9), 3, 3, BT601_FULL);
        assert_eq!(out[9..13], [85, 85, 85, 85]);
    }

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

    #[test]
    fn overlay_input_args_copy_the_source_tags() {
        let color = YuvColor::from_probe(Some("unknown"), Some("pc")).unwrap();
        assert_eq!(
            OverlayPixelFormat::Yuva420p(color).ffmpeg_input_args(),
            ["-pix_fmt", "yuva420p", "-color_range", "pc"]
        );
        let color = YuvColor::from_probe(Some("bt709"), Some("tv")).unwrap();
        assert_eq!(
            OverlayPixelFormat::Yuva420p(color).ffmpeg_input_args(),
            [
                "-pix_fmt",
                "yuva420p",
                "-color_range",
                "tv",
                "-colorspace",
                "bt709"
            ]
        );
        assert_eq!(
            OverlayPixelFormat::Rgba.ffmpeg_input_args(),
            ["-pix_fmt", "rgba"]
        );
    }
}
