//! Coverage-mask shadows shared by text, icons and widget shapes.
use skia_safe::{BlurStyle, Color, MaskFilter, Paint};

/// Retains the source geometry/style and its alpha, replacing its color with the shadow color.
/// Callers draw at the shadow offset before drawing the original foreground.
pub fn shadow_paint(source: &Paint, color: Color, sigma: f32) -> Option<Paint> {
    if sigma <= 0.0 {
        return None;
    }
    let mut paint = source.clone();
    paint.set_color(color);
    // The previous image filter colored the source's alpha, including its paint opacity.
    paint.set_alpha_f(source.alpha_f() * f32::from(color.a()) / 255.0);
    paint.set_mask_filter(
        MaskFilter::blur(BlurStyle::Normal, sigma, true).expect("positive shadow sigma"),
    );
    Some(paint)
}
