//! What the widget benchmarks take from the editor: its theme and the HUD template's defaults.

use ovrley_app::store::template::ThemeColors;
use serde_json::{Map, Value};
use std::path::Path;

/// The `Theme` global's colours (crates/ovrley_ui/ui/theme/theme.slint), which a benchmark has no
/// window to read from.
pub fn theme() -> ThemeColors {
    ThemeColors {
        ice: "#86aaaa".into(),
        aqua: "#40e0d0".into(),
        teal: "#005b5b".into(),
        accent: "#ad5013".into(),
    }
}

/// The futuristic HUD's scene, and its global defaults over the app's.
pub fn hud_template(root: &Path) -> Result<(Map<String, Value>, Map<String, Value>), String> {
    let path = root.join("templates/futuristic-hud.json");
    let bytes = std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let template: Value = serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))?;
    let field = |pointer: &str| {
        template
            .pointer(pointer)
            .and_then(Value::as_object)
            .cloned()
            .ok_or_else(|| format!("{}: {pointer} must be an object", path.display()))
    };
    let mut globals = ovrley_core::template::template_manifest().globals.clone();
    globals.extend(field("/settings/globalDefaults")?);
    Ok((field("/config/scene")?, globals))
}
