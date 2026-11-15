//! The overlay document and its catalog-driven edits: the one home of what `lib/widget`
//! (`widget-config`, `widget-resolver`, `widget-defaults`, `unit-system`, `altitude`) and
//! `lib/color-utils` do to widget data (Phase 3, D1 and D3).
//!
//! The document is JSON with a typed outer shape (D1): a scene object and five ordered
//! collections of widget objects. Template ingress (`ovrley_core`) and the native editor
//! (`ovrley_app`) both edit it through these functions; nothing re-implements them.

use super::catalog::{catalog, LayoutMode};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// One widget's data: a JSON object whose `id` is a non-empty string, validated at ingress.
pub type WidgetData = Map<String, Value>;

/// The editor's document (D1).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OverlayConfig {
    pub scene: Map<String, Value>,
    pub backdrops: Vec<WidgetData>,
    pub rasters: Vec<WidgetData>,
    pub labels: Vec<WidgetData>,
    pub values: Vec<WidgetData>,
    pub plots: Vec<WidgetData>,
}

/// The document collections, in catalog order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Collection {
    Backdrops,
    Rasters,
    Labels,
    Values,
    Plots,
}

impl Collection {
    pub const ALL: [Collection; 5] = [
        Self::Backdrops,
        Self::Rasters,
        Self::Labels,
        Self::Values,
        Self::Plots,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Backdrops => "backdrops",
            Self::Rasters => "rasters",
            Self::Labels => "labels",
            Self::Values => "values",
            Self::Plots => "plots",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|collection| collection.name() == name)
    }

    /// The widget type of a widget in this collection: the collection's own type, or the field
    /// that names it.
    pub fn widget_type(self, data: &WidgetData) -> Result<String, String> {
        let definition = catalog()
            .collection(self.name())
            .expect("the catalog defines every collection");
        match (&definition.kind, &definition.type_field) {
            (Some(kind), _) => Ok(kind.clone()),
            (None, Some(field)) => data
                .get(field)
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| {
                    format!(
                        "Invalid {} widget type: {}",
                        self.name(),
                        data.get(field).unwrap_or(&Value::Null)
                    )
                }),
            (None, None) => unreachable!("a collection names its type or its type field"),
        }
    }
}

impl OverlayConfig {
    pub fn collection(&self, collection: Collection) -> &Vec<WidgetData> {
        match collection {
            Collection::Backdrops => &self.backdrops,
            Collection::Rasters => &self.rasters,
            Collection::Labels => &self.labels,
            Collection::Values => &self.values,
            Collection::Plots => &self.plots,
        }
    }

    pub fn collection_mut(&mut self, collection: Collection) -> &mut Vec<WidgetData> {
        match collection {
            Collection::Backdrops => &mut self.backdrops,
            Collection::Rasters => &mut self.rasters,
            Collection::Labels => &mut self.labels,
            Collection::Values => &mut self.values,
            Collection::Plots => &mut self.plots,
        }
    }

    /// Every widget with its collection, in document order.
    pub fn widgets(&self) -> impl Iterator<Item = (Collection, &WidgetData)> {
        Collection::ALL.into_iter().flat_map(move |collection| {
            self.collection(collection)
                .iter()
                .map(move |data| (collection, data))
        })
    }

    /// The ids of every widget, in document order.
    pub fn widget_ids(&self) -> Vec<String> {
        self.widgets()
            .map(|(_, data)| widget_id(data).to_string())
            .collect()
    }

    /// `ensureWidgetIdsInConfig`: document ingress. Absent collections become empty and absent
    /// ids are assigned; present collections and ids must be valid, and duplicates are rejected.
    pub fn from_value(value: &Value) -> Result<Self, String> {
        let object = value.as_object().ok_or("Widget config must be an object")?;
        let scene = match object.get("scene") {
            None => Map::new(),
            Some(Value::Object(scene)) => scene.clone(),
            Some(_) => return Err("Template scene must be an object".into()),
        };
        let mut config = Self {
            scene,
            ..Self::default()
        };
        let mut used = std::collections::HashSet::new();
        for collection in Collection::ALL {
            let items = match object.get(collection.name()) {
                None => continue,
                Some(Value::Array(items)) => items,
                Some(_) => return Err(format!("Invalid widget collection: {}", collection.name())),
            };
            for item in items {
                let widget = item
                    .as_object()
                    .ok_or_else(|| format!("Invalid {} widget", collection.name()))?;
                match widget.get("id") {
                    None => {}
                    Some(Value::String(id)) if !id.trim().is_empty() => {
                        if !used.insert(id.clone()) {
                            return Err(format!("Duplicate widget ID: {id}"));
                        }
                    }
                    Some(_) => return Err("Invalid widget ID".into()),
                }
                config.collection_mut(collection).push(widget.clone());
            }
        }
        let mut next_id = config.next_generated_id();
        for collection in Collection::ALL {
            for widget in config.collection_mut(collection) {
                if !widget.contains_key("id") {
                    widget.insert("id".into(), Value::String(format!("widget-{next_id}")));
                    next_id += 1;
                }
            }
        }
        Ok(config)
    }

