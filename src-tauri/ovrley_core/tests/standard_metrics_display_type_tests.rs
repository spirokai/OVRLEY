//! Display-type definition and manifest tests for standard metrics.
//!
//! Verifies that the shared `assets/standard-metrics.json` manifest is loaded
//! correctly and the display-type helpers expose the formal presentation
//! contract: label, layout mode (intrinsic/boxed), default frame dimensions,
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
    default_frame_dimensions, display_type_definition, display_type_label, is_boxed_display_type,
    is_display_type_supported, supported_display_types, DisplayTypeLayoutMode,
};
use ovrley_core::MetricKind;

#[test]
fn display_type_definitions_load_from_manifest() {
    let text = display_type_definition("text").expect("text must exist");
    assert_eq!(text.label, "Text");
    assert_eq!(text.layout_mode, DisplayTypeLayoutMode::Intrinsic);
    assert!(text.default_frame_width.is_none());
    assert!(text.default_frame_height.is_none());

    let linear = display_type_definition("linear").expect("linear must exist");
    assert_eq!(linear.label, "Linear");
    assert_eq!(linear.layout_mode, DisplayTypeLayoutMode::Boxed);
    assert_eq!(linear.default_frame_width, Some(200));
    assert_eq!(linear.default_frame_height, Some(60));

    let arc = display_type_definition("arc").expect("arc must exist");
    assert_eq!(arc.label, "Arc");
    assert_eq!(arc.layout_mode, DisplayTypeLayoutMode::Boxed);
    assert_eq!(arc.default_frame_width, Some(120));
    assert_eq!(arc.default_frame_height, Some(120));

    let heading_tape = display_type_definition("heading_tape").expect("heading_tape must exist");
    assert_eq!(heading_tape.label, "Heading Tape");
    assert_eq!(heading_tape.layout_mode, DisplayTypeLayoutMode::Boxed);
    assert_eq!(heading_tape.default_frame_width, Some(200));
    assert_eq!(heading_tape.default_frame_height, Some(60));
}

#[test]
fn display_type_label_returns_label_or_key() {
    assert_eq!(display_type_label("text"), "Text");
    assert_eq!(display_type_label("heading_tape"), "Heading Tape");
    assert_eq!(display_type_label("linear"), "Linear");
    assert_eq!(display_type_label("unknown_type"), "unknown_type");
}

#[test]
fn is_boxed_display_type_correct() {
    assert!(!is_boxed_display_type("text"));
    assert!(is_boxed_display_type("linear"));
    assert!(is_boxed_display_type("bars"));
    assert!(is_boxed_display_type("arc"));
    assert!(is_boxed_display_type("corner"));
    assert!(is_boxed_display_type("heading_tape"));
    assert!(!is_boxed_display_type("nonexistent"));
}

#[test]
fn default_frame_dimensions_for_boxed_types() {
    assert_eq!(default_frame_dimensions("text"), None);
    assert_eq!(default_frame_dimensions("linear"), Some((200, 60)));
    assert_eq!(default_frame_dimensions("bars"), Some((200, 60)));
    assert_eq!(default_frame_dimensions("arc"), Some((120, 120)));
    assert_eq!(default_frame_dimensions("corner"), Some((200, 60)));
    assert_eq!(default_frame_dimensions("heading_tape"), Some((200, 60)));
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
    assert!(speed.iter().any(|dt| dt == "bars"));
    assert!(speed.iter().any(|dt| dt == "arc"));
    assert!(speed.iter().any(|dt| dt == "corner"));

    let core_temp = supported_display_types(MetricKind::CoreTemperature);
    assert_eq!(core_temp.len(), 1);
    assert!(core_temp.iter().any(|dt| dt == "text"));

    let left_right = supported_display_types(MetricKind::LeftRightBalance);
    assert_eq!(left_right.len(), 1);
    assert!(left_right.iter().any(|dt| dt == "text"));

    let time = supported_display_types(MetricKind::Time);
    assert_eq!(time.len(), 1);
    assert!(time.iter().any(|dt| dt == "text"));
}

#[test]
fn is_display_type_supported_checks_permitted_types() {
    assert!(is_display_type_supported(MetricKind::Speed, "text"));
    assert!(is_display_type_supported(MetricKind::Speed, "linear"));
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
