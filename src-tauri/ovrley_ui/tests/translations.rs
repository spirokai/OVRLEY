//! The `.slint` files' `@tr` strings match the React app's locale files, which stay the source of
//! the translations during the migration, and the JSON-to-gettext conversion keeps i18next's rules.

// `convert` and `DOMAIN` serve build.rs only.
#[allow(dead_code)]
#[path = "../build_support/translations.rs"]
mod translations;

use regex::Regex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Keys React translates with a code default (`t(key, default)`) that no locale file has. They
/// show the `@tr` text in every language, as i18next shows the default.
const CODE_DEFAULT_KEYS: &[&str] = &["components.gradient"];

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn english() -> BTreeMap<String, String> {
    let path = manifest_dir().join("../../app/src/i18n/locales/en-translation.json");
    translations::flatten(&serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap())
}

fn slint_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            slint_files(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "slint") {
            out.push(path);
        }
    }
}

#[test]
fn every_tr_names_a_json_key_and_its_english_text() {
    let english = english();
    let tr = Regex::new(r#"@tr\(\s*"((?:[^"\\]|\\.)*)"\s*(=>\s*"((?:[^"\\]|\\.)*)")?"#).unwrap();
    let mut files = Vec::new();
    slint_files(&manifest_dir().join("ui"), &mut files);
    slint_files(&manifest_dir().join("tests"), &mut files);
    let mut checked = 0;
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        for captures in tr.captures_iter(&text) {
            let (Some(key), Some(source)) = (captures.get(1), captures.get(3)) else {
                panic!("{}: `{}` needs a key context: @tr(\"namespace.key\" => \"English text\")", file.display(), &captures[0]);
            };
            let (key, source) = (key.as_str(), source.as_str().replace("\\\"", "\""));
            match english.get(key) {
                Some(english_text) => {
                    let expected = translations::to_slint_format(english_text, &translations::placeholder_names(english_text)).unwrap();
                    assert_eq!(source, expected, "{}: the English text of {key} changed", file.display());
                }
                None => assert!(CODE_DEFAULT_KEYS.contains(&key), "{}: {key} is not in en-translation.json", file.display()),
            }
            checked += 1;
        }
    }
    assert!(checked > 0, "no @tr strings found");
}

#[test]
fn placeholders_become_positional_in_english_order() {
    let names = translations::placeholder_names("{{count}} of {{total}} ({{count}})");
    assert_eq!(names, ["count", "total"]);
    assert_eq!(translations::to_slint_format("{{total}} sur {{count}}", &names).unwrap(), "{1} sur {0}");
    assert_eq!(translations::to_slint_format("a {b} c", &[]).unwrap(), "a {{b}} c");
    assert!(translations::to_slint_format("{{other}}", &names).is_err());
}

#[test]
fn a_locale_lacking_a_key_falls_back_to_english() {
    let english = BTreeMap::from([("a.one".to_string(), "One".to_string()), ("a.two".to_string(), "Two \"{{n}}\"".to_string())]);
    let german = BTreeMap::from([("a.two".to_string(), "Zwei \"{{n}}\"".to_string())]);
    let po = translations::po_file("de", &english, &german).unwrap();
    assert!(!po.contains("a.one"), "no entry, so Slint shows the English text");
    assert!(po.contains("msgctxt \"a.two\"\nmsgid \"Two \\\"{0}\\\"\"\nmsgstr \"Zwei \\\"{0}\\\"\""), "{po}");
}

#[test]
fn the_bundled_translation_switches_at_runtime() {
    use slint::ComponentHandle;
    i_slint_backend_testing::init_no_event_loop();
    let window = ovrley_ui::AppWindow::new().unwrap();
    for (locale, _) in ovrley_app::i18n::LOCALES {
        slint::select_bundled_translation(locale.code()).unwrap_or_else(|error| panic!("{}: {error:?}", locale.code()));
    }
    drop(window.as_weak());
}
