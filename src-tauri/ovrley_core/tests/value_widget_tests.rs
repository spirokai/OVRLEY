use ovrley_core::render::text::MeasuredText;
use ovrley_core::render::widgets::value::{
    gradient_triangle_height, metric_icon_top_from_value_layout, metric_vertical_metrics_text,
    NUMERIC_VERTICAL_METRICS_TEXT,
};

#[test]
// Verifies missing and near-zero gradients produce no triangle height.
fn gradient_triangle_height_is_zero_for_zero_and_missing_values() {
    assert_eq!(gradient_triangle_height(None, 72.0), 0.0);
    assert_eq!(gradient_triangle_height(Some(0.0), 72.0), 0.0);
}

#[test]
// Verifies grade visualization uses the configured half-angle geometry.
fn gradient_triangle_height_uses_half_angle_rule() {
    let expected = (72.0_f32) * (5.0_f32.to_radians().tan());
    let actual = gradient_triangle_height(Some(10.0), 72.0);
    assert!((actual - expected).abs() < 0.001);
}

#[test]
// Verifies numeric values use stable preview-compatible vertical metrics.
fn metric_vertical_metrics_text_uses_canonical_numeric_sample() {
    assert_eq!(
        metric_vertical_metrics_text("19:00"),
        NUMERIC_VERTICAL_METRICS_TEXT
    );
    assert_eq!(
        metric_vertical_metrics_text("-12.5%"),
        NUMERIC_VERTICAL_METRICS_TEXT
    );
    assert_eq!(metric_vertical_metrics_text("TEMP"), "TEMP");
}

#[test]
// Verifies icon placement centers on value glyphs instead of the row box.
fn metric_icon_top_centers_on_value_glyph_box() {
    let measure = MeasuredText {
        width: 100.0,
        bounds_left: 0.0,
        bounds_top: -70.0,
        bounds_right: 100.0,
        bounds_bottom: 10.0,
        ascent: -80.0,
        descent: 20.0,
    };

    let actual = metric_icon_top_from_value_layout(92.0, 92.0, &measure, 44.0);

    assert!((actual - 30.0).abs() < 0.001);
}
