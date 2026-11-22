use super::frame::format_heading_compass_value;
use super::layout::{
    bearing_screen_angle, heading_compass_layout, indicator_path, indicator_vertices, label_fade,
    readout_anchor, GAP_CENTER_ANGLE,
};

#[test]
fn heading_compass_readout_wraps_whole_degrees_and_marks_missing_samples() {
    assert_eq!(format_heading_compass_value(Some(23.4)), "23");
    assert_eq!(format_heading_compass_value(Some(359.6)), "0");
    assert_eq!(format_heading_compass_value(Some(-10.0)), "350");
    assert_eq!(format_heading_compass_value(None), "--");
}

#[test]
fn heading_compass_track_grows_outwards_and_opens_around_the_readout() {
    let thin = heading_compass_layout(240.0, 4.0, 20.0, 270.0);
    let thick = heading_compass_layout(240.0, 30.0, 20.0, 270.0);

    assert_eq!(thin.inner_radius, thick.inner_radius);
    assert_eq!(thin.label_radius, thick.label_radius);
    assert_eq!(thick.outer_radius, 150.0);
    assert_eq!(thick.size(), 300.0);
    assert_eq!(thick.track_sweep_angle, 270.0);
    // The 90° gap is centred on the readout angle.
    assert_eq!(thick.track_start_angle, GAP_CENTER_ANGLE + 45.0);
    assert_eq!(
        (thick.track_start_angle + thick.track_sweep_angle) % 360.0,
        GAP_CENTER_ANGLE - 45.0
    );
}

#[test]
fn heading_compass_bearings_map_north_up_and_follow_the_card() {
    assert_eq!(bearing_screen_angle(0.0, 0.0), -90.0);
    assert_eq!(bearing_screen_angle(90.0, 0.0), 0.0);
    // Turning the card by -heading brings that heading to the top.
    assert_eq!(bearing_screen_angle(120.0, -120.0), -90.0);
}

#[test]
fn heading_compass_needle_rounding_stays_inside_its_sharp_outline() {
    let sharp = indicator_path(80.0, 0.0).compute_tight_bounds();
    let round = indicator_path(80.0, 100.0).compute_tight_bounds();
    let tip = indicator_vertices(80.0)[0];

    assert_eq!(sharp.top, tip.y);
    assert!(round.top > sharp.top);
    assert!(round.left >= sharp.left && round.right <= sharp.right);
    assert!(round.bottom <= sharp.bottom);
}

#[test]
fn heading_compass_labels_fade_across_the_gap_and_stay_opaque_over_the_track() {
    // 270° track: the 90° gap spans screen angles 0°..90°, fading over 25° from each end.
    let layout = heading_compass_layout(240.0, 10.0, 20.0, 270.0);

    assert_eq!(label_fade(&layout, -90.0), 1.0);
    assert_eq!(label_fade(&layout, 0.0), 1.0);
    assert_eq!(label_fade(&layout, 90.0), 1.0);
    assert_eq!(label_fade(&layout, 45.0), 0.0);
    assert!((label_fade(&layout, 12.5) - 0.5).abs() < 1e-5);
    assert!((label_fade(&layout, 360.0 + 77.5) - 0.5).abs() < 1e-5);
    assert_eq!(
        label_fade(&heading_compass_layout(240.0, 10.0, 20.0, 360.0), 45.0),
        1.0
    );
}

#[test]
fn heading_compass_readout_starts_at_the_gap_opening() {
    // 270° track ends at 0° and 90° on the 125px centre line: the opening's midpoint.
    let layout = heading_compass_layout(240.0, 10.0, 20.0, 270.0);
    let anchor = readout_anchor(&layout);
    assert!((anchor.x - 62.5).abs() < 1e-3 && (anchor.y - 62.5).abs() < 1e-3);
    // The opening's midpoint stays on the gap's bisector for any track angle.
    let narrow = readout_anchor(&heading_compass_layout(240.0, 10.0, 20.0, 340.0));
    assert!((narrow.y.atan2(narrow.x).to_degrees() - GAP_CENTER_ANGLE).abs() < 1e-3);
}
