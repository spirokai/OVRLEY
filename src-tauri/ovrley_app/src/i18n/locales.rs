//! Port of `i18n/locales.js`: the supported locales, in the language select's order, with their
//! native labels.

use serde::{Deserialize, Serialize};
use serde_json::Value;

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

/// `DEFAULT_LOCALE`, also i18next's fallback language.
pub const DEFAULT_LOCALE: Locale = Locale::En;

/// `locales`: every locale with its native label, in order.
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

    /// `requireLocale`: the one point where an unknown code fails.
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
    use serde_json::json;

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
