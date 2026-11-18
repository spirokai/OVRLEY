//! The toolbar feature's hooks: `useDrawerPreference` (hydrating and persisting the shared drawer
//! preference) and `useToolbarDrawer`'s Escape rule.

use crate::i18n::catalog;
use crate::lib::file_dialog::filename_from_selected_path;
use crate::lib::format::{compare_text, format_clock_duration, format_locale_number, format_zoned_date_time};
use crate::lib::widget::activity_attribute_label;
use crate::store::slices::media::{ActivitySource, ActivitySummary, MetricSource};
use ovrley_widgets::model::document::js_round;
use ovrley_widgets::widgets::lap_timer::format::format_lap_duration;
use crate::lib::keyboard_shortcuts::{match_keyboard_shortcut, KeyInput, Scope};
use crate::lib::preferences_store::PreferencesStore;
use crate::store::services::Services;
use crate::store::slices::layout::DrawerPreference;
use crate::store::Store;

/// The preference key, shared with the React app.
pub const DRAWER_PREFERENCE_KEY: &str = "leftDrawer";

/// `useDrawerPreference`'s `lastObservedPreferenceRef`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawerPreferenceState {
    pub last_observed: Option<DrawerPreference>,
}

/// `normalizePreference`. A value that is not an object with a boolean `pinned` and a known
/// `activeTool` becomes the default `{pinned: false, activeTool: "widgets"}`. This repairs
/// malformed data, which the code rules forbid for new code; it is the React behaviour, kept at
/// the user's request (phase 2 Q5).
pub fn normalize_drawer_preference(value: Option<&serde_json::Value>) -> DrawerPreference {
    value
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default()
}

/// The first effect: read `leftDrawer` once and initialize the drawer from it. Runs before the
/// window exists, so the drawer state is known before the first frame.
pub fn hydrate_drawer_preference(store: &mut Store, preferences: &PreferencesStore) {
    let preference = normalize_drawer_preference(preferences.get(DRAWER_PREFERENCE_KEY));
    store.drawer_preference.last_observed = Some(preference);
    store.layout.initialize_left_drawer(preference);
}

/// The second effect, a reconcile step: once the drawer is initialized, writes `{pinned,
/// activeTool}` when it differs from the last value seen.
pub fn persist_drawer_preference(store: &mut Store, services: &mut Services) {
    if !store.layout.left_drawer_initialized {
        return;
    }
    let preference = DrawerPreference {
        pinned: store.layout.left_drawer_pinned,
        active_tool: store.layout.active_left_drawer_tool,
    };
    if store.drawer_preference.last_observed == Some(preference) {
        return;
    }
    store.drawer_preference.last_observed = Some(preference);
    services.preferences.set(
        DRAWER_PREFERENCE_KEY,
        serde_json::to_value(preference).expect("a drawer preference serializes"),
    );
}

/// `useToolbarDrawer`'s capture-phase Escape rule (`drawer.close`): Escape dismisses an unpinned
/// drawer. Returns whether the key is consumed. A pinned drawer leaves Escape alone: in Slint an
/// open select or popover takes its own Escape before the window sees it, which is what React's
/// `hasOpenPopup` check achieves.
pub fn handle_drawer_capture_key(store: &mut Store, input: &KeyInput) -> bool {
    let is_close = match_keyboard_shortcut(input, Scope::Drawer).is_some_and(|found| found.command.id == "drawer.close");
    if !is_close || !store.layout.is_unpinned_overlay() {
        return false;
    }
    store.layout.dismiss_left_drawer_overlay();
    true
}

/// One metadata row of the activity drawer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityRow {
    pub label: String,
    pub value: String,
}

/// One lap row (`buildLapRows`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LapRow {
    pub id: usize,
    pub number: String,
    pub duration: String,
    pub start: String,
    pub enabled: bool,
}

/// `buildActivityDrawerViewModel`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityDrawerView {
    pub file_name: String,
    pub can_delete: bool,
    pub format_label: String,
    pub metadata: Vec<ActivityRow>,
    pub extracted: Vec<String>,
    pub derived: Vec<String>,
    pub laps: Vec<LapRow>,
}

/// `buildLapRows`: the duration as the lap timer widget shows it (its formatter, D20) and the start
/// as a clock duration.
pub fn lap_rows(summary: &ActivitySummary, disabled: &[usize]) -> Vec<LapRow> {
    summary
        .laps
        .iter()
        .map(|lap| LapRow {
            id: lap.id,
            number: (lap.id + 1).to_string(),
            duration: format_lap_duration(lap.duration_seconds),
            start: format_clock_duration(lap.start_seconds),
            enabled: !disabled.contains(&lap.id),
        })
        .collect()
}

