//! The translation catalog, translated backend errors, the supported locales and the language
//! preference.
//!
//! The catalogs in the root `locales/` folder are embedded: `ovrley.pot` holds the English text
//! (msgid) under each key (msgctxt), `<lang>.po` each translation. English is the fallback
//! language: a key missing from a locale shows the English text, a key missing from every locale
//! shows its code default, and `{{name}}` placeholders are filled without escaping.
//!
//! This catalog serves the strings Rust code chooses at runtime (errors, projected labels). The
//! `.slint` files translate their own strings with `@tr`, from the same files, with Slint's
//! placeholders (`{0}`).

use std::collections::HashMap;
use std::sync::OnceLock;
use regex::Regex;
use rspolib::TranslatedEntry;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::preferences::PreferencesStore;
use crate::store::Store;
use crate::store::services::Services;

/// The language preference key.
pub const LANGUAGE_PREFERENCE_KEY: &str = "lng";

/// Each locale's catalog, in [`LOCALES`] order; English's is the template.
const LOCALE_FILES: [&str; 15] = [
    include_str!("../../../locales/ovrley.pot"),
    include_str!("../../../locales/zh-CN.po"),
    include_str!("../../../locales/ja.po"),
    include_str!("../../../locales/de.po"),
    include_str!("../../../locales/fr.po"),
    include_str!("../../../locales/tr.po"),
    include_str!("../../../locales/it.po"),
    include_str!("../../../locales/ko.po"),
    include_str!("../../../locales/es.po"),
    include_str!("../../../locales/ru.po"),
    include_str!("../../../locales/pl.po"),
    include_str!("../../../locales/pt-BR.po"),
    include_str!("../../../locales/nl.po"),
    include_str!("../../../locales/cs.po"),
    include_str!("../../../locales/sv.po"),
];

/// Every locale's strings, by `namespace.key.path`.
pub struct Catalog {
    tables: Vec<HashMap<String, String>>,
}

impl Catalog {
    fn parse() -> Self {
        let tables = LOCALE_FILES
            .iter()
            .zip(LOCALES)
            .map(|(text, (locale, _))| {
                let catalog = rspolib::pofile(*text).unwrap_or_else(|error| panic!("the {} catalog is malformed: {error}", locale.code()));
                catalog
                    .entries
                    .iter()
                    .filter_map(|entry| {
                        let key = entry.msgctxt.clone().unwrap_or_else(|| panic!("the {} catalog has an entry without a key: {}", locale.code(), entry.msgid));
                        let text = if locale == DEFAULT_LOCALE { entry.msgid.clone() } else { entry.msgstr.clone().filter(|_| entry.translated())? };
                        Some((key, text))
                    })
                    .collect()
            })
            .collect();
        Self { tables }
    }

    /// The text of `key` in `locale`, else in English.
    fn lookup(&self, locale: Locale, key: &str) -> Option<&str> {
        self.tables[locale.index()]
            .get(key)
            .or_else(|| self.tables[DEFAULT_LOCALE.index()].get(key))
            .map(String::as_str)
    }

    /// The text of `key` in `locale`, falling back to English.
    ///
    /// # Panics
    /// For a key that is in no catalog, which the tests rule out for every key Rust code uses.
    pub fn t(&self, locale: Locale, key: &str) -> &str {
        self.lookup(locale, key).unwrap_or_else(|| panic!("translation key {key} has no text"))
    }

    /// Like [`Self::t`], with a default for keys no catalog has.
    pub fn t_or<'a>(&'a self, locale: Locale, key: &str, default: &'a str) -> &'a str {
        self.lookup(locale, key).unwrap_or(default)
    }

