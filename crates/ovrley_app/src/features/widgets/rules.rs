//! How an inspector edit becomes a widget patch. A cell's field id is its JSON path; the plain rule
//! patches that path, a display variant's geometry keys through `build_frame_geometry_update`. The
//! named rules are the edits that patch more than their own field, each beside the renderer's own
//! geometry where it has one.

use ovrley_widgets::model::document::{build_display_mode_update, build_display_type_change_update, build_frame_geometry_update, build_metric_unit_update, build_uniform_resize_update, round_half_up, number, UnitSystem};
use ovrley_widgets::model::catalog::heart_zone_model;
use ovrley_widgets::widgets::gauges::track;
use ovrley_widgets::widgets::heart_zones::lthr_from_max_heartrate;
use serde_json::{Map, Value};
use crate::features::widgets::blocks::{Cell, CellKind, Rule};

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
    /// `font` and its `font_family`.
    Font,
    /// The display type select.
    DisplayType,
    /// Altitude widgets convert their starting altitude.
    MetricUnit,
    /// The inspector's reset button: fresh defaults for the type.
    Reset,
    /// A gauge track's size, thickness or bars: the field, and the corner radius clamped to what
    /// the new geometry allows.
    TrackGeometry(Track),
    /// Switching to bars suggests their geometry.
    BarFillStyle(Track),
    /// The linear gauge's orientation: width and height swap, the labels move to the matching
    /// side, the corner radius stays within the new geometry.
    LinearOrientation,
    /// The value clamped to a range (the corner and strip sliders).
    Clamp { min: f64, max: f64 },
    /// The Size slider of gauges, lean angle and the compass.
    UniformSize,
    /// The G-force plot's Size slider, in plot diameter: the frame scales by the same factor.
    GForceDiameter,
    /// A slider over the indices of these values (the altitude tape's interval).
    Index(Vec<f64>),
    ZoneCalculation,
    /// A number field stored as at least `min` (LTHR, max heart rate).
    AtLeast(f64),
    /// One bound of a manual heart-rate zone.
    ManualZone { index: usize, bound: String },
    /// Only a positive length is stored.
    SplitDistance,
    /// A split column toggled; the last one stays. It is a draft, which the next commit stores.
    SplitColumn,
    /// The border and corner radius stay within the bar.
    SplitBarHeight,
    /// The lap timer's readout.
    DisplayMode,
    /// A switch the editor writes as a draft only (the heading compass's label and value
    /// toggles); the next commit stores it.
    DraftOnly,
    /// A G-force axis: picking the other direction's axis swaps the two.
    GForceAxis { horizontal: bool },
    /// The label heading's italic toggle.
    ToggleItalic,
    /// The backdrop's display type.
    BackdropDisplay,
    /// The backdrop's diameter: a whole number, the current one while the input is not one.
    BackdropDiameter,
    /// The rectangle backdrop's width or height: the corner radius and strip thickness stay
    /// within the new size.
    BackdropSize,
    /// The backdrop's shape grid: a corner's rounding, or an edge strip in canonical order.
    ShapeToggle,
    /// The value rounded to `digits` decimals.
    Fixed(usize),
    /// The route line's Thickness: both line widths.
    BothLineWidths,
    /// The finished line colour, which is also the widget's colour.
    FinishedColor,
    /// A field of a nested object (`point_label.font_size`): the object is kept.
    ObjectField,
    /// The elevation's starting-altitude unit: the value converts; the reset clears it.
    AltitudeInputUnit,
    /// A chart series row: `metric:<type>`, `color:<hex>`, `up`, `down` or `remove`.
    ChartSeries(usize),
    /// Add series: the first unplotted metric in the first unused palette colour.
    ChartAdd,
    /// The chart's time window, typed as `45` or `1:30`.
    WindowSeconds,
    /// The raster's folder button: the image dialog, then loading the chosen image.
    RasterPick,
    /// A circular map is square, so choosing one crops the longer side.
    MapShape,
    /// A circle's width and height, at least the smallest map edge.
    MapDiameter,
}

/// The smallest map edge, as the resize gesture's (`MIN_MAP_EDGE`).
const MIN_MAP_EDGE: f64 = 8.0;

