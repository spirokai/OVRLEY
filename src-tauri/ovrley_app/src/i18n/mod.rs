//! Port of `app/src/i18n`: the translation catalog, `translateOptions`, `translateBackendError`
//! and the language preference (plan D6).
//!
//! The 15 locale files are embedded from `app/src/i18n/locales/`, where the React app keeps them.
//! i18next's behaviour is kept: English is the fallback language, a key missing from a locale shows
//! the English text, a key missing from every locale shows its code default, and `{{name}}`
//! placeholders are filled without escaping.
//!
//! This catalog serves the strings Rust code chooses at runtime (errors, projected labels). The
//! `.slint` files translate their own strings with `@tr`, from gettext catalogs that `ovrley_ui`'s
//! build converts from the same JSON files.

pub mod locales;

pub use locales::{Locale, DEFAULT_LOCALE, LOCALES};

use crate::lib::preferences_store::PreferencesStore;
use crate::store::services::Services;
use crate::store::Store;
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

/// The preference key, shared with the React app.
pub const LANGUAGE_PREFERENCE_KEY: &str = "lng";

/// Each locale's file, in [`LOCALES`] order.
const LOCALE_FILES: [&str; 15] = [
    include_str!("../../../../app/src/i18n/locales/en-translation.json"),
    include_str!("../../../../app/src/i18n/locales/zh-CN-translation.json"),
    include_str!("../../../../app/src/i18n/locales/ja-translation.json"),
    include_str!("../../../../app/src/i18n/locales/de-translation.json"),
    include_str!("../../../../app/src/i18n/locales/fr-translation.json"),
    include_str!("../../../../app/src/i18n/locales/tr-translation.json"),
    include_str!("../../../../app/src/i18n/locales/it-translation.json"),
    include_str!("../../../../app/src/i18n/locales/ko-translation.json"),
    include_str!("../../../../app/src/i18n/locales/es-translation.json"),
    include_str!("../../../../app/src/i18n/locales/ru-translation.json"),
    include_str!("../../../../app/src/i18n/locales/pl-translation.json"),
    include_str!("../../../../app/src/i18n/locales/pt-BR-translation.json"),
    include_str!("../../../../app/src/i18n/locales/nl-translation.json"),
    include_str!("../../../../app/src/i18n/locales/cs-translation.json"),
    include_str!("../../../../app/src/i18n/locales/sv-translation.json"),
];

/// Every locale's strings, flattened to `namespace.key.path`.
pub struct Catalog {
    tables: Vec<HashMap<String, String>>,
}

impl Catalog {
    fn parse() -> Self {
        let tables = LOCALE_FILES
            .iter()
            .zip(LOCALES)
            .map(|(text, (locale, _))| {
                let value: Value = serde_json::from_str(text).unwrap_or_else(|error| panic!("{}-translation.json is malformed: {error}", locale.code()));
                let mut table = HashMap::new();
                flatten("", &value, &mut table, locale);
                table
            })
            .collect();
        Self { tables }
    }

    /// `t(key)` in `locale`, with i18next's fallbacks: the locale, then English, then the code
    /// default.
    ///
    /// # Panics
    /// For a key that is in no catalog, which the tests rule out for every key Rust code uses
    /// impossible.
    pub fn t(&self, locale: Locale, key: &str) -> &str {
        self.tables[locale.index()]
            .get(key)
            .or_else(|| self.tables[DEFAULT_LOCALE.index()].get(key))
            .map(String::as_str)
            .unwrap_or_else(|| panic!("translation key {key} has no text"))
    }

    /// `t(key, defaultValue)`: like [`Self::t`], with a default for keys no catalog has.
    pub fn t_or<'a>(&'a self, locale: Locale, key: &str, default: &'a str) -> &'a str {
        self.tables[locale.index()]
            .get(key)
            .or_else(|| self.tables[DEFAULT_LOCALE.index()].get(key))
            .map_or(default, String::as_str)
    }

    /// `t(key, {count})`: i18next's plural keys (`key_one`, `key_few`, `key_other`…) chosen by the
    /// language's cardinal rules, `{{count}}` filled; `default` when no form exists.
    pub fn t_count(&self, locale: Locale, key: &str, count: usize, default: &str) -> String {
        use icu::plurals::{PluralCategory, PluralRules};
        let category = icu::locale::Locale::try_from_str(locale.code())
            .ok()
            .and_then(|language| PluralRules::try_new_cardinal(language.into()).ok())
            .map_or(PluralCategory::Other, |rules| rules.category_for(count));
        let suffix = match category {
            PluralCategory::Zero => "zero",
            PluralCategory::One => "one",
            PluralCategory::Two => "two",
            PluralCategory::Few => "few",
            PluralCategory::Many => "many",
            PluralCategory::Other => "other",
        };
        let template = [format!("{key}_{suffix}"), format!("{key}_other")]
            .iter()
            .find_map(|candidate| self.tables[locale.index()].get(candidate).or_else(|| self.tables[DEFAULT_LOCALE.index()].get(candidate)))
            .map_or(default, String::as_str);
        interpolate(template, &[("count", &count.to_string())])
    }

