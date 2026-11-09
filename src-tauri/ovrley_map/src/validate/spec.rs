//! MapLibre's style validator, `validateStyleMin` from `@maplibre/maplibre-gl-style-spec`, ported so
//! the shell can tell which installed styles MapLibre will load. It reads the same reference at the
//! version the frontend's maplibre-gl ships: `style-spec/v8.json`, written by
//! `scripts/prune-map-style-spec.mjs` (rerun it after updating maplibre-gl).
//!
//! Parity is on acceptance: a style passes exactly when MapLibre reports no errors. Messages follow
//! MapLibre's and are keyed by where in the style they are, without its line numbers. Warnings are
//! not reported, because MapLibre loads a style that only has warnings. Expressions parse and
//! type-check with `maplibre-expr`, which accepts a few constant expressions with unreachable bad
//! branches that MapLibre rejects.
//!
//! Each function names the MapLibre source file it ports. The structure stays close to it, quirks
//! included, so the two can be compared line by line when the reference is refreshed.

use maplibre_expr::{format_number, Color};
use serde_json::{json, Value as Json};
use std::sync::OnceLock;

pub(super) const REFERENCE_JSON: &str = include_str!("../../style-spec/v8.json");

/// The embedded style reference.
pub(super) fn reference() -> &'static Json {
    static REFERENCE: OnceLock<Json> = OnceLock::new();
    REFERENCE.get_or_init(|| {
        serde_json::from_str(REFERENCE_JSON).expect("the embedded style reference is valid JSON")
    })
}

/// Every error MapLibre's `validateStyleMin` finds in `style`. Empty means MapLibre loads it.
pub fn style_errors(style: &Json) -> Vec<String> {
    let mut validator = Validator {
        spec: reference(),
        style,
        errors: Vec::new(),
    };
    // validate_style.min.ts
    validator.object(
        "",
        style,
        &reference()["$root"],
        Some(Elements {
            named: &["glyphs"],
            star: true,
            validate: &mut |validator, spec_key, _, key, value| {
                if spec_key == "glyphs" {
                    validator.glyphs(key, value);
                }
            },
        }),
    );
    if truthy(style.get("constants")) {
        validator.error("constants", "constants have been deprecated as of v8");
    }
    validator.errors
}

/// Where a value sits, which decides how its expressions are read: MapLibre's `expressionContext`.
#[derive(Clone, Copy)]
pub(super) enum Context<'k> {
    None,
    Property { layout: bool, name: &'k str },
    Filter,
    ClusterMap,
    ClusterReduce,
}

/// How [`Validator::object`] hands elements to their own validators: MapLibre's
/// `objectElementValidators`. `None` where MapLibre passes none.
pub(super) struct Elements<'c, 's> {
    /// Elements with their own validator, which are also exempt from the `required` check.
    pub(super) named: &'c [&'c str],
    /// Whether an element with neither a validator nor a spec goes to `validate` too, as `"*"`,
    /// rather than being an unknown property.
    pub(super) star: bool,
    /// Called with the element's spec key (`"*"` for the catch-all), its key in the object, its full
    /// key and its value.
    pub(super) validate: &'c mut dyn FnMut(&mut Validator<'s>, &str, &str, &str, &'s Json),
}

pub(super) struct Validator<'s> {
    pub(super) spec: &'static Json,
    pub(super) style: &'s Json,
    pub(super) errors: Vec<String>,
}

impl<'s> Validator<'s> {
    pub(super) fn error(&mut self, key: &str, message: impl AsRef<str>) {
        let message = message.as_ref();
        self.errors.push(if key.is_empty() {
            message.to_string()
        } else {
            format!("{key}: {message}")
        });
    }

    /// validate/validate.ts
    pub(super) fn validate(
        &mut self,
        key: &str,
        value: &'s Json,
        spec: &Json,
        context: Context<'_>,
    ) {
        let has_expression = truthy(spec.get("expression"));
        if has_expression && value.is_object() {
            return self.function(key, value, spec);
        }
        if has_expression && maplibre_expr::is_expression(value) {
            return self.expression(key, value, Some(spec), context);
        }
        match spec.get("type").and_then(Json::as_str) {
            Some("*") => {}
            Some("array") => self.array(key, value, spec),
            Some("boolean") => self.boolean(key, value),
            Some("number") => self.number(key, value, spec),
            Some("color") => self.color(key, value),
            Some("constants") => {
                if truthy(Some(value)) {
                    self.error(key, "constants have been deprecated as of v8");
                }
            }
            Some("enum") => self.enumeration(key, value, spec),
            Some("filter") => self.filter(key, value),
            Some("function") => self.function(key, value, spec),
            Some("layer") => self.layer(key, value, 0),
            Some("object") => self.object(key, value, spec, None),
            Some("source") => self.source(key, value),
            Some("light") => self.light(key, value),
            Some("sky") => self.sky(key, value),
            Some("terrain") => self.terrain(key, value),
            Some("projection") => self.projection(key, value),
            Some("projectionDefinition") => self.projection_definition(key, value),
            Some("string") => self.string(key, value),
            // validate_formatted.ts, validate_image.ts
            Some("formatted" | "resolvedImage") => {
                if !value.is_string() {
                    self.expression(key, value, Some(spec), context);
                }
            }
            Some("padding") => self.padding(key, value),
            Some("numberArray") => self.number_array(key, value),
            Some("colorArray") => self.color_array(key, value),
            Some("variableAnchorOffsetCollection") => {
                self.variable_anchor_offset_collection(key, value)
            }
            Some("sprite") => self.sprite(key, value),
            Some("state") => self.state(key, value),
            Some("fontFaces") => self.font_faces(key, value),
            Some(other) => {
                let spec = &self.spec[other];
                self.object(key, value, spec, None);
            }
            None => self.object(key, value, spec, None),
        }
    }