    /// The next `widget-<n>` number after every generated-looking id.
    fn next_generated_id(&self) -> u64 {
        let mut next = 1;
        for (_, widget) in self.widgets() {
            if let Some(number) = widget
                .get("id")
                .and_then(Value::as_str)
                .and_then(|id| id.strip_prefix("widget-"))
            {
                if !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()) {
                    if let Ok(number) = number.parse::<u64>() {
                        next = next.max(number + 1);
                    }
                }
            }
        }
        next
    }

    /// Where a widget is, or an error naming the unknown id.
    pub fn find_widget(&self, widget_id: &str) -> Result<(Collection, usize), String> {
        for collection in Collection::ALL {
            if let Some(index) = self
                .collection(collection)
                .iter()
                .position(|widget| widget_id_of(widget) == Some(widget_id))
            {
                return Ok((collection, index));
            }
        }
        Err(format!("Unknown widget ID: {widget_id}"))
    }

    pub fn widget(&self, widget_id: &str) -> Result<&WidgetData, String> {
        let (collection, index) = self.find_widget(widget_id)?;
        Ok(&self.collection(collection)[index])
    }

    /// `updateWidgetsInConfig`: merges each patch into its widget atomically; colour fields of
    /// the patch are normalized and the id is kept.
    pub fn update_widgets(
        &mut self,
        updates: &[(String, Map<String, Value>)],
    ) -> Result<(), String> {
        let mut next = self.clone();
        for (widget_id, patch) in updates {
            let (collection, index) = next.find_widget(widget_id)?;
            let widgets = next.collection_mut(collection);
            let mut patch = normalize_color_fields(patch);
            canonical_numbers(&mut patch);
            let mut merged = merge_widget_data(Some(&widgets[index]), &patch);
            merged.insert("id".into(), Value::String(widget_id.clone()));
            widgets[index] = merged;
        }
        *self = next;
        Ok(())
    }

    /// `replaceWidgetInConfig`: replaces a widget's data, keeping its id.
    pub fn replace_widget(
        &mut self,
        widget_id: &str,
        data: &Map<String, Value>,
    ) -> Result<(), String> {
        let (collection, index) = self.find_widget(widget_id)?;
        let mut data = data.clone();
        data.insert("id".into(), Value::String(widget_id.to_string()));
        self.collection_mut(collection)[index] = data;
        Ok(())
    }

    /// `deleteWidgetsInConfig`.
    pub fn delete_widgets(&mut self, widget_ids: &[String]) {
        for collection in Collection::ALL {
            self.collection_mut(collection).retain(|widget| {
                !widget_id_of(widget).is_some_and(|id| widget_ids.iter().any(|target| target == id))
            });
        }
    }

    /// `duplicateWidgetsInConfig`: appends copies with new ids, offset by (24, 24) unless given,
    /// and returns the new ids in document order.
    pub fn duplicate_widgets(
        &mut self,
        widgets: &[(Collection, WidgetData)],
        offset: (f64, f64),
    ) -> Vec<String> {
        let before: Vec<usize> = Collection::ALL
            .iter()
            .map(|collection| self.collection(*collection).len())
            .collect();
        let mut next_id = self.next_generated_id();
        for (collection, data) in widgets {
            let mut data = data.clone();
            data.insert("id".into(), Value::String(format!("widget-{next_id}")));
            next_id += 1;
            for (field, delta) in [("x", offset.0), ("y", offset.1)] {
                let position = data.get(field).and_then(Value::as_f64).unwrap_or_default();
                data.insert(field.into(), number(position + delta));
            }
            self.collection_mut(*collection).push(data);
        }
        Collection::ALL
            .iter()
            .zip(before)
            .flat_map(|(collection, start)| {
                self.collection(*collection)[start..]
                    .iter()
                    .map(|widget| widget_id(widget).to_string())
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// `scaleWidgetPositions`: scales every position about `origin` (left, top).
    pub fn scale_widget_positions(&mut self, factor: f64, origin: (f64, f64)) {
        if factor == 1.0 {
            return;
        }
        for collection in Collection::ALL {
            for widget in self.collection_mut(collection) {
                for (field, anchor) in [("x", origin.0), ("y", origin.1)] {
                    let position = widget
                        .get(field)
                        .and_then(Value::as_f64)
                        .unwrap_or_default();
                    widget.insert(field.into(), number(anchor + (position - anchor) * factor));
                }
            }
        }
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("an overlay config serializes")
    }
}

fn widget_id_of(widget: &WidgetData) -> Option<&str> {
    widget.get("id").and_then(Value::as_str)
}

/// A validated widget's id.
pub fn widget_id(widget: &WidgetData) -> &str {
    widget_id_of(widget).expect("ingress assigns every widget an id")
}

/// `normalizeSelectionIds`: the requested ids that exist, in document order.
pub fn normalize_selection_ids(widget_ids: &[String], ordered_ids: &[String]) -> Vec<String> {
    ordered_ids
        .iter()
        .filter(|id| widget_ids.contains(id))
        .cloned()
        .collect()
}

/// `getPrimarySelectionId`: the preferred id when selected, else the last selected.
pub fn primary_selection_id(widget_ids: &[String], preferred: Option<&str>) -> Option<String> {
    match preferred {
        Some(preferred) if widget_ids.iter().any(|id| id == preferred) => {
            Some(preferred.to_string())
        }
        _ => widget_ids.last().cloned(),
    }
}

/// A JSON number that stays an integer when the value is whole, as JavaScript writes it.
pub fn number(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 9.007_199_254_740_992e15 {
        Value::from(value as i64)
    } else {
        Value::from(value)
    }
}

/// Whole floating-point numbers as integers, recursively: JavaScript has one number type, so a
/// patch from a Rust struct (`100.0`) must equal the document's `100`.
pub fn canonical_numbers(map: &mut Map<String, Value>) {
    fn visit(value: &mut Value) {
        match value {
            Value::Number(current) if current.is_f64() => {
                if let Some(float) = current.as_f64() {
                    *value = number(float);
                }
            }
            Value::Object(map) => map.values_mut().for_each(visit),
            Value::Array(items) => items.iter_mut().for_each(visit),
            _ => {}
        }
    }
    map.values_mut().for_each(visit);
}

/// `Math.round`: halves round towards positive infinity.
pub fn js_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

/// `pickDefined`: the present keys of `source` (null counts as present), in `keys` order.
pub fn pick_defined(source: Option<&Map<String, Value>>, keys: &[String]) -> Map<String, Value> {
    let mut picked = Map::new();
    if let Some(source) = source {
        for key in keys {
            if let Some(value) = source.get(key) {
                picked.insert(key.clone(), value.clone());
            }
        }
    }
    picked
}

fn is_hex(text: &str, len: usize) -> bool {
    text.len() == len && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn expand_short(digits: &str) -> String {
    digits
        .chars()
        .flat_map(|c| [c, c])
        .collect::<String>()
        .to_lowercase()
}

/// `normalizeHexColor`: 3, 4, 6 and 8 digits (with or without `#`) become `#rrggbb`, any alpha
/// is dropped, and anything else yields `fallback`.
pub fn normalize_hex_color(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    let digits = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if is_hex(digits, 3) {
        format!("#{}", expand_short(digits))
    } else if is_hex(digits, 4) {
        format!("#{}", expand_short(&digits[..3]))
    } else if is_hex(digits, 6) {
        format!("#{}", digits.to_lowercase())
    } else if is_hex(digits, 8) {
        format!("#{}", digits[..6].to_lowercase())
    } else {
        fallback.into()
    }
}

/// Whether a template field key holds a colour.
pub fn is_color_field_key(key: &str) -> bool {
    key == "color" || key.ends_with("_color") || key.starts_with("color_")
}

/// `normalizeColorFields`: every colour field of an object as `#rrggbb` (`#000000` when it is not
/// a colour).
pub fn normalize_color_fields(record: &Map<String, Value>) -> Map<String, Value> {
    record
        .iter()
        .map(|(key, value)| {
            let value = if is_color_field_key(key) {
                Value::String(normalize_hex_color(value.as_str().unwrap_or(""), "#000000"))
            } else {
                value.clone()
            };
            (key.clone(), value)
        })
        .collect()
}

fn object(value: Option<&Value>) -> Option<&Map<String, Value>> {
    value.and_then(Value::as_object)
}

/// `mergeWidgetData`: a patch over data, with display variants merged per display type.
pub fn merge_widget_data(
    data: Option<&Map<String, Value>>,
    updates: &Map<String, Value>,
) -> Map<String, Value> {
    let mut next = data.cloned().unwrap_or_default();
    for (key, value) in updates {
        next.insert(key.clone(), value.clone());
    }
    if let Some(variant_updates) = object(updates.get("display_variants")) {
        let current = data.and_then(|data| object(data.get("display_variants")));
        let mut variants = current.cloned().unwrap_or_default();
        for (display_type, fields) in variant_updates {
            let mut variant = current
                .and_then(|current| object(current.get(display_type)))
                .cloned()
                .unwrap_or_default();
            if let Some(fields) = fields.as_object() {
                for (key, value) in fields {
                    variant.insert(key.clone(), value.clone());
                }
            }
            variants.insert(display_type.clone(), Value::Object(variant));
        }
        next.insert("display_variants".into(), Value::Object(variants));
    }
    next
}

fn display_type(data: &Map<String, Value>) -> Option<&str> {
    data.get("display_type").and_then(Value::as_str)
}

/// `resolveActiveWidgetData`: the active display's variant flattened over the shared data, with
/// the display's resolved frame (a circle's diameter as width and height).
pub fn resolve_active_widget_data(data: &Map<String, Value>) -> Map<String, Value> {
    let Some(kind) = display_type(data) else {
        return data.clone();
    };
    let definition = catalog().require_display(kind);
    if definition.layout_mode == LayoutMode::Intrinsic {
        return data.clone();
    }
    let mut resolved = data.clone();
    if let Some(variant) =
        object(data.get("display_variants")).and_then(|variants| object(variants.get(kind)))
    {
        for (key, value) in variant {
            resolved.insert(key.clone(), value.clone());
        }
    }
    for (target, source) in &definition.resolved_frame {
        if let Some(value) = resolved.get(source).cloned() {
            resolved.insert(target.clone(), value);
        }
    }
    resolved
}

/// The unit systems; manifest defaults are metric.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum UnitSystem {
    #[default]
    Metric,
    Imperial,
}

impl UnitSystem {
    /// `requireUnitSystem`.
    pub fn require(value: &str) -> Result<Self, String> {
        match value {
            "metric" => Ok(Self::Metric),
            "imperial" => Ok(Self::Imperial),
            other => Err(format!("Unknown unit system: {other}")),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Metric => "metric",
            Self::Imperial => "imperial",
        }
    }
}

/// `initDisplayVariant`: initializes a boxed display's variant at creation, ingress or display
/// selection. `frame_fallback` is the document geometry (`None` uses the manifest's), and the
/// manifest defaults are read in `unit_system`.
pub fn init_display_variant(
    data: &Map<String, Value>,
    kind: &str,
    frame_fallback: Option<&Map<String, Value>>,
    unit_system: UnitSystem,
) -> Map<String, Value> {
    let definition = catalog().require_display(kind);
    if definition.layout_mode == LayoutMode::Intrinsic {
        return data.clone();
    }
    let mut variant = convert_units(&definition.variant_defaults, unit_system);
    for (key, value) in pick_defined(frame_fallback, &definition.geometry_keys) {
        variant.insert(key, value);
    }
    let variants = object(data.get("display_variants"));
    if let Some(existing) = variants.and_then(|variants| object(variants.get(kind))) {
        for (key, value) in existing {
            variant.insert(key.clone(), value.clone());
        }
    }
    for (target, source) in &definition.inherited_fields {
        if variant.get(target).is_none_or(Value::is_null) {
            if let Some(value) = data.get(source) {
                variant.insert(target.clone(), value.clone());
            }
        }
    }
    let mut next = data.clone();
    let mut next_variants = variants.cloned().unwrap_or_default();
    next_variants.insert(kind.to_string(), Value::Object(variant));
    next.insert("display_variants".into(), Value::Object(next_variants));
    next
}

/// `buildFrameGeometryUpdate`: a gesture's or the inspector's geometry as a patch that follows the
/// display's geometry contract.
pub fn build_frame_geometry_update(
    data: &Map<String, Value>,
    geometry: &Map<String, Value>,
) -> Map<String, Value> {
    let Some(kind) = display_type(data) else {
        return geometry.clone();
    };
    let definition = catalog().require_display(kind);
    if definition.layout_mode == LayoutMode::Intrinsic {
        return geometry.clone();
    }
    let mut fields = Map::new();
    for (target, sources) in &definition.geometry_bindings {
        if let Some(value) = sources.iter().find_map(|source| geometry.get(source)) {
            fields.insert(target.clone(), value.clone());
        }
    }
    let mut patch = pick_defined(Some(geometry), &definition.geometry_fields);
    let mut variants = Map::new();
    variants.insert(kind.to_string(), Value::Object(fields));
    let mut update = Map::new();
    update.insert("display_variants".into(), Value::Object(variants));
    let merged = merge_widget_data(Some(data), &update);
    patch.insert(
        "display_variants".into(),
        merged["display_variants"].clone(),
    );
    patch
}

/// `buildDisplayTypeChangeUpdate`: the patch that selects a display, restoring its geometry and
/// shared defaults.
pub fn build_display_type_change_update(
    data: &Map<String, Value>,
    kind: &str,
    unit_system: UnitSystem,
) -> Map<String, Value> {
    let definition = catalog().require_display(kind);
    let initialized = init_display_variant(data, kind, None, unit_system);
    let mut patch = Map::new();
    patch.insert("display_type".into(), Value::String(kind.to_string()));
    if let Some(variants) = initialized.get("display_variants") {
        patch.insert("display_variants".into(), variants.clone());
    }
    for (key, value) in convert_units(&definition.defaults, unit_system) {
        patch.insert(key, value);
    }
    let active =
        object(initialized.get("display_variants")).and_then(|variants| object(variants.get(kind)));
    for (key, value) in pick_defined(active, &definition.geometry_fields) {
        patch.insert(key, value);
    }
    patch
}

/// `buildDisplayModeUpdate`: a mode's labels and typography defaults.
pub fn build_display_mode_update(kind: &str, value: &str) -> Result<Map<String, Value>, String> {
    Ok(catalog()
        .require_display(kind)
        .require_mode(value)?
        .defaults
        .clone())
}

/// `convertAltitudeValue`.
pub fn convert_altitude_value(value: f64, from_unit: &str, to_unit: &str) -> f64 {
    if from_unit == to_unit {
        return value;
    }
    let catalog = catalog();
    let scale = |unit: &str| {
        catalog
            .altitude_unit_scale(unit)
            .unwrap_or_else(|| panic!("unknown altitude unit {unit}"))
    };
    value / scale(from_unit) * scale(to_unit)
}

/// `convertAltitudeInputValue`: an absent or empty altitude stays as it is; a number converts and
/// rounds to a whole unit.
pub fn convert_altitude_input_value(value: &Value, from_unit: &str, to_unit: &str) -> Value {
    match value.as_f64() {
        Some(number_value) => number(js_round(convert_altitude_value(
            number_value,
            from_unit,
            to_unit,
        ))),
        None => value.clone(),
    }
}

/// `buildMetricUnitUpdate`: a unit change; altitude widgets convert their starting altitude.
pub fn build_metric_unit_update(
    metric_type: &str,
    starting_altitude: &Value,
    current_unit: &str,
    next_unit: &str,
) -> Map<String, Value> {
    let mut update = Map::new();
    update.insert("display_unit".into(), Value::String(next_unit.to_string()));
    let has_source = catalog()
        .widget(metric_type)
        .is_some_and(|definition| definition.altitude_source.is_some());
    if has_source {
        update.insert(
            "starting_altitude".into(),
            convert_altitude_input_value(starting_altitude, current_unit, next_unit),
        );
    }
    update
}

fn accepts_unit(data: &Map<String, Value>, field: &str, unit: &str) -> bool {
    if field != "display_unit" {
        return true;
    }
    let kind = data
        .get("value")
        .and_then(Value::as_str)
        .expect("a display unit belongs to value widget data");
    catalog().widget(kind).is_some_and(|definition| {
        definition
            .supported_display_units
            .iter()
            .any(|option| option.value == unit)
    })
}

fn convert_unit(data: &Map<String, Value>, field: &str, system: UnitSystem) -> Value {
    let current = &data[field];
    let (from, to) = match system {
        UnitSystem::Imperial => (0, 1),
        UnitSystem::Metric => (1, 0),
    };
    catalog()
        .unit_conversions
        .iter()
        .find(|pair| {
            current.as_str() == Some(pair[from].as_str()) && accepts_unit(data, field, &pair[to])
        })
        .map_or_else(|| current.clone(), |pair| Value::String(pair[to].clone()))
}

/// `convertUnits`: converts whichever unit fields and label toggles an object carries (widget
/// data, a display variant, or display defaults).
pub fn convert_units(data: &Map<String, Value>, system: UnitSystem) -> Map<String, Value> {
    let catalog = catalog();
    let mut next = data.clone();
    for field in &catalog.unit_fields {
        if data.contains_key(field) {
            next.insert(field.clone(), convert_unit(data, field, system));
        }
    }
    if let Some(starting_altitude) = data.get("starting_altitude") {
        let kind = data
            .get("value")
            .and_then(Value::as_str)
            .expect("a starting altitude belongs to value widget data");
        let unit_field = &catalog
            .widget(kind)
            .and_then(|definition| definition.altitude_source.as_ref())
            .expect("a widget with a starting altitude has an altitude source")
            .unit_field;
        let unit = |source: &Map<String, Value>| {
            source
                .get(unit_field)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        next.insert(
            "starting_altitude".into(),
            convert_altitude_input_value(starting_altitude, &unit(data), &unit(&next)),
        );
    }
    for [metric, imperial] in &catalog.unit_toggles {
        if data.contains_key(metric) && data.get(metric) != data.get(imperial) {
            next.insert(metric.clone(), Value::Bool(system == UnitSystem::Metric));
            next.insert(
                imperial.clone(),
                Value::Bool(system == UnitSystem::Imperial),
            );
        }
    }
    if let Some(variants) = object(data.get("display_variants")) {
        let converted = variants
            .iter()
            .map(|(kind, variant)| {
                let variant = variant.as_object().map_or_else(
                    || variant.clone(),
                    |variant| Value::Object(convert_units(variant, system)),
                );
                (kind.clone(), variant)
            })
            .collect();
        next.insert("display_variants".into(), Value::Object(converted));
    }
    next
}

/// `convertConfigUnits`: every widget in the target system.
pub fn convert_config_units(config: &OverlayConfig, system: UnitSystem) -> OverlayConfig {
    let mut next = config.clone();
    for collection in Collection::ALL {
        for widget in next.collection_mut(collection) {
            *widget = convert_units(widget, system);
        }
    }
    next
}

/// `getCourseWidgetDimensions`: a route's frame, or `None` when fewer than two points can be
/// measured.
pub fn course_widget_dimensions(
    points: &[(Option<f64>, Option<f64>)],
    max_dimension: f64,
    min_dimension: f64,
) -> Option<(f64, f64)> {
    let valid: Vec<(f64, f64)> = points
        .iter()
        .filter_map(|point| match point {
            (Some(latitude), Some(longitude)) if latitude.is_finite() && longitude.is_finite() => {
                Some((*latitude, *longitude))
            }
            _ => None,
        })
        .collect();
    if valid.len() < 2 {
        return None;
    }
    let mean_latitude =
        (valid.iter().map(|(latitude, _)| latitude).sum::<f64>() / valid.len() as f64).to_radians();
    let xs: Vec<f64> = valid
        .iter()
        .map(|(_, longitude)| longitude * mean_latitude.cos())
        .collect();
    let ys: Vec<f64> = valid.iter().map(|(latitude, _)| *latitude).collect();
    let span = |values: &[f64]| {
        (values.iter().cloned().fold(f64::MIN, f64::max)
            - values.iter().cloned().fold(f64::MAX, f64::min))
        .max(1e-6)
    };
    let (span_x, span_y) = (span(&xs), span(&ys));
    let factor = max_dimension / span_x.max(span_y);
    Some((
        js_round(span_x * factor).max(min_dimension),
        js_round(span_y * factor).max(min_dimension),
    ))
}

/// A drawer selection: a widget type, its display, a drawer group's preset, and a display mode.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WidgetSelection {
    pub kind: String,
    pub display_type: Option<String>,
    pub preset: Map<String, Value>,
    /// The display-owned mode selection (for example the lap timer's mode).
    pub mode: Option<String>,
}

/// Creation inputs taken from the activity and the scene.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WidgetCreationOptions {
    pub course_points: Vec<(Option<f64>, Option<f64>)>,
    pub scene_font_size: Option<f64>,
    pub unit_system: UnitSystem,
}

fn apply_global_bindings(
    data: &mut Map<String, Value>,
    bindings: &[super::catalog::GlobalBinding],
    globals: &Map<String, Value>,
) {
    for binding in bindings {
        let (last, parents) = binding.path.split_last().expect("a binding names a field");
        let mut target = &mut *data;
        for parent in parents {
            target = target
                .get_mut(parent)
                .and_then(Value::as_object_mut)
                .unwrap_or_else(|| panic!("binding parent {parent} must be an object"));
        }
        target.insert(
            last.clone(),
            globals.get(&binding.source).cloned().unwrap_or(Value::Null),
        );
    }
}

/// `createWidgetDefaults`: new widget data from the catalog's defaults, the global defaults and
/// the activity, in the requested unit system. The document assigns its id.
pub fn create_widget_defaults(
    selection: &WidgetSelection,
    globals: &Map<String, Value>,
    options: &WidgetCreationOptions,
) -> Result<Map<String, Value>, String> {
    let catalog = catalog();
    let definition = catalog.require_widget(&selection.kind)?;
    let default_display = definition
        .defaults
        .get("display_type")
        .and_then(Value::as_str)
        .map(str::to_string);
    let display = selection.display_type.clone().or(default_display.clone());
    if display != default_display
        && !display
            .as_ref()
            .is_some_and(|display| definition.display_types.contains(display))
    {
        return Err(format!(
            "Unsupported display type: {}",
            display.unwrap_or_default()
        ));
    }
    let mut data = definition.defaults.clone();
    for (key, value) in &selection.preset {
        data.insert(key.clone(), value.clone());
    }
    for initializer in &definition.initializers {
        match initializer.as_str() {
            "courseSizing" => {
                let sizing = definition
                    .course_sizing
                    .expect("courseSizing needs the widget's sizing");
                match course_widget_dimensions(
                    &options.course_points,
                    sizing.max_dimension,
                    sizing.min_dimension,
                ) {
                    Some((width, height)) => {
                        data.insert("width".into(), number(width));
                        data.insert("height".into(), number(height));
                    }
                    // `Object.assign(data, null)` keeps the manifest frame.
                    None => {}
                }
            }
            "pointLabelFontSize" => {
                if let Some(size) = options.scene_font_size {
                    if let Some(label) = data.get_mut("point_label").and_then(Value::as_object_mut)
                    {
                        label.insert("font_size".into(), number(size));
                    }
                }
            }
            other => return Err(format!("Unknown widget initializer: {other}")),
        }
    }
    let Some(display) = display else {
        apply_global_bindings(&mut data, &definition.global_bindings, globals);
        return Ok(convert_units(&data, options.unit_system));
    };
    let display_definition = catalog.require_display(&display);
    for (key, value) in &display_definition.defaults {
        data.insert(key.clone(), value.clone());
    }
    data.insert("display_type".into(), Value::String(display.clone()));
    apply_global_bindings(&mut data, &definition.global_bindings, globals);
    if !display_definition.modes.is_empty() {
        let mode = selection
            .mode
            .as_deref()
            .ok_or_else(|| format!("Unsupported {display} mode: undefined"))?;
        for (key, value) in build_display_mode_update(&display, mode)? {
            data.insert(key, value);
        }
    }
    let mut types: Vec<String> = definition.initial_display_types.clone();
    if !types.contains(&display) {
        types.push(display.clone());
    }
    for kind in types {
        data = init_display_variant(&data, &kind, None, UnitSystem::Metric);
        let variant_display = catalog.require_display(&kind);
        if variant_display.layout_mode == LayoutMode::Boxed {
            let variants = data
                .get_mut("display_variants")
                .and_then(Value::as_object_mut)
                .expect("initialized above");
            let variant = variants
                .get_mut(&kind)
                .and_then(Value::as_object_mut)
                .expect("initialized above");
            apply_global_bindings(variant, &variant_display.global_bindings, globals);
        } else {
            apply_global_bindings(&mut data, &variant_display.global_bindings, globals);
        }
    }
    Ok(convert_units(&data, options.unit_system))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn map(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    fn globals() -> Map<String, Value> {
        map(
            json!({"font_values": "Oxanium.ttf", "font_text": "Oxanium.ttf", "color_values": "#ffffff", "color_text": "#ffffff",
            "color_icons": "#ffffff", "color_units": "#ffffff", "opacity": 1}),
        )
    }

    fn create(kind: &str, display: Option<&str>) -> Map<String, Value> {
        create_widget_defaults(
            &WidgetSelection {
                kind: kind.into(),
                display_type: display.map(str::to_string),
                ..Default::default()
            },
            &globals(),
            &WidgetCreationOptions::default(),
        )
        .unwrap()
    }

    fn config(value: Value) -> OverlayConfig {
        OverlayConfig::from_value(&value).unwrap()
    }

    #[test]
    fn backdrops_keep_shared_fields_at_the_top_and_rectangle_geometry_in_the_variant() {
        let backdrop = create("backdrop", None);
        let rectangle = catalog().require_display("rectangle");
        assert!(backdrop.get("width").is_none());
        assert_eq!(
            backdrop["display_variants"]["rectangle"]["width"],
            rectangle.variant_defaults["width"]
        );
        let resolved = resolve_active_widget_data(&backdrop);
        assert_eq!(resolved["width"], rectangle.variant_defaults["width"]);
        assert_eq!(resolved["display_type"], "rectangle");
    }

    #[test]
    fn rectangle_and_circle_frame_geometry_follow_their_bindings() {
        let backdrop = create("backdrop", None);
        let patch = build_frame_geometry_update(
            &backdrop,
            &map(json!({"x": 120, "y": 140, "width": 320, "height": 180})),
        );
        assert_eq!(patch["x"], 120);
        assert_eq!(patch["display_variants"]["rectangle"]["width"], 320);
        assert!(patch.get("width").is_none());
        let resolved = resolve_active_widget_data(&merge_widget_data(Some(&backdrop), &patch));
        assert_eq!(
            (resolved["width"].clone(), resolved["height"].clone()),
            (json!(320), json!(180))
        );

        let circle = create("backdrop", Some("circle"));
        let resolved = resolve_active_widget_data(&circle);
        assert_eq!(
            (
                resolved["diameter"].clone(),
                resolved["width"].clone(),
                resolved["height"].clone()
            ),
            (json!(200), json!(200), json!(200))
        );
        let patch =
            build_frame_geometry_update(&circle, &map(json!({"width": 240, "height": 220})));
        assert_eq!(patch["display_variants"]["circle"]["diameter"], 240);
        let resolved = resolve_active_widget_data(&merge_widget_data(Some(&circle), &patch));
        assert_eq!(resolved["height"], 240);
    }

    #[test]
    fn init_display_variant_keeps_existing_variants_and_seeds_missing_ones() {
        let backdrop = map(
            json!({"display_type": "rectangle", "display_variants": {"rectangle": {"width": 240, "height": 140, "corner_radius": 8}}}),
        );
        let result = init_display_variant(&backdrop, "circle", Some(&backdrop), UnitSystem::Metric);
        assert_eq!(
            result["display_variants"]["rectangle"],
            backdrop["display_variants"]["rectangle"]
        );
        assert_eq!(
            result["display_variants"]["circle"],
            json!({"diameter": 200})
        );
    }

    #[test]
    fn init_display_variant_seeds_frame_geometry_and_backfills_defaults() {
        let data = map(
            json!({"value": "heading", "display_type": "heading_tape", "width": 600, "height": 120, "rotation": 45}),
        );
        let result = init_display_variant(&data, "heading_tape", Some(&data), UnitSystem::Metric);
        assert_eq!(result["display_variants"]["heading_tape"]["height"], 120);
        assert_eq!(result["display_variants"]["heading_tape"]["rotation"], 45);

        let data = map(
            json!({"value": "speed", "display_type": "linear", "display_variants": {"linear": {"width": 320, "height": 90, "track_corner_radius": 12}}}),
        );
        let linear = &init_display_variant(&data, "linear", Some(&data), UnitSystem::Metric)
            ["display_variants"]["linear"];
        assert_eq!(linear["width"], 320);
        assert_eq!(linear["track_corner_radius"], 12);
        assert_eq!(linear["orientation"], "horizontal");
        assert_eq!(linear["show_min_max_labels"], false);

        let text = map(json!({"value": "heading", "display_type": "text"}));
        assert_eq!(
            init_display_variant(&text, "text", Some(&text), UnitSystem::Metric),
            text
        );
    }

    #[test]
    fn a_new_g_force_variant_inherits_the_value_font() {
        let data = map(json!({"value": "g_force", "display_type": "text", "font": "Roboto.ttf"}));
        let result = init_display_variant(&data, "g_force", Some(&data), UnitSystem::Metric);
        assert_eq!(
            result["display_variants"]["g_force"]["label_font"],
            "Roboto.ttf"
        );
    }

    #[test]
    fn resolve_flattens_boxed_variants_and_leaves_text_alone() {
        let text = map(json!({"value": "speed", "display_type": "text", "font_size": 60}));
        assert_eq!(resolve_active_widget_data(&text), text);
        let tape = map(
            json!({"id": "widget-42", "value": "heading", "display_type": "heading_tape", "tick_color": "#000000",
            "display_variants": {"heading_tape": {"width": 500, "tick_color": "#ff0000"}}}),
        );
        let resolved = resolve_active_widget_data(&tape);
        assert_eq!(resolved["width"], 500);
        assert_eq!(resolved["tick_color"], "#ff0000");
        assert_eq!(resolved["id"], "widget-42");
        let lean = map(
            json!({"value": "lean_angle", "display_type": "lean_angle", "display_variants": {"lean_angle": {"diameter": 180}}}),
        );
        let resolved = resolve_active_widget_data(&lean);
        assert!(resolved.get("width").is_none());
        assert_eq!(resolve_active_widget_data(&resolved), resolved);
    }

    #[test]
    fn ingress_assigns_ids_and_rejects_bad_collections_and_duplicates() {
        let assigned = config(
            json!({"labels": [{"text": "A"}, {"text": "B"}], "values": [{"id": "widget-4", "value": "speed"}]}),
        );
        assert_eq!(assigned.widget_ids(), ["widget-5", "widget-6", "widget-4"]);
        assert!(assigned.rasters.is_empty());
        assert_eq!(
            config(json!({"labels": [{"id": "title"}]})).widget_ids(),
            ["title"]
        );
        assert_eq!(
            OverlayConfig::from_value(&json!({"labels": [{"id": "a"}], "values": [{"id": "a"}]}))
                .unwrap_err(),
            "Duplicate widget ID: a"
        );
        assert_eq!(
            OverlayConfig::from_value(&json!({"labels": {}})).unwrap_err(),
            "Invalid widget collection: labels"
        );
        assert_eq!(
            OverlayConfig::from_value(&json!({"labels": [{"id": " "}]})).unwrap_err(),
            "Invalid widget ID"
        );
        assert_eq!(
            OverlayConfig::from_value(&json!([])).unwrap_err(),
            "Widget config must be an object"
        );
    }

    #[test]
    fn edits_keep_identities() {
        let mut document = config(
            json!({"labels": [{"id": "widget-1", "text": "A", "x": 0, "y": 0}, {"id": "widget-2", "text": "B", "x": 10, "y": 10}]}),
        );
        document
            .update_widgets(&[
                (
                    "widget-1".into(),
                    map(json!({"text": "A2", "color": "#ABC"})),
                ),
                ("widget-2".into(), map(json!({"x": 5}))),
            ])
            .unwrap();
        assert_eq!(document.labels[0]["color"], "#aabbcc");
        assert_eq!(document.labels[1]["x"], 5);
        let before = document.clone();
        assert_eq!(
            document
                .update_widgets(&[
                    ("widget-1".into(), map(json!({"x": 1}))),
                    ("nope".into(), Map::new())
                ])
                .unwrap_err(),
            "Unknown widget ID: nope"
        );
        assert_eq!(document, before, "a failed batch changes nothing");
        document
            .replace_widget("widget-2", &map(json!({"text": "Reset", "x": 100})))
            .unwrap();
        assert_eq!(
            document.labels[1],
            map(json!({"text": "Reset", "x": 100, "id": "widget-2"}))
        );
        let inserted = document.duplicate_widgets(
            &[(Collection::Labels, document.labels[0].clone())],
            (24.0, 24.0),
        );
        assert_eq!(inserted, ["widget-3"]);
        assert_eq!(document.labels[2]["x"], 24);
        document.delete_widgets(&["widget-1".into(), "widget-3".into()]);
        assert_eq!(document.widget_ids(), ["widget-2"]);
    }

    #[test]
    fn selection_helpers_follow_document_order() {
        let order: Vec<String> = ["a", "b", "c"].map(String::from).to_vec();
        assert_eq!(
            normalize_selection_ids(&["c".into(), "x".into(), "a".into()], &order),
            ["a", "c"]
        );
        assert_eq!(primary_selection_id(&order, Some("b")), Some("b".into()));
        assert_eq!(primary_selection_id(&order, Some("x")), Some("c".into()));
        assert_eq!(primary_selection_id(&[], None), None);
    }

    #[test]
    fn units_convert_through_the_manifest_pairs() {
        let speed = map(json!({"value": "speed", "display_unit": "kmh"}));
        assert_eq!(
            convert_units(&speed, UnitSystem::Imperial)["display_unit"],
            "mph"
        );
        assert_eq!(
            convert_units(
                &convert_units(&speed, UnitSystem::Imperial),
                UnitSystem::Metric
            )["display_unit"],
            "kmh"
        );
        let mps = map(json!({"value": "speed", "display_unit": "mps"}));
        assert_eq!(
            convert_units(&mps, UnitSystem::Imperial)["display_unit"],
            "mps",
            "speed keeps m/s"
        );
        let altitude =
            map(json!({"value": "altitude", "display_unit": "m", "starting_altitude": 100}));
        let imperial = convert_units(&altitude, UnitSystem::Imperial);
        assert_eq!(imperial["display_unit"], "ft");
        assert_eq!(imperial["starting_altitude"], 328);
    }

    #[test]
    fn color_fields_normalize() {
        assert_eq!(
            normalize_color_fields(&map(json!({"color": "#ABC", "fill_color": 3, "x": "#abc"}))),
            map(json!({"color": "#aabbcc", "fill_color": "#000000", "x": "#abc"}))
        );
        assert_eq!(normalize_hex_color("#A0B1C2FF", "#000000"), "#a0b1c2");
        assert!(is_color_field_key("color_primary") && !is_color_field_key("colorful"));
    }

    #[test]
    fn edits_store_whole_numbers_as_integers() {
        let mut document = config(json!({"labels": [{"id": "a", "x": 1}]}));
        document.update_widgets(&[("a".into(), map(json!({"x": 100.0, "nested": {"y": 2.5, "z": 3.0}})))]).unwrap();
        assert_eq!(document.labels[0]["x"], json!(100));
        assert_eq!(document.labels[0]["nested"], json!({"y": 2.5, "z": 3}));
    }

    #[test]
    fn numbers_stay_integers_when_whole() {
        assert_eq!(number(12.0), json!(12));
        assert_eq!(number(12.5), json!(12.5));
        assert_eq!(js_round(-2.5), -2.0);
        assert_eq!(js_round(2.5), 3.0);
    }

    #[test]
    fn course_dimensions_measure_the_route() {
        assert_eq!(
            course_widget_dimensions(&[(Some(0.0), Some(0.0))], 400.0, 80.0),
            None
        );
        let (width, height) = course_widget_dimensions(
            &[
                (Some(0.0), Some(0.0)),
                (Some(1.0), Some(2.0)),
                (None, Some(5.0)),
            ],
            400.0,
            80.0,
        )
        .unwrap();
        assert_eq!(height, 200.0);
        assert_eq!(width, 400.0);
    }
}