/// What an edit asks the editor to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Patch {
    /// A data patch.
    Data(Map<String, Value>),
    /// A number field's raw text.
    Numeric {
        key: String,
        raw: String,
        options: super::NumericFieldOptions,
    },
    Reset,
    /// A live draft, whatever the control.
    Draft(Map<String, Value>),
    /// The chart window's text did not parse; it stays as an invalid draft.
    InvalidWindow(String),
    /// Open the raster image dialog.
    PickRaster,
    /// Nothing to store (an input the rule ignores).
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
/// gets its geometry keys through the display's geometry contract, so the top-level frame and the
/// variant stay in sync.
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

/// The patch for `updates` to the `display` variant.
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
        Named::RasterPick => return Ok(Patch::PickRaster),
        Named::MapShape => {
            let shape = text_input(cell, &input)?;
            let mut patch = Map::new();
            if shape == "circle" {
                let size = |key: &str| data.get(key).and_then(Value::as_f64).unwrap_or_default();
                let diameter = number(size("width").min(size("height")));
                patch.insert("width".into(), diameter.clone());
                patch.insert("height".into(), diameter);
            }
            patch.insert("shape".into(), Value::String(shape));
            patch
        }
        Named::MapDiameter => {
            let current = data.get("width").and_then(Value::as_f64).unwrap_or_default();
            let diameter = number(parse_integer(&text_input(cell, &input)?, current)?.max(MIN_MAP_EDGE));
            let mut patch = Map::new();
            patch.insert("width".into(), diameter.clone());
            patch.insert("height".into(), diameter);
            patch
        }
        Named::ToggleItalic => {
            let mut patch = Map::new();
            patch.insert("italic".into(), Value::Bool(!data.get("italic").and_then(Value::as_bool).unwrap_or_default()));
            patch
        }
        Named::BackdropDisplay => {
            let display = text_input(cell, &input)?;
            let next = ovrley_widgets::model::document::init_display_variant(data, &display, None, context.unit_system);
            let mut patch = Map::new();
            patch.insert("display_type".into(), Value::String(display));
            patch.insert("display_variants".into(), next.get("display_variants").cloned().unwrap_or(Value::Null));
            patch
        }
        Named::BackdropDiameter => {
            let (display, key) = variant_key(cell)?;
            let current = variant(data, display);
            let value = parse_integer(&text_input(cell, &input)?, current.get(key).and_then(Value::as_f64).unwrap_or(1.0))?;
            let mut updates = Map::new();
            updates.insert(key.into(), number(value));
            variant_patch(data, display, updates)
        }
        Named::BackdropSize => {
            let (display, key) = variant_key(cell)?;
            let current = variant(data, display);
            let value = parse_integer(&text_input(cell, &input)?, current.get(key).and_then(Value::as_f64).unwrap_or(1.0))?;
            let width = if key == "width" { value } else { field(&current, "width") };
            let height = if key == "height" { value } else { field(&current, "height") };
            let radius_max = (width.min(height) * 0.5).max(0.0);
            let mut updates = Map::new();
            updates.insert(key.into(), number(value));
            updates.insert("corner_radius".into(), number(field(&current, "corner_radius").max(0.0).min(radius_max)));
            updates.insert("edge_strip_thickness".into(), number(field(&current, "edge_strip_thickness").min(max_edge_strip_thickness(width, height))));
            variant_patch(data, display, updates)
        }
        Named::ShapeToggle => {
            let key = text_input(cell, &input)?;
            let current = variant(data, "rectangle");
            let mut updates = Map::new();
            if RECTANGLE_EDGES.contains(&key.as_str()) {
                let edges: Vec<&str> = current.get("edge_strips").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
                let has = |edge: &str| if edge == key { !edges.contains(&edge) } else { edges.contains(&edge) };
                updates.insert("edge_strips".into(), Value::Array(RECTANGLE_EDGES.iter().filter(|edge| has(edge)).map(|edge| Value::String((*edge).into())).collect()));
            } else {
                updates.insert(key.clone(), Value::Bool(!current.get(&key).and_then(Value::as_bool).unwrap_or_default()));
            }
            variant_patch(data, "rectangle", updates)
        }
        Named::Fixed(digits) => at_cell(data, cell, number(crate::format::round_decimals(number_input(cell, &input)?, *digits))),
        Named::BothLineWidths => {
            let value = number(number_input(cell, &input)?);
            let mut patch = Map::new();
            patch.insert("completed_line_width".into(), value.clone());
            patch.insert("remaining_line_width".into(), value);
            patch
        }
        Named::FinishedColor => {
            let color = Value::String(text_input(cell, &input)?);
            let mut patch = Map::new();
            patch.insert("completed_line_color".into(), color.clone());
            patch.insert("color".into(), color);
            patch
        }
        Named::ObjectField => {
            let (parent, key) = cell.id.split_once('.').ok_or_else(|| format!("{} is not a nested field", cell.id))?;
            let mut object = data.get(parent).and_then(Value::as_object).cloned().unwrap_or_default();
            object.insert(key.into(), plain_value(cell, &input)?);
            let mut patch = Map::new();
            patch.insert(parent.into(), Value::Object(object));
            patch
        }
        Named::AltitudeInputUnit => {
            let mut patch = Map::new();
            if input == Input::Clear {
                patch.insert("starting_altitude".into(), Value::Null);
            } else {
                let unit = text_input(cell, &input)?;
                let current = data.get("starting_altitude_unit").and_then(Value::as_str).unwrap_or_default();
                let converted = ovrley_widgets::model::document::convert_altitude_input_value(data.get("starting_altitude").unwrap_or(&Value::Null), current, &unit);
                patch.insert("starting_altitude".into(), converted);
                patch.insert("starting_altitude_unit".into(), Value::String(unit));
            }
            patch
        }
        Named::ChartSeries(index) => {
            let action = text_input(cell, &input)?;
            let series = chart_series(data);
            let next = match action.split_once(':') {
                Some(("metric", metric)) => replace_series(&series, *index, "metric", metric),
                Some(("color", color)) => replace_series(&series, *index, "color", color),
                _ => match action.as_str() {
                    "up" | "down" => {
                        let other = if action == "up" { index.checked_sub(1) } else { Some(index + 1) }.filter(|other| *other < series.len());
                        let other = other.ok_or_else(|| format!("series {index} cannot move {action}"))?;
                        let mut next = series.clone();
                        next.swap(*index, other);
                        next
                    }
                    "remove" if series.len() > 1 => series.iter().enumerate().filter(|(item, _)| item != index).map(|(_, value)| value.clone()).collect(),
                    other => return Err(format!("unknown chart series action {other}")),
                },
            };
            let mut patch = Map::new();
            patch.insert("series".into(), Value::Array(next));
            patch
        }
        Named::ChartAdd => {
            let mut series = chart_series(data);
            let unused = unused_chart_metrics(&series);
            let metric = unused.first().ok_or("every metric is plotted")?;
            let palette = chart_palette();
            let used: Vec<&str> = series.iter().filter_map(|item| item.get("color").and_then(Value::as_str)).collect();
            let color = palette.iter().find(|color| !used.contains(&color.as_str())).cloned().unwrap_or_else(|| palette[series.len() % palette.len()].clone());
            series.push(serde_json::json!({"metric": metric, "color": color}));
            let mut patch = Map::new();
            patch.insert("series".into(), Value::Array(series));
            patch
        }
        Named::WindowSeconds => {
            // Escape discards the draft.
            if input == Input::Clear {
                return Ok(Patch::Nothing);
            }
            let text = text_input(cell, &input)?;
            let Some(seconds) = parse_window_seconds(&text) else { return Ok(Patch::InvalidWindow(text)) };
            if Some(seconds) == data.get("window_seconds").and_then(Value::as_f64) {
                return Ok(Patch::Nothing);
            }
            at_cell(data, cell, number(seconds))
        }
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
            let value = parse_number_text(&text_input(cell, &input)?);
            if value.is_nan() {
                return Err(format!("{}: not a number", cell.id));
            }
            at_cell(data, cell, number(value.max(*min)))
        }
        Named::ManualZone { index, bound } => {
            let value = parse_number_text(&text_input(cell, &input)?);
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
            let value = parse_number_text(&text_input(cell, &input)?);
            if value.is_nan() || value <= 0.0 {
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

// ── Gauge track geometry (over the renderer's `bar_geometry`) ──────────────────────────────────

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

fn bars(map: &Map<String, Value>, span: f32) -> Result<Option<track::ResolvedBarGeometry>, String> {
    if map.get("track_fill_style").and_then(Value::as_str) != Some("bars") {
        return Ok(None);
    }
    let count = field(map, "bar_count") as u32;
    track::resolve_bar_geometry(span, count, field(map, "bar_gap") as f32, "bars").map(Some).map_err(|error| error.to_string())
}

fn linear_span(map: &Map<String, Value>) -> (f32, f32) {
    let (width, height) = (field(map, "width") as f32, field(map, "height") as f32);
    if map.get("orientation").and_then(Value::as_str) == Some("horizontal") {
        (width, height)
    } else {
        (height, width)
    }
}

/// The track's radius and sweep.
fn arc_dimensions(map: &Map<String, Value>, display: &str) -> (f32, f32) {
    track::arc_track_dimensions(
        field(map, "width").min(field(map, "height")) as f32,
        field(map, "track_thickness") as f32,
        field(map, "track_corner_radius") as f32,
        field(map, "track_border_thickness") as f32,
        field(map, "arc_angle") as f32,
        display == "corner",
    )
}

/// The track's corner radius maximum, linear or arc.
pub fn corner_radius_max(track: &Track, map: &Map<String, Value>) -> Result<f64, String> {
    Ok(f64::from(match track {
        Track::Linear => {
            let (span, cross) = linear_span(map);
            track::track_corner_radius_max(cross, span, bars(map, span)?.as_ref())
        }
        Track::Arc(display) => {
            let (radius, sweep) = arc_dimensions(map, display);
            let thickness = field(map, "track_thickness") as f32;
            track::track_corner_radius_max(thickness, thickness, bars(map, track::arc_span(radius, sweep))?.as_ref())
        }
    }))
}

/// Bar Gap's maximum, linear or arc.
pub fn bar_gap_max(track: &Track, map: &Map<String, Value>) -> Result<f64, String> {
    let span = match track {
        Track::Linear => linear_span(map).0,
        Track::Arc(display) => {
            let (radius, sweep) = arc_dimensions(map, display);
            track::arc_span(radius, sweep)
        }
    };
    track::bar_gap_max(span, field(map, "bar_count") as u32).map(f64::from).map_err(|error| error.to_string())
}

/// Switching to bars suggests a bar count and gap, and keeps the corner
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
            track::suggested_bar_geometry(span, cross, track::SUGGESTED_LINEAR_BAR_COUNT)
        }
        Track::Arc(display) => {
            let (radius, sweep) = arc_dimensions(map, display);
            track::suggested_arc_bar_geometry(radius, sweep, field(map, "track_thickness") as f32, display == "corner")
        }
    }
    .map_err(|error| error.to_string())?;
    updates.insert("bar_count".into(), number(f64::from(suggestion.count)));
    updates.insert("bar_gap".into(), number(f64::from(suggestion.gap)));
    updates.insert("track_corner_radius".into(), number(field(map, "track_corner_radius").min(f64::from(suggestion.corner_radius_max))));
    Ok(updates)
}

