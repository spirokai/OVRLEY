//! Port of the catalog part of `lib/fonts.js` (D16): font selections, the font select's
//! options, and the font ids a document references. Label typography (weight axes, italic)
//! arrives with the text widget editor in Phase 5.

use ovrley_core::fonts::FontFamily;
use serde_json::{Map, Value};

const FONT_EXTENSIONS: [&str; 6] = ["ttf", "otf", "ttc", "woff", "woff2", "fon"];

/// `getFontFamilyName`: a font identity without its font-file extension.
pub fn font_family_name(value: &str) -> String {
    let trimmed = value.trim();
    if let Some((stem, extension)) = trimmed.rsplit_once('.') {
        if FONT_EXTENSIONS.iter().any(|known| extension.eq_ignore_ascii_case(known)) {
            return stem.to_string();
        }
    }
    trimmed.to_string()
}

/// `createFontSelection`: `font` and its `font_family`.
pub fn font_selection(value: &str) -> Map<String, Value> {
    let mut selection = Map::new();
    selection.insert("font".into(), Value::String(value.to_string()));
    selection.insert("font_family".into(), Value::String(font_family_name(value)));
    selection
}

/// One option of the font select.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontOption {
    pub id: String,
    pub name: String,
}

impl From<&FontFamily> for FontOption {
    fn from(family: &FontFamily) -> Self {
        Self {
            id: family.id.clone(),
            name: family.name.clone(),
        }
    }
}

/// `getFontSelectOptions`: the bundled options (with an unavailable saved selection first) and the
/// system families whose names the bundled list does not already have.
pub fn font_select_options(value: &str, recommended: &[FontOption], system: &[FontOption]) -> (Vec<FontOption>, Vec<FontOption>) {
    let mut known = value.is_empty();
    let mut names = std::collections::HashSet::new();
    for font in recommended {
        known |= font.id == value;
        names.insert(font.name.as_str());
    }
    let mut filtered = Vec::new();
    for font in system {
        known |= font.id == value;
        if !names.contains(font.name.as_str()) {
            filtered.push(font.clone());
        }
    }
    let recommended_options = if known {
        recommended.to_vec()
    } else {
        let name = font_family_name(value);
        let mut options = vec![FontOption {
            id: value.to_string(),
            name: if name.is_empty() { "Custom font".into() } else { name },
        }];
        options.extend(recommended.iter().cloned());
        options
    };
    (recommended_options, filtered)
}

const FONT_KEYS: [&str; 5] = ["font", "label_font", "min_max_label_font", "font_text", "font_values"];

/// `collectFontIds`: every font id a value references, inactive variants included, in first-seen
/// order.
pub fn collect_font_ids(value: &Value) -> Vec<String> {
    fn walk(value: &Value, ids: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                for (key, item) in map {
                    match item {
                        Value::String(id) if FONT_KEYS.contains(&key.as_str()) => {
                            if !ids.contains(id) {
                                ids.push(id.clone());
                            }
                        }
                        Value::Object(_) | Value::Array(_) => walk(item, ids),
                        _ => {}
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|item| walk(item, ids)),
            _ => {}
        }
    }
    let mut ids = Vec::new();
    walk(value, &mut ids);
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn option(id: &str, name: &str) -> FontOption {
        FontOption { id: id.into(), name: name.into() }
    }

    #[test]
    fn family_names_drop_font_extensions() {
        assert_eq!(font_family_name(" Inter.TTF "), "Inter");
        assert_eq!(font_family_name("Roboto Mono"), "Roboto Mono");
        assert_eq!(font_selection("Teko.otf"), json!({"font": "Teko.otf", "font_family": "Teko"}).as_object().unwrap().clone());
    }

    #[test]
    fn select_options_keep_an_unavailable_selection_and_drop_duplicate_system_names() {
        let recommended = [option("Inter.ttf", "Inter")];
        let system = [option("sys:Inter", "Inter"), option("sys:Arial", "Arial")];
        let (bundled, others) = font_select_options("Gone.ttf", &recommended, &system);
        assert_eq!(bundled, [option("Gone.ttf", "Gone"), option("Inter.ttf", "Inter")]);
        assert_eq!(others, [option("sys:Arial", "Arial")]);
        let (bundled, _) = font_select_options("sys:Arial", &recommended, &system);
        assert_eq!(bundled, recommended);
    }

    #[test]
    fn collects_every_referenced_font_once() {
        let value = json!({"globalDefaults": {"font_text": "A.ttf", "font_values": "B.ttf"},
            "values": [{"font": "B.ttf", "display_variants": {"linear": {"min_max_label_font": "C.ttf"}}}], "labels": [{"font": null}]});
        assert_eq!(collect_font_ids(&value), ["A.ttf", "B.ttf", "C.ttf"]);
    }
}
