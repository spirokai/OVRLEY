use super::layout::{heart_zones_layout, scale_hsv, IndicatorShape};
use super::validation::HeartZonesIndicatorStyle;
use super::zones::{coggan_zones, zone_position, HeartZoneRange};
use skia_safe::Color;

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1e-3, "{actual} != {expected}");
}

#[test]
fn coggan_zones_span_forty_to_one_hundred_fifteen_percent_lthr() {
    let zones = coggan_zones(200.0);
    let bounds: Vec<(f32, f32)> = zones.iter().map(|zone| (zone.from, zone.to)).collect();
    let expected = [
        (80.0, 138.0),
        (138.0, 168.0),
        (168.0, 190.0),
        (190.0, 212.0),
        (212.0, 230.0),
    ];
    for ((from, to), (expected_from, expected_to)) in bounds.into_iter().zip(expected) {
        assert_close(from, expected_from);
        assert_close(to, expected_to);
    }
}

#[test]
fn zone_position_tracks_progress_and_clamps_outside_the_track() {
    let zones = coggan_zones(200.0);
    assert_eq!(zone_position(&zones, 153.0), (1, 0.5));
    assert_eq!(zone_position(&zones, 168.0), (2, 0.0));
    assert_eq!(zone_position(&zones, 60.0), (0, 0.0));
    assert_eq!(zone_position(&zones, 109.0), (0, 0.5));
    assert_eq!(zone_position(&zones, 260.0), (4, 1.0));
}

#[test]
fn zone_position_assigns_manual_gaps_to_the_lower_zone() {
    let zone = |from, to| HeartZoneRange { from, to };
    let zones = [
        zone(80.0, 120.0),
        zone(121.0, 140.0),
        zone(141.0, 160.0),
        zone(161.0, 175.0),
        zone(176.0, 195.0),
    ];
    assert_eq!(zone_position(&zones, 120.5), (0, 1.0));
    assert_eq!(zone_position(&zones, 121.0), (1, 0.0));
}

#[test]
fn heart_zones_layout_splits_equal_zones_and_caps_the_outer_radius() {
    let layout = heart_zones_layout(10.0, 20.0, 520.0, 40.0, 5.0, 100.0);
    assert_eq!(layout.zone_width, 100.0);
    assert_eq!(layout.corner_radius, 20.0);
    assert_eq!(layout.zone_rect(4).right, 530.0);
    assert_eq!(layout.indicator_x(2, 0.25), 10.0 + 2.0 * 105.0 + 25.0);
    let active = layout.active_rect(0);
    assert_eq!(active.top, 20.0 - layout.grow);
    // Enlarging is vertical only, so the gaps between zones are preserved.
    assert_eq!(active.left, 10.0);
    assert_eq!(
        layout.active_rect(1).left - layout.active_rect(0).right,
        5.0
    );
}

#[test]
fn arrow_indicators_point_at_the_enlarged_active_zone_from_outside() {
    let layout = heart_zones_layout(0.0, 0.0, 500.0, 40.0, 4.0, 8.0);
    let arrow = IndicatorShape::new(&layout, HeartZonesIndicatorStyle::Arrow, 16.0);
    let line = IndicatorShape::new(&layout, HeartZonesIndicatorStyle::Line, 16.0);
    assert_eq!(arrow.top, -layout.grow);
    assert_eq!(arrow.bottom, 40.0 + layout.grow);
    assert_eq!(arrow.extent().top, -layout.grow - 12.0);
    assert!(line.top > -layout.grow && line.bottom < 40.0 + layout.grow);
    assert_eq!(arrow.extent().width(), 16.0);
}

#[test]
fn zone_colors_scale_value_and_saturation_without_shifting_hue_or_alpha() {
    let brighten = |color| scale_hsv(color, 1.45, 1.3);
    assert_eq!(
        brighten(Color::from_argb(255, 40, 60, 100)),
        Color::from_argb(255, 32, 70, 145)
    );
    // Value and saturation both cap at full; alpha is untouched.
    assert_eq!(
        brighten(Color::from_argb(200, 200, 100, 50)),
        Color::from_argb(200, 255, 89, 6)
    );
    // Greys have no hue to saturate and only brighten.
    assert_eq!(
        brighten(Color::from_argb(255, 128, 128, 128)),
        Color::from_argb(255, 186, 186, 186)
    );
    assert_eq!(brighten(Color::BLACK), Color::BLACK);
}

#[test]
fn inactive_zones_wash_out_saturation_and_keep_value() {
    // Saturation 0.6 -> 0.48 at value 100: the darkest channel rises from 40 to 52.
    assert_eq!(
        scale_hsv(Color::from_argb(255, 40, 60, 100), 1.0, 0.8),
        Color::from_argb(255, 52, 68, 100)
    );
}