fn swapped_label_position(position: &str) -> &str {
    match position {
        "bottom" => "left",
        "top" => "right",
        "left" => "bottom",
        "right" => "top",
        other => other,
    }
}

// ── Heart zones (over the renderer's zone model) ────────────────────────────────────────────────

/// Coggan zones for an LTHR in whole bpm, kept strictly ascending.
pub fn derive_lthr_zones(lthr: f64) -> Vec<Value> {
    let mut boundaries: Vec<f64> = Vec::new();
    for share in heart_zone_model().lthr_boundaries {
        let previous = boundaries.last().copied().unwrap_or(-1.0);
        boundaries.push(round_half_up(lthr * f64::from(share)).max(previous + 1.0));
    }
    boundaries.windows(2).map(|pair| serde_json::json!({"from": number(pair[0]), "to": number(pair[1])})).collect()
}

/// Switching to manual seeds the zones from the previous mode.
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

/// One bound, clamped so zones stay ascending and apart.
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

// ── Splits ──────────────────────────────────────────────────────────────────────────────────────

/// `SPLIT_COLUMNS`, in drawn order.
pub const SPLIT_COLUMNS: [&str; 3] = ["split", "bar", "pace"];
/// The border slider range of the linear gauge, which split bars follow.
const SPLIT_BAR_BORDER_MAX: f64 = 6.0;