    /// validate/validate_object.ts
    pub(super) fn object(
        &mut self,
        key: &str,
        value: &'s Json,
        specs: &Json,
        mut elements: Option<Elements<'_, 's>>,
    ) {
        let Some(object) = value.as_object() else {
            self.error(key, format!("object expected, {} found", type_name(value)));
            return;
        };
        let named = elements.as_ref().map_or(&[][..], |elements| elements.named);
        let star = elements.as_ref().is_some_and(|elements| elements.star);
        for (name, element) in object {
            let spec_key = name.split('.').next().unwrap_or(name);
            let element_key = join_key(key, name);
            let own = if named.contains(&spec_key) {
                Some(spec_key)
            } else if specs.get(spec_key).is_some_and(|spec| truthy(Some(spec))) {
                None
            } else if star {
                Some("*")
            } else {
                None
            };
            if let (Some(own), Some(elements)) = (own, elements.as_mut()) {
                (elements.validate)(self, own, name, &element_key, element);
            } else if let Some(spec) = specs.get(spec_key).filter(|spec| truthy(Some(spec))) {
                self.validate(&element_key, element, spec, Context::None);
            } else if let Some(spec) = specs.get("*") {
                self.validate(&element_key, element, spec, Context::None);
            } else {
                self.error(key, format!("unknown property \"{name}\""));
            }
        }
        let Some(specs) = specs.as_object() else {
            return;
        };
        for (name, spec) in specs {
            if named.contains(&name.as_str()) {
                continue;
            }
            if truthy(spec.get("required"))
                && spec.get("default").is_none()
                && !object.contains_key(name)
            {
                self.error(key, format!("missing required property \"{name}\""));
            }
        }
    }

    /// validate/validate_array.ts
    pub(super) fn array(&mut self, key: &str, value: &'s Json, spec: &Json) {
        let Some(array) = value.as_array() else {
            self.error(key, format!("array expected, {} found", type_name(value)));
            return;
        };
        if let Some(length) = spec
            .get("length")
            .and_then(Json::as_u64)
            .filter(|length| *length > 0)
        {
            if array.len() as u64 != length {
                self.error(
                    key,
                    format!(
                        "array length {length} expected, length {} found",
                        array.len()
                    ),
                );
                return;
            }
        }
        let element_spec = match spec.get("value") {
            Some(value_spec @ Json::Object(_)) => value_spec.clone(),
            value_spec => {
                let mut element_spec = serde_json::Map::new();
                if let Some(value_spec) = value_spec {
                    element_spec.insert("type".into(), value_spec.clone());
                }
                if let Some(values) = spec.get("values") {
                    element_spec.insert("values".into(), values.clone());
                }
                Json::Object(element_spec)
            }
        };
        for (index, item) in array.iter().enumerate() {
            let item_key = format!("{key}[{index}]");
            if element_spec["type"] == "layer" {
                self.layer(&item_key, item, index);
            } else {
                self.validate(&item_key, item, &element_spec, Context::None);
            }
        }
    }

    /// validate/validate_number.ts
    pub(super) fn number(&mut self, key: &str, value: &Json, spec: &Json) {
        let Some(number) = value.as_f64() else {
            self.error(key, format!("number expected, {} found", type_name(value)));
            return;
        };
        if let Some(minimum) = spec.get("minimum").and_then(Json::as_f64) {
            if number < minimum {
                self.error(
                    key,
                    format!(
                        "{} is less than the minimum value {}",
                        format_number(number),
                        format_number(minimum)
                    ),
                );
                return;
            }
        }
        if let Some(maximum) = spec.get("maximum").and_then(Json::as_f64) {
            if number > maximum {
                self.error(
                    key,
                    format!(
                        "{} is greater than the maximum value {}",
                        format_number(number),
                        format_number(maximum)
                    ),
                );
            }
        }
    }

    /// validate/validate_string.ts
    pub(super) fn string(&mut self, key: &str, value: &Json) {
        if !value.is_string() {
            self.error(key, format!("string expected, {} found", type_name(value)));
        }
    }

    /// validate/validate_boolean.ts
    pub(super) fn boolean(&mut self, key: &str, value: &Json) {
        if !value.is_boolean() {
            self.error(key, format!("boolean expected, {} found", type_name(value)));
        }
    }

