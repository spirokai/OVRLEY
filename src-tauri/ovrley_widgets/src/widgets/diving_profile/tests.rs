use super::{
    config::DivingProfileConfig,
    geometry::DivingGeometry,
    sampling::{CeilingCurve, DivingActivity, DivingSamples},
    validation::validate_diving_profile,
};
use crate::model::scene::{validate_scene_presentation, ScenePresentationConfig};
use crate::widgets::route::sampling::RouteWindow;

fn config() -> super::validation::ValidatedDivingProfile {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/standard-widgets.json"
    )))
    .unwrap();
    let widget = manifest["definitions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["type"] == "diving_profile")
        .unwrap();
    let config: DivingProfileConfig = serde_json::from_value(widget["defaults"].clone()).unwrap();
    let scene: ScenePresentationConfig = serde_json::from_value(serde_json::json!({
        "width": 1920, "height": 1080, "scale": 1, "opacity": 1, "font": "Arial.ttf", "shadow_strength": 0,
        "shadow_distance": 0, "shadow_color": "#000000", "border_thickness": 0, "border_color": "#000000"
    })).unwrap();
    validate_diving_profile(&config, 0, &validate_scene_presentation(scene).unwrap()).unwrap()
}

#[test]
fn ceiling_reaches_new_stop_at_transition_without_overshoot_and_preserves_surface_intervals() {
    let curve = CeilingCurve::new(&[
        (0.0, 9.0),
        (9.0, 9.0),
        (10.0, 6.0),
        (14.0, 6.0),
        (15.0, 3.0),
        (19.0, 3.0),
        (20.0, 0.0),
        (25.0, 0.0),
    ]);
    for (time, depth) in [
        (0.0, 9.0),
        (10.0, 6.0),
        (15.0, 3.0),
        (20.0, 0.0),
        (25.0, 0.0),
    ] {
        assert_eq!(curve.at(time), Some(depth));
    }
    let values: Vec<_> = (0..=250)
        .map(|i| curve.at(i as f64 / 10.0).unwrap())
        .collect();
    assert!(values.windows(2).all(|p| p[0] >= p[1] && p[1] >= 0.0));
    let reversal = CeilingCurve::new(&[
        (0.0, 0.0),
        (9.0, 0.0),
        (10.0, 6.0),
        (15.0, 3.0),
        (20.0, 9.0),
        (25.0, 0.0),
    ]);
    assert_eq!(reversal.at(5.0), Some(0.0));
    assert!(reversal.at(8.0).unwrap() > 0.0);
    assert_eq!(reversal.at(10.0), Some(6.0));
    for pair in reversal.knots.windows(2) {
        for i in 0..=100 {
            let time = pair[0].0 + (pair[1].0 - pair[0].0) * i as f64 / 100.0;
            let depth = reversal.at(time).unwrap();
            assert!(
                depth >= pair[0].1.min(pair[1].1) - 1e-9
                    && depth <= pair[0].1.max(pair[1].1) + 1e-9
            );
        }
    }
}

#[test]
fn smoothing_and_detail_change_depth_geometry_and_invalidate_its_cache_key() {
    let mut config = config();
    config.width = 40.0;
    config.simplify_tolerance_px = 0.0;
    config.target_density = 1.5;
    let samples = DivingSamples {
        depth: vec![(0..=100)
            .map(|i| {
                let progress = i as f32 / 100.0;
                (progress, 10.0 + (i as f64 * 0.4).sin(), progress)
            })
            .collect()],
        ceiling: Vec::new(),
    };
    let detailed = DivingGeometry::new(&config, 1.0, &samples);
    config.simplify_tolerance_px = 4.0;
    let smooth = DivingGeometry::new(&config, 1.0, &samples);
    assert!(smooth.segments[0].points.len() < detailed.segments[0].points.len());
    assert_ne!(smooth.key, detailed.key);
    config.simplify_tolerance_px = 0.0;
    config.target_density = 0.25;
    let coarse = DivingGeometry::new(&config, 1.0, &samples);
    assert!(coarse.segments[0].points.len() < detailed.segments[0].points.len());
    assert_ne!(coarse.key, detailed.key);
}

#[test]
fn time_axis_crop_and_missing_telemetry_keep_surface_and_profile_gaps() {
    let activity = DivingActivity::new(
        &[0.0, 10.0, 20.0, 30.0, 40.0],
        &[Some(0.0), Some(20.0), None, Some(10.0), Some(0.0)],
        &[],
        40.0,
    );
    let config = config();
    let samples = activity.samples(None, config.width);
    let geometry = DivingGeometry::new(&config, 1.0, &samples);
    assert_eq!(geometry.segments.len(), 2);
    assert!(geometry.paths.lock().unwrap().ceiling_area.is_empty());
    let first = activity.frame_state(&geometry, 0.0, None);
    assert_eq!(first.marker, Some((0.0, 0.0)));
    let state = activity.frame_state(&geometry, 10.0, None);
    assert_eq!(state.marker, Some((config.width * 0.25, config.height)));
    assert!(activity.frame_state(&geometry, 20.0, None).marker.is_none());
    let window = Some(RouteWindow {
        start: 5.0,
        end: 35.0,
    });
    let cropped = activity.samples(window, config.width);
    assert_eq!(cropped.depth[0][0], (0.0, 10.0, 0.0));
    assert_eq!(*cropped.depth[1].last().unwrap(), (1.0, 5.0, 1.0));
    let mut exaggerated = config.clone();
    exaggerated.y_scale = 2.0;
    assert_eq!(
        DivingGeometry::new(&exaggerated, 1.0, &samples).segments[0].points[0].1,
        0.0
    );
    let ceiling_gaps = DivingActivity::new(
        &[0.0, 10.0, 20.0, 30.0, 40.0],
        &[],
        &[Some(6.0), Some(3.0), None, Some(3.0), Some(0.0)],
        40.0,
    );
    assert_eq!(ceiling_gaps.samples(None, config.width).ceiling.len(), 2);
}
