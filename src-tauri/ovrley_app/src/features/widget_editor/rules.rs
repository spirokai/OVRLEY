//! How an inspector edit becomes a widget patch (D1, D2). A cell's field id is its JSON path; the
//! plain rule patches that path, as `useDisplayVariantUpdater` does for a display variant (geometry
//! keys through `build_frame_geometry_update`). The named rules port the editors' handlers that
//! patch more than their own field, each beside the renderer's own geometry where it has one.

use super::blocks::{Cell, CellKind, Rule};
use ovrley_widgets::model::document::{
    build_display_mode_update, build_display_type_change_update, build_frame_geometry_update,
    build_metric_unit_update, build_uniform_resize_update, js_round, number, UnitSystem,
};
use ovrley_widgets::model::standard_metrics::heart_zone_model;
use ovrley_widgets::widgets::gauges::bar_geometry;
use ovrley_widgets::widgets::heart_zones::zones::lthr_from_max_heartrate;
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
    /// A gauge track's size, thickness or bars: the field, and the corner radius clamped to what
    /// the new geometry allows.
    TrackGeometry(Track),
    /// `BarFillStyleField`: switching to bars suggests their geometry.
    BarFillStyle(Track),
    /// The linear gauge's orientation: width and height swap, the labels move to the matching
    /// side, the corner radius stays within the new geometry.
    LinearOrientation,
    /// The value clamped to a range (`updateBoundedNumber`, the corner and strip sliders).
    Clamp { min: f64, max: f64 },
    /// The Size slider of gauges, lean angle and the compass (`buildUniformResizeUpdate`).
    UniformSize,
    /// The G-force plot's Size slider, in plot diameter: the frame scales by the same factor.
    GForceDiameter,
    /// A slider over the indices of these values (the altitude tape's interval).
    Index(Vec<f64>),
    /// `buildZoneCalculationUpdate`.
    ZoneCalculation,
    /// A number field stored as `Math.max(Number(raw), min)` (LTHR, max heart rate).
    AtLeast(f64),
    /// One bound of a manual heart-rate zone (`updateManualZone`).
    ManualZone { index: usize, bound: String },
    /// `buildSplitDistanceUpdate`: only a positive length is stored.
    SplitDistance,
    /// A split column toggled (`buildVisibleColumnsUpdate`); the last one stays. As in React it
    /// is a draft, which the next commit stores.
    SplitColumn,
    /// `buildSplitBarHeightUpdate`: the border and corner radius stay within the bar.
    SplitBarHeight,
    /// The lap timer's readout (`buildDisplayModeUpdate`).
    DisplayMode,
    /// A switch the editor writes as a draft only (the heading compass's label and value
    /// toggles); the next commit stores it. Oddity kept.
    DraftOnly,
    /// A G-force axis: picking the other direction's axis swaps the two.
    GForceAxis { horizontal: bool },
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
    /// A live draft, whatever the control.
    Draft(Map<String, Value>),
    /// Nothing to store (an input the rule ignores, as React's handlers return early).
    Nothing,
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
        Rule::Named(named) => return named_patch(named, cell, input, context),
    };
    Ok(Patch::Data(patch))
}

fn number_input(cell: &Cell, input: &Input) -> Result<f64, String> {
    match input {
        Input::Number(value) => Ok(*value),
        other => Err(format!("{} takes a number, not {other:?}", cell.id)),
    }
}

/// The variant display and key of a `display_variants.<display>.<key>` id.
fn variant_key(cell: &Cell) -> Result<(&str, &str), String> {
    match cell.id.split('.').collect::<Vec<_>>().as_slice() {
        ["display_variants", display, key] => Ok((display, key)),
        _ => Err(format!("{} is not a display variant field", cell.id)),
    }
}

/// A patch of `value` at the cell's path, a variant field or a top-level one.
fn at_cell(data: &Map<String, Value>, cell: &Cell, value: Value) -> Map<String, Value> {
    path_patch(data, &cell.id, value)
}