    /// validate/validate_color.ts
    pub(super) fn color(&mut self, key: &str, value: &Json) {
        match value.as_str() {
            None => self.error(key, format!("color expected, {} found", type_name(value))),
            Some(color) if Color::parse(color).is_none() => {
                self.error(key, format!("color expected, \"{color}\" found"))
            }
            Some(_) => {}
        }
    }

    /// validate/validate_enum.ts. The reference lists `$root.version`'s values as an array and every
    /// other enum's as an object.
    pub(super) fn enumeration(&mut self, key: &str, value: &Json, spec: &Json) {
        let (allowed, listed): (bool, Vec<String>) = match &spec["values"] {
            Json::Array(values) => (
                values.iter().any(|allowed| js_equal(allowed, value)),
                values.iter().map(js_display).collect(),
            ),
            Json::Object(values) => (
                value
                    .as_str()
                    .is_some_and(|value| values.contains_key(value)),
                values.keys().cloned().collect(),
            ),
            _ => (false, Vec::new()),
        };
        if !allowed {
            self.error(
                key,
                format!("expected one of [{}], {value} found", listed.join(", ")),
            );
        }
    }

    /// validate/validate_padding.ts
    pub(super) fn padding(&mut self, key: &str, value: &'s Json) {
        let Some(values) = value.as_array() else {
            return self.number(key, value, &json!({}));
        };
        if values.is_empty() || values.len() > 4 {
            self.error(
                key,
                format!(
                    "padding requires 1 to 4 values; {} values found",
                    values.len()
                ),
            );
            return;
        }
        for (index, item) in values.iter().enumerate() {
            self.validate(
                &format!("{key}[{index}]"),
                item,
                &json!({ "type": "number" }),
                Context::None,
            );
        }
    }

    /// validate/validate_number_array.ts
    pub(super) fn number_array(&mut self, key: &str, value: &'s Json) {
        let Some(values) = value.as_array() else {
            return self.number(key, value, &json!({}));
        };
        if values.is_empty() {
            self.error(key, "array length at least 1 expected, length 0 found");
            return;
        }
        for (index, item) in values.iter().enumerate() {
            self.validate(
                &format!("{key}[{index}]"),
                item,
                &json!({ "type": "number" }),
                Context::None,
            );
        }
    }

    /// validate/validate_color_array.ts
    pub(super) fn color_array(&mut self, key: &str, value: &Json) {
        let Some(values) = value.as_array() else {
            return self.color(key, value);
        };
        if values.is_empty() {
            self.error(key, "array length at least 1 expected, length 0 found");
            return;
        }
        for (index, item) in values.iter().enumerate() {
            self.color(&format!("{key}[{index}]"), item);
        }
    }

    /// validate/validate_variable_anchor_offset_collection.ts
    pub(super) fn variable_anchor_offset_collection(&mut self, key: &str, value: &'s Json) {
        let Some(values) = value
            .as_array()
            .filter(|values| !values.is_empty() && values.len() % 2 == 0)
        else {
            self.error(
                key,
                "variableAnchorOffsetCollection requires a non-empty array of even length",
            );
            return;
        };
        let anchor = &self.spec["layout_symbol"]["text-anchor"];
        for index in (0..values.len()).step_by(2) {
            self.enumeration(&format!("{key}[{index}]"), &values[index], anchor);
            self.array(
                &format!("{key}[{}]", index + 1),
                &values[index + 1],
                &json!({ "length": 2, "value": "number" }),
            );
        }
    }
}

/// `options.key` joined with an element's, as MapLibre builds them.
pub(super) fn join_key(key: &str, element: &str) -> String {
    if key.is_empty() {
        element.to_string()
    } else {
        format!("{key}.{element}")
    }
}

/// util/get_type.ts
pub(super) fn type_name(value: &Json) -> &'static str {
    match value {
        Json::Null => "null",
        Json::Bool(_) => "boolean",
        Json::Number(_) => "number",
        Json::String(_) => "string",
        Json::Array(_) => "array",
        Json::Object(_) => "object",
    }
}

/// Whether JavaScript reads the value as true. Absent is `undefined`.
pub(super) fn truthy(value: Option<&Json>) -> bool {
    match value {
        None | Some(Json::Null) => false,
        Some(Json::Bool(value)) => *value,
        Some(Json::Number(number)) => number.as_f64().is_some_and(|number| number != 0.0),
        Some(Json::String(value)) => !value.is_empty(),
        Some(Json::Array(_) | Json::Object(_)) => true,
    }
}

/// JavaScript's `===` on JSON scalars. Arrays and objects are never equal, as two parsed ones are
/// distinct objects.
pub(super) fn js_equal(a: &Json, b: &Json) -> bool {
    match (a, b) {
        (Json::Number(a), Json::Number(b)) => a.as_f64() == b.as_f64(),
        (Json::Array(_) | Json::Object(_), _) | (_, Json::Array(_) | Json::Object(_)) => false,
        (a, b) => a == b,
    }
}

