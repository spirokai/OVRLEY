//! The `.slint` files' `@tr` strings match the template in `locales/`, every template entry is
//! still used, and every catalog follows the template.

use regex::Regex;
use rspolib::TranslatedEntry;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Keys that Rust builds from a code: the part after the prefix is never written out.
const BUILT_KEY_PREFIXES: &[&str] = &["app-update.", "raster.errors.", "render-video.pathErrors.", "widgets.activityAttributes."];
const PLURAL_SUFFIXES: &[&str] = &["_zero", "_one", "_two", "_few", "_many", "_other"];

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn locales() -> PathBuf {
    manifest_dir().join("../../locales")
}

/// A catalog's entries: key → English text and translation.
fn entries(path: &Path) -> BTreeMap<String, (String, Option<String>)> {
    let catalog = rspolib::pofile(std::fs::read_to_string(path).unwrap().as_str()).unwrap();
    catalog
        .entries
        .iter()
        .map(|entry| (entry.msgctxt.clone().expect("an entry has a key"), (entry.msgid.clone(), entry.msgstr.clone().filter(|_| entry.translated()))))
        .collect()
}

fn template() -> BTreeMap<String, String> {
    entries(&locales().join("ovrley.pot")).into_iter().map(|(key, (english, _))| (key, english)).collect()
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
fn every_tr_has_a_template_entry_with_its_english_text() {
    let english = template();
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
            let expected = english.get(key).unwrap_or_else(|| panic!("{}: {key} is not in locales/ovrley.pot", file.display()));
            assert_eq!(&source, expected, "{}: the English text of {key} changed", file.display());
            checked += 1;
        }
    }
    assert!(checked > 0, "no @tr strings found");
}

#[test]
fn every_template_entry_is_still_used() {
    fn sources(dir: &Path, quoted: &Regex, out: &mut BTreeSet<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                sources(&path, quoted, out);
            } else if path.extension().is_some_and(|extension| extension == "rs" || extension == "slint" || extension == "json") {
                let text = std::fs::read_to_string(&path).unwrap();
                out.extend(quoted.captures_iter(&text).map(|captures| captures[1].to_string()));
            }
        }
    }
    let quoted = Regex::new(r#""([A-Za-z0-9_.-]+)""#).unwrap();
    let mut used = BTreeSet::new();
    let crates = manifest_dir().join("..");
    for krate in ["ovrley_app", "ovrley_core", "ovrley_mcp", "ovrley_services", "ovrley_ui", "ovrley_widgets"] {
        sources(&crates.join(krate).join("src"), &quoted, &mut used);
    }
    for dir in [manifest_dir().join("ui"), manifest_dir().join("tests"), manifest_dir().join("../../assets")] {
        sources(&dir, &quoted, &mut used);
    }
    let unused: Vec<String> = template()
        .into_keys()
        .filter(|key| {
            let base = PLURAL_SUFFIXES.iter().find_map(|suffix| key.strip_suffix(suffix)).unwrap_or(key);
            !used.contains(key) && !used.contains(base) && !BUILT_KEY_PREFIXES.iter().any(|prefix| key.starts_with(prefix))
        })
        .collect();
    assert!(unused.is_empty(), "nothing uses these keys of locales/ovrley.pot: {unused:?}");
}

#[test]
fn every_catalog_follows_the_template() {
    let english = template();
    let placeholder = Regex::new(r"\{\{[^{}]+\}\}|\{[0-9]+\}").unwrap();
    let placeholders = |text: &str| placeholder.find_iter(text).map(|found| found.as_str().to_string()).collect::<BTreeSet<_>>();
    for entry in std::fs::read_dir(locales()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|extension| extension != "po") {
            continue;
        }
        for (key, (msgid, translated)) in entries(&path) {
            assert_eq!(Some(&msgid), english.get(&key), "{}: {key} differs from the template", path.display());
            if let Some(translated) = translated {
                assert!(placeholders(&translated).is_subset(&placeholders(&msgid)), "{}: {key} has a placeholder English lacks", path.display());
            }
        }
    }
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
