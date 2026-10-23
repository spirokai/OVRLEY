//! One frame selects the cached active-zone state and places the indicator.
use super::{layout::HeartZonesCache, zones::zone_position};
use skia_safe::{Canvas, FilterMode, Paint, SamplingOptions};

pub struct HeartZonesFrame {
    /// Index into the cached states; 0 when the heart rate is missing.
    state: usize,
    indicator_x: Option<f32>,
}

impl HeartZonesCache {
    /// Missing external heart rate shows the resting track without an indicator.
    pub fn prepare_frame(&self, raw: Option<f64>) -> HeartZonesFrame {
        match raw {
            Some(heartrate) => {
                let (index, progress) = zone_position(&self.zones, heartrate as f32);
                HeartZonesFrame {
                    state: index + 1,
                    indicator_x: Some(self.layout.indicator_x(index, progress)),
                }
            }
            None => HeartZonesFrame { state: 0, indicator_x: None },
        }
    }
}

impl HeartZonesFrame {
    pub fn draw(&self, canvas: &Canvas, cache: &HeartZonesCache) {
        let mut paint = Paint::default();
        paint.set_alpha_f(cache.opacity);
        cache.states[self.state].draw(canvas, Some(&paint));
        let Some(x) = self.indicator_x else {
            return;
        };
        if let Some((shadow, origin)) = &cache.indicator_shadow {
            canvas.draw_image_with_sampling_options(
                shadow,
                (x + origin.x, origin.y),
                SamplingOptions::from(FilterMode::Linear),
                Some(&paint),
            );
        }
        let mut indicator = cache.indicator_paint.clone();
        indicator.set_alpha_f(indicator.alpha_f() * cache.opacity);
        canvas.save();
        canvas.translate((x, 0.0));
        canvas.draw_path(&cache.indicator.path(), &indicator);
        canvas.restore();
    }
}