/// A value as JavaScript's template strings write it.
pub(super) fn js_display(value: &Json) -> String {
    match value {
        Json::Null => "null".to_string(),
        Json::Bool(value) => value.to_string(),
        Json::Number(number) => number
            .as_f64()
            .map_or_else(|| number.to_string(), format_number),
        Json::String(value) => value.clone(),
        Json::Array(values) => values.iter().map(js_display).collect::<Vec<_>>().join(","),
        Json::Object(_) => "[object Object]".to_string(),
    }
}

/// util/properties.ts `supportsPropertyExpression`
pub(super) fn supports_property_expression(spec: &Json) -> bool {
    matches!(
        spec["property-type"].as_str(),
        Some("data-driven" | "cross-faded-data-driven")
    )
}

/// util/properties.ts `supportsZoomExpression`
pub(super) fn supports_zoom_expression(spec: &Json) -> bool {
    spec["expression"]["parameters"]
        .as_array()
        .is_some_and(|parameters| parameters.iter().any(|parameter| parameter == "zoom"))
}

/// util/properties.ts `supportsInterpolation`
pub(super) fn supports_interpolation(spec: &Json) -> bool {
    truthy(spec["expression"].get("interpolated"))
}

// --- validate/validate_layer.ts, validate_property.ts and validate_filter.ts, with the filter
// classification from feature_filter/index.ts.

impl<'s> Validator<'s> {
    /// `index` is the layer's place in `layers`: an id may not repeat one before it.
    pub(super) fn layer(&mut self, key: &str, layer: &'s Json, index: usize) {
        let Some(object) = layer.as_object() else {
            self.error(key, format!("object expected, {} found", type_name(layer)));
            return;
        };
        if !truthy(layer.get("type")) && !truthy(layer.get("ref")) {
            self.error(key, "either \"type\" or \"ref\" is required");
        }
        let mut layer_type = layer.get("type");
        let (style, reference) = (self.style, self.spec);
        let layers = style["layers"].as_array().map_or(&[][..], Vec::as_slice);
        if let Some(id) = layer.get("id").filter(|id| truthy(Some(id))) {
            for other in layers.iter().take(index) {
                if other.get("id").is_some_and(|other| js_equal(other, id)) {
                    self.error(
                        key,
                        format!("duplicate layer id \"{}\", previously used", js_display(id)),
                    );
                }
            }
        }
        let id = layer
            .get("id")
            .map_or_else(|| "undefined".to_string(), js_display);
        let is =
            |layer_type: Option<&Json>, name: &str| layer_type.is_some_and(|value| value == name);

        if let Some(reference) = layer.get("ref") {
            for property in ["type", "source", "source-layer", "filter", "layout"] {
                if object.contains_key(property) {
                    self.error(key, format!("\"{property}\" is prohibited for ref layers"));
                }
            }
            // MapLibre keeps the last match.
            let parent = layers
                .iter()
                .rfind(|other| other.get("id").is_some_and(|id| js_equal(id, reference)));
            match parent {
                None => self.error(
                    key,
                    format!("ref layer \"{}\" not found", js_display(reference)),
                ),
                Some(parent) if truthy(parent.get("ref")) => {
                    self.error(key, "ref cannot reference another ref layer")
                }
                Some(parent) => layer_type = parent.get("type"),
            }
        } else if !is(layer_type, "background") {
            match layer.get("source").filter(|source| truthy(Some(source))) {
                None => self.error(key, "missing required property \"source\""),
                Some(source_name) => {
                    let name = js_display(source_name);
                    let source = style["sources"]
                        .get(&name)
                        .filter(|source| truthy(Some(source)));
                    let source_type = source.and_then(|source| source.get("type"));
                    let message = match source {
                        None => Some(format!("source \"{name}\" not found")),
                        Some(_) if is(source_type, "vector") && is(layer_type, "raster") => {
                            Some(format!("layer \"{id}\" requires a raster source"))
                        }
                        Some(_)
                            if !is(source_type, "raster-dem")
                                && (is(layer_type, "hillshade") || is(layer_type, "color-relief")) =>
                        {
                            Some(format!("layer \"{id}\" requires a raster-dem source"))
                        }
                        Some(_) if is(source_type, "raster") && !is(layer_type, "raster") => {
                            Some(format!("layer \"{id}\" requires a vector source"))
                        }
                        Some(_)
                            if is(source_type, "vector") && !truthy(layer.get("source-layer")) =>
                        {
                            Some(format!("layer \"{id}\" must specify a \"source-layer\""))
                        }
                        Some(_)
                            if is(source_type, "raster-dem")
                                && !is(layer_type, "hillshade")
                                && !is(layer_type, "color-relief") =>
                        {
                            Some("raster-dem source can only be used with layer type 'hillshade' or 'color-relief'.".to_string())
                        }
                        Some(source)
                            if is(layer_type, "line")
                                && truthy(layer.get("paint"))
                                && truthy(layer["paint"].get("line-gradient"))
                                && (!is(source_type, "geojson")
                                    || !truthy(source.get("lineMetrics"))) =>
                        {
                            Some(format!("layer \"{id}\" specifies a line-gradient, which requires a GeoJSON source with `lineMetrics` enabled."))
                        }
                        Some(_) => None,
                    };
                    if let Some(message) = message {
                        self.error(key, message);
                    }
                }
            }
        }

        if is(layer_type, "raster")
            && truthy(layer["paint"].get("resampling"))
            && truthy(layer["paint"].get("raster-resampling"))
        {
            self.error(
                key,
                format!("layer \"{id}\" redundantly specifies \"resampling\" and \"raster-resampling\" paint properties, but only one is allowed. It is advised to use \"resampling\"."),
            );
        }

        let layer_type = layer_type.and_then(Json::as_str);
        self.object(
            key,
            layer,
            &reference["layer"],
            Some(Elements {
                named: &["type", "filter", "layout", "paint"],
                star: true,
                validate: &mut |validator, spec_key, _, key, value| match spec_key {
                    "type" => {
                        validator.validate(key, value, &reference["layer"]["type"], Context::None)
                    }
                    "filter" => validator.filter(key, value),
                    "layout" | "paint" => {
                        let layout = spec_key == "layout";
                        validator.object(
                            key,
                            value,
                            &Json::Null,
                            Some(Elements {
                                named: &[],
                                star: true,
                                validate: &mut |validator, _, name, key, value| {
                                    validator.property(key, name, value, layout, layer_type)
                                },
                            }),
                        );
                    }
                    _ => {}
                },
            }),
        );
    }

