//! One frame scrolls the cached tick period, lays out the visible labels and the two readouts.
use super::{
    layout::{tape_y, trend_length, visible_major_values, AltitudeTapeCache, TextPaint},
    validation::TapeOrientation,
};
use crate::{
    model::types::MetricKind,
    render::text::layout_text,
    widgets::value::format::{convert_standard_metric_value, format_number},
};
use skia_safe::{
    canvas::SaveLayerRec, Canvas, ClipOp, FilterMode, Paint, Point, Rect, TextBlob,
};

/// Missing external data shows this placeholder instead of a number.
pub const MISSING_TEXT: &str = "--";

pub struct AltitudeTapeFrame {
    /// Tape position in display units; a missing sample rests the tape on zero.
    altitude: f64,
    value: Option<(TextBlob, Point)>,
    vertical_speed: Option<(TextBlob, Point, Option<bool>)>,
    trend: Option<(Rect, bool)>,
}

/// Altitude or depth readout in the tape's unit.
pub fn format_tape_value(metric: MetricKind, value_m: Option<f64>, unit: &str, decimal: bool) -> String {
    value_m.map_or_else(
        || MISSING_TEXT.into(),
        |value| {
            let value = convert_standard_metric_value(metric, Some(unit), value);
            format_number(value, usize::from(decimal))
        },
    )
}

/// Vertical-speed number in its unit, and whether it climbs (`Some(true)`) or descends. A value
/// that reads as zero or a missing sample has no direction.
pub fn format_vertical_speed_value(
    speed_mps: Option<f64>,
    unit: &str,
    decimal: bool,
) -> (String, Option<bool>) {
    let Some(speed) = speed_mps else {
        return (MISSING_TEXT.into(), None);
    };
    let value = convert_standard_metric_value(MetricKind::VerticalSpeed, Some(unit), speed);
    let text = format_number(value, usize::from(decimal));
    // The sign follows the rounded text, so "0.0" is never coloured as a climb or descent.
    let climbing = match text.parse::<f64>().expect("formatted readout is numeric") {
        shown if shown > 0.0 => Some(true),
        shown if shown < 0.0 => Some(false),
        _ => None,
    };
    (text, climbing)
}

impl AltitudeTapeCache {
    /// `value_m` is the source's altitude (with its starting offset) or depth in metres;
    /// `vertical_speed_mps` is positive while ascending for either source.
    pub fn prepare_frame(
        &self,
        value_m: Option<f64>,
        vertical_speed_mps: Option<f64>,
    ) -> AltitudeTapeFrame {
        let layout = &self.layout;
        let metric = self.source.metric();
        let value_text =
            format_tape_value(metric, value_m, &self.display_unit, self.show_value_decimal);
        let value = layout_text(&value_text, &self.value_font, 0.0, self.numerals);
        let value = value.blob.map(|blob| {
            (
                blob,
                Point::new(
                    layout.text_x(self.value_anchor, value.measurement.width),
                    layout.center_y() + self.value_baseline_offset,
                ),
            )
        });
        let vertical_speed = self.show_vertical_speed.then(|| {
            let (text, climbing) = format_vertical_speed_value(
                vertical_speed_mps,
                &self.vertical_speed_unit,
                self.show_vertical_speed_decimal,
            );
            let speed = layout_text(&text, &self.vertical_speed_font, 0.0, self.numerals);
            // Numbers right-align in the readout's fixed box, so the unit never moves.
            speed.blob.map(|blob| {
                (
                    blob,
                    Point::new(
                        self.vertical_speed_value_right - speed.measurement.width,
                        self.vertical_speed_baseline,
                    ),
                    climbing,
                )
            })
        });
        let trend = vertical_speed_mps
            .filter(|_| self.show_vertical_speed)
            .map(|speed| {
                trend_length(
                    speed,
                    self.trend_seconds,
                    self.pixels_per_unit * self.units_per_meter,
                    layout.body.height(),
                )
            })
            .filter(|length| *length != 0.0)
            .map(|length| {
                let center = layout.center_y();
                // The bar starts at the pointer tip and stays inside the frame.
                let tip = layout.tip_x();
                let (left, right) = match layout.orientation {
                    TapeOrientation::Left => (tip, tip + self.bar_thickness),
                    TapeOrientation::Right => (tip - self.bar_thickness, tip),
                };
                (
                    Rect::new(
                        left,
                        center.min(center - length),
                        right,
                        center.max(center - length),
                    ),
                    length > 0.0,
                )
            });
        AltitudeTapeFrame {
            altitude: value_m.map_or(0.0, |value| {
                convert_standard_metric_value(metric, Some(&self.display_unit), value)
            }),
            value,
            vertical_speed: vertical_speed.flatten(),
            trend,
        }
    }