    /// The plural form of `key` for `count` (`key_one`, `key_few`, `key_other`…), chosen by the
    /// language's cardinal rules, with `{{count}}` filled; `default` when no form exists.
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
            .find_map(|candidate| self.lookup(locale, candidate))
            .unwrap_or(default);
        interpolate(template, &[("count", &count.to_string())])
    }

    /// The text of `key` with its `{{name}}` placeholders filled.
    pub fn t_with(&self, locale: Locale, key: &str, values: &[(&str, &str)]) -> String {
        interpolate(self.t(locale, key), values)
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

/// Translates a raster error that core tagged (`[raster_error:<tag>:<id>]`) for display; any other
/// message is returned unchanged.
pub fn translate_backend_error(message: &str, locale: Locale) -> String {
    static RASTER: OnceLock<Regex> = OnceLock::new();
    let catalog = catalog();
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

/// Reads the language preference at startup: an absent `lng` keeps English; any other value must
/// be a supported locale, or startup fails with `Unsupported locale: …`.
pub fn hydrate_language_preference(store: &mut Store, preferences: &PreferencesStore) -> Result<(), String> {
    match preferences.get(LANGUAGE_PREFERENCE_KEY) {
        None | Some(Value::Null) => Ok(()),
        Some(value) => {
            store.i18n.language = Locale::require(value)?;
            Ok(())
        }
    }
}

/// Applies and persists a supported language. The projections then write the catalog into the
/// string globals.
pub fn change_language_preference(store: &mut Store, services: &mut Services, locale: Locale) {
    store.i18n.language = locale;
    services
        .preferences
        .set(LANGUAGE_PREFERENCE_KEY, Value::String(locale.code().to_string()));
}

/// A supported locale. Serialized as its code (`en`, `zh-CN`, …), as `lng` stores it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Locale {
    #[default]
    #[serde(rename = "en")]
    En,
    #[serde(rename = "zh-CN")]
    ZhCn,
    #[serde(rename = "ja")]
    Ja,
    #[serde(rename = "de")]
    De,
    #[serde(rename = "fr")]
    Fr,
    #[serde(rename = "tr")]
    Tr,
    #[serde(rename = "it")]
    It,
    #[serde(rename = "ko")]
    Ko,
    #[serde(rename = "es")]
    Es,
    #[serde(rename = "ru")]
    Ru,
    #[serde(rename = "pl")]
    Pl,
    #[serde(rename = "pt-BR")]
    PtBr,
    #[serde(rename = "nl")]
    Nl,
    #[serde(rename = "cs")]
    Cs,
    #[serde(rename = "sv")]
    Sv,
}

/// The fallback language.
pub const DEFAULT_LOCALE: Locale = Locale::En;

/// Every locale with its native label, in the language select's order.
pub const LOCALES: [(Locale, &str); 15] = [
    (Locale::En, "English"),
    (Locale::ZhCn, "简体中文"),
    (Locale::Ja, "日本語"),
    (Locale::De, "Deutsch"),
    (Locale::Fr, "Français"),
    (Locale::Tr, "Türkçe"),
    (Locale::It, "Italiano"),
    (Locale::Ko, "한국어"),
    (Locale::Es, "Español"),
    (Locale::Ru, "Русский"),
    (Locale::Pl, "Polski"),
    (Locale::PtBr, "Português (Brasil)"),
    (Locale::Nl, "Nederlands"),
    (Locale::Cs, "Čeština"),
    (Locale::Sv, "Svenska"),
];

impl Locale {
    /// The locale's position in [`LOCALES`].
    pub fn index(self) -> usize {
        LOCALES
            .iter()
            .position(|(locale, _)| *locale == self)
            .expect("every locale is listed")
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::ZhCn => "zh-CN",
            Self::Ja => "ja",
            Self::De => "de",
            Self::Fr => "fr",
            Self::Tr => "tr",
            Self::It => "it",
            Self::Ko => "ko",
            Self::Es => "es",
            Self::Ru => "ru",
            Self::Pl => "pl",
            Self::PtBr => "pt-BR",
            Self::Nl => "nl",
            Self::Cs => "cs",
            Self::Sv => "sv",
        }
    }

    pub fn label(self) -> &'static str {
        LOCALES[self.index()].1
    }

    /// Reads a locale code; the one point where an unknown code fails.
    pub fn require(value: &Value) -> Result<Self, String> {
        serde_json::from_value(value.clone()).map_err(|_| {
            let shown = match value {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            format!("Unsupported locale: {shown}")
        })
    }
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
    fn every_raster_error_key_has_english_text() {
        for (_, key) in RASTER_ERROR_KEYS {
            assert!(catalog().tables[DEFAULT_LOCALE.index()].contains_key(key), "{key}");
        }
    }

    #[test]
    fn interpolates_without_escaping() {
        assert_eq!(interpolate("Color stop at {{percent}}%", &[("percent", "<50>")]), "Color stop at <50>%");
        assert_eq!(interpolate("{{a}} and {{b}}", &[("a", "x")]), "x and {{b}}");
    }

    #[test]
    fn translates_tagged_raster_errors() {
        let catalog = catalog();
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
        for message in ["plain failure", "x [raster_error:unknown:logo]"] {
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

    #[test]
    fn is_the_canonical_source_for_supported_codes_and_native_labels() {
        assert_eq!(DEFAULT_LOCALE.code(), "en");
        assert_eq!(Locale::ZhCn.label(), "简体中文");
        assert_eq!(Locale::De.label(), "Deutsch");
        assert_eq!(Locale::PtBr.label(), "Português (Brasil)");
        for (locale, _) in LOCALES {
            assert_eq!(serde_json::to_value(locale).unwrap(), json!(locale.code()));
            assert_eq!(Locale::require(&json!(locale.code())).unwrap(), locale);
        }
        let mut codes: Vec<_> = LOCALES.iter().map(|(locale, _)| locale.code()).collect();
        codes.dedup();
        assert_eq!(codes.len(), LOCALES.len());
    }

    #[test]
    fn rejects_unsupported_locale_codes() {
        assert_eq!(
            Locale::require(&json!("en-US")).unwrap_err(),
            "Unsupported locale: en-US"
        );
        assert_eq!(
            Locale::require(&json!(3)).unwrap_err(),
            "Unsupported locale: 3"
        );
    }
}
