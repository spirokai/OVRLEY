//! Generates the i18n string code from `app/src/i18n/locales/en-translation.json` (plan D6).
//!
//! Shared by `examples/generate_strings.rs`, which writes the files, and `tests/generated_strings.rs`,
//! which fails when the committed files are stale. Three outputs:
//! - `ovrley_app/src/i18n/keys.rs`: one Rust constant per key, plus the code defaults of keys the
//!   React code uses with a default but that no locale file has;
//! - `ovrley_ui/ui/i18n/strings.slint`: one Slint global per top-level namespace, one property per
//!   key in kebab-case, each defaulting to the English text;
//! - `ovrley_ui/src/i18n_strings.rs`: the function that writes a whole locale into those globals.

use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Keys that React translates with a code default (`t(key, default)`) but that are missing from
/// `en-translation.json`. i18next shows the default; so does the port. Inventoried from `app/src`
/// on 2026-11-15; extend it when the React code adds one.
pub const CODE_DEFAULTS: &[(&str, &str)] = &[
    ("components.gradient", "Gradient"),
    ("videoSync.mapStyle", "Map style"),
    ("widget-editor.contentAlignment", "Content Alignment"),
    ("widget-editor.dimensions", "Dimensions"),
    ("widget-editor.horizontalAnchor", "Horizontal Anchor"),
];

/// Phase 1 named the components' global before the generator existed; the name is kept.
const GLOBAL_NAME_OVERRIDES: &[(&str, &str)] = &[("components", "ComponentStrings")];

pub struct GeneratedFile {
    pub path: PathBuf,
    pub contents: String,
}

/// The repository root, from this crate's manifest directory (`src-tauri/ovrley_ui`).
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

pub fn english_catalog_path(repo_root: &Path) -> PathBuf {
    repo_root.join("app/src/i18n/locales/en-translation.json")
}

struct Key {
    /// `app-shell.close`, as React writes it.
    full: String,
    /// `close`: the Slint property within its namespace global.
    property: String,
    /// `APP_SHELL_CLOSE`.
    constant: String,
    english: String,
}

/// `pinDrawer` → `pin-drawer`; `blueColorComponent0255` → `blue-color-component-0255`; nested keys
/// are joined with `-`. A name that would start with a digit gets a `k-` prefix, since Slint
/// identifiers cannot.
pub fn kebab(path: &str) -> String {
    let mut out = String::new();
    let mut previous: Option<char> = None;
    for c in path.chars() {
        if c == '.' || c == '_' || c == '-' {
            out.push('-');
            previous = None;
            continue;
        }
        if let Some(p) = previous {
            let boundary = (p.is_ascii_lowercase() && c.is_ascii_uppercase())
                || (p.is_ascii_alphabetic() && c.is_ascii_digit())
                || (p.is_ascii_digit() && c.is_ascii_alphabetic());
            if boundary {
                out.push('-');
            }
        }
        out.push(c.to_ascii_lowercase());
        previous = Some(c);
    }
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert_str(0, "k-");
    }
    out
}

fn pascal(namespace: &str) -> String {
    kebab(namespace)
        .split('-')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + chars.as_str()
            })
        })
        .collect()
}

fn global_name(namespace: &str) -> String {
    GLOBAL_NAME_OVERRIDES
        .iter()
        .find(|(name, _)| *name == namespace)
        .map_or_else(
            || format!("{}Strings", pascal(namespace)),
            |(_, global)| global.to_string(),
        )
}

fn flatten(prefix: &str, value: &Value, out: &mut Vec<(String, String)>) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten(&path, value, out);
            }
        }
        Value::String(text) => out.push((prefix.to_string(), text.clone())),
        other => panic!("{prefix}: translation values must be strings, found {other}"),
    }
}

fn slint_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Every key, grouped by namespace in file order: the English catalog's keys, then the code
/// defaults the catalog lacks.
fn keys_by_namespace(english_json: &str) -> BTreeMap<String, Vec<Key>> {
    let catalog: Value =
        serde_json::from_str(english_json).expect("en-translation.json is valid JSON");
    let mut entries = Vec::new();
    flatten("", &catalog, &mut entries);
    // Sorted, so the output does not depend on whether a build enables serde_json's
    // `preserve_order` (feature unification decides that, not this crate).
    entries.sort();
    for (key, default) in CODE_DEFAULTS {
        assert!(
            !entries.iter().any(|(existing, _)| existing == key),
            "{key} is in en-translation.json now; remove it from CODE_DEFAULTS"
        );
        entries.push((key.to_string(), default.to_string()));
    }

    let mut namespaces: BTreeMap<String, Vec<Key>> = BTreeMap::new();
    for (full, english) in entries {
        let (namespace, rest) = full
            .split_once('.')
            .unwrap_or_else(|| panic!("{full}: top-level strings need a namespace"));
        let property = kebab(rest);
        let keys = namespaces.entry(namespace.to_string()).or_default();
        if let Some(clash) = keys.iter().find(|key| key.property == property) {
            panic!(
                "{full} and {} both map to the Slint property `{property}`",
                clash.full
            );
        }
        keys.push(Key {
            constant: format!("{}_{}", kebab(namespace), property)
                .replace('-', "_")
                .to_ascii_uppercase(),
            full,
            property,
            english,
        });
    }
    namespaces
}

