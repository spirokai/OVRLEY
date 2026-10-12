/// Metric widget layout: text positioning, icon placement, unit sizing, and
/// vertical-metrics helpers.
///
/// The metric row is manually laid out so icon, value, and units can each use
/// their own size while sharing one configurable point anchor.
use crate::error::WidgetResult;
use crate::model::types::DisplayType;
use crate::render::text::{
    draw_text_with_vertical_metrics_text, measure_text, parse_color, ResolvedTextStyle,
};
use crate::widgets::value::validation::{ContentAlignment, ValidatedValueWidget};
use skia_safe::Canvas;

pub const METRIC_WIDGET_LINE_HEIGHT: f32 = 0.92;
pub const METRIC_WIDGET_UNITS_GAP_PX: f32 = 8.0;
pub const MIN_UNITS_FONT_SIZE: f32 = 12.0;
pub const METRIC_WIDGET_UNIT_RATIO: f32 = 0.28;

pub const NUMERIC_VERTICAL_METRICS_TEXT: &str = "0123456789-:.%";
const COORDINATE_VERTICAL_METRICS_TEXT: &str = "NSEW88\u{00B0}88.888\u{2032}88\u{2033}";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StaticMetricParts {
    pub icon: bool,
    pub unit: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MetricHorizontalLayout {
    pub(crate) content_width: f32,
    pub(crate) row_origin_x: f32,
    pub(crate) icon_x: Option<f32>,
    pub(crate) value_x: f32,
    pub(crate) unit_x: Option<f32>,
}

fn metric_row_origin_x(alignment: ContentAlignment, anchor_x: f32, content_width: f32) -> f32 {
    match alignment {
        ContentAlignment::Left => anchor_x,
        ContentAlignment::Center => anchor_x - content_width * 0.5,
        ContentAlignment::Right => anchor_x - content_width,
    }
}

pub(crate) fn metric_horizontal_layout(
    alignment: ContentAlignment,
    anchor_x: f32,
    icon_slot_width: Option<f32>,
    value_width: f32,
    unit_width: Option<f32>,
    unit_gap: f32,
) -> MetricHorizontalLayout {
    let icon_width = icon_slot_width.unwrap_or(0.0);
    let trailing_width = unit_width.map_or(0.0, |width| unit_gap + width);
    let content_width = icon_width + value_width + trailing_width;
    let row_origin_x = metric_row_origin_x(alignment, anchor_x, content_width);
    MetricHorizontalLayout {
        content_width,
        row_origin_x,
        icon_x: icon_slot_width.map(|_| row_origin_x),
        value_x: row_origin_x + icon_width,
        unit_x: unit_width.map(|_| row_origin_x + icon_width + value_width + unit_gap),
    }
}

/// Draws the icon, value text, and optional unit text for a metric widget.
///
/// All output-affecting fields are read from the pre-validated type — zero
/// backend-owned defaults are applied.
pub fn draw_metric_parts(
    canvas: &Canvas,
    base_style: &ResolvedTextStyle,
    parts: &crate::widgets::value::format::MetricDisplayParts,
    scale: f32,
    fonts: &dyn crate::render::text::FontResolver,
    static_parts: StaticMetricParts,
    validated: &ValidatedValueWidget,
) -> WidgetResult<()> {
    let frame = crate::widgets::value::frame::prepare_metric_frame(
        base_style, parts, scale, fonts, validated,
    )?;
    draw_prepared_metric_frame(
        canvas,
        base_style,
        parts,
        fonts,
        static_parts,
        validated,
        &frame,
    )
}

pub fn draw_prepared_metric_frame(
    canvas: &Canvas,
    style: &ResolvedTextStyle,
    parts: &crate::widgets::value::format::MetricDisplayParts,
    fonts: &dyn crate::render::text::FontResolver,
    static_parts: StaticMetricParts,
    validated: &ValidatedValueWidget,
    frame: &crate::widgets::value::frame::MetricFrame,
) -> WidgetResult<()> {
    if let Some(icon) = frame.icon.filter(|_| !static_parts.icon) {
        crate::render::icons::draw_metric_icon(
            canvas,
            parts.icon_asset_file,
            &ColorHexSlice(validated.icon_color).to_hex_string(),
            style.opacity,
            style.shadow_color,
            style.shadow_strength,
            style.shadow_distance,
            icon.left,
            icon.top,
            icon.width(),
        );
    }
    if matches!(
        parts.content,
        crate::widgets::value::format::MetricDisplayContent::Coordinates(_)
    ) {
        for text in &frame.coordinate_text {
            draw_text_with_vertical_metrics_text(
                canvas,
                &text.text,
                &text.reference,
                &text.style,
                fonts,
            )?;
        }
        return Ok(());
    }
    let (value_text, unit_text) = parts.standard_text();
    draw_text_with_vertical_metrics_text(
        canvas,
        value_text,
        metric_vertical_metrics_text(value_text),
        &frame.value_style,
        fonts,
    )?;
    if let (Some(text), Some((unit_style, reference))) = (unit_text, &frame.unit) {
        if !static_parts.unit {
            draw_text_with_vertical_metrics_text(canvas, text, reference, unit_style, fonts)?;
        }
    }
    Ok(())
}

/// Small wrapper to present `[u8; 4]` RGBA bytes as a `#RRGGBBAA` hex string
/// for functions that still expect a `&str` colour.
pub(crate) struct ColorHexSlice(pub(crate) [u8; 4]);

impl ColorHexSlice {
    pub(crate) fn to_hex_string(&self) -> String {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3]
        )
    }
}

