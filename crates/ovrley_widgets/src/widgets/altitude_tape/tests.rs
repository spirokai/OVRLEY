use super::frame::{format_tape_value, format_vertical_speed_value, MISSING_TEXT};
use crate::model::types::MetricKind;
use super::layout::{altitude_tape_layout, tape_y, trend_length, visible_major_values};
use super::validation::TapeOrientation;
use skia_safe::Rect;

#[test]
fn layout_centres_the_readout_box_on_the_body_above_the_footer() {
    let frame = Rect::from_xywh(10.0, 20.0, 100.0, 400.0);
    let layout = altitude_tape_layout(frame, TapeOrientation::Left, Some(20.0), 30.0, 40.0, 20.0);
    assert_eq!(layout.footer, Rect::new(10.0, 384.0, 110.0, 420.0));
    assert_eq!(layout.body, Rect::new(10.0, 20.0, 110.0, 384.0));
    assert_eq!(layout.value_box.height(), 42.0);
    assert_eq!(layout.value_box.center_y(), layout.body.center_y());
    // The box spans the tape and its pointer tip sits on the left edge.
    assert_eq!((layout.value_box.left, layout.value_box.right), (10.0, 110.0));
    assert_eq!(layout.major_length, 40.0);
    assert_eq!(layout.minor_length, 20.0);
    assert_eq!(layout.tip_x(), 10.0);
    assert_eq!(layout.tick_span(40.0), (60.0, 100.0));
    // Right-pointing tapes mirror ticks, text alignment and the tip; a hidden readout drops the footer.
    let mirrored = altitude_tape_layout(frame, TapeOrientation::Right, None, 30.0, 40.0, 20.0);
    assert_eq!(mirrored.tip_x(), 110.0);
    assert_eq!(mirrored.tick_span(40.0), (0.0, 40.0));
    assert_eq!(mirrored.from_tick_edge(5.0), 15.0);
    assert_eq!(mirrored.text_x(15.0, 30.0), 15.0);
    assert_eq!(layout.text_x(105.0, 30.0), 75.0);
    assert_eq!(mirrored.body.bottom, 420.0);
    assert_eq!(mirrored.footer.height(), 0.0);
}

#[test]
fn higher_tape_values_sit_above_the_centre() {
    assert_eq!(tape_y(200.0, 40.0, 33.0, 2.0, 1.0), 186.0);
    assert_eq!(tape_y(200.0, 20.0, 33.0, 2.0, 1.0), 226.0);
    // Depth runs the other way: deeper values sit below the centre.
    assert_eq!(tape_y(200.0, 40.0, 33.0, 2.0, -1.0), 214.0);
    assert_eq!(tape_y(200.0, 20.0, 33.0, 2.0, -1.0), 174.0);
}

#[test]
fn visible_majors_cover_the_body_around_the_current_value() {
    let values: Vec<f64> = visible_major_values(33.7, 20, 2.0, 100.0).collect();
    assert_eq!(values, vec![0.0, 20.0, 40.0, 60.0, 80.0]);
    let negative: Vec<f64> = visible_major_values(-5.0, 10, 1.0, 20.0).collect();
    assert_eq!(negative, vec![-20.0, -10.0, 0.0, 10.0]);
}

#[test]
fn trend_projects_vertical_speed_and_caps_at_half_the_body() {
    // 1 m/s for 6 s at 2 px/m is 12 px upward.
    assert_eq!(trend_length(1.0, 6.0, 2.0, 400.0), 12.0);
    assert_eq!(trend_length(-1.0, 6.0, 2.0, 400.0), -12.0);
    assert_eq!(trend_length(50.0, 6.0, 2.0, 400.0), 200.0);
    assert_eq!(trend_length(-50.0, 6.0, 2.0, 400.0), -200.0);
}

#[test]
fn readouts_convert_units_round_and_mark_missing_data() {
    assert_eq!(format_tape_value(MetricKind::Altitude, Some(33.74), "m", true), "33.7");
    assert_eq!(format_tape_value(MetricKind::Altitude, Some(100.0), "ft", false), "328");
    assert_eq!(format_tape_value(MetricKind::Depth, Some(30.48), "ft", false), "100");
    assert_eq!(format_tape_value(MetricKind::Altitude, None, "m", true), MISSING_TEXT);
    let speed = |mps, unit, decimal| format_vertical_speed_value(mps, unit, decimal);
    assert_eq!(speed(Some(-2.0), "mps", true), ("-2.0".into(), Some(false)));
    assert_eq!(speed(Some(1.0), "ftmin", false), ("197".into(), Some(true)));
    assert_eq!(speed(Some(2.0), "ftps", true), ("6.6".into(), Some(true)));
    // Readings that round to zero, and missing samples, have no climb/descent colour.
    assert_eq!(speed(Some(-0.01), "mps", true), ("0.0".into(), None));
    assert_eq!(speed(Some(0.4), "mps", false), ("0".into(), None));
    assert_eq!(speed(None, "mps", false), (MISSING_TEXT.into(), None));
}