    /// validate/validate_property.ts. `name` is the property's key in its `layout` or `paint`.
    pub(super) fn property(
        &mut self,
        key: &str,
        name: &str,
        value: &'s Json,
        layout: bool,
        layer_type: Option<&str>,
    ) {
        let section = if layout { "layout" } else { "paint" };
        let reference = self.spec;
        let Some(layer_spec) = layer_type.and_then(|layer_type| {
            reference
                .get(format!("{section}_{layer_type}"))
                .filter(|spec| truthy(Some(spec)))
        }) else {
            return;
        };
        if !layout {
            if let Some(property) = name.strip_suffix("-transition") {
                if truthy(
                    layer_spec
                        .get(property)
                        .and_then(|spec| spec.get("transition")),
                ) {
                    self.validate(key, value, &reference["transition"], Context::None);
                    return;
                }
            }
        }
        let Some(spec) = layer_spec.get(name).filter(|spec| truthy(Some(spec))) else {
            self.error(key, format!("unknown property \"{name}\""));
            return;
        };
        if let Some(token) = value
            .as_str()
            .filter(|_| supports_property_expression(spec) && !truthy(spec.get("tokens")))
            .and_then(|value| value.strip_prefix('{')?.strip_suffix('}'))
            .filter(|token| !token.is_empty() && !token.contains('}'))
        {
            self.error(
                key,
                format!("\"{name}\" does not support interpolation syntax\nUse an identity property function instead: `{{ \"type\": \"identity\", \"property\": {} }}`.", json!(token)),
            );
            return;
        }
        if layer_type == Some("symbol")
            && name == "text-font"
            && value.is_object()
            && value["type"] == "identity"
        {
            self.error(key, "\"text-font\" does not support identity functions");
        }
        self.validate(key, value, spec, Context::Property { layout, name });
    }

    /// validate/validate_filter.ts. Mixed legacy filters are only a warning there, so they are not
    /// reported.
    pub(super) fn filter(&mut self, key: &str, value: &'s Json) {
        if matches!(classify_filter(value), FilterSyntax::Legacy) {
            self.legacy_filter(key, value);
        } else {
            self.expression(key, value, None, Context::Filter);
        }
    }

    pub(super) fn legacy_filter(&mut self, key: &str, value: &'s Json) {
        let Some(filter) = value.as_array() else {
            self.error(key, format!("array expected, {} found", type_name(value)));
            return;
        };
        if filter.is_empty() {
            self.error(key, "filter array must have at least 1 element");
            return;
        }
        let reference = self.spec;
        self.enumeration(
            &format!("{key}[0]"),
            &filter[0],
            &reference["filter_operator"],
        );
        let operator = js_display(&filter[0]);
        let comparison = matches!(filter[0].as_str(), Some("<" | "<=" | ">" | ">="));
        let equality = comparison || matches!(filter[0].as_str(), Some("==" | "!="));
        let membership = equality || matches!(filter[0].as_str(), Some("in" | "!in"));
        if comparison && filter.len() >= 2 && filter[1] == "$type" {
            self.error(
                key,
                format!("\"$type\" cannot be use with operator \"{operator}\""),
            );
        }
        if equality && filter.len() != 3 {
            self.error(
                key,
                format!("filter array for operator \"{operator}\" must have 3 elements"),
            );
        }
        if membership {
            if filter.len() >= 2 && !filter[1].is_string() {
                self.error(
                    &format!("{key}[1]"),
                    format!("string expected, {} found", type_name(&filter[1])),
                );
            }
            for (index, item) in filter.iter().enumerate().skip(2) {
                let item_key = format!("{key}[{index}]");
                if filter[1] == "$type" {
                    self.enumeration(&item_key, item, &reference["geometry_type"]);
                } else if !matches!(item, Json::String(_) | Json::Number(_) | Json::Bool(_)) {
                    self.error(
                        &item_key,
                        format!(
                            "string, number, or boolean expected, {} found",
                            type_name(item)
                        ),
                    );
                }
            }
            return;
        }
        match filter[0].as_str() {
            Some("any" | "all" | "none") => {
                for (index, item) in filter.iter().enumerate().skip(1) {
                    self.legacy_filter(&format!("{key}[{index}]"), item);
                }
            }
            Some("has" | "!has") => {
                if filter.len() != 2 {
                    self.error(
                        key,
                        format!("filter array for \"{operator}\" operator must have 2 elements"),
                    );
                } else if !filter[1].is_string() {
                    self.error(
                        &format!("{key}[1]"),
                        format!("string expected, {} found", type_name(&filter[1])),
                    );
                }
            }
            _ => {}
        }
    }
}