fn named_patch(named: &Named, cell: &Cell, input: Input, context: &RuleContext) -> Result<Patch, String> {
    let data = context.data;
    let patch = match named {
        Named::Font => ovrley_widgets::model::document::font_selection(&text_input(cell, &input)?),
        Named::DisplayType => build_display_type_change_update(data, &text_input(cell, &input)?, context.unit_system),
        Named::MetricUnit => {
            let unit = text_input(cell, &input)?;
            let current = data.get("display_unit").and_then(Value::as_str).unwrap_or_default();
            build_metric_unit_update(context.kind, data.get("starting_altitude").unwrap_or(&Value::Null), current, &unit)
        }
        Named::Reset => unreachable!("reset is not a data patch"),
        Named::TrackGeometry(track) => {
            let (display, key) = variant_key(cell)?;
            let value = number_input(cell, &input)?;
            let current = variant(data, display);
            let mut next = current.clone();
            next.insert(key.into(), number(value));
            let mut updates = Map::new();
            updates.insert(key.into(), number(value));
            updates.insert("track_corner_radius".into(), number(field(&current, "track_corner_radius").min(corner_radius_max(track, &next)?)));
            variant_patch(data, display, updates)
        }
        Named::BarFillStyle(track) => variant_patch(data, track.display(), fill_style_update(track, &variant(data, track.display()), &text_input(cell, &input)?)?),
        Named::LinearOrientation => {
            let orientation = text_input(cell, &input)?;
            let current = variant(data, "linear");
            if current.get("orientation").and_then(Value::as_str) == Some(orientation.as_str()) {
                return Ok(Patch::Nothing);
            }
            let mut next = current.clone();
            next.insert("orientation".into(), Value::String(orientation.clone()));
            next.insert("width".into(), current.get("height").cloned().unwrap_or(Value::Null));
            next.insert("height".into(), current.get("width").cloned().unwrap_or(Value::Null));
            let position = current.get("min_max_label_position").and_then(Value::as_str).unwrap_or_default();
            let mut updates = Map::new();
            updates.insert("orientation".into(), Value::String(orientation));
            updates.insert("width".into(), next["width"].clone());
            updates.insert("height".into(), next["height"].clone());
            updates.insert("track_corner_radius".into(), number(field(&current, "track_corner_radius").min(corner_radius_max(&Track::Linear, &next)?)));
            updates.insert("min_max_label_position".into(), Value::String(swapped_label_position(position).into()));
            variant_patch(data, "linear", updates)
        }
        Named::Clamp { min, max } => {
            let value = number_input(cell, &input)?;
            if !value.is_finite() {
                return Ok(Patch::Nothing);
            }
            at_cell(data, cell, number(value.max(*min).min(*max)))
        }
        Named::UniformSize => build_uniform_resize_update(data, number_input(cell, &input)?)?,
        Named::GForceDiameter => {
            let current = variant(data, "g_force");
            let frame = field(&current, "width") * (number_input(cell, &input)? / field(&current, "diameter"));
            build_uniform_resize_update(data, frame)?
        }
        Named::Index(values) => {
            let index = number_input(cell, &input)?.round() as usize;
            let value = values.get(index).ok_or_else(|| format!("{}: no value at {index}", cell.id))?;
            at_cell(data, cell, number(*value))
        }
        Named::ZoneCalculation => variant_patch(data, "heart_zones", zone_calculation_update(&variant(data, "heart_zones"), &text_input(cell, &input)?)),
        Named::AtLeast(min) => {
            let value = js_number(&text_input(cell, &input)?);
            if value.is_nan() {
                return Err(format!("{}: not a number", cell.id));
            }
            at_cell(data, cell, number(value.max(*min)))
        }
        Named::ManualZone { index, bound } => {
            let value = js_number(&text_input(cell, &input)?);
            if value.is_nan() {
                return Err(format!("{}: not a number", cell.id));
            }
            let current = variant(data, "heart_zones");
            let zones = current.get("manual_zones").and_then(Value::as_array).cloned().unwrap_or_default();
            let mut updates = Map::new();
            updates.insert("manual_zones".into(), Value::Array(manual_zone_update(&zones, *index, bound, value)));
            variant_patch(data, "heart_zones", updates)
        }
        Named::SplitDistance => {
            let value = js_number(&text_input(cell, &input)?);
            if !(value > 0.0) {
                return Ok(Patch::Nothing);
            }
            at_cell(data, cell, number(value))
        }
        Named::SplitColumn => {
            let toggled = text_input(cell, &input)?;
            let current: Vec<String> = data.get("visible_columns").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
            let has = |column: &str| if column == toggled { !current.iter().any(|item| item == column) } else { current.iter().any(|item| item == column) };
            let next: Vec<Value> = SPLIT_COLUMNS.iter().filter(|column| has(column)).map(|column| Value::String((*column).into())).collect();
            if next.is_empty() {
                return Ok(Patch::Nothing);
            }
            return Ok(Patch::Draft(at_cell(data, cell, Value::Array(next))));
        }
        Named::SplitBarHeight => {
            let thickness = number_input(cell, &input)?;
            let mut next = data.clone();
            next.insert("track_thickness".into(), number(thickness));
            let mut updates = Map::new();
            updates.insert("track_thickness".into(), number(thickness));
            updates.insert("track_corner_radius".into(), number(field(data, "track_corner_radius").min(split_corner_radius_max(&next))));
            updates.insert("track_border_thickness".into(), number(field(data, "track_border_thickness").min(split_border_max(&next))));
            updates
        }
        Named::DisplayMode => {
            let display = data.get("display_type").and_then(Value::as_str).unwrap_or_default().to_string();
            build_display_mode_update(&display, &text_input(cell, &input)?)?
        }
        Named::DraftOnly => return Ok(Patch::Draft(at_cell(data, cell, plain_value(cell, &input)?))),
        Named::GForceAxis { horizontal } => {
            if let Input::Flag(inverted) = input {
                let mut updates = Map::new();
                updates.insert(if *horizontal { "invert_horizontal" } else { "invert_vertical" }.into(), Value::Bool(inverted));
                return Ok(Patch::Data(variant_patch(data, "g_force", updates)));
            }
            let axis = text_input(cell, &input)?;
            let current = variant(data, "g_force");
            let (own, other) = if *horizontal { ("axis_horizontal", "axis_vertical") } else { ("axis_vertical", "axis_horizontal") };
            let mut updates = Map::new();
            if current.get(other).and_then(Value::as_str) == Some(axis.as_str()) {
                updates.insert(other.into(), current.get(own).cloned().unwrap_or(Value::Null));
            }
            updates.insert(own.into(), Value::String(axis));
            let mut ordered = Map::new();
            for key in ["axis_horizontal", "axis_vertical"] {
                if let Some(value) = updates.remove(key) {
                    ordered.insert(key.into(), value);
                }
            }
            variant_patch(data, "g_force", ordered)
        }
    };
    Ok(Patch::Data(patch))
}

