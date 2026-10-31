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
#[derive(Clone, Copy)]
struct Coefficients {
    y: [i32; 3],
    cb: [i32; 3],
    cr: [i32; 3],
    y_offset: i32,
}

const FIXED_ONE: f64 = 65536.0;
/// Alpha of a 2×2 block of opaque pixels.
const OPAQUE_BLOCK: i32 = 4 * 255;
/// Blocks converted before their alphas are checked.
const BLOCK_RUN: usize = 64;
/// Every other byte of a pair of pixels, so summing up to four pairs cannot carry between them.
const EVEN_BYTES: u64 = 0x00FF_00FF_00FF_00FF;

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
        let fixed = |value: f64| (value * FIXED_ONE).round() as i32;
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

/// Chroma of a block of opaque pixels from its colour sums. Weighting by alpha multiplies numerator
/// and denominator alike by 255, so this rounds exactly as [`weighted_chroma`] does, without dividing.
#[inline(always)]
fn opaque_chroma([kr, kg, kb]: [i32; 3], [r, g, b]: [i32; 3]) -> u8 {
    let numerator = kr * r + kg * g + kb * b;
    let sign = numerator >> 31;
    let magnitude = (((numerator ^ sign) - sign) + (1 << 17)) >> 18;
    (128 + ((magnitude ^ sign) - sign)).clamp(0, 255) as u8
}

/// Chroma of any block: averages its pixels weighted by alpha. On the edges of an odd frame a block
/// covers fewer pixels.
fn weighted_chroma(
    rgba: &[u8],
    (width, height): (usize, usize),
    (chroma_column, chroma_row): (usize, usize),
    coefficients: &Coefficients,
) -> (u8, u8) {
    let (mut sum_alpha, mut sum_r, mut sum_g, mut sum_b) = (0i64, 0i64, 0i64, 0i64);
    for row in (2 * chroma_row)..(2 * chroma_row + 2).min(height) {
        for column in (2 * chroma_column)..(2 * chroma_column + 2).min(width) {
            let offset = (row * width + column) * 4;
            let alpha = i64::from(rgba[offset + 3]);
            sum_alpha += alpha;
            sum_r += alpha * i64::from(rgba[offset]);
            sum_g += alpha * i64::from(rgba[offset + 1]);
            sum_b += alpha * i64::from(rgba[offset + 2]);
        }
    }
    if sum_alpha == 0 {
        return (128, 128);
    }
    let chroma = |[kr, kg, kb]: [i32; 3]| {
        let numerator = i64::from(kr) * sum_r + i64::from(kg) * sum_g + i64::from(kb) * sum_b;
        let denominator = sum_alpha << 16;
        // Rounded division of a signed numerator.
        let rounded = if numerator >= 0 {
            (numerator + denominator / 2) / denominator
        } else {
            -((-numerator + denominator / 2) / denominator)
        };
        (128 + rounded).clamp(0, 255) as u8
    };
    (chroma(coefficients.cb), chroma(coefficients.cr))
}

/// Luma and alpha of a run of whole rows. A fully transparent stretch needs neither colour nor luma.
#[inline(always)]
fn convert_luma_alpha(
    rgba: &[u8],
    coefficients: &Coefficients,
    black_luma: u8,
    y_rows: &mut [u8],
    a_rows: &mut [u8],
) {
    let [yr, yg, yb] = coefficients.y;
    for ((pixels, y_run), a_run) in rgba
        .as_chunks::<4>()
        .0
        .chunks(BLOCK_RUN)
        .zip(y_rows.chunks_mut(BLOCK_RUN))
        .zip(a_rows.chunks_mut(BLOCK_RUN))
    {
        for (a, pixel) in a_run.iter_mut().zip(pixels) {
            *a = pixel[3];
        }
        if a_run.iter().fold(0, |any, a| any | *a) == 0 {
            y_run.fill(black_luma);
            continue;
        }
        let mut luma = [0i32; BLOCK_RUN];
        for (luma, pixel) in luma.iter_mut().zip(pixels) {
            let word = u32::from_le_bytes(*pixel);
            let (r, g, b) = (word & 255, (word >> 8) & 255, (word >> 16) & 255);
            *luma = (yr * r as i32
                + yg * g as i32
                + yb * b as i32
                + coefficients.y_offset
                + 32768)
                >> 16;
        }
        for ((y, luma), a) in y_run.iter_mut().zip(&luma).zip(a_run.iter()) {
            *y = if *a == 0 {
                black_luma
            } else {
                (*luma).clamp(0, 255) as u8
            };
        }
    }
}