pub fn split_corner_radius_max(data: &Map<String, Value>) -> f64 {
    field(data, "track_thickness") / 2.0
}

pub fn split_border_max(data: &Map<String, Value>) -> f64 {
    ((field(data, "track_thickness") - 1.0) / 2.0).floor().clamp(0.0, SPLIT_BAR_BORDER_MAX)
}

/// The number an editor input holds: 0 when blank, NaN when it does not parse.
pub fn parse_number_text(text: &str) -> f64 {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0.0;
    }
    trimmed.parse().unwrap_or(f64::NAN)
}

// ── Backdrop shape ──────────────────────────────────────────────────────────────────────────────

/// `RECTANGLE_EDGE_KEYS`, the canonical order of `edge_strips`.
pub const RECTANGLE_EDGES: [&str; 4] = ["top", "right", "bottom", "left"];

/// Opposing strips may meet but not overlap, as the renderer requires.
pub fn max_edge_strip_thickness(width: f64, height: f64) -> f64 {
    (width.min(height) / 2.0).floor()
}

pub fn parse_integer(text: &str, fallback: f64) -> Result<f64, String> {
    if text.is_empty() {
        return Ok(fallback);
    }
    let parsed = parse_number_text(text);
    if !parsed.is_finite() {
        return Err(format!("Invalid integer input: {text}"));
    }
    Ok(parsed.trunc())
}