/// Which syntax a filter node is written in: feature_filter/index.ts `FilterClassification`.
#[derive(Clone, Copy)]
pub(super) enum FilterSyntax {
    Expression,
    Legacy,
    /// Reads the same either way, so it says nothing about the tree around it.
    Neutral,
}

/// feature_filter/index.ts `classifyFilter`. Anything but legacy validates as an expression.
pub(super) fn classify_filter(filter: &Json) -> FilterSyntax {
    if filter.is_boolean() {
        return FilterSyntax::Neutral;
    }
    let Some(filter) = filter.as_array().filter(|filter| !filter.is_empty()) else {
        return FilterSyntax::Legacy;
    };
    match filter[0].as_str() {
        Some("has") => {
            if filter.len() < 2 || filter[1] == "$id" || filter[1] == "$type" {
                FilterSyntax::Legacy
            } else if filter.len() == 2 {
                FilterSyntax::Neutral
            } else {
                FilterSyntax::Expression
            }
        }
        Some("in") => {
            if filter.len() >= 3 && (!filter[1].is_string() || filter[2].is_array()) {
                FilterSyntax::Expression
            } else {
                FilterSyntax::Legacy
            }
        }
        Some("!in" | "!has" | "none") => FilterSyntax::Legacy,
        Some("==" | "!=" | ">" | ">=" | "<" | "<=") => {
            if filter.len() != 3 || filter[1].is_array() || filter[2].is_array() {
                FilterSyntax::Expression
            } else {
                FilterSyntax::Legacy
            }
        }
        Some("any" | "all") => {
            let mut legacy = false;
            for child in &filter[1..] {
                match classify_filter(child) {
                    FilterSyntax::Expression => return FilterSyntax::Expression,
                    FilterSyntax::Legacy => legacy = true,
                    FilterSyntax::Neutral => {}
                }
            }
            if legacy {
                FilterSyntax::Legacy
            } else {
                FilterSyntax::Neutral
            }
        }
        _ => FilterSyntax::Expression,
    }
}

#[cfg(test)]
pub(super) fn is_legacy_filter(filter: &Json) -> bool {
    matches!(classify_filter(filter), FilterSyntax::Legacy)
}

// --- validate/validate_source.ts and validate_raster_dem_source.ts.

pub(super) const CUSTOM_ENCODING_KEYS: [&str; 4] =
    ["redFactor", "greenFactor", "blueFactor", "baseShift"];

impl<'s> Validator<'s> {
    pub(super) fn source(&mut self, key: &str, value: &'s Json) {
        if !truthy(value.get("type")) {
            self.error(key, "\"type\" is required");
            return;
        }
        let reference = self.spec;
        match value["type"].as_str() {
            Some(source_type @ ("vector" | "raster")) => self.object(
                key,
                value,
                &reference[format!("source_{source_type}")],
                Some(Elements {
                    named: &["promoteId"],
                    star: false,
                    validate: &mut |validator, _, _, key, value| validator.promote_id(key, value),
                }),
            ),
            Some("raster-dem") => self.raster_dem_source(key, value),
            Some("geojson") => {
                self.object(
                    key,
                    value,
                    &reference["source_geojson"],
                    Some(Elements {
                        named: &["promoteId"],
                        star: false,
                        validate: &mut |validator, _, _, key, value| {
                            validator.promote_id(key, value)
                        },
                    }),
                );
                if truthy(value.get("cluster")) {
                    self.cluster_properties(key, value);
                }
            }
            Some(source_type @ ("video" | "image")) => self.object(
                key,
                value,
                &reference[format!("source_{source_type}")],
                None,
            ),
            Some("canvas") => self.error(
                key,
                "Please use runtime APIs to add canvas sources, rather than including them in stylesheets.",
            ),
            _ => self.enumeration(
                &format!("{key}.type"),
                &value["type"],
                &json!({ "values": ["vector", "raster", "raster-dem", "geojson", "video", "image"] }),
            ),
        }
    }