    /// `t(key, {name: value, …})`.
    pub fn t_with(&self, locale: Locale, key: &str, values: &[(&str, &str)]) -> String {
        interpolate(self.t(locale, key), values)
    }
}

fn flatten(prefix: &str, value: &Value, table: &mut HashMap<String, String>, locale: Locale) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                let path = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
                flatten(&path, value, table, locale);
            }
        }
        Value::String(text) => {
            table.insert(prefix.to_string(), text.clone());
        }
        other => panic!("{}-translation.json: {prefix} must be a string, found {other}", locale.code()),
    }
}

/// The catalog, parsed on first use.
pub fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(Catalog::parse)
}

/// Fills `{{name}}` placeholders, without escaping. A placeholder with no value is left as is.
pub fn interpolate(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (name, value) in values {
        out = out.replace(&format!("{{{{{name}}}}}"), value);
    }
    out
}

/// A select option whose label is translated (`translateOptions`'s input).
#[derive(Clone, Copy, Debug)]
pub struct OptionDefinition<T> {
    pub value: T,
    pub label_key: &'static str,
}

/// `translateOptions`: each option's value with its translated label.
pub fn translate_options<T: Copy>(options: &[OptionDefinition<T>], locale: Locale) -> Vec<(T, String)> {
    options
        .iter()
        .map(|option| (option.value, catalog().t(locale, option.label_key).to_string()))
        .collect()
}

/// `[project_error:<tag>]` → its message key.
const PROJECT_ERROR_KEYS: [(&str, &str); 4] = [
    ("archive_size", "projects.archiveTooLarge"),
    ("invalid_archive", "projects.invalidArchive"),
    ("raster_unavailable", "projects.rasterUnavailable"),
    ("save_failed", "projects.saveFailed"),
];

/// `[raster_error:<tag>:<id>]` → its message key.
const RASTER_ERROR_KEYS: [(&str, &str); 12] = [
    ("invalid_config", "raster.errors.invalidConfig"),
    ("no_image", "raster.errors.noImage"),
    ("missing_resource", "raster.errors.renderUnavailable"),
    ("relative_path", "raster.errors.relativePath"),
    ("missing", "raster.errors.missing"),
    ("unreadable", "raster.errors.unreadable"),
    ("unsupported_type", "raster.errors.unsupportedType"),
    ("encoded_size", "raster.errors.encodedSize"),
    ("resolution", "raster.errors.resolution"),
    ("decode", "raster.errors.decode"),
    ("missing_asset", "raster.errors.missingAsset"),
    ("corrupt_asset", "raster.errors.corruptAsset"),
];

/// `translateBackendError`: formats tagged backend errors (`[project_error:…]`,
/// `[raster_error:…:…]`) for display, and leaves any other message unchanged.
pub fn translate_backend_error(message: &str, locale: Locale) -> String {
    static PROJECT: OnceLock<Regex> = OnceLock::new();
    static RASTER: OnceLock<Regex> = OnceLock::new();
    let catalog = catalog();

    let project = PROJECT.get_or_init(|| Regex::new(r"\[project_error:([a-z_]+)\]").expect("valid regex"));
    if let Some(code) = project.captures(message).map(|captures| captures.get(1).expect("one group").as_str()) {
        let key = PROJECT_ERROR_KEYS.iter().find(|(tag, _)| *tag == code).map(|(_, key)| *key);
        if let Some(key) = key {
            return catalog.t(locale, key).to_string();
        }
    }

    let raster = RASTER.get_or_init(|| Regex::new(r"\[raster_error:([a-z_]+):([^\]]+)\]").expect("valid regex"));
    let Some(captures) = raster.captures(message) else {
        return message.to_string();
    };
    let id = captures.get(2).expect("two groups").as_str();
    let tag = captures.get(1).expect("two groups").as_str();
    let Some(key) = RASTER_ERROR_KEYS.iter().find(|(candidate, _)| *candidate == tag).map(|(_, key)| *key) else {
        return message.to_string();
    };
    let translated = catalog.t_with(locale, key, &[("id", id)]);
    if tag == "no_image" || tag == "missing_resource" || id == "configuration" {
        translated
    } else {
        format!("{id}: {translated}")
    }
}

/// The resolved language.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct I18nState {
    pub language: Locale,
}