// ── Rolling chart ───────────────────────────────────────────────────────────────────────────────

fn chart_definition() -> &'static ovrley_widgets::model::catalog::WidgetDefinition {
    ovrley_widgets::model::catalog::catalog().require_widget("rolling_chart").expect("the catalog defines the rolling chart")
}

pub fn chart_limits() -> &'static ovrley_widgets::model::catalog::ChartLimits {
    chart_definition().chart_limits.as_ref().expect("the rolling chart declares its limits")
}

fn chart_palette() -> Vec<String> {
    chart_definition().series_palette.clone()
}

fn chart_series(data: &Map<String, Value>) -> Vec<Value> {
    data.get("series").and_then(Value::as_array).cloned().unwrap_or_default()
}

fn replace_series(series: &[Value], index: usize, key: &str, value: &str) -> Vec<Value> {
    series.iter().enumerate().map(|(item, row)| {
        let mut row = row.clone();
        if item == index {
            row[key] = Value::String(value.into());
        }
        row
    }).collect()
}

/// `PLOTTABLE_METRICS`, in catalog order.
pub fn plottable_metrics() -> Vec<String> {
    ovrley_widgets::model::catalog::catalog().widgets().iter().filter(|definition| definition.plottable).map(|definition| definition.kind.clone()).collect()
}

pub fn unused_chart_metrics(series: &[Value]) -> Vec<String> {
    let used: Vec<&str> = series.iter().filter_map(|item| item.get("metric").and_then(Value::as_str)).collect();
    plottable_metrics().into_iter().filter(|metric| !used.contains(&metric.as_str())).collect()
}

/// Whole seconds (`45`) or minutes and seconds (`1:30`) within the limits.
/// `lib/seconds_input.rs` parses clock times with fractions and hours, so it is not reused.
pub fn parse_window_seconds(text: &str) -> Option<f64> {
    let text = text.trim();
    let seconds = match text.split_once(':') {
        Some((minutes, seconds)) => {
            let all_digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
            if !all_digits(minutes) || seconds.len() != 2 || !all_digits(seconds) || seconds.as_bytes()[0] > b'5' {
                return None;
            }
            minutes.parse::<f64>().ok()? * 60.0 + seconds.parse::<f64>().ok()?
        }
        None if !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()) => text.parse().ok()?,
        None => return None,
    };
    let limits = chart_limits();
    (seconds >= f64::from(limits.min_window_seconds) && seconds <= f64::from(limits.max_window_seconds)).then_some(seconds)
}

/// `m:ss`.
pub fn format_window_seconds(seconds: f64) -> String {
    let whole = seconds as u64;
    format!("{}:{:02}", whole / 60, whole % 60)
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
