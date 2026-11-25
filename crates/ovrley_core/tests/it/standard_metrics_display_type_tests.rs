//! Manifest-backed display support used by native widget validation.

use ovrley_widgets::model::catalog::{catalog, DisplayType, MetricKind};

fn supported(kind: MetricKind) -> Vec<DisplayType> {
    DisplayType::ALL
        .into_iter()
        .filter(|display| catalog().supports_display(kind, *display))
        .collect()
}

#[test]
fn supported_display_types_per_metric() {
    use DisplayType::*;
    let heading = supported(MetricKind::Heading);
    assert!(heading.contains(&Text) && heading.contains(&HeadingTape));
    assert!(!heading.contains(&Linear));
    let speed = supported(MetricKind::Speed);
    assert!(speed.contains(&Text) && speed.contains(&Linear) && speed.contains(&Arc));
    assert_eq!(supported(MetricKind::CoreTemperature), [Text]);
    assert_eq!(supported(MetricKind::LeftRightBalance), [Text]);
    assert_eq!(supported(MetricKind::Time), [Text]);
    assert_eq!(supported(MetricKind::LeanAngle), [Text, LeanAngle]);
    assert_eq!(supported(MetricKind::GForce), [Text, GForce]);
}
