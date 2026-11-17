//! How an inspector edit becomes a widget patch (D1, D2). A cell's field id is its JSON path; the
//! plain rule patches that path, as `useDisplayVariantUpdater` does for a display variant (geometry
//! keys through `build_frame_geometry_update`). The named rules port the editors' handlers that
//! patch more than their own field, each beside the renderer's own geometry where it has one.

use super::blocks::{Cell, CellKind, Rule};
use ovrley_widgets::model::document::{
    build_display_type_change_update, build_frame_geometry_update, build_metric_unit_update,
    number, UnitSystem,
};
use serde_json::{Map, Value};

/// What a control reported.
#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    Number(f64),
    Text(String),
    Flag(bool),
    /// A field's reset button.
    Clear,
    /// A button.
    Press,
    /// A gradient picker's stops.
    Stops(Vec<super::blocks::StopView>),
}

/// The editors' named rules.
#[derive(Clone, Debug, PartialEq)]
pub enum Named {
    /// `changeFont`: `font` and its `font_family`.
    Font,
    /// `handleDisplayTypeChange` (`buildDisplayTypeChangeUpdate`).
    DisplayType,
    /// `handleUnitChange`: altitude widgets convert their starting altitude.
    MetricUnit,
    /// The inspector's reset button (`resetWidget`): fresh defaults for the type.
    Reset,
}

/// What an edit asks the editor to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Patch {
    /// A data patch.
    Data(Map<String, Value>),
    /// `setNumericField` with the raw text.
    Numeric {
        key: String,
        raw: String,
        options: super::NumericFieldOptions,
    },
    /// `resetWidget`.
    Reset,
}

/// The context a rule reads: the widget's data as the inspector shows it (drafts applied), its type
/// and the unit system.
pub struct RuleContext<'a> {
    pub data: &'a Map<String, Value>,
    pub kind: &'a str,
    pub unit_system: UnitSystem,
}

const GEOMETRY_KEYS: [&str; 3] = ["width", "height", "rotation"];

/// A patch that sets `path` to `value`: `{a: {b: value}}` for `a.b`. A display variant's path
/// follows `useDisplayVariantUpdater`: its geometry keys go through the display's geometry
/// contract, so the top-level frame and the variant stay in sync.
pub fn path_patch(data: &Map<String, Value>, path: &str, value: Value) -> Map<String, Value> {
    let parts: Vec<&str> = path.split('.').collect();
    if let ["display_variants", display, key] = parts.as_slice() {
        let mut updates = Map::new();
        updates.insert((*key).to_string(), value);
        return variant_patch(data, display, updates);
    }
    let mut patch = Map::new();
    let mut value = value;
    for part in parts.iter().skip(1).rev() {
        let mut map = Map::new();
        map.insert((*part).to_string(), value);
        value = Value::Object(map);
    }
    patch.insert(parts[0].to_string(), value);
    patch
}

/// `useDisplayVariantUpdater`'s patch for `updates` to the `display` variant.
pub fn variant_patch(
    data: &Map<String, Value>,
    display: &str,
    updates: Map<String, Value>,
) -> Map<String, Value> {
    let geometry: Map<String, Value> = updates
        .iter()
        .filter(|(key, _)| GEOMETRY_KEYS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let mut patch = if geometry.is_empty() {
        Map::new()
    } else {
        build_frame_geometry_update(data, &geometry)
    };
    let mut variants = patch
        .remove("display_variants")
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    let mut variant = variants
        .remove(display)
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    variant.extend(updates);
    variants.insert(display.to_string(), Value::Object(variant));
    patch.insert("display_variants".into(), Value::Object(variants));
    patch
}

fn text_input(cell: &Cell, input: &Input) -> Result<String, String> {
    match input {
        Input::Text(text) => Ok(text.clone()),
        other => Err(format!("{} takes text, not {other:?}", cell.id)),
    }
}

fn plain_value(cell: &Cell, input: &Input) -> Result<Value, String> {
    Ok(match (input, &cell.kind) {
        (Input::Number(value), _) => number(*value),
        (Input::Flag(value), _) => Value::Bool(*value),
        (Input::Clear, _) => Value::Null,
        (Input::Text(text), _) => Value::String(text.clone()),
        (Input::Stops(stops), _) => Value::Array(stops.iter().map(|stop| serde_json::json!({"offset": number(stop.offset), "color": stop.color})).collect()),
        (Input::Press, _) => return Err(format!("{} is not a button", cell.id)),
    })
}

/// The patch an edit of `cell` makes.
pub fn patch(cell: &Cell, input: Input, context: &RuleContext) -> Result<Patch, String> {
    let data = context.data;
    let patch = match &cell.rule {
        Rule::None => return Err(format!("{} is not editable", cell.id)),
        Rule::Field => path_patch(data, &cell.id, plain_value(cell, &input)?),
        Rule::Scaled(divisor) => match input {
            Input::Number(value) => path_patch(data, &cell.id, number(value / divisor)),
            other => return Err(format!("{} takes a number, not {other:?}", cell.id)),
        },
        Rule::NumberChoice => {
            let text = text_input(cell, &input)?;
            let value: f64 = text
                .parse()
                .map_err(|_| format!("{}: {text} is not a number", cell.id))?;
            path_patch(data, &cell.id, number(value))
        }
        Rule::Numeric(options) => match input {
            Input::Text(raw) => {
                return Ok(Patch::Numeric {
                    key: cell.id.clone(),
                    raw,
                    options: *options,
                })
            }
            Input::Clear => path_patch(data, &cell.id, Value::Null),
            other => return Err(format!("{} takes text, not {other:?}", cell.id)),
        },
        Rule::Named(Named::Reset) => return Ok(Patch::Reset),
        Rule::Named(named) => named_patch(named, cell, input, context)?,
    };
    Ok(Patch::Data(patch))
}

fn named_patch(
    named: &Named,
    cell: &Cell,
    input: Input,
    context: &RuleContext,
) -> Result<Map<String, Value>, String> {
    let data = context.data;
    Ok(match named {
        Named::Font => ovrley_widgets::model::document::font_selection(&text_input(cell, &input)?),
        Named::DisplayType => {
            build_display_type_change_update(data, &text_input(cell, &input)?, context.unit_system)
        }
        Named::MetricUnit => {
            let unit = text_input(cell, &input)?;
            let current = data
                .get("display_unit")
                .and_then(Value::as_str)
                .unwrap_or_default();
            build_metric_unit_update(
                context.kind,
                data.get("starting_altitude").unwrap_or(&Value::Null),
                current,
                &unit,
            )
        }
        Named::Reset => unreachable!("reset is not a data patch"),
    })
}

/// Whether the cell reports drafts (a slider's live value) rather than commits.
pub fn drafts(cell: &Cell) -> bool {
    matches!(
        cell.kind,
        CellKind::Slider { .. } | CellKind::Gradient { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn map(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn paths_patch_nested_fields_and_variants() {
        let data = map(
            json!({"display_type": "arc", "display_variants": {"arc": {"width": 200, "height": 200}}}),
        );
        assert_eq!(
            path_patch(&data, "font_size", json!(40)),
            map(json!({"font_size": 40}))
        );
        assert_eq!(
            path_patch(&data, "display_variants.arc.track_thickness", json!(12)),
            map(json!({"display_variants": {"arc": {"track_thickness": 12}}}))
        );
    }
}