/// Returns the independently cacheable parts contributed by a validated value.
pub fn static_metric_parts_for_value(validated: &ValidatedValueWidget) -> StaticMetricParts {
    if validated.display_type != DisplayType::Text {
        return StaticMetricParts::default();
    }

    let icon = validated.content_alignment == ContentAlignment::Left
        && validated.show_icon
        && validated.icon_size > 0.0
        && crate::render::icons::metric_icon_asset_file(validated.metric).is_some();
    let unit = validated.content_alignment == ContentAlignment::Right
        && validated.metric != crate::model::types::MetricKind::GpsCoordinates
        && validated.show_units
        && !crate::model::standard_metrics::standard_metric_unit_label(
            validated.metric,
            Some(&validated.display_unit),
        )
        .is_empty();

    StaticMetricParts { icon, unit }
}

/// Draws static metric parts from a validated value — zero backend defaults.
pub fn draw_static_metric_parts_for_value(
    canvas: &Canvas,
    validated: &ValidatedValueWidget,
    base_style: &ResolvedTextStyle,
    scale: f32,
    fonts: &dyn crate::render::text::FontResolver,
) -> WidgetResult<StaticMetricParts> {
    let static_parts = static_metric_parts_for_value(validated);
    if !static_parts.icon {
        if static_parts.unit {
            draw_static_metric_unit(canvas, validated, base_style, scale, fonts)?;
        }
        return Ok(static_parts);
    }
    let Some(icon_asset_file) = crate::render::icons::metric_icon_asset_file(validated.metric)
    else {
        return Ok(static_parts);
    };
    if !validated.show_icon {
        return Ok(static_parts);
    }

    let icon_size = validated.icon_size * scale;
    if icon_size <= 0.0 {
        return Ok(static_parts);
    }

    let icon_color_hex = ColorHexSlice(validated.icon_color).to_hex_string();
    let icon_top = if validated.metric == crate::model::types::MetricKind::GpsCoordinates {
        let is_stacked = validated.display_unit == "both";
        let coordinate_font_size = if is_stacked {
            base_style.font_size * 0.4
        } else {
            base_style.font_size
        };
        let coordinate_line_height = coordinate_font_size * METRIC_WIDGET_LINE_HEIGHT;
        let coordinate_line_gap = if is_stacked {
            coordinate_font_size * 0.08
        } else {
            0.0
        };
        let line_count = if is_stacked { 2.0 } else { 1.0 };
        let text_height =
            coordinate_line_height * line_count + coordinate_line_gap * (line_count - 1.0);
        let row_height = icon_size.max(text_height);
        base_style.y + (row_height - icon_size) * 0.5
    } else {
        let value_line_height = base_style.font_size * METRIC_WIDGET_LINE_HEIGHT;
        let units_font_size =
            (base_style.font_size * METRIC_WIDGET_UNIT_RATIO).max(MIN_UNITS_FONT_SIZE * scale);
        let text_group_height = if validated.show_units {
            value_line_height.max(units_font_size * METRIC_WIDGET_LINE_HEIGHT)
        } else {
            value_line_height
        };
        let row_height = icon_size.max(text_group_height);
        let text_group_top = base_style.y + ((row_height - text_group_height) * 0.5);
        let text_group_bottom = text_group_top + text_group_height;
        let value_vertical_measure =
            measure_text(NUMERIC_VERTICAL_METRICS_TEXT, base_style, fonts)?;
        metric_icon_top_from_value_layout(
            text_group_bottom,
            value_line_height,
            &value_vertical_measure,
            icon_size,
        )
    };
    crate::render::icons::draw_metric_icon(
        canvas,
        Some(icon_asset_file),
        &icon_color_hex,
        base_style.opacity,
        base_style.shadow_color,
        base_style.shadow_strength,
        base_style.shadow_distance,
        metric_row_origin_x(validated.content_alignment, base_style.x, 0.0)
            + validated.icon_offset_x * scale,
        icon_top + validated.icon_offset_y * scale,
        icon_size,
    );
    Ok(static_parts)
}

