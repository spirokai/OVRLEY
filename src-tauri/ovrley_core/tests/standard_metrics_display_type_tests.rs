//! Manifest-backed display support used by native widget validation.

use ovrley_widgets::model::standard_metrics::{is_display_type_supported, supported_display_types};
use ovrley_widgets::model::types::MetricKind;

#[test]
fn supported_display_types_per_metric() {
    let heading = supported_display_types(MetricKind::Heading);
    assert!(heading.iter().any(|dt| dt == "text"));
    assert!(heading.iter().any(|dt| dt == "heading_tape"));
    assert!(!heading.iter().any(|dt| dt == "linear"));

    let speed = supported_display_types(MetricKind::Speed);
    assert!(speed.iter().any(|dt| dt == "text"));
    assert!(speed.iter().any(|dt| dt == "linear"));
    assert!(speed.iter().any(|dt| dt == "arc"));

    let core_temp = supported_display_types(MetricKind::CoreTemperature);
    assert_eq!(core_temp.len(), 1);
    assert!(core_temp.iter().any(|dt| dt == "text"));

    let left_right = supported_display_types(MetricKind::LeftRightBalance);
    assert_eq!(left_right.len(), 1);
    assert!(left_right.iter().any(|dt| dt == "text"));

    let time = supported_display_types(MetricKind::Time);
    assert_eq!(time.len(), 1);
    assert!(time.iter().any(|dt| dt == "text"));

    let lean_angle = supported_display_types(MetricKind::LeanAngle);
    assert_eq!(lean_angle, ["text", "lean_angle"]);

    let g_force = supported_display_types(MetricKind::GForce);
    assert_eq!(g_force, ["text", "g_force"]);
}

#[test]
fn is_display_type_supported_checks_permitted_types() {
    assert!(is_display_type_supported(MetricKind::Speed, "text"));
    assert!(is_display_type_supported(MetricKind::Speed, "linear"));
    assert!(is_display_type_supported(MetricKind::Speed, "arc"));
    assert!(!is_display_type_supported(MetricKind::Heading, "linear"));
    assert!(is_display_type_supported(
        MetricKind::Heading,
        "heading_tape"
    ));
    assert!(!is_display_type_supported(
        MetricKind::CoreTemperature,
        "linear"
    ));
}
