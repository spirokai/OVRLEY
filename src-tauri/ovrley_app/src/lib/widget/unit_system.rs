//! Port of `lib/unit-system-preference.js` (D19): the persisted unit system, read strictly at
//! startup, and Settings → Units.

use crate::lib::preferences_store::PreferencesStore;
use crate::store::services::Services;
use crate::store::Store;
use ovrley_widgets::model::document::UnitSystem;
use serde_json::Value;

const UNIT_SYSTEM_PREFERENCE_KEY: &str = "unitSystem";

/// `hydrateUnitSystemPreference`: an absent preference keeps metric; any other value than a known
/// system fails startup, as `requireUnitSystem` does in `main.jsx`.
pub fn hydrate_unit_system_preference(store: &mut Store, preferences: &PreferencesStore) -> Result<(), String> {
    match preferences.get(UNIT_SYSTEM_PREFERENCE_KEY) {
        None | Some(Value::Null) => Ok(()),
        Some(Value::String(system)) => {
            store.editor.initialize_unit_system(UnitSystem::require(system)?);
            Ok(())
        }
        Some(other) => Err(format!("Unknown unit system: {other}")),
    }
}

/// `changeUnitSystemPreference`: converts the open overlay (one undo step) and persists the choice.
pub fn change_unit_system_preference(store: &mut Store, services: &mut Services, system: UnitSystem) {
    store.editor.set_unit_system(system);
    services.preferences.set(UNIT_SYSTEM_PREFERENCE_KEY, Value::String(system.name().to_string()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::services::fakes::fake_services;
    use crate::store::Dispatcher;
    use ovrley_widgets::model::document::OverlayConfig;
    use serde_json::json;

    fn hydrated(preferences: Value) -> Result<Store, String> {
        let (parts, _) = fake_services(preferences);
        Store::hydrate(&parts.preferences)
    }

    #[test]
    fn startup_reads_the_unit_system_strictly() {
        assert_eq!(hydrated(json!({})).unwrap().editor.unit_system, UnitSystem::Metric);
        assert_eq!(hydrated(json!({"unitSystem": "imperial"})).unwrap().editor.unit_system, UnitSystem::Imperial);
        assert_eq!(hydrated(json!({"unitSystem": "nautical"})).unwrap_err(), "Unknown unit system: nautical");
        assert!(hydrated(json!({"unitSystem": 3})).is_err());
    }

    #[test]
    fn a_change_converts_the_overlay_as_one_undo_step_and_persists() {
        let (parts, fakes) = fake_services(json!({}));
        let store = Store::hydrate(&parts.preferences).unwrap();
        let dispatcher = Dispatcher::new(store, parts);
        dispatcher.dispatch(|store, _| {
            store.editor.set_config(OverlayConfig::from_value(&json!({"values": [{"id": "s", "value": "speed", "display_unit": "kmh"}]})).unwrap());
        });
        dispatcher.dispatch(|store, _| store.history.past.clear());
        dispatcher.dispatch(|store, services| change_unit_system_preference(store, services, UnitSystem::Imperial));
        dispatcher.with_store(|store| {
            assert_eq!(store.editor.config.values[0]["display_unit"], "mph");
            assert_eq!(store.history.past.len(), 1);
        });
        assert_eq!(fakes.preferences.saves.borrow().last().unwrap()["unitSystem"], "imperial");
    }
}