    pub(super) fn promote_id(&mut self, key: &str, value: &Json) {
        match value {
            Json::String(_) => {}
            Json::Object(ids) => {
                for (layer, id) in ids {
                    self.string(&join_key(key, layer), id);
                }
            }
            Json::Array(ids) => {
                for (index, id) in ids.iter().enumerate() {
                    self.string(&join_key(key, &index.to_string()), id);
                }
            }
            _ => {}
        }
    }

    pub(super) fn cluster_properties(&mut self, key: &str, source: &Json) {
        let Some(properties) = source.get("clusterProperties").and_then(Json::as_object) else {
            return;
        };
        for (property, entry) in properties {
            let Some(entry) = entry.as_array() else {
                self.error(
                    &join_key(key, property),
                    format!("array expected, {} found", type_name(entry)),
                );
                continue;
            };
            let map_key = format!("{key}.{property}.map");
            match entry.get(1) {
                Some(map) => self.expression(&map_key, map, None, Context::ClusterMap),
                None => self.error(&map_key, "'undefined' value invalid. Use null instead."),
            }
            let reduce = match entry.first() {
                Some(Json::String(operator)) => {
                    json!([operator, ["accumulated"], ["get", property]])
                }
                Some(operator) => operator.clone(),
                None => {
                    self.error(
                        &format!("{key}.{property}.reduce"),
                        "'undefined' value invalid. Use null instead.",
                    );
                    continue;
                }
            };
            self.expression(
                &format!("{key}.{property}.reduce"),
                &reduce,
                None,
                Context::ClusterReduce,
            );
        }
    }

    pub(super) fn raster_dem_source(&mut self, key: &str, value: &'s Json) {
        let Some(object) = value.as_object() else {
            self.error(key, format!("object expected, {} found", type_name(value)));
            return;
        };
        let reference = self.spec;
        let spec = &reference["source_raster_dem"];
        let encoding = value.get("encoding");
        let custom = encoding.is_some_and(|encoding| encoding == "custom");
        let encoding_name = if truthy(encoding) {
            format!("\"{}\"", js_display(encoding.unwrap_or(&Json::Null)))
        } else {
            "Default".to_string()
        };
        for (name, property) in object {
            let property_key = join_key(key, name);
            if !custom && CUSTOM_ENCODING_KEYS.contains(&name.as_str()) {
                self.error(
                    &property_key,
                    format!("In \"{key}\": \"{name}\" is only valid when \"encoding\" is set to \"custom\". {encoding_name} encoding found"),
                );
            } else if let Some(property_spec) = spec.get(name).filter(|spec| truthy(Some(spec))) {
                self.validate(&property_key, property, property_spec, Context::None);
            } else {
                self.error(&property_key, format!("unknown property \"{name}\""));
            }
        }
    }
}

// --- validate/validate_function.ts: legacy `{"stops": ...}` property functions, which the
// OpenFreeMap styles still use throughout.

/// What MapLibre's closure keeps across one function's stops.
pub(super) const MAX_CODE_POINT: u32 = 0x10ffff;

impl<'s> Validator<'s> {
    pub(super) fn light(&mut self, key: &str, value: &'s Json) {
        self.top_level_object(key, value, "light", true);
    }

    pub(super) fn sky(&mut self, key: &str, value: &'s Json) {
        self.top_level_object(key, value, "sky", true);
    }

    pub(super) fn terrain(&mut self, key: &str, value: &'s Json) {
        self.top_level_object(key, value, "terrain", false);
    }

    pub(super) fn projection(&mut self, key: &str, value: &'s Json) {
        self.top_level_object(key, value, "projection", false);
    }

    /// An object whose every property has a spec in `section`. Transitionable ones also take a
    /// `-transition` property when `transitions` is set.
    pub(super) fn top_level_object(
        &mut self,
        key: &str,
        value: &'s Json,
        section: &str,
        transitions: bool,
    ) {
        let Some(object) = value.as_object() else {
            self.error(key, format!("object expected, {} found", type_name(value)));
            return;
        };
        let reference = self.spec;
        let specs = &reference[section];
        for (name, property) in object {
            let property_key = join_key(key, name);
            let transition = transitions
                && name.strip_suffix("-transition").is_some_and(|base| {
                    truthy(specs.get(base).and_then(|spec| spec.get("transition")))
                });
            if transition {
                self.validate(
                    &property_key,
                    property,
                    &reference["transition"],
                    Context::None,
                );
            } else if let Some(spec) = specs.get(name).filter(|spec| truthy(Some(spec))) {
                self.validate(&property_key, property, spec, Context::None);
            } else {
                self.error(&property_key, format!("unknown property \"{name}\""));
            }
        }
    }

    pub(super) fn projection_definition(&mut self, key: &str, value: &Json) {
        match value {
            Json::Array(definition) => {
                let projection = definition.len() == 3
                    && definition[0].is_string()
                    && definition[1].is_string()
                    && definition[2].is_number();
                let property_value = definition.first().is_some_and(|head| {
                    ["interpolate", "step", "literal"]
                        .iter()
                        .any(|op| head == op)
                });
                if !projection && !property_value {
                    self.error(
                        key,
                        format!("projection expected, invalid array {value} found"),
                    );
                }
            }
            Json::String(_) => {}
            _ => self.error(
                key,
                format!(
                    "projection expected, invalid type \"{}\" found",
                    type_name(value)
                ),
            ),
        }
    }

