//! Display-type definition and manifest tests for standard metrics.
//!
//! Verifies that the shared `assets/standard-metrics.json` manifest is loaded
//! correctly and the display-type helpers expose the formal presentation
//! contract: label key, layout mode (intrinsic/boxed), default frame dimensions,
//! per-metric overrides, and supported display types.
//!
//! ## Type
//! Integration test. Reads the compiled-in manifest via the public
//! `ovrley_core::standard_metrics` API.
//!
//! ## Regressions guarded
//! - Malformed or incomplete display-type definitions in the manifest
//! - Layout mode mismatches between frontend and backend
//! - Missing default frame dimensions for boxed display types
//! - Per-metric override regressions

use ovrley_core::standard_metrics::{
    default_frame_dimensions, display_type_definition, display_type_label_key,
    is_boxed_display_type, is_display_type_supported, supported_display_types,
    DisplayTypeLayoutMode,
};
use ovrley_core::MetricKind;

const LINEAR_FRAME: (u32, u32) = (200, 24);
const ARC_FRAME: (u32, u32) = (220, 220);
const CORNER_FRAME: (u32, u32) = (162, 162);
const G_FORCE_FRAME: (u32, u32) = (274, 274);

#[test]
fn display_type_definitions_load_from_manifest() {
    let text = display_type_definition("text").expect("text must exist");
    assert_eq!(text.label_key, "widgets.displayTypes.text");
    assert_eq!(text.layout_mode, DisplayTypeLayoutMode::Intrinsic);
    assert!(text.default_frame_width.is_none());
    assert!(text.default_frame_height.is_none());

    let linear = display_type_definition("linear").expect("linear must exist");
    assert_eq!(linear.label_key, "widgets.displayTypes.linear");
    assert_eq!(linear.layout_mode, DisplayTypeLayoutMode::Boxed);
    assert_eq!(linear.default_frame_width, Some(LINEAR_FRAME.0));
    assert_eq!(linear.default_frame_height, Some(LINEAR_FRAME.1));

    let arc = display_type_definition("arc").expect("arc must exist");
    assert_eq!(arc.label_key, "widgets.displayTypes.arc");
    assert_eq!(arc.layout_mode, DisplayTypeLayoutMode::Boxed);
    assert_eq!(arc.default_frame_width, Some(ARC_FRAME.0));
    assert_eq!(arc.default_frame_height, Some(ARC_FRAME.1));

    let heading_tape = display_type_definition("heading_tape").expect("heading_tape must exist");
    assert_eq!(heading_tape.label_key, "widgets.displayTypes.headingTape");
    assert_eq!(heading_tape.layout_mode, DisplayTypeLayoutMode::Boxed);
    assert_eq!(heading_tape.default_frame_width, Some(600));
    assert_eq!(heading_tape.default_frame_height, Some(100));

    let lean_angle = display_type_definition("lean_angle").expect("lean_angle must exist");
    assert_eq!(lean_angle.label_key, "widgets.displayTypes.leanAngle");
    assert_eq!(lean_angle.layout_mode, DisplayTypeLayoutMode::Boxed);
    assert!(lean_angle.default_frame_width.is_none());
    assert!(lean_angle.default_frame_height.is_none());

    let g_force = display_type_definition("g_force").expect("g_force must exist");
    assert_eq!(g_force.label_key, "widgets.displayTypes.gForce");
    assert_eq!(g_force.layout_mode, DisplayTypeLayoutMode::Boxed);
    assert_eq!(g_force.default_frame_width, Some(G_FORCE_FRAME.0));
    assert_eq!(g_force.default_frame_height, Some(G_FORCE_FRAME.1));
}

#[test]
fn display_type_label_key_returns_translation_key_for_known_types() {
    assert_eq!(
        display_type_label_key("text"),
        Some("widgets.displayTypes.text")
    );
    assert_eq!(
        display_type_label_key("heading_tape"),
        Some("widgets.displayTypes.headingTape")
    );
    assert_eq!(
        display_type_label_key("linear"),
        Some("widgets.displayTypes.linear")
    );
    assert_eq!(display_type_label_key("unknown_type"), None);
}

#[test]
fn is_boxed_display_type_correct() {
    assert!(!is_boxed_display_type("text"));
    assert!(is_boxed_display_type("linear"));
    assert!(is_boxed_display_type("arc"));
    assert!(is_boxed_display_type("corner"));
    assert!(is_boxed_display_type("heading_tape"));
    assert!(is_boxed_display_type("lean_angle"));
    assert!(is_boxed_display_type("g_force"));
    assert!(!is_boxed_display_type("nonexistent"));
}

#[test]
fn default_frame_dimensions_for_boxed_types() {
    assert_eq!(default_frame_dimensions("text"), None);
    assert_eq!(default_frame_dimensions("linear"), Some(LINEAR_FRAME));
    assert_eq!(default_frame_dimensions("arc"), Some(ARC_FRAME));
    assert_eq!(default_frame_dimensions("corner"), Some(CORNER_FRAME));
    assert_eq!(default_frame_dimensions("heading_tape"), Some((600, 100)));
    assert_eq!(default_frame_dimensions("lean_angle"), None);
    assert_eq!(default_frame_dimensions("g_force"), Some(G_FORCE_FRAME));
    assert_eq!(default_frame_dimensions("nonexistent"), None);
}

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
