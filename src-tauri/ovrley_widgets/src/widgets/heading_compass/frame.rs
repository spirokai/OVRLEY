//! One frame rotates the moving dial part and lays out the heading readout.
use super::{
    layout::{readout_anchor, HeadingCompassCache},
    validation::CompassRotationMode,
};
use crate::{
    error::{WidgetError, WidgetResult},
    model::{
        scene::Numerals, standard_metrics::standard_metric_unit_label,
        standard_widgets::widget_font_weight, types::MetricKind,
    },
    render::text::{
        draw_text_blob, layout_text, measure_text_with_font, parse_color, resolve_styled_font,
        text_selection_bounds, FontResolver, MeasuredText, ResolvedTextStyle,
    },
    widgets::value::layout::{
        metric_vertical_metrics_text, METRIC_WIDGET_LINE_HEIGHT, METRIC_WIDGET_UNITS_GAP_PX,
        METRIC_WIDGET_UNIT_RATIO, MIN_UNITS_FONT_SIZE,
    },
};
use skia_safe::{Canvas, Point, Rect, TextBlob};

/// Whole degrees in 0..360; a missing heading shows a placeholder.
pub fn format_heading_compass_value(raw: Option<f64>) -> String {
    raw.map_or_else(
        || "--".into(),
        |heading| (heading.round() as i64).rem_euclid(360).to_string(),
    )
}

struct TextRun {
    blob: TextBlob,
    baseline: f32,
    style: ResolvedTextStyle,
}

pub struct HeadingCompassFrame {
    heading: f32,
    value: Option<TextRun>,
    unit: Option<TextRun>,
    pub bounds: Rect,
    pub content_bounds: Rect,
    pub content_width: f32,
    pub marker: Point,
}

impl HeadingCompassFrame {
    pub fn draw(&self, canvas: &Canvas, cache: &HeadingCompassCache) {
        cache.draw_static(canvas);
        match cache.rotation_mode {
            CompassRotationMode::Indicator => cache.draw_indicator(canvas, self.heading),
            CompassRotationMode::Compass => cache.draw_card(canvas, -self.heading),
        }
        for run in [&self.value, &self.unit].into_iter().flatten() {
            draw_text_blob(canvas, &run.blob, run.baseline, &run.style);
        }
    }
}