    pub(super) fn sprite(&mut self, key: &str, value: &'s Json) {
        let Some(sprites) = value.as_array() else {
            return self.string(key, value);
        };
        let pair = json!({
            "id": { "type": "string", "required": true },
            "url": { "type": "string", "required": true },
        });
        let (mut ids, mut urls): (Vec<&Json>, Vec<&Json>) = (Vec::new(), Vec::new());
        for (index, sprite) in sprites.iter().enumerate() {
            for (field, seen, what) in [("id", &mut ids, "ids"), ("url", &mut urls, "URLs")] {
                let Some(found) = sprite.get(field) else {
                    continue;
                };
                if truthy(Some(found)) && seen.iter().any(|seen| js_equal(seen, found)) {
                    self.error(
                        key,
                        format!(
                            "all the sprites' {what} must be unique, but {} is duplicated",
                            js_display(found)
                        ),
                    );
                }
                seen.push(found);
            }
            self.object(&format!("{key}[{index}]"), sprite, &pair, None);
        }
    }

    pub(super) fn glyphs(&mut self, key: &str, value: &Json) {
        let Some(url) = value.as_str() else {
            return self.string(key, value);
        };
        if !url.contains("{fontstack}") {
            self.error(key, "\"glyphs\" url must include a \"{fontstack}\" token");
        }
        if !url.contains("{range}") {
            self.error(key, "\"glyphs\" url must include a \"{range}\" token");
        }
    }

    pub(super) fn font_faces(&mut self, key: &str, value: &'s Json) {
        let Some(fonts) = value.as_object() else {
            self.error(key, format!("object expected, {} found", type_name(value)));
            return;
        };
        let face_spec = json!({
            "url": { "type": "string", "required": true },
            "unicode-range": { "type": "array", "value": "string" },
        });
        for (name, font) in fonts {
            let font_key = join_key(key, name);
            match font {
                Json::String(_) => {}
                Json::Array(faces) => {
                    for (index, face) in faces.iter().enumerate() {
                        let face_key = format!("{font_key}[{index}]");
                        self.object(&face_key, face, &face_spec, None);
                        let Some(ranges) = face.get("unicode-range").and_then(Json::as_array)
                        else {
                            continue;
                        };
                        for (range_index, range) in ranges.iter().enumerate() {
                            self.unicode_range(
                                &format!("{face_key}.unicode-range[{range_index}]"),
                                range,
                            );
                        }
                    }
                }
                _ => self.error(
                    &font_key,
                    format!("string or array expected, {} found", type_name(font)),
                ),
            }
        }
    }

    /// validate_font_faces.ts `validateUnicodeRange`, without its regular expression: `U+26`,
    /// `U+0-7F` or `U+4??`, with at most six hex digits either side.
    pub(super) fn unicode_range(&mut self, key: &str, value: &Json) {
        let Some(range) = value.as_str() else {
            return;
        };
        let hex = |digits: &str| {
            (1..=6).contains(&digits.len()) && digits.bytes().all(|byte| byte.is_ascii_hexdigit())
        };
        let lower = range.to_ascii_lowercase();
        let parsed = lower.strip_prefix("u+").and_then(|body| {
            if let Some(first_wildcard) = body.find('?') {
                let (digits, wildcards) = body.split_at(first_wildcard);
                let valid = digits.len() <= 5
                    && digits.bytes().all(|byte| byte.is_ascii_hexdigit())
                    && wildcards.len() <= 6
                    && wildcards.bytes().all(|byte| byte == b'?')
                    && body.len() <= 6;
                return valid.then_some(None);
            }
            let (start, end) = body
                .split_once('-')
                .map_or((body, None), |(start, end)| (start, Some(end)));
            if !hex(start) || end.is_some_and(|end| !hex(end)) {
                return None;
            }
            let parse = |digits: &str| u32::from_str_radix(digits, 16).ok();
            let start = parse(start)?;
            Some(Some((start, end.map_or(Some(start), parse)?)))
        });
        match parsed {
            None => self.error(
                key,
                "invalid unicode range, expected a value such as \"U+26\", \"U+0-10FFFF\" or \"U+4??\"",
            ),
            Some(Some((start, end))) if start > MAX_CODE_POINT || end > MAX_CODE_POINT => self.error(
                key,
                "unicode range is out of bounds, the maximum code point is U+10FFFF",
            ),
            Some(Some((start, end))) if start > end => self.error(
                key,
                format!("unicode range start must not be greater than its end, but {range} is"),
            ),
            Some(_) => {}
        }
    }

    pub(super) fn state(&mut self, key: &str, value: &Json) {
        if !value.is_object() {
            self.error(key, format!("object expected, {} found", type_name(value)));
        }
    }
}

// --- validate/validate_expression.ts, with `createExpression` and `createPropertyExpression` from
// expression/index.ts. `maplibre-expr` parses and type-checks; the checks MapLibre makes on the
// parsed tree are ported here over its `Expr`.
