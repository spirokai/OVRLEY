//! The toolbar feature's hooks: `useDrawerPreference` (hydrating and persisting the shared drawer
//! preference).

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
}
