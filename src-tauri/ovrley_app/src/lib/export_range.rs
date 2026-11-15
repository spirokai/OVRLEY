//! The part of `lib/export-range.js` the preview scene needs: the export range and its window.
//! Phase 4 ports the rest (the range handles and the sidebar inputs).

use crate::lib::utils::clamp;
use ovrley_core::activity::schema::ParsedActivity;
use ovrley_core::normalize::template::template_manifest;
use serde::Deserialize;

/// Whether the whole activity or a custom part of it exports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportRangeType {
    All,
    Custom,
}

/// An export range.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportRange {
    #[serde(rename = "type")]
    pub kind: ExportRangeType,
    pub from: f64,
    pub to: f64,
}

impl ExportRange {
    /// `DEFAULT_EXPORT_RANGE`, from the template manifest.
    pub fn default_range() -> Self {
        serde_json::from_value(template_manifest().export_range.clone()).expect("the template manifest's export range is valid")
    }
}

/// `resolveActivityDuration`: the trimmed duration (always present natively), never negative.
pub fn activity_duration(activity: Option<&ParsedActivity>) -> f64 {
    let duration = activity.map_or(0.0, |activity| activity.trim_end_seconds);
    if duration.is_finite() {
        duration.max(0.0)
    } else {
        0.0
    }
}

/// The window an export range covers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExportRangeWindow {
    pub active: bool,
    pub start: f64,
    pub end: f64,
}

/// `resolveExportRangeWindow`: a custom range clamped to the activity, else the whole activity.
pub fn resolve_export_range_window(activity: Option<&ParsedActivity>, range: &ExportRange) -> ExportRangeWindow {
    let duration = activity_duration(activity);
    let whole = ExportRangeWindow { active: false, start: 0.0, end: duration };
    if range.kind != ExportRangeType::Custom {
        return whole;
    }
    let start = clamp(range.from, 0.0, duration);
    let end = clamp(range.to, 0.0, duration);
    if end > start {
        ExportRangeWindow { active: true, start, end }
    } else {
        whole
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn activity(duration: f64) -> ParsedActivity {
        ParsedActivity { trim_end_seconds: duration, ..Default::default() }
    }

    #[test]
    fn the_default_range_exports_everything() {
        let range = ExportRange::default_range();
        assert_eq!(range.kind, ExportRangeType::All);
        assert_eq!(resolve_export_range_window(Some(&activity(90.0)), &range), ExportRangeWindow { active: false, start: 0.0, end: 90.0 });
        assert_eq!(resolve_export_range_window(None, &range).end, 0.0);
    }

    #[test]
    fn custom_ranges_clamp_and_fall_back_when_empty() {
        let custom = |from, to| ExportRange { kind: ExportRangeType::Custom, from, to };
        assert_eq!(resolve_export_range_window(Some(&activity(60.0)), &custom(10.0, 90.0)), ExportRangeWindow { active: true, start: 10.0, end: 60.0 });
        assert!(!resolve_export_range_window(Some(&activity(60.0)), &custom(70.0, 90.0)).active);
    }
}
