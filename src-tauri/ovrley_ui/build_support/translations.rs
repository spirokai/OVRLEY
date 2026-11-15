//! Converts the React app's i18next locale files to the gettext catalogs Slint bundles.
//!
//! During the migration `app/src/i18n/locales/*-translation.json` stay the single source of the
//! translations, because the React app keeps editing them. `build.rs` converts them into
//! `<out>/<lang>/LC_MESSAGES/ovrley_ui.po` on every build; nothing generated is committed. When the
//! React app is gone (Phase 9), the `.po` files become the source and this converter is deleted.
//!
//! Each entry's context is the JSON key and its msgid the English text, as the `.slint` files write
//! them: `@tr("toolbar.pinDrawer" => "Pin drawer")`. The key is the context because the same English
//! word is translated differently under different keys ("Duration" for a lap and for a video).
//! i18next's `{{name}}` placeholders become Slint's positional `{0}`, `{1}`, numbered in the order
//! they appear in English; literal braces are doubled.

use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The gettext domain Slint looks up: the crate name.
pub const DOMAIN: &str = "ovrley_ui";

/// Flattens a locale file to `namespace.key.path` → text.
pub fn flatten(value: &Value) -> BTreeMap<String, String> {
    fn walk(prefix: &str, value: &Value, out: &mut BTreeMap<String, String>) {
        match value {
            Value::Object(map) => {
                for (key, value) in map {
                    let path = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
                    walk(&path, value, out);
                }
            }
            Value::String(text) => {
                out.insert(prefix.to_string(), text.clone());
            }
            other => panic!("{prefix}: translation values must be strings, found {other}"),
        }
    }
    let mut out = BTreeMap::new();
    walk("", value, &mut out);
    out
}

/// The `{{name}}` placeholders of a text, in order of first appearance.
pub fn placeholder_names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { break };
        let name = after[..end].trim().to_string();
        if !names.contains(&name) {
            names.push(name);
        }
        rest = &after[end + 2..];
    }
    names
}

/// Rewrites an i18next text in Slint's `@tr` format, numbering placeholders by `names`.
pub fn to_slint_format(text: &str, names: &[String]) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let Some(start) = rest.find(['{', '}']) else {
            out.push_str(rest);
            return Ok(out);
        };
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        if let Some(after) = tail.strip_prefix("{{") {
            if let Some(end) = after.find("}}") {
                let name = after[..end].trim();
                let index = names
                    .iter()
                    .position(|candidate| candidate == name)
                    .ok_or_else(|| format!("placeholder {{{{{name}}}}} is not in the English text"))?;
                write!(out, "{{{index}}}").unwrap();
                rest = &after[end + 2..];
                continue;
            }
        }
        // A literal brace.
        let brace = &tail[..1];
        out.push_str(brace);
        out.push_str(brace);
        rest = &tail[1..];
    }
}

fn po_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// One language's catalog. A key the locale lacks gets no entry, so it shows the English text, as
/// i18next falls back to English.
pub fn po_file(language: &str, english: &BTreeMap<String, String>, translated: &BTreeMap<String, String>) -> Result<String, String> {
    let mut po = String::new();
    writeln!(po, "msgid \"\"\nmsgstr \"\"\n\"Language: {language}\\n\"\n\"Content-Type: text/plain; charset=UTF-8\\n\"\n").unwrap();
    for (key, english_text) in english {
        let Some(text) = translated.get(key) else { continue };
        let names = placeholder_names(english_text);
        let msgid = to_slint_format(english_text, &names).map_err(|error| format!("en {key}: {error}"))?;
        let msgstr = to_slint_format(text, &names).map_err(|error| format!("{language} {key}: {error}"))?;
        writeln!(po, "msgctxt {}\nmsgid {}\nmsgstr {}\n", po_string(key), po_string(&msgid), po_string(&msgstr)).unwrap();
    }
    Ok(po)
}

/// Converts every `<lang>-translation.json` but English into `<out>/<lang>/LC_MESSAGES/ovrley_ui.po`.
/// Returns the files read, for `cargo:rerun-if-changed`.
pub fn convert(locales_dir: &Path, out_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let read = |path: &Path| -> Result<BTreeMap<String, String>, String> {
        let text = std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let value: Value = serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
        Ok(flatten(&value))
    };
    let english_path = locales_dir.join("en-translation.json");
    let english = read(&english_path)?;
    let mut inputs = vec![english_path];
    let mut entries: Vec<_> = std::fs::read_dir(locales_dir)
        .map_err(|error| format!("{}: {error}", locales_dir.display()))?
        .collect::<Result<_, _>>()
        .map_err(|error| error.to_string())?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(language) = name.strip_suffix("-translation.json") else { continue };
        if language == "en" {
            continue;
        }
        let path = entry.path();
        let po = po_file(language, &english, &read(&path)?)?;
        let target = out_dir.join(language).join("LC_MESSAGES");
        std::fs::create_dir_all(&target).map_err(|error| error.to_string())?;
        std::fs::write(target.join(format!("{DOMAIN}.po")), po).map_err(|error| error.to_string())?;
        inputs.push(path);
    }
    Ok(inputs)
}