fn draw_static_metric_unit(
    canvas: &Canvas,
    validated: &ValidatedValueWidget,
    base_style: &ResolvedTextStyle,
    scale: f32,
    fonts: &dyn crate::render::text::FontResolver,
) -> WidgetResult<()> {
    let unit_text = crate::model::standard_metrics::standard_metric_unit_label(
        validated.metric,
        Some(&validated.display_unit),
    );
    let mut unit_style = base_style.clone();
    unit_style.font_size =
        (base_style.font_size * METRIC_WIDGET_UNIT_RATIO).max(MIN_UNITS_FONT_SIZE * scale);
    unit_style.line_height = unit_style.font_size * METRIC_WIDGET_LINE_HEIGHT;
    unit_style.color = parse_color(
        &ColorHexSlice(validated.unit_color).to_hex_string(),
        base_style.opacity,
    );
    let unit_measure = measure_text(unit_text, &unit_style, fonts)?;
    let horizontal_layout = metric_horizontal_layout(
        validated.content_alignment,
        base_style.x,
        None,
        0.0,
        Some(unit_measure.width),
        0.0,
    );
    unit_style.x = horizontal_layout
        .unit_x
        .expect("static unit must have a horizontal position");

    let value_line_height = base_style.font_size * METRIC_WIDGET_LINE_HEIGHT;
    let text_group_height = value_line_height.max(unit_style.line_height);
    let show_icon = validated.show_icon
        && validated.icon_size > 0.0
        && crate::render::icons::metric_icon_asset_file(validated.metric).is_some();
    let row_height = if show_icon {
        (validated.icon_size * scale).max(text_group_height)
    } else {
        text_group_height
    };
    let text_group_bottom =
        base_style.y + (row_height - text_group_height) * 0.5 + text_group_height;
    let unit_vertical_metrics_text = if unit_text == "\u{00B0}" {
        "\u{00B0}C"
    } else {
        unit_text
    };
    let vertical_measure = measure_text(unit_vertical_metrics_text, &unit_style, fonts)?;
    let glyph_height = (vertical_measure.bounds_bottom - vertical_measure.bounds_top).abs();
    unit_style.y = text_group_bottom - (unit_style.line_height + glyph_height) * 0.5;
    draw_text_with_vertical_metrics_text(
        canvas,
        unit_text,
        unit_vertical_metrics_text,
        &unit_style,
        fonts,
    )
}

/// Returns the text used for vertical alignment measurements.
///
/// Numeric metrics (digits, `:`, `.`, `%`, `+`, `-`) use a stable reference
/// string (`"888:88"`) so vertical layout does not jump when the displayed
/// value changes. Coordinate values use a stable reference containing the
/// direction letters and DMS/DDM symbols. Neutral gear uses the numeric
/// reference; other text passes through unchanged.
pub fn metric_vertical_metrics_text(text: &str) -> &str {
    if text.contains('\u{00B0}') && (text.contains('\u{2032}') || text.contains('\u{2033}')) {
        COORDINATE_VERTICAL_METRICS_TEXT
    } else if text == "N"
        || (!text.is_empty()
            && text
                .chars()
                .all(|ch| ch.is_ascii_digit() || matches!(ch, ':' | '.' | '%' | '+' | '-' | '/')))
    {
        NUMERIC_VERTICAL_METRICS_TEXT
    } else {
        text
    }
}

