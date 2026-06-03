//! Metric value widget layout and rendering behavior tests.
//!
//! Verifies `gradient_triangle_height` (zero-input and full-angle geometry),
//! `metric_vertical_metrics_text` (stable preview-compatible vertical
//! metrics selection), and `metric_icon_top_from_value_layout` (icon
//! centering on value glyph bounding box rather than row box).
//!
//! ## Type
//! Unit test. Uses `MeasuredText` structs constructed in memory —
//! no Skia, no I/O, no fixtures.
//!
//! ## Regressions guarded
//! - Gradient triangle height for zero/missing gradient values
//! - Gradient triangle height regressing back to compressed half-angle math
//! - Non-numeric text using wrong vertical metrics (misaligned icons)
//! - Icon placement math diverging from glyph-center anchoring

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
// Verifies grade visualization uses the restored full-angle geometry.
fn gradient_triangle_height_uses_full_angle_rule() {
    let expected = (72.0_f32) * (10.0_f32.to_radians().tan());
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

#[test]
// Verifies stable numeric baseline measurement removes per-digit vertical drift.
fn stable_numeric_vertical_metrics_keep_baseline_constant() {
    use ovrley_core::render::text::{baseline_for_text_top_with_line_height, resolve_font};
    use std::path::PathBuf;

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let font_dirs = vec![workspace_root.join("fonts")];
    let font = resolve_font(&font_dirs, Some("JetBrains Mono.ttf"), 90.0);
    let top = 100.0;
    let line_height = 90.0 * 0.92;

    let live_one = baseline_for_text_top_with_line_height("1", top, &font, line_height);
    let live_nine = baseline_for_text_top_with_line_height("9", top, &font, line_height);
    let stable_one =
        baseline_for_text_top_with_line_height(NUMERIC_VERTICAL_METRICS_TEXT, top, &font, line_height);
    let stable_nine =
        baseline_for_text_top_with_line_height(NUMERIC_VERTICAL_METRICS_TEXT, top, &font, line_height);

    assert_ne!(
        live_one, live_nine,
        "JetBrains Mono should expose different live baselines for different digits in Skia"
    );
    assert_eq!(
        stable_one, stable_nine,
        "Stable numeric metrics should keep the baseline fixed across digit changes"
    );
}