/// Whether the cell reports drafts (a slider's live value) rather than commits.
pub fn drafts(cell: &Cell) -> bool {
    matches!(
        cell.kind,
        CellKind::Slider { .. } | CellKind::Gradient { .. }
    )
}

// ── Gauge track geometry (`gaugeGeometry.js` over the renderer's `bar_geometry`) ───────────────

/// Which gauge track a rule bounds: the linear gauge, or an arc display (`arc`, `corner`,
/// `gt_gauge`).
#[derive(Clone, Debug, PartialEq)]
pub enum Track {
    Linear,
    Arc(String),
}

impl Track {
    pub fn display(&self) -> &str {
        match self {
            Track::Linear => "linear",
            Track::Arc(display) => display,
        }
    }
}

/// The display's variant of `data`.
pub fn variant(data: &Map<String, Value>, display: &str) -> Map<String, Value> {
    data.get("display_variants").and_then(|variants| variants.get(display)).and_then(Value::as_object).cloned().unwrap_or_default()
}

fn field(map: &Map<String, Value>, key: &str) -> f64 {
    map.get(key).and_then(Value::as_f64).unwrap_or_default()
}

fn bars(map: &Map<String, Value>, span: f32) -> Result<Option<bar_geometry::ResolvedBarGeometry>, String> {
    if map.get("track_fill_style").and_then(Value::as_str) != Some("bars") {
        return Ok(None);
    }
    let count = field(map, "bar_count") as u32;
    bar_geometry::resolve_bar_geometry(span, count, field(map, "bar_gap") as f32, "bars").map(Some).map_err(|error| error.to_string())
}

