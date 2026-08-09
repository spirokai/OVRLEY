//! Lap timer text widgets.
//!
//! The widget presents current-lap, best-lap, delta, and lap-log modes from
//! the frame-aligned lap series in
//! [`crate::activity::schema::DenseActivityReport`]. Preparation builds
//! static text layers for states that do not change within a frame; drawing
//! composes those layers with the current frame's dynamic text.
//!
//! Module ownership:
//! - `text` — lap-state lookup, table-row derivation, and display formatting.
//! - `layout` — text measurement, positioning, and static Skia layer creation.
//! - `prepare` — cache construction for the validated lap-timer configuration.
//! - `draw` — per-frame composition and dynamic current-row rendering.

mod draw;
mod layout;
mod prepare;
mod text;

#[cfg(test)]
mod tests {
    use super::layout::prepare_content_layer;
    use super::text::{delta_color, format_lap_delta, format_lap_duration, lap_timer_label_text};
    use crate::render::text::ResolvedTextStyle;
    use skia_safe::Color;

    /// Verifies compact and hour-inclusive lap duration formatting.
    #[test]
    fn formats_sub_hour_and_hour_plus_laps() {
        assert_eq!(format_lap_duration(3.456), "00:03.46");
        assert_eq!(format_lap_duration(3599.999), "01:00:00.00");
        assert_eq!(format_lap_duration(3661.2), "01:01:01.20");
    }

    /// Verifies explicit signs and hundredth rounding for lap deltas.
    #[test]
    fn formats_delta_with_an_explicit_sign_and_positive_zero() {
        assert_eq!(format_lap_delta(None), "+0.00");
        assert_eq!(format_lap_delta(Some(0.0)), "+0.00");
        assert_eq!(format_lap_delta(Some(0.125)), "+0.13");
        assert_eq!(format_lap_delta(Some(-0.125)), "-0.13");
    }

    /// Verifies that missing and non-positive deltas use the negative color.
    #[test]
    fn uses_negative_color_for_missing_and_zero_delta() {
        let positive = [0, 255, 0, 255];
        let negative = [255, 0, 0, 255];

        assert_eq!(delta_color(positive, negative, None), negative);
        assert_eq!(delta_color(positive, negative, Some(0.0)), negative);
        assert_eq!(delta_color(positive, negative, Some(0.25)), positive);
        assert_eq!(delta_color(positive, negative, Some(-0.25)), negative);
    }

    /// Verifies that configured lap timer labels are uppercased for rendering.
    #[test]
    fn capitalizes_lap_timer_labels_for_rendering() {
        assert_eq!(lap_timer_label_text("current lap"), "CURRENT LAP");
        assert_eq!(lap_timer_label_text("Delta"), "DELTA");
    }

    /// Verifies that best-lap text is prepared as a widget-local image layer.
    #[test]
    fn prepares_best_lap_text_as_a_positioned_widget_local_layer() {
        let style = ResolvedTextStyle {
            x: 900.0,
            y: 500.0,
            font_name: None,
            font_size: 72.0,
            line_height: 66.24,
            color: Color::WHITE,
            opacity: 1.0,
            shadow_color: Some(Color::BLACK),
            shadow_strength: 5.0,
            shadow_distance: 8.0,
            border_color: Some(Color::BLACK),
            border_thickness: 4.0,
        };

        let layer = prepare_content_layer(&style, "Best Lap", true, "01:23.45", &[]).unwrap();

        assert!(layer.x > 800.0);
        assert!(layer.y > 400.0);
        assert!(layer.image.width() < 500);
        assert!(layer.image.height() < 250);
    }
}

pub(crate) use draw::draw_lap_timer;
pub(crate) use prepare::prepare_lap_timer_cache;
pub use text::{lap_log_text_state, lap_timer_value_text, LapLogTextState};

/// Font-size ratio used for lap timer labels and table headers.
const LABEL_FONT_RATIO: f32 = 0.35;
/// Line-height ratio used by lap timer value and table text.
const LINE_HEIGHT_RATIO: f32 = 0.92;
/// Font-size ratio used for horizontal gaps between table columns.
const LOG_COLUMN_GAP_RATIO: f32 = 1.8;
/// Font-size ratio used for vertical gaps between table rows.
const LOG_ROW_GAP_RATIO: f32 = 0.38;
/// Opacity applied to the lap-log table header.
const LOG_HEADER_OPACITY: f32 = 0.7;
/// Stable glyph sample used to measure lap-log cells.
const TABLE_VERTICAL_METRICS_TEXT: &str = "0123456789+-:.";
/// Text shown when no lap duration is available yet.
const PLACEHOLDER: &str = "--:--.--";