/// One frame's planes, a pair of rows at a time so the frame is read from memory once. Inlined into
/// each instruction set it is compiled for.
///
/// Performance: do not split this into separate luma and chroma passes over the frame, which reads
/// the whole atlas from memory twice. Output must stay byte-identical: see the division-based test.
#[inline(always)]
fn convert_planes(
    rgba: &[u8],
    (width, height): (usize, usize),
    coefficients: &Coefficients,
    [y_plane, u_plane, v_plane, a_plane]: [&mut [u8]; 4],
) {
    let black_luma = ((coefficients.y_offset + 32768) >> 16).clamp(0, 255) as u8;
    let size = (width, height);
    let stride = width * 4;
    let chroma_width = width.div_ceil(2);
    let whole_blocks = width / 2;
    for chroma_row in 0..height / 2 {
        convert_luma_alpha(
            &rgba[2 * chroma_row * stride..][..2 * stride],
            coefficients,
            black_luma,
            &mut y_plane[2 * chroma_row * width..][..2 * width],
            &mut a_plane[2 * chroma_row * width..][..2 * width],
        );
        let top = &rgba[2 * chroma_row * stride..][..stride];
        let bottom = &rgba[(2 * chroma_row + 1) * stride..][..stride];
        let u_row = &mut u_plane[chroma_row * chroma_width..][..chroma_width];
        let v_row = &mut v_plane[chroma_row * chroma_width..][..chroma_width];
        let mut column = 0;
        for ((top, bottom), (u_run, v_run)) in top
            .as_chunks::<8>()
            .0
            .chunks(BLOCK_RUN)
            .zip(bottom.as_chunks::<8>().0.chunks(BLOCK_RUN))
            .zip(
                u_row[..whole_blocks]
                    .chunks_mut(BLOCK_RUN)
                    .zip(v_row[..whole_blocks].chunks_mut(BLOCK_RUN)),
            )
        {
            // Each block's channel sums, red and green in the low half of a word, blue and alpha in the high.
            // Performance: one load per block, written to vectorize. Reading per channel was slower.
            let (mut red_blue, mut green_alpha) = ([0u32; BLOCK_RUN], [0u32; BLOCK_RUN]);
            for (((red_blue, green_alpha), top), bottom) in
                red_blue.iter_mut().zip(&mut green_alpha).zip(top).zip(bottom)
            {
                let (top, bottom) = (u64::from_le_bytes(*top), u64::from_le_bytes(*bottom));
                let even = (top & EVEN_BYTES) + (bottom & EVEN_BYTES);
                let odd = ((top >> 8) & EVEN_BYTES) + ((bottom >> 8) & EVEN_BYTES);
                *red_blue = (even + (even >> 32)) as u32;
                *green_alpha = (odd + (odd >> 32)) as u32;
            }
            let green_alpha = &green_alpha[..top.len()];
            let alpha = |word: &u32| (*word >> 16) as i32;
            if green_alpha.iter().fold(0, |any, word| any | alpha(word)) == 0 {
                u_run.fill(128);
                v_run.fill(128);
            } else if green_alpha.iter().fold(true, |opaque, word| opaque & (alpha(word) == OPAQUE_BLOCK)) {
                let colour = |index: usize| {
                    [
                        (red_blue[index] & 0xFFFF) as i32,
                        (green_alpha[index] & 0xFFFF) as i32,
                        (red_blue[index] >> 16) as i32,
                    ]
                };
                for (index, u) in u_run.iter_mut().enumerate() {
                    *u = opaque_chroma(coefficients.cb, colour(index));
                }
                for (index, v) in v_run.iter_mut().enumerate() {
                    *v = opaque_chroma(coefficients.cr, colour(index));
                }
            } else {
                for (index, word) in green_alpha.iter().enumerate() {
                    (u_run[index], v_run[index]) = if alpha(word) == 0 {
                        (128, 128)
                    } else {
                        weighted_chroma(rgba, size, (column + index, chroma_row), coefficients)
                    };
                }
            }
            column += top.len();
        }
        if whole_blocks != chroma_width {
            (u_row[whole_blocks], v_row[whole_blocks]) =
                weighted_chroma(rgba, size, (whole_blocks, chroma_row), coefficients);
        }
    }
    if height % 2 == 1 {
        let last = height - 1;
        convert_luma_alpha(
            &rgba[last * stride..],
            coefficients,
            black_luma,
            &mut y_plane[last * width..],
            &mut a_plane[last * width..],
        );
        let chroma_row = height / 2;
        for column in 0..chroma_width {
            let index = chroma_row * chroma_width + column;
            (u_plane[index], v_plane[index]) =
                weighted_chroma(rgba, size, (column, chroma_row), coefficients);
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
fn convert_planes_avx2(
    rgba: &[u8],
    size: (usize, usize),
    coefficients: &Coefficients,
    planes: [&mut [u8]; 4],
) {
    convert_planes(rgba, size, coefficients, planes);
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
    let (y_plane, rest) = out.split_at_mut(luma_len);
    let (u_plane, rest) = rest.split_at_mut(chroma_len);
    let (v_plane, a_plane) = rest.split_at_mut(chroma_len);
    let planes = [y_plane, u_plane, v_plane, a_plane];
    #[cfg(target_arch = "x86_64")]
    if std::arch::is_x86_feature_detected!("avx2") {
        // SAFETY: the feature this function is compiled for was just detected.
        unsafe { convert_planes_avx2(rgba, (width, height), &coefficients, planes) };
        return;
    }
    convert_planes(rgba, (width, height), &coefficients, planes);
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

    /// The division-based conversion the fast one must reproduce byte for byte.
    fn reference(rgba: &[u8], width: usize, height: usize, color: YuvColor) -> Vec<u8> {
        let c = Coefficients::new(color);
        let chroma_width = width.div_ceil(2);
        let black = ((c.y_offset + 32768) >> 16).clamp(0, 255) as u8;
        let mut y = Vec::new();
        let mut a = Vec::new();
        for pixel in rgba.chunks_exact(4) {
            a.push(pixel[3]);
            let [r, g, b] = [pixel[0], pixel[1], pixel[2]].map(i64::from);
            let [yr, yg, yb] = c.y.map(i64::from);
            let luma = (yr * r + yg * g + yb * b + i64::from(c.y_offset) + 32768) >> 16;
            y.push(if pixel[3] == 0 {
                black
            } else {
                luma.clamp(0, 255) as u8
            });
        }
        let (mut u, mut v) = (Vec::new(), Vec::new());
        for chroma_row in 0..height.div_ceil(2) {
            for chroma_column in 0..chroma_width {
                let (mut sa, mut sr, mut sg, mut sb) = (0i64, 0i64, 0i64, 0i64);
                for row in (2 * chroma_row)..(2 * chroma_row + 2).min(height) {
                    for column in (2 * chroma_column)..(2 * chroma_column + 2).min(width) {
                        let o = (row * width + column) * 4;
                        let alpha = i64::from(rgba[o + 3]);
                        sa += alpha;
                        sr += alpha * i64::from(rgba[o]);
                        sg += alpha * i64::from(rgba[o + 1]);
                        sb += alpha * i64::from(rgba[o + 2]);
                    }
                }
                if sa == 0 {
                    u.push(128);
                    v.push(128);
                    continue;
                }
                let chroma = |k: [i32; 3]| {
                    let [kr, kg, kb] = k.map(i64::from);
                    let n = kr * sr + kg * sg + kb * sb;
                    let d = sa << 16;
                    let rounded = if n >= 0 {
                        (n + d / 2) / d
                    } else {
                        -((-n + d / 2) / d)
                    };
                    (128 + rounded).clamp(0, 255) as u8
                };
                u.push(chroma(c.cb));
                v.push(chroma(c.cr));
            }
        }
        [y, u, v, a].concat()
    }

    #[test]
    fn matches_the_division_based_conversion_byte_for_byte() {
        let colors = [
            BT601_FULL,
            BT709_LIMITED,
            YuvColor::from_probe(Some("bt2020nc"), Some("tv")).unwrap(),
            YuvColor::from_probe(Some("bt709"), Some("pc")).unwrap(),
        ];
        let mut state = 0x2545_f491u32;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        // Opaque, mixed and partial pixels, and saturated colours that clamp.
        for (width, height) in [
            (1, 1),
            (2, 2),
            (3, 3),
            (2, 5),
            (130, 4),
            (131, 5),
            (257, 9),
            (64, 64),
        ] {
            for pattern in 0..5 {
                let mut rgba = Vec::new();
                for index in 0..width * height {
                    let random = next().to_le_bytes();
                    let alpha = match (pattern, random[3] % 4) {
                        (0, _) => 255,
                        // Stretches longer than a block run, opaque and transparent, with colour left in.
                        (4, _) if (index / 70) % 2 == 0 => 0,
                        (4, _) => 255,
                        (1, 0) => 0,
                        (1, _) => 255,
                        (2, 0) => 255,
                        (2, 1) => 0,
                        (3, _) => 255,
                        _ => random[3],
                    };
                    let tone = |byte: u8| match (pattern, byte < 128) {
                        (3, true) => 0,
                        (3, false) => 255,
                        _ => byte,
                    };
                    rgba.extend([tone(random[0]), tone(random[1]), tone(random[2]), alpha]);
                }
                for color in colors {
                    assert_eq!(
                        convert(&rgba, width as u32, height as u32, color),
                        reference(&rgba, width, height, color),
                        "{width}x{height} pattern {pattern}"
                    );
                }
            }
        }
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
