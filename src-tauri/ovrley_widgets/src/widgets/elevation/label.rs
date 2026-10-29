//! Bounded label shaping cache and playback-wide raster bounds.
use crate::{
    error::{WidgetError, WidgetResult},
    render::text::{layout_text, measure_text_with_font, ResolvedTextStyle, TextPaints},
};
use skia_safe::{Canvas, Font, Rect, TextBlob};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub(crate) struct LabelRun {
    value: Option<i64>,
    blob: TextBlob,
    baseline: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct ElevationLabel {
    pub offset: (f32, f32),
    unit: &'static str,
    conversion: f64,
    font: Font,
    style: ResolvedTextStyle,
    paints: TextPaints,
    cached: Arc<Mutex<Option<Arc<LabelRun>>>>,
}

impl ElevationLabel {
    pub fn new(
        unit: &'static str,
        conversion: f64,
        offset: (f32, f32),
        font: Font,
        style: ResolvedTextStyle,
    ) -> Self {
        Self {
            unit,
            conversion,
            offset,
            font,
            paints: TextPaints::new(&style),
            style,
            cached: Arc::new(Mutex::new(None)),
        }
    }

    fn value(&self, elevation: f64, correction: f64) -> i64 {
        ((elevation + correction) * self.conversion).round() as i64
    }

    pub fn prepare(&self, elevation: Option<f64>, correction: f64) -> WidgetResult<Arc<LabelRun>> {
        let value = elevation.map(|elevation| self.value(elevation, correction));
        let mut cached = self.cached.lock().expect("elevation label cache lock");
        if let Some(run) = cached.as_ref().filter(|run| run.value == value) {
            return Ok(run.clone());
        }
        let text = value.map_or_else(
            || format!("-- {}", self.unit),
            |value| format!("{value} {}", self.unit),
        );
        let run = layout_text(&text, &self.font, 0.0, self.style.numerals);
        let m = run.measurement;
        let baseline =
            (self.style.line_height - (m.bounds_bottom - m.bounds_top)) * 0.5 - m.bounds_top;
        let run = Arc::new(LabelRun {
            value,
            baseline,
            blob: run
                .blob
                .ok_or_else(|| WidgetError::Render("unable to lay out elevation label".into()))?,
        });
        *cached = Some(run.clone());
        Ok(run)
    }

    pub fn draw(&self, canvas: &Canvas, run: &LabelRun, marker: (f32, f32)) {
        canvas.save();
        canvas.translate((marker.0 + self.offset.0, marker.1 + self.offset.1));
        self.paints.draw(canvas, &run.blob, (0.0, run.baseline));
        canvas.restore();
    }

    /// Bound every rounded label once, including missing data and unequal digit advances.
    pub fn raster_bounds(&self, marker: Rect, range: Option<(f64, f64)>, correction: f64) -> Rect {
        let (min, max) = range.unwrap_or((0.0, 0.0));
        let digits = [min, max, 0.0]
            .map(|value| {
                self.value(value, correction)
                    .unsigned_abs()
                    .to_string()
                    .len()
            })
            .into_iter()
            .max()
            .expect("label range endpoints");
        let mut advance = 0.0_f32;
        let mut left = 0.0_f32;
        let mut overhang = 0.0_f32;
        let mut top = 0.0_f32;
        let mut bottom = 0.0_f32;
        for c in "0123456789- MFT".chars() {
            let m = measure_text_with_font(&c.to_string(), &self.font, self.style.numerals);
            if c.is_ascii_digit() {
                advance = advance.max(m.width);
            }
            left = left.min(m.bounds_left);
            overhang = overhang.max(m.bounds_right - m.width);
            top = top.min(m.bounds_top);
            bottom = bottom.max(m.bounds_bottom);
        }
        let measure = |text: String| measure_text_with_font(&text, &self.font, self.style.numerals);
        let suffix = measure(format!("- {}", self.unit)).width;
        let placeholder = measure(format!("-- {}", self.unit)).width;
        let width = (digits as f32 * advance + suffix).max(placeholder) + overhang;
        let half_border = self.style.border_thickness * 0.5;
        let height = bottom - top;
        let mut bounds = Rect::new(
            marker.left + self.offset.0 + left,
            marker.top + self.offset.1 + (self.style.line_height - height) * 0.5,
            marker.right + self.offset.0 + width,
            marker.bottom + self.offset.1 + (self.style.line_height + height) * 0.5,
        )
        .with_outset((half_border, half_border));
        if self.style.shadow_strength > 0.0 {
            bounds.join(
                bounds
                    .with_outset((
                        self.style.shadow_strength * 3.0,
                        self.style.shadow_strength * 3.0,
                    ))
                    .with_offset((self.style.shadow_distance, self.style.shadow_distance)),
            );
        }
        bounds
    }
}
