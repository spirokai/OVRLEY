//! Port of `store/slices/createMediaSlice.js`: the error message (Phase 2) and the activity part
//! (Phase 3). Video telemetry, render progress and video import arrive with Phases 6 and 7.

use ovrley_core::activity::schema::ParsedActivity;
use serde_json::Value;
use std::sync::Arc;

/// Where the activity came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActivitySource {
    File { path: String },
}

/// Which kind of source the parsed activity is. Video telemetry joins in Phase 6.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParsedActivitySource {
    ActivityFile,
}

/// The imported activity: the parsed data, and the serialized copy the preview's sampler reads
/// (D4). Dropping it releases both.
#[derive(Clone, Debug)]
pub struct ImportedActivity {
    pub activity: Arc<ParsedActivity>,
    pub preview_bytes: Arc<Vec<u8>>,
}

impl PartialEq for ImportedActivity {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.activity, &other.activity)
    }
}

/// How a metric was obtained.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricSource {
    Direct,
    Derived,
    Mixed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AvailableMetric {
    pub attribute: String,
    pub source: MetricSource,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActivityLap {
    pub id: usize,
    pub start_seconds: f64,
    pub duration_seconds: f64,
}

/// `createActivitySummary`.
#[derive(Clone, Debug, PartialEq)]
pub struct ActivitySummary {
    pub available_metrics: Vec<AvailableMetric>,
    pub duration_seconds: f64,
    pub file_format: Option<String>,
    pub file_name: Option<String>,
    pub laps: Vec<ActivityLap>,
    pub original_sample_count: Option<f64>,
    pub sample_count: f64,
    pub sport: Option<String>,
    pub sync_time: Option<String>,
    pub timezone: Option<String>,
    pub total_distance_meters: f64,
}

/// `getDurationSeconds`: the trimmed end, else the metadata duration.
pub fn activity_duration_seconds(activity: &ParsedActivity) -> f64 {
    // `trim_end_seconds ?? metadata.duration_seconds`: the field is always present natively, and a
    // parser that does not trim leaves 0, which JavaScript's `??` would keep as well.
    let duration = activity.trim_end_seconds;
    if duration.is_finite() {
        duration
    } else {
        0.0
    }
}

impl ActivitySummary {
    /// Activity is external data: absent or odd metadata becomes the summary's empty values.
    pub fn of(activity: &ParsedActivity) -> Self {
        let attributes = |key: &str| -> Vec<String> {
            activity.extra.get(key).and_then(|value| serde_json::from_value(value.clone()).ok()).unwrap_or_default()
        };
        let coverage = activity.extra.get("coverage");
        let available_metrics = attributes("valid_attributes")
            .into_iter()
            .chain(attributes("extended_attributes"))
            .filter_map(|attribute| {
                let source = coverage
                    .and_then(|coverage| coverage.get(&attribute))
                    .and_then(|entry| entry.get("source"))
                    .and_then(|source| serde_json::from_value(source.clone()).ok())?;
                Some(AvailableMetric { attribute, source })
            })
            .collect();
        let metadata = &activity.metadata;
        let text = |key: &str| metadata.get(key).and_then(Value::as_str).map(str::to_string);
        Self {
            available_metrics,
            duration_seconds: activity_duration_seconds(activity),
            file_format: activity.file_format.clone(),
            file_name: activity.file_name.clone(),
            laps: activity
                .lap_start_elapsed_seconds
                .iter()
                .enumerate()
                .map(|(id, start)| ActivityLap {
                    id,
                    start_seconds: *start,
                    duration_seconds: activity.lap_durations_seconds.get(id).copied().unwrap_or_default(),
                })
                .collect(),
            original_sample_count: metadata.get("original_sample_count").and_then(Value::as_f64),
            sample_count: metadata.get("sample_count").and_then(Value::as_f64).unwrap_or_default(),
            sport: text("sport"),
            sync_time: activity.sync_time.clone(),
            timezone: text("timezone"),
            total_distance_meters: metadata.get("total_distance_m").and_then(Value::as_f64).unwrap_or_default(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MediaSlice {
    pub error_message: Option<String>,
    pub is_processing: bool,
    pub activity_source: Option<ActivitySource>,
    pub activity_summary: Option<ActivitySummary>,
    pub parsed_activity: Option<ImportedActivity>,
    pub parsed_activity_source: Option<ParsedActivitySource>,
    /// Strictly increasing lap ids excluded from lap widgets.
    pub disabled_laps: Vec<usize>,
}

impl MediaSlice {
    pub fn set_error_message(&mut self, message: String) {
        self.error_message = Some(message);
    }

    pub fn clear_error(&mut self) {
        self.error_message = None;
    }

    pub fn set_processing(&mut self, processing: bool) {
        self.is_processing = processing;
    }

    /// `setLapEnabled`: keeps the disabled list sorted.
    pub fn set_lap_enabled(&mut self, lap_id: usize, enabled: bool) {
        self.disabled_laps.retain(|id| *id != lap_id);
        if !enabled {
            self.disabled_laps.push(lap_id);
        }
        self.disabled_laps.sort_unstable();
    }

    pub fn set_activity_source(&mut self, source: Option<ActivitySource>) {
        self.activity_source = source;
    }

    /// Replaces the parsed activity; lap ids belong to one activity, so any replacement or removal
    /// purges the disabled laps (`setActivityState`).
    fn set_parsed_activity(&mut self, activity: Option<ImportedActivity>, source: Option<ParsedActivitySource>) {
        if self.parsed_activity != activity {
            self.disabled_laps.clear();
        }
        self.parsed_activity = activity;
        self.parsed_activity_source = source;
    }

    /// `activateActivityFile`.
    pub fn activate_activity_file(&mut self, activity: ImportedActivity) {
        let summary = ActivitySummary::of(&activity.activity);
        self.set_parsed_activity(Some(activity), Some(ParsedActivitySource::ActivityFile));
        self.activity_summary = Some(summary);
    }

    /// `clearActivityFile`: only an activity file is cleared. Without video telemetry to restore
    /// (Phase 6), the activity and its summary go. `clear_filename` false keeps the source.
    pub fn clear_activity_file(&mut self, clear_filename: bool) {
        if self.parsed_activity_source != Some(ParsedActivitySource::ActivityFile) {
            return;
        }
        if clear_filename {
            self.activity_source = None;
        }
        self.set_parsed_activity(None, None);
        self.activity_summary = None;
    }

    /// `clearActivitySummary`: clears the activity file, keeping its source.
    pub fn clear_activity_summary(&mut self) {
        self.clear_activity_file(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn imported(laps: usize) -> ImportedActivity {
        let mut activity = ParsedActivity {
            file_name: Some("ride.fit".into()),
            file_format: Some("fit".into()),
            metadata: json!({"sport": "cycling", "timezone": "Europe/Prague", "total_distance_m": 1234.5, "sample_count": 10, "original_sample_count": 12}),
            trim_end_seconds: 73.6,
            lap_start_elapsed_seconds: (0..laps).map(|lap| lap as f64 * 10.0).collect(),
            lap_durations_seconds: vec![10.0; laps],
            ..Default::default()
        };
        activity.extra.insert("valid_attributes".into(), json!(["speed", "heartrate"]));
        activity.extra.insert("extended_attributes".into(), json!(["gradient"]));
        activity.extra.insert("coverage".into(), json!({"speed": {"source": "direct"}, "heartrate": {"source": "direct"}, "gradient": {"source": "derived"}}));
        ImportedActivity {
            activity: Arc::new(activity),
            preview_bytes: Arc::new(Vec::new()),
        }
    }

    #[test]
    fn activation_summarizes_the_activity() {
        let mut media = MediaSlice::default();
        media.activate_activity_file(imported(2));
        let summary = media.activity_summary.clone().unwrap();
        assert_eq!(summary.duration_seconds, 73.6);
        assert_eq!(summary.laps.len(), 2);
        assert_eq!(summary.laps[1].start_seconds, 10.0);
        assert_eq!(summary.available_metrics.len(), 3);
        assert_eq!(summary.available_metrics[2].source, MetricSource::Derived);
        assert_eq!(summary.sport.as_deref(), Some("cycling"));
        assert_eq!(summary.original_sample_count, Some(12.0));
        assert_eq!(media.parsed_activity_source, Some(ParsedActivitySource::ActivityFile));
    }

    #[test]
    fn laps_stay_sorted_and_a_new_activity_clears_them() {
        let mut media = MediaSlice::default();
        media.activate_activity_file(imported(4));
        media.set_lap_enabled(3, false);
        media.set_lap_enabled(1, false);
        media.set_lap_enabled(3, true);
        media.set_lap_enabled(2, false);
        assert_eq!(media.disabled_laps, [1, 2]);
        media.activate_activity_file(imported(4));
        assert!(media.disabled_laps.is_empty());
    }

    #[test]
    fn clearing_keeps_or_drops_the_source() {
        let mut media = MediaSlice::default();
        media.set_activity_source(Some(ActivitySource::File { path: "/a/ride.fit".into() }));
        media.activate_activity_file(imported(1));
        media.clear_activity_summary();
        assert!(media.parsed_activity.is_none() && media.activity_summary.is_none());
        assert!(media.activity_source.is_some(), "the summary clear keeps the source");
        media.activate_activity_file(imported(1));
        media.clear_activity_file(true);
        assert!(media.activity_source.is_none());
        media.clear_activity_file(true);
    }
}
