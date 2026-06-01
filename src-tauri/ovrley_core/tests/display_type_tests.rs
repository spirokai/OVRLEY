//! DisplayType serde compatibility tests for the `value` widget config.
//!
//! Verifies that the `display_type` field on `ValueConfig`:
//! - Defaults to `Text` when the field is absent, null, or carries an
//!   unrecognized value (preserves backward compatibility for old templates).
//! - Parses each recognized JSON string (`"text"`, `"linear"`, `"bars"`,
//!   `"arc"`, `"corner"`, `"heading_tape"`) into the matching enum variant.
//! - Round-trips through serialize -> deserialize without changing variant.
//!
//! ## Type
//! Integration test. Pure JSON parsing via the public `ovrley_core` API.
//!
//! ## Regressions guarded
//! - Renaming variants in a way that breaks serialized templates
//! - Unknown `display_type` values crashing or producing parse errors
//! - Lossy round-trips (e.g. serializing `Linear` and reading back as `Text`)

use ovrley_core::config::{parse_config_json, ValueConfig};
use ovrley_core::types::DisplayType;

mod common;

#[test]
// Verifies a value widget template that omits `display_type` parses to `Text`.
// This is the core backward-compatibility behavior: old templates must keep working.
fn value_config_without_display_type_defaults_to_text() {
    let config = parse_config_json(
        r#"{
            "scene": {
                "fps": 30,
                "start": 0,
                "end": 10,
                "ffmpeg": {}
            },
            "labels": [],
            "values": [
                {
                    "value": "speed",
                    "x": 10,
                    "y": 20
                }
            ],
            "plots": []
        }"#,
    )
    .unwrap();

    assert_eq!(config.values.len(), 1);
    assert_eq!(config.values[0].display_type, DisplayType::Text);
}

#[test]
// Touches the same surface from a struct-level view (not just via parse_config_json)
// so a future refactor that moves parsing into ValueConfig directly still gets
// coverage. The struct can be deserialized in isolation because every other
// required field is present in this fixture.
fn value_config_struct_without_display_type_defaults_to_text() {
    let value: ValueConfig = serde_json::from_str(
        r#"{
            "value": "speed",
            "x": 0,
            "y": 0
        }"#,
    )
    .unwrap();

    assert_eq!(value.display_type, DisplayType::Text);
}

#[test]
// Verifies every recognized `display_type` string parses to the right variant.
// Uses a single table-driven test so adding a variant only requires touching
// the input fixture table, not adding a new test function.
fn value_config_with_each_recognized_display_type_parses_to_correct_variant() {
    let cases = [
        (r#""text""#, DisplayType::Text),
        (r#""linear""#, DisplayType::Linear),
        (r#""bars""#, DisplayType::Bars),
        (r#""arc""#, DisplayType::Arc),
        (r#""corner""#, DisplayType::Corner),
        (r#""heading_tape""#, DisplayType::Tape),
    ];

    for (json_value, expected) in cases {
        let value: ValueConfig = serde_json::from_str(&format!(
            r#"{{
                "value": "speed",
                "x": 0,
                "y": 0,
                "display_type": {json_value}
            }}"#
        ))
        .unwrap();

        assert_eq!(
            value.display_type, expected,
            "display_type {json_value} should parse to {expected:?}"
        );
    }
}

#[test]
// Verifies an explicit JSON `null` for `display_type` still defaults to `Text`.
// The deserializer must treat null and absent identically so a stale editor
// that emits `display_type: null` (e.g. from a partially-cleared dropdown)
// does not produce a parse error.
fn value_config_with_null_display_type_defaults_to_text() {
    let value: ValueConfig = serde_json::from_str(
        r#"{
            "value": "speed",
            "x": 0,
            "y": 0,
            "display_type": null
        }"#,
    )
    .unwrap();

    assert_eq!(value.display_type, DisplayType::Text);
}

#[test]
// Verifies an unrecognized `display_type` value falls back to `Text` instead
// of failing the whole template parse. Templates authored against a future
// schema version (or with typos like `"liner"`) must remain loadable.
fn value_config_with_unknown_display_type_defaults_to_text() {
    let value: ValueConfig = serde_json::from_str(
        r#"{
            "value": "speed",
            "x": 0,
            "y": 0,
            "display_type": "radial"
        }"#,
    )
    .unwrap();

    assert_eq!(value.display_type, DisplayType::Text);
}

#[test]
// Verifies a non-string JSON value (a number) for `display_type` also falls
// back to `Text` rather than producing a parse error. The field is a string
// enum, but a corrupt template or hand-edited file might have any shape.
fn value_config_with_non_string_display_type_defaults_to_text() {
    let value: ValueConfig = serde_json::from_str(
        r#"{
            "value": "speed",
            "x": 0,
            "y": 0,
            "display_type": 42
        }"#,
    )
    .unwrap();

    assert_eq!(value.display_type, DisplayType::Text);
}

#[test]
// Verifies the full `display_type` enum round-trips through serialize +
// deserialize. Each variant must serialize to the exact frontend string,
// and parsing that string back must yield the same variant. This guards
// `#[serde(rename)]` consistency.
fn display_type_round_trips_each_variant() {
    let cases = [
        (DisplayType::Text, r#""text""#),
        (DisplayType::Linear, r#""linear""#),
        (DisplayType::Bars, r#""bars""#),
        (DisplayType::Arc, r#""arc""#),
        (DisplayType::Corner, r#""corner""#),
        (DisplayType::Tape, r#""heading_tape""#),
    ];

    for (variant, expected_json) in cases {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(
            serialized, expected_json,
            "serialization mismatch for {variant:?}"
        );

        let deserialized: DisplayType = serde_json::from_str(expected_json).unwrap();
        assert_eq!(
            deserialized, variant,
            "deserialization mismatch for {variant:?}"
        );
    }
}

#[test]
// Verifies a fully-populated `ValueConfig` with a non-default `display_type`
// survives a parse -> serialize -> parse round trip without losing the
// display type or other fields. This protects against a future refactor
// that accidentally drops the `display_type` field during serialization.
fn value_config_with_display_type_round_trips_through_full_config() {
    let original = parse_config_json(
        r#"{
            "scene": {
                "fps": 30,
                "start": 0,
                "end": 10,
                "ffmpeg": {}
            },
            "labels": [],
            "values": [
                {
                    "value": "speed",
                    "x": 10,
                    "y": 20,
                    "display_type": "linear"
                }
            ],
            "plots": []
        }"#,
    )
    .unwrap();

    let serialized = serde_json::to_string(&original).unwrap();
    let reparsed = parse_config_json(&serialized).unwrap();

    assert_eq!(reparsed.values.len(), 1);
    assert_eq!(reparsed.values[0].display_type, DisplayType::Linear);
    assert_eq!(reparsed.values[0].x, 10.0);
    assert_eq!(reparsed.values[0].y, 20.0);
}