/// Computes the icon top position so the icon is visually centered on the
/// value glyphs rather than on the row line box. This matches the frontend
/// preview layout so the Rust renderer produces identical icon placement.
pub fn metric_icon_top_from_value_layout(
    text_group_bottom: f32,
    value_line_height: f32,
    value_measure: &crate::render::text::MeasuredText,
    icon_size: f32,
) -> f32 {
    let value_glyph_height = (value_measure.bounds_bottom - value_measure.bounds_top).abs();
    let value_top = text_group_bottom - (value_line_height + value_glyph_height) * 0.5;
    value_top + (value_line_height * 0.5) - (icon_size * 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::types::MetricKind;
    use crate::widgets::value::validation::ValidatedValueFormatting;

    fn value_widget(alignment: ContentAlignment) -> ValidatedValueWidget {
        ValidatedValueWidget {
            metric: MetricKind::Speed,
            x: 300.0,
            y: 0.0,
            display_type: DisplayType::Text,
            content_alignment: alignment,
            font_name: "Arial.ttf".to_string(),
            font_size: 100.0,
            color: [255; 4],
            opacity: 1.0,
            show_icon: true,
            icon_color: [255; 4],
            icon_size: 45.0,
            icon_offset_x: 0.0,
            icon_offset_y: 0.0,
            show_units: true,
            show_full_distance: None,
            show_full_ascent: None,
            coordinate_format: None,
            unit_color: [255; 4],
            display_unit: "kmh".to_string(),
            starting_altitude_m: None,
            prefix: String::new(),
            suffix: String::new(),
            formatting: ValidatedValueFormatting::DecimalPlaces { decimals: 0 },
            hours_offset: None,
            format: None,
        }
    }

    #[test]
    fn point_anchor_origins_follow_the_canonical_formula() {
        for (alignment, expected) in [
            (ContentAlignment::Left, 300.0),
            (ContentAlignment::Center, 240.0),
            (ContentAlignment::Right, 180.0),
        ] {
            let layout =
                metric_horizontal_layout(alignment, 300.0, Some(20.0), 60.0, Some(20.0), 20.0);
            assert_eq!(layout.content_width, 120.0);
            assert_eq!(layout.row_origin_x, expected);
            assert_eq!(layout.icon_x, Some(expected));
            assert_eq!(layout.value_x, expected + 20.0);
            assert_eq!(layout.unit_x, Some(expected + 100.0));
        }
    }

    #[test]
    fn static_metric_parts_follow_alignment_and_visibility() {
        let mut widget = value_widget(ContentAlignment::Left);
        assert_eq!(
            static_metric_parts_for_value(&widget),
            StaticMetricParts {
                icon: true,
                unit: false
            }
        );

        widget.content_alignment = ContentAlignment::Center;
        assert_eq!(
            static_metric_parts_for_value(&widget),
            StaticMetricParts::default()
        );

        widget.content_alignment = ContentAlignment::Right;
        assert_eq!(
            static_metric_parts_for_value(&widget),
            StaticMetricParts {
                icon: false,
                unit: true
            }
        );

        widget.metric = MetricKind::Time;
        widget.show_units = false;
        assert_eq!(
            static_metric_parts_for_value(&widget),
            StaticMetricParts::default()
        );
        widget.content_alignment = ContentAlignment::Left;
        assert!(static_metric_parts_for_value(&widget).icon);

        widget.metric = MetricKind::Speed;
        widget.show_icon = false;
        widget.show_units = false;
        assert_eq!(
            static_metric_parts_for_value(&widget),
            StaticMetricParts::default()
        );
    }

    #[test]
    fn static_eligibility_remains_per_widget() {
        let cached = value_widget(ContentAlignment::Left);
        let dynamic = value_widget(ContentAlignment::Center);

        assert!(static_metric_parts_for_value(&cached).icon);
        assert_eq!(
            static_metric_parts_for_value(&dynamic),
            StaticMetricParts::default()
        );
    }
}