/// The activity drawer's view of the loaded activity; `None` without one.
pub fn activity_drawer_view(store: &Store) -> Option<ActivityDrawerView> {
    let summary = store.media.activity_summary.as_ref()?;
    let locale = store.i18n.language;
    let t = |key: &str| catalog().t(locale, key).to_string();
    let unknown = t("toolbar.unknown");
    let start_time = match &summary.timezone {
        Some(timezone) => summary
            .sync_time
            .as_deref()
            .and_then(|sync_time| format_zoned_date_time(sync_time, timezone, locale))
            .unwrap_or_else(|| unknown.clone()),
        None => unknown.clone(),
    };
    let meters = summary.total_distance_meters;
    let distance = if meters >= 1000.0 {
        format!("{} km", format_locale_number(meters / 1000.0, 2))
    } else {
        format!("{} m", format_locale_number(js_round(meters), 3))
    };
    let mut metadata = vec![
        ActivityRow { label: t("toolbar.startTime"), value: start_time },
        ActivityRow { label: t("toolbar.timezone"), value: summary.timezone.clone().unwrap_or_else(|| unknown.clone()) },
        ActivityRow { label: t("toolbar.distance"), value: distance },
        ActivityRow { label: t("toolbar.duration"), value: format_clock_duration(summary.duration_seconds) },
        ActivityRow { label: t("toolbar.activityType"), value: summary.sport.clone().unwrap_or_else(|| unknown.clone()) },
    ];
    if let Some(count) = summary.original_sample_count {
        metadata.push(ActivityRow {
            label: t("toolbar.extracted"),
            value: catalog().t_with(locale, "toolbar.valPoints", &[("val", &format_locale_number(count, 3))]),
        });
    }
    let mut extracted = Vec::new();
    let mut derived = Vec::new();
    for metric in &summary.available_metrics {
        let label = activity_attribute_label(&metric.attribute, locale);
        match metric.source {
            MetricSource::Direct => extracted.push(label),
            MetricSource::Derived | MetricSource::Mixed => derived.push(label),
        }
    }
    extracted.sort_by(|left, right| compare_text(left, right, locale));
    derived.sort_by(|left, right| compare_text(left, right, locale));
    let file_name = match &store.media.activity_source {
        Some(ActivitySource::File { path }) => filename_from_selected_path(path).unwrap_or(path).to_string(),
        None => summary.file_name.clone().unwrap_or_default(),
    };
    Some(ActivityDrawerView {
        file_name,
        // `hasExternalActivity`.
        can_delete: store.media.parsed_activity.is_some(),
        format_label: summary.file_format.as_deref().map_or(unknown, str::to_uppercase),
        metadata,
        extracted,
        derived,
        laps: lap_rows(summary, &store.media.disabled_laps),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::services::fakes::fake_services;
    use crate::store::slices::layout::LeftDrawerTool;
    use crate::store::Dispatcher;
    use serde_json::json;

    #[test]
    fn normalizes_malformed_preferences_to_the_default() {
        for value in [
            json!(null),
            json!([]),
            json!("widgets"),
            json!({"pinned": "yes", "activeTool": "video"}),
            json!({"pinned": true, "activeTool": "timeline"}),
            json!({"pinned": true}),
        ] {
            assert_eq!(
                normalize_drawer_preference(Some(&value)),
                DrawerPreference::default(),
                "{value}"
            );
        }
        assert_eq!(
            normalize_drawer_preference(None),
            DrawerPreference::default()
        );
        assert_eq!(
            normalize_drawer_preference(Some(&json!({"pinned": true, "activeTool": "videoSync"}))),
            DrawerPreference {
                pinned: true,
                active_tool: LeftDrawerTool::VideoSync
            }
        );
    }

    #[test]
    fn the_drawer_preference_is_written_once_per_change_and_not_at_hydration() {
        let (parts, fakes) =
            fake_services(json!({"leftDrawer": {"pinned": true, "activeTool": "video"}}));
        let store = Store::hydrate(&parts.preferences).unwrap();
        assert!(store.layout.left_drawer_visible);
        assert_eq!(store.layout.active_left_drawer_tool, LeftDrawerTool::Video);
        let dispatcher = Dispatcher::new(store, parts);
        let saved_drawer = || {
            fakes
                .preferences
                .saves
                .borrow()
                .iter()
                .map(|saved| saved[DRAWER_PREFERENCE_KEY].clone())
                .collect::<Vec<_>>()
        };

        dispatcher.dispatch(|_, _| {});
        assert!(saved_drawer().is_empty(), "nothing changed since hydration");

        dispatcher.dispatch(|store, _| {
            store
                .layout
                .select_left_drawer_tool(LeftDrawerTool::Widgets)
        });
        dispatcher.dispatch(|store, _| store.layout.dismiss_left_drawer_overlay());
        dispatcher.dispatch(|store, _| store.layout.set_left_drawer_pinned(false));
        dispatcher.dispatch(|store, _| store.layout.dismiss_left_drawer_overlay());

        assert_eq!(
            saved_drawer(),
            [
                json!({"pinned": true, "activeTool": "widgets"}),
                json!({"pinned": false, "activeTool": "widgets"})
            ]
        );
    }

    #[test]
    fn escape_dismisses_only_an_unpinned_drawer() {
        let mut store = Store::default();
        let escape = KeyInput::new("escape");
        assert!(!handle_drawer_capture_key(&mut store, &escape), "a hidden drawer leaves Escape alone");

        store.layout.select_left_drawer_tool(LeftDrawerTool::Activity);
        assert!(handle_drawer_capture_key(&mut store, &escape));
        assert!(!store.layout.left_drawer_visible);

        store.layout.set_left_drawer_pinned(true);
        assert!(!handle_drawer_capture_key(&mut store, &escape), "a pinned drawer leaves Escape to the focused popup");
        assert!(store.layout.left_drawer_visible);
    }

    #[test]
    fn other_keys_do_not_close_the_drawer() {
        let mut store = Store::default();
        store.layout.select_left_drawer_tool(LeftDrawerTool::Activity);
        let mut shifted = KeyInput::new("escape");
        shifted.shift = true;
        assert!(!handle_drawer_capture_key(&mut store, &shifted));
        assert!(!handle_drawer_capture_key(&mut store, &KeyInput::new("x")));
        assert!(store.layout.left_drawer_visible);
    }

    #[test]
    fn the_activity_drawer_formats_details_metrics_and_laps() {
        use crate::store::slices::media::{ActivityLap, AvailableMetric};
        let mut store = Store::default();
        assert!(activity_drawer_view(&store).is_none());
        store.media.activity_source = Some(ActivitySource::File { path: "/rides/day 2.fit".into() });
        store.media.parsed_activity = Some(crate::store::slices::media::ImportedActivity {
            activity: std::sync::Arc::new(Default::default()),
            preview_bytes: std::sync::Arc::new(Vec::new()),
        });
        store.media.activity_summary = Some(ActivitySummary {
            available_metrics: vec![
                AvailableMetric { attribute: "speed".into(), source: MetricSource::Direct },
                AvailableMetric { attribute: "gradient".into(), source: MetricSource::Mixed },
            ],
            duration_seconds: 3725.0,
            end_time: None,
            file_format: Some("fit".into()),
            file_name: Some("x.fit".into()),
            laps: vec![ActivityLap { id: 0, start_seconds: 0.0, duration_seconds: 61.234 }, ActivityLap { id: 1, start_seconds: 61.2, duration_seconds: 3700.0 }],
            original_sample_count: None,
            sample_count: 10.0,
            sport: None,
            sync_time: None,
            timezone: None,
            total_distance_meters: 999.6,
        });
        store.media.disabled_laps = vec![1];
        let view = activity_drawer_view(&store).unwrap();
        assert_eq!(view.file_name, "day 2.fit");
        assert!(view.can_delete);
        assert_eq!(view.format_label, "FIT");
        assert_eq!(view.metadata[0].value, "Unknown");
        assert_eq!(view.metadata[3].value, "1:02:05");
        assert!(view.metadata[2].value.ends_with(" m"));
        assert_eq!(view.metadata.len(), 5, "no extracted-points row without a count");
        assert_eq!(view.extracted.len(), 1);
        assert_eq!(view.derived.len(), 1, "mixed metrics count as calculated");
        assert_eq!((view.laps[0].duration.as_str(), view.laps[1].duration.as_str()), ("01:01.23", "01:01:40.00"));
        assert_eq!(view.laps[1].start, "01:01");
        assert!(!view.laps[1].enabled);
    }
}