fn linear_span(map: &Map<String, Value>) -> (f32, f32) {
    let (width, height) = (field(map, "width") as f32, field(map, "height") as f32);
    if map.get("orientation").and_then(Value::as_str) == Some("horizontal") {
        (width, height)
    } else {
        (height, width)
    }
}

/// `getArcTrackDimensions`: the track's radius and sweep.
fn arc_dimensions(map: &Map<String, Value>, display: &str) -> (f32, f32) {
    bar_geometry::arc_track_dimensions(
        field(map, "width").min(field(map, "height")) as f32,
        field(map, "track_thickness") as f32,
        field(map, "track_corner_radius") as f32,
        field(map, "track_border_thickness") as f32,
        field(map, "arc_angle") as f32,
        display == "corner",
    )
}

/// The track's corner radius maximum (`getLinearTrackCornerRadiusMax`,
/// `getArcTrackCornerRadiusMax`).
pub fn corner_radius_max(track: &Track, map: &Map<String, Value>) -> Result<f64, String> {
    Ok(f64::from(match track {
        Track::Linear => {
            let (span, cross) = linear_span(map);
            bar_geometry::track_corner_radius_max(cross, span, bars(map, span)?.as_ref())
        }
        Track::Arc(display) => {
            let (radius, sweep) = arc_dimensions(map, display);
            let thickness = field(map, "track_thickness") as f32;
            bar_geometry::track_corner_radius_max(thickness, thickness, bars(map, bar_geometry::arc_span(radius, sweep))?.as_ref())
        }
    }))
}

/// Bar Gap's maximum (`getLinearBarGapMax`, `getArcBarGapMax`).
pub fn bar_gap_max(track: &Track, map: &Map<String, Value>) -> Result<f64, String> {
    let span = match track {
        Track::Linear => linear_span(map).0,
        Track::Arc(display) => {
            let (radius, sweep) = arc_dimensions(map, display);
            bar_geometry::arc_span(radius, sweep)
        }
    };
    bar_geometry::bar_gap_max(span, field(map, "bar_count") as u32).map(f64::from).map_err(|error| error.to_string())
}

/// `buildFillStyleUpdate`: switching to bars suggests a bar count and gap, and keeps the corner
/// radius within what they allow.
fn fill_style_update(track: &Track, map: &Map<String, Value>, style: &str) -> Result<Map<String, Value>, String> {
    let mut updates = Map::new();
    updates.insert("track_fill_style".into(), Value::String(style.into()));
    if style != "bars" || map.get("track_fill_style").and_then(Value::as_str) == Some("bars") {
        return Ok(updates);
    }
    let suggestion = match track {
        Track::Linear => {
            let (span, cross) = linear_span(map);
            bar_geometry::suggested_bar_geometry(span, cross, bar_geometry::SUGGESTED_LINEAR_BAR_COUNT)
        }
        Track::Arc(display) => {
            let (radius, sweep) = arc_dimensions(map, display);
            bar_geometry::suggested_arc_bar_geometry(radius, sweep, field(map, "track_thickness") as f32, display == "corner")
        }
    }
    .map_err(|error| error.to_string())?;
    updates.insert("bar_count".into(), number(f64::from(suggestion.count)));
    updates.insert("bar_gap".into(), number(f64::from(suggestion.gap)));
    updates.insert("track_corner_radius".into(), number(field(map, "track_corner_radius").min(f64::from(suggestion.corner_radius_max))));
    Ok(updates)
}