const HEADER: &str = "Generated by `cargo run -p ovrley_ui --example generate_strings` from\n\
                      app/src/i18n/locales/en-translation.json. Do not edit by hand.";

fn comment(prefix: &str) -> String {
    HEADER
        .lines()
        .map(|line| format!("{prefix} {line}\n"))
        .collect()
}

/// Generates the three files from the English catalog's text.
pub fn generate(repo_root: &Path, english_json: &str) -> Vec<GeneratedFile> {
    let namespaces = keys_by_namespace(english_json);

    let mut keys_rs = comment("//!");
    keys_rs.push_str("//!\n//! One constant per translation key. Rust code looks keys up through these, never by string.\n\n");
    for keys in namespaces.values() {
        for key in keys {
            writeln!(
                keys_rs,
                "pub const {}: &str = {:?};",
                key.constant, key.full
            )
            .unwrap();
        }
    }
    keys_rs.push_str("\n/// Keys React translates with a code default that no locale file has (i18next shows the default).\n");
    keys_rs.push_str("pub const CODE_DEFAULTS: &[(&str, &str)] = &[\n");
    for (key, default) in CODE_DEFAULTS {
        writeln!(keys_rs, "    ({key:?}, {default:?}),").unwrap();
    }
    keys_rs.push_str("];\n");

    let mut slint = comment("//");
    slint.push_str("//\n// One global per top-level namespace. Each property defaults to the English text; the app writes\n");
    slint.push_str("// the active locale into them (`src/i18n_strings.rs`). Interpolated strings keep their `{{name}}`\n");
    slint.push_str("// placeholders and are filled with `replace-all`.\n");
    for (namespace, keys) in &namespaces {
        writeln!(slint, "\nexport global {} {{", global_name(namespace)).unwrap();
        for key in keys {
            writeln!(
                slint,
                "    in-out property <string> {}: {};",
                key.property,
                slint_string(&key.english)
            )
            .unwrap();
        }
        slint.push_str("}\n");
    }

    let mut rust = comment("//!");
    rust.push_str(
        "//!\n//! Writes a whole locale into the string globals of `ui/i18n/strings.slint`.\n\n",
    );
    rust.push_str("use crate::*;\nuse ovrley_app::i18n::{keys, Catalog, Locale};\nuse slint::{ComponentHandle, SharedString};\n\n");
    rust.push_str("/// Writes `locale`'s text, with i18next's fallbacks, into every string global of `window`.\n");
    rust.push_str("pub fn apply_locale(window: &AppWindow, catalog: &Catalog, locale: Locale) {\n");
    rust.push_str("    let t = |key: &'static str| SharedString::from(catalog.t(locale, key));\n");
    for (namespace, keys) in &namespaces {
        let global = global_name(namespace);
        writeln!(rust, "    let strings = window.global::<{global}>();").unwrap();
        for key in keys {
            writeln!(
                rust,
                "    strings.set_{}(t(keys::{}));",
                key.property.replace('-', "_"),
                key.constant
            )
            .unwrap();
        }
    }
    rust.push_str("}\n");

    let ui = repo_root.join("src-tauri/ovrley_ui");
    let index_path = ui.join("ui/index.slint");
    let index = std::fs::read_to_string(&index_path).expect("read ui/index.slint");
    let globals: Vec<String> = namespaces.keys().map(|namespace| global_name(namespace)).collect();
    let index = replace_marked_region(
        &index,
        "// <generated-string-globals>",
        "// </generated-string-globals>",
        &format!("export {{ {} }} from \"i18n/strings.slint\";\n", globals.join(", ")),
    );
    vec![
        GeneratedFile {
            path: index_path,
            contents: index,
        },
        GeneratedFile {
            path: repo_root.join("src-tauri/ovrley_app/src/i18n/keys.rs"),
            contents: keys_rs,
        },
        GeneratedFile {
            path: ui.join("ui/i18n/strings.slint"),
            contents: slint,
        },
        GeneratedFile {
            path: ui.join("src/i18n_strings.rs"),
            contents: rust,
        },
    ]
}

/// Replaces the lines between the line that starts with `start` and the line `end`.
fn replace_marked_region(text: &str, start: &str, end: &str, region: &str) -> String {
    let start_line = text.find(start).unwrap_or_else(|| panic!("marker {start} is missing"));
    let region_start = start_line + text[start_line..].find('\n').expect("the start marker ends its line") + 1;
    let region_end = region_start + text[region_start..].find(end).unwrap_or_else(|| panic!("marker {end} is missing"));
    format!("{}{region}{}", &text[..region_start], &text[region_end..])
}
