//! Straight-alpha preview transport without Skia's floating-point conversion path.
use crate::error::{WidgetError, WidgetResult};
use skia_safe::{AlphaType, ColorType, ImageInfo, Surface};

// Precompute the existing integer rounding for every channel/alpha byte pair.
// Alpha zero is skipped by the caller. This 64 KiB table needs no runtime setup.
static UNPREMULTIPLY: [[u8; 256]; 256] = {
    let mut table = [[0; 256]; 256];
    let mut alpha = 1;
    while alpha < 256 {
        let mut channel = 0;
        while channel < 256 {
            table[alpha][channel] = ((channel * 255 + alpha / 2) / alpha) as u8;
            channel += 1;
        }
        alpha += 1;
    }
    table
};

/// Reads browser RGBA with integer rounding; RGB may differ by one from Skia's float conversion.
pub fn read_surface_rgba(surface: &mut Surface, pixels: &mut [u8]) -> WidgetResult<()> {
    let info = ImageInfo::new(
        (surface.width(), surface.height()),
        ColorType::RGBA8888,
        AlphaType::Premul,
        None,
    );
    if !surface.read_pixels(&info, pixels, surface.width() as usize * 4, (0, 0)) {
        return Err(WidgetError::Render(
            "unable to read widget RGBA pixels".into(),
        ));
    }
    // ImageData needs straight alpha; transparent and opaque pixels already match.
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = usize::from(pixel[3]);
        if alpha == 0 || alpha == 255 {
            continue;
        }
        let channels = &UNPREMULTIPLY[alpha];
        for channel in &mut pixel[..3] {
            *channel = channels[usize::from(*channel)];
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{read_surface_rgba, UNPREMULTIPLY};
    use skia_safe::{surfaces, AlphaType, Color, ColorType, ImageInfo};

    #[test]
    fn lookup_matches_integer_conversion_for_every_channel_and_nonzero_alpha() {
        for alpha in 1..=255u32 {
            for channel in 0..=255u32 {
                let expected = ((channel * 255 + alpha / 2) / alpha) as u8;
                assert_eq!(
                    UNPREMULTIPLY[alpha as usize][channel as usize], expected,
                    "channel {channel}, alpha {alpha}",
                );
            }
        }
    }

    #[test]
    fn readback_preserves_alpha_and_matches_skia_within_one_rgb_rounding_step() {
        let info = ImageInfo::new((256, 256), ColorType::RGBA8888, AlphaType::Premul, None);
        let original: Vec<u8> = (0..=255u8)
            .flat_map(|alpha| {
                (0..=255u8).flat_map(move |channel| {
                    let red = channel.min(alpha);
                    [red, alpha - red, alpha, alpha]
                })
            })
            .collect();
        let mut source = original.clone();
        let mut surface = surfaces::wrap_pixels(&info, &mut source, 256 * 4, None).unwrap();
        let mut expected = vec![0; original.len()];
        assert!(surface.read_pixels(
            &info.with_alpha_type(AlphaType::Unpremul),
            &mut expected,
            256 * 4,
            (0, 0),
        ));
        let mut actual = vec![0; original.len()];
        read_surface_rgba(&mut surface, &mut actual).unwrap();
        for (index, (actual, expected)) in actual
            .chunks_exact(4)
            .zip(expected.chunks_exact(4))
            .enumerate()
        {
            assert_eq!(actual[3], expected[3], "alpha at pixel {index}");
            for channel in 0..3 {
                assert!(
                    actual[channel].abs_diff(expected[channel]) <= 1,
                    "RGB at pixel {index}, channel {channel}: {actual:?} vs {expected:?}",
                );
            }
            if expected[3] == 0 || expected[3] == 255 {
                assert_eq!(actual, expected, "transparent/opaque pixel {index}");
            }
        }
        let mut repeated = vec![0; original.len()];
        read_surface_rgba(&mut surface, &mut repeated).unwrap();
        assert_eq!(actual, repeated);
        drop(surface);
        assert_eq!(
            source, original,
            "readback must not unpremultiply the drawing surface",
        );
    }

    #[test]
    fn reused_output_replaces_previous_colors_and_alpha() {
        let info = ImageInfo::new((2, 1), ColorType::RGBA8888, AlphaType::Premul, None);
        let mut surface = surfaces::raster(&info, None, None).unwrap();
        let mut pixels = vec![0; 8];
        surface.canvas().clear(Color::from_argb(128, 255, 0, 0));
        read_surface_rgba(&mut surface, &mut pixels).unwrap();
        assert_eq!(pixels, [255, 0, 0, 128, 255, 0, 0, 128]);
        surface.canvas().clear(Color::TRANSPARENT);
        read_surface_rgba(&mut surface, &mut pixels).unwrap();
        assert_eq!(pixels, [0; 8]);
    }
}