    fn draw_ticks(&self, canvas: &Canvas, altitude: f64) {
        let layout = &self.layout;
        let interval = f64::from(self.major_tick_interval);
        let major = (altitude / interval).floor() * interval;
        let major_y = tape_y(
            layout.center_y(),
            major,
            altitude,
            self.pixels_per_unit,
            self.source.direction(),
        );
        // The strip starts on the last major at or above the body; it then covers the body.
        let top = major_y - ((major_y - layout.body.top) / self.period).ceil() * self.period;
        // Whole host pixels let the strip copy without resampling and keep thin ticks crisp.
        let snap = |value: f32| (value * self.raster_scale).round() / self.raster_scale;
        let (left, top) = (snap(layout.frame.left), snap(top));
        let scale = self.raster_scale.recip();
        canvas.draw_image_rect_with_sampling_options(
            &self.tick_strip,
            None,
            Rect::from_xywh(
                left,
                top,
                self.tick_strip.width() as f32 * scale,
                self.tick_strip.height() as f32 * scale,
            ),
            FilterMode::Nearest,
            &Paint::default(),
        );
    }

    fn draw_labels(&self, canvas: &Canvas, altitude: f64) {
        let layout = &self.layout;
        let half_extent = layout.body.height() * 0.5 + self.label_font.size();
        for value in visible_major_values(
            altitude,
            self.major_tick_interval,
            self.pixels_per_unit,
            half_extent,
        ) {
            let text = layout_text(&format_number(value, 0), &self.label_font, 0.0, self.numerals);
            let Some(blob) = text.blob else {
                continue;
            };
            let origin = Point::new(
                layout.text_x(self.label_anchor, text.measurement.width),
                tape_y(
                    layout.center_y(),
                    value,
                    altitude,
                    self.pixels_per_unit,
                    self.source.direction(),
                )
                    + self.label_baseline_offset,
            );
            draw_text(canvas, &blob, origin, &self.label_paint);
        }
    }
}

fn draw_text(canvas: &Canvas, blob: &TextBlob, origin: Point, paint: &TextPaint) {
    if let Some(shadow) = &paint.shadow {
        canvas.draw_text_blob(
            blob,
            origin + Point::new(paint.shadow_distance, paint.shadow_distance),
            shadow,
        );
    }
    canvas.draw_text_blob(blob, origin, &paint.fill);
}

impl AltitudeTapeFrame {
    pub fn draw(&self, canvas: &Canvas, cache: &AltitudeTapeCache) {
        let layout = &cache.layout;
        // Translucent widgets composite as one layer so the trend never shows through the box.
        let layer = cache.opacity < 1.0;
        if layer {
            let mut opacity = Paint::default();
            opacity.set_alpha_f(cache.opacity);
            canvas.save_layer(&SaveLayerRec::default().bounds(&cache.bounds).paint(&opacity));
        }
        canvas.save();
        let mut body = layout.body;
        body.right += cache.tick_overhang;
        // A hard clip rounds to whole pixels; an anti-aliased one at a fractional edge would
        // force every tick and label draw inside it through a coverage mask.
        canvas.clip_rect(body, ClipOp::Intersect, false);
        cache.draw_ticks(canvas, self.altitude);
        if cache.show_tick_labels {
            cache.draw_labels(canvas, self.altitude);
        }
        canvas.restore();
        if let Some((rect, climbing)) = &self.trend {
            let paint = if *climbing {
                &cache.bar_positive
            } else {
                &cache.bar_negative
            };
            canvas.draw_rect(rect, paint);
        }
        cache.value_box.draw(canvas, None);
        if let Some((blob, origin)) = &self.value {
            canvas.draw_text_blob(blob, *origin, &cache.value_paint);
        }
        if let Some((blob, origin, climbing)) = &self.vertical_speed {
            // The readout shares the trend's colours; a level reading keeps the label colour.
            let paint = match climbing {
                Some(true) => &cache.climb_text_paint,
                Some(false) => &cache.descent_text_paint,
                None => &cache.label_paint,
            };
            draw_text(canvas, blob, *origin, paint);
            if let Some((unit, unit_origin)) = &cache.vertical_speed_unit_text {
                draw_text(canvas, unit, *unit_origin, paint);
            }
        }
        if layer {
            canvas.restore();
        }
    }
}