/// `hydrateLanguagePreference`: an absent `lng` keeps English; any other value must be a supported
/// locale, or startup fails with `Unsupported locale: …`.
pub fn hydrate_language_preference(store: &mut Store, preferences: &PreferencesStore) -> Result<(), String> {
    match preferences.get(LANGUAGE_PREFERENCE_KEY) {
        None | Some(Value::Null) => Ok(()),
        Some(value) => {
            store.i18n.language = Locale::require(value)?;
            Ok(())
        }
    }
}

/// `changeLanguagePreference`: applies and persists a supported language. The projections then
/// write the catalog into the string globals.
pub fn change_language_preference(store: &mut Store, services: &mut Services, locale: Locale) {
    store.i18n.language = locale;
    services
        .preferences
        .set(LANGUAGE_PREFERENCE_KEY, Value::String(locale.code().to_string()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::services::fakes::fake_services;
    use crate::store::Dispatcher;
    use serde_json::json;

    #[test]
    fn every_locale_file_parses_and_english_is_complete() {
        let catalog = catalog();
        assert_eq!(catalog.t(Locale::De, "app-shell.close"), catalog.tables[Locale::De.index()]["app-shell.close"]);
        assert_eq!(catalog.t(Locale::En, "toolbar.pinDrawer"), catalog.tables[0]["toolbar.pinDrawer"]);
    }

    #[test]
    fn falls_back_to_english() {
        let catalog = catalog();
        let missing_in_german = catalog.tables[0]
            .keys()
            .find(|key| !catalog.tables[Locale::De.index()].contains_key(*key))
            .cloned();
        if let Some(key) = missing_in_german.as_deref() {
            assert_eq!(catalog.t(Locale::De, key), catalog.t(Locale::En, key));
        }
    }

    #[test]
    fn every_backend_error_key_has_english_text() {
        for (_, key) in PROJECT_ERROR_KEYS.iter().chain(RASTER_ERROR_KEYS.iter()) {
            assert!(catalog().tables[DEFAULT_LOCALE.index()].contains_key(*key), "{key}");
        }
    }

    #[test]
    fn interpolates_without_escaping() {
        assert_eq!(interpolate("Color stop at {{percent}}%", &[("percent", "<50>")]), "Color stop at <50>%");
        assert_eq!(interpolate("{{a}} and {{b}}", &[("a", "x")]), "x and {{b}}");
    }

    #[test]
    fn translates_both_backend_error_tag_families() {
        let catalog = catalog();
        assert_eq!(
            translate_backend_error("boom [project_error:save_failed]", Locale::En),
            catalog.t(Locale::En, "projects.saveFailed")
        );
        assert_eq!(
            translate_backend_error("x [raster_error:missing:logo]", Locale::En),
            format!("logo: {}", catalog.t_with(Locale::En, "raster.errors.missing", &[("id", "logo")]))
        );
        assert_eq!(
            translate_backend_error("x [raster_error:no_image:logo]", Locale::En),
            catalog.t_with(Locale::En, "raster.errors.noImage", &[("id", "logo")])
        );
        assert_eq!(
            translate_backend_error("x [raster_error:missing:configuration]", Locale::En),
            catalog.t_with(Locale::En, "raster.errors.missing", &[("id", "configuration")])
        );
    }

    #[test]
    fn leaves_unknown_tags_and_plain_messages_unchanged() {
        for message in ["plain failure", "x [project_error:unknown]", "x [raster_error:unknown:logo]"] {
            assert_eq!(translate_backend_error(message, Locale::En), message);
        }
    }

    #[test]
    fn hydrates_lng_before_the_application_mounts() {
        let (preferences, _) = PreferencesStore::in_memory(json!({"lng": "de"}));
        let mut store = Store::default();
        hydrate_language_preference(&mut store, &preferences).unwrap();
        assert_eq!(store.i18n.language, Locale::De);
    }

    #[test]
    fn keeps_the_configured_default_when_lng_is_absent() {
        let (preferences, _) = PreferencesStore::in_memory(json!({}));
        let mut store = Store::default();
        hydrate_language_preference(&mut store, &preferences).unwrap();
        assert_eq!(store.i18n.language, Locale::En);
    }

    #[test]
    fn rejects_malformed_persisted_locales() {
        let (preferences, _) = PreferencesStore::in_memory(json!({"lng": "unsupported"}));
        assert_eq!(Store::hydrate(&preferences).unwrap_err(), "Unsupported locale: unsupported");
    }

    #[test]
    fn changes_and_persists_the_selected_locale() {
        let (parts, fakes) = fake_services(json!({}));
        let dispatcher = Dispatcher::new(Store::default(), parts);
        dispatcher.dispatch(|store, services| change_language_preference(store, services, Locale::Fr));
        assert_eq!(dispatcher.with_store(|store| store.i18n.language), Locale::Fr);
        assert_eq!(fakes.preferences.saves.borrow().last().unwrap()["lng"], json!("fr"));
    }
}