/// `LABEL_POSITION_SWAP`.
fn swapped_label_position(position: &str) -> &str {
    match position {
        "bottom" => "left",
        "top" => "right",
        "left" => "bottom",
        "right" => "top",
        other => other,
    }
}

// ── Heart zones (`heartZones.js` over the renderer's zone model) ────────────────────────────────

/// `deriveLthrZones`: Coggan zones for an LTHR in whole bpm, kept strictly ascending.
pub fn derive_lthr_zones(lthr: f64) -> Vec<Value> {
    let mut boundaries: Vec<f64> = Vec::new();
    for share in heart_zone_model().lthr_boundaries {
        let previous = boundaries.last().copied().unwrap_or(-1.0);
        boundaries.push(js_round(lthr * f64::from(share)).max(previous + 1.0));
    }
    boundaries.windows(2).map(|pair| serde_json::json!({"from": number(pair[0]), "to": number(pair[1])})).collect()
}

/// `buildZoneCalculationUpdate`: switching to manual seeds the zones from the previous mode.
fn zone_calculation_update(map: &Map<String, Value>, calculation: &str) -> Map<String, Value> {
    let mut updates = Map::new();
    updates.insert("zone_calculation".into(), Value::String(calculation.into()));
    let current = map.get("zone_calculation").and_then(Value::as_str);
    if calculation != "manual" || current == Some("manual") {
        return updates;
    }
    let lthr = if current == Some("lthr") {
        field(map, "lthr")
    } else {
        f64::from(lthr_from_max_heartrate(field(map, "max_heartrate") as f32))
    };
    updates.insert("manual_zones".into(), Value::Array(derive_lthr_zones(lthr)));
    updates
}

/// `updateManualZone`: one bound, clamped so zones stay ascending and apart.
fn manual_zone_update(zones: &[Value], index: usize, bound: &str, value: f64) -> Vec<Value> {
    let get = |zone: &Value, key: &str| zone.get(key).and_then(Value::as_f64).unwrap_or_default();
    let zone = &zones[index];
    let mut next = zone.clone();
    if bound == "from" {
        let floor = if index == 0 { 0.0 } else { get(&zones[index - 1], "to") };
        next["from"] = number(value.max(floor).min(get(zone, "to") - 1.0));
    } else {
        let ceiling = if index == zones.len() - 1 { f64::INFINITY } else { get(&zones[index + 1], "from") };
        next["to"] = number(value.min(ceiling).max(get(zone, "from") + 1.0));
    }
    zones.iter().enumerate().map(|(item_index, item)| if item_index == index { next.clone() } else { item.clone() }).collect()
}

// ── Splits (`splitsRun.js`) ──────────────────────────────────────────────────────────────────────

/// `SPLIT_COLUMNS`, in drawn order.
pub const SPLIT_COLUMNS: [&str; 3] = ["split", "bar", "pace"];
/// The border slider range of the linear gauge, which split bars follow.
const SPLIT_BAR_BORDER_MAX: f64 = 6.0;

/// `maxSplitBarCornerRadius`.
pub fn split_corner_radius_max(data: &Map<String, Value>) -> f64 {
    field(data, "track_thickness") / 2.0
}

/// `maxSplitBarBorderThickness`.
pub fn split_border_max(data: &Map<String, Value>) -> f64 {
    ((field(data, "track_thickness") - 1.0) / 2.0).floor().max(0.0).min(SPLIT_BAR_BORDER_MAX)
}

/// `Number(text)` for an editor input.
pub fn js_number(text: &str) -> f64 {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0.0;
    }
    trimmed.parse().unwrap_or(f64::NAN)
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