impl HeadingCompassCache {
    pub fn prepare_frame(
        &self,
        raw: Option<f64>,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<HeadingCompassFrame> {
        let widget = &self.config;
        let scene = &self.scene;
        let scale = scene.scale;
        // Missing external heading deliberately displays the north-up dial.
        let heading = raw.unwrap_or(0.0) as f32;
        let needle_angle = match self.rotation_mode {
            CompassRotationMode::Indicator => heading,
            CompassRotationMode::Compass => 0.0,
        };
        let marker_angle = (needle_angle - 90.0).to_radians();
        let marker = Point::new(
            self.center.x + self.indicator_size * 0.5 * marker_angle.cos(),
            self.center.y + self.indicator_size * 0.5 * marker_angle.sin(),
        );
        let size = self.layout.size();
        let mut bounds = self.dial_bounds;
        let mut content_bounds = Rect::from_xywh(self.x, self.y, size, size);
        let (value, unit) = if widget.show_value {
            let opacity = self.opacity;
            let font_size = widget.font_size * scale;
            let mut value_style = ResolvedTextStyle {
                x: 0.0,
                y: 0.0,
                font_name: Some(widget.font.clone()),
                font_size,
                font_weight: widget_font_weight(),
                italic: false,
                letter_spacing: 0.0,
                line_height: font_size * METRIC_WIDGET_LINE_HEIGHT,
                color: parse_color(&widget.color, opacity),
                opacity,
                shadow_color: (scene.shadow_strength > 0.0)
                    .then(|| parse_color(&scene.shadow_color, opacity)),
                shadow_strength: scene.shadow_strength * scale,
                shadow_distance: scene.shadow_distance * scale,
                border_color: Some(parse_color(&scene.border_color, opacity)),
                border_thickness: scene.border_thickness * scale,
                numerals: scene.numerals,
            };
            let value_text = format_heading_compass_value(raw);
            let value_font = resolve_styled_font(
                fonts,
                Some(&widget.font),
                font_size,
                widget_font_weight(),
                false,
            )?;
            let value = layout_text(&value_text, &value_font, 0.0, value_style.numerals);
            let value_vertical = measure_text_with_font(
                metric_vertical_metrics_text(&value_text),
                &value_font,
                Numerals::Default,
            );
            let unit = if raw.is_some() && widget.show_units {
                let unit_size =
                    (font_size * METRIC_WIDGET_UNIT_RATIO).max(MIN_UNITS_FONT_SIZE * scale);
                let font = resolve_styled_font(
                    fonts,
                    Some(&widget.font),
                    unit_size,
                    widget_font_weight(),
                    false,
                )?;
                let text = layout_text(
                    standard_metric_unit_label(MetricKind::Heading, None),
                    &font,
                    0.0,
                    Numerals::Default,
                );
                let style = ResolvedTextStyle {
                    font_size: unit_size,
                    line_height: unit_size * METRIC_WIDGET_LINE_HEIGHT,
                    color: parse_color(&widget.unit_color, opacity),
                    ..value_style.clone()
                };
                Some((text, style))
            } else {
                None
            };
            // The superscript degree sits half the usual unit gap after the digits.
            let unit_x = value.measurement.width + METRIC_WIDGET_UNITS_GAP_PX * scale * 0.5;
            // The value's leading ink edge starts at the gap anchor; digits set the vertical centre.
            let anchor = self.center
                + readout_anchor(&self.layout)
                + Point::new(widget.value_offset_x * scale, widget.value_offset_y * scale);
            value_style.x = anchor.x - value.measurement.bounds_left;
            let value_baseline =
                anchor.y - (value_vertical.bounds_top + value_vertical.bounds_bottom) * 0.5;
            let digit_top = value_baseline + value_vertical.bounds_top;
            let shadow_padding = if scene.shadow_strength > 0.0 {
                (scene.shadow_strength * 3.0 + scene.shadow_distance) * scale
            } else {
                0.0
            };
            let border = value_style.border_thickness * 0.5;
            let mut text_bounds = |ink: &MeasuredText, x: f32, baseline: f32| {
                let mut painted = Rect::new(
                    x + ink.bounds_left,
                    baseline + ink.bounds_top,
                    x + ink.bounds_right,
                    baseline + ink.bounds_bottom,
                );
                painted.outset((border + shadow_padding + 2.0, border + shadow_padding + 2.0));
                bounds.join(painted);
                let mut selection = text_selection_bounds(ink, x, baseline);
                selection.outset((border, border));
                content_bounds.join(selection);
            };
            text_bounds(&value.measurement, value_style.x, value_baseline);
            let unit = unit
                .map(|(text, mut style)| -> WidgetResult<TextRun> {
                    style.x = value_style.x + unit_x;
                    // Superscript: the degree's ink top meets the digits' ink top.
                    let baseline = digit_top - text.measurement.bounds_top;
                    text_bounds(&text.measurement, style.x, baseline);
                    Ok(TextRun {
                        blob: text.blob.ok_or_else(|| {
                            WidgetError::Render("unable to lay out heading compass unit".into())
                        })?,
                        baseline,
                        style,
                    })
                })
                .transpose()?;
            let value = TextRun {
                blob: value.blob.ok_or_else(|| {
                    WidgetError::Render("unable to lay out heading compass value".into())
                })?,
                baseline: value_baseline,
                style: value_style,
            };
            (Some(value), unit)
        } else {
            (None, None)
        };
        Ok(HeadingCompassFrame {
            heading,
            value,
            unit,
            bounds: Rect::new(
                bounds.left.floor(),
                bounds.top.floor(),
                bounds.right.ceil(),
                bounds.bottom.ceil(),
            ),
            content_bounds,
            content_width: size,
            marker,
        })
    }
}
