//! MapLibre's style validator, `validateStyleMin` from `@maplibre/maplibre-gl-style-spec`, ported so
//! the shell can tell which installed styles MapLibre will load. It reads the same reference at the
//! version the frontend's maplibre-gl ships: see `style-spec/ATTRIBUTION.md`.
//!
//! Parity is on acceptance: a style passes exactly when MapLibre reports no errors. Messages follow
//! MapLibre's and are keyed by where in the style they are, without its line numbers. Warnings are
//! not reported, because MapLibre loads a style that only has warnings. Expressions parse and
//! type-check with `maplibre-expr`, which accepts a few constant expressions with unreachable bad
//! branches that MapLibre rejects.
//!
//! Each function names the MapLibre source file it ports. The structure stays close to it, quirks
//! included, so the two can be compared line by line when the reference is refreshed.

use maplibre_expr::{format_number, parse, typecheck, Color, Expr, ParseErrorKind, Type};
use serde_json::{json, Value as Json};
use std::sync::OnceLock;

const REFERENCE_JSON: &str = include_str!("../style-spec/v8.json");

fn reference() -> &'static Json {
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
enum Context<'k> {
    None,
    Property { layout: bool, name: &'k str },
    Filter,
    ClusterMap,
    ClusterReduce,
}

/// How [`Validator::object`] hands elements to their own validators: MapLibre's
/// `objectElementValidators`. `None` where MapLibre passes none.
struct Elements<'c, 's> {
    /// Elements with their own validator, which are also exempt from the `required` check.
    named: &'c [&'c str],
    /// Whether an element with neither a validator nor a spec goes to `validate` too, as `"*"`,
    /// rather than being an unknown property.
    star: bool,
    /// Called with the element's spec key (`"*"` for the catch-all), its key in the object, its full
    /// key and its value.
    validate: &'c mut dyn FnMut(&mut Validator<'s>, &str, &str, &str, &'s Json),
}

struct Validator<'s> {
    spec: &'static Json,
    style: &'s Json,
    errors: Vec<String>,
}

impl<'s> Validator<'s> {
    fn error(&mut self, key: &str, message: impl AsRef<str>) {
        let message = message.as_ref();
        self.errors.push(if key.is_empty() {
            message.to_string()
        } else {
            format!("{key}: {message}")
        });
    }

    /// validate/validate.ts
    fn validate(&mut self, key: &str, value: &'s Json, spec: &Json, context: Context<'_>) {
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
    fn object(
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
    fn array(&mut self, key: &str, value: &'s Json, spec: &Json) {
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
    fn number(&mut self, key: &str, value: &Json, spec: &Json) {
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
    fn string(&mut self, key: &str, value: &Json) {
        if !value.is_string() {
            self.error(key, format!("string expected, {} found", type_name(value)));
        }
    }

    /// validate/validate_boolean.ts
    fn boolean(&mut self, key: &str, value: &Json) {
        if !value.is_boolean() {
            self.error(key, format!("boolean expected, {} found", type_name(value)));
        }
    }

    /// validate/validate_color.ts
    fn color(&mut self, key: &str, value: &Json) {
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
    fn enumeration(&mut self, key: &str, value: &Json, spec: &Json) {
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
    fn padding(&mut self, key: &str, value: &'s Json) {
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
    fn number_array(&mut self, key: &str, value: &'s Json) {
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
    fn color_array(&mut self, key: &str, value: &Json) {
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
    fn variable_anchor_offset_collection(&mut self, key: &str, value: &'s Json) {
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
fn join_key(key: &str, element: &str) -> String {
    if key.is_empty() {
        element.to_string()
    } else {
        format!("{key}.{element}")
    }
}

/// util/get_type.ts
fn type_name(value: &Json) -> &'static str {
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
fn truthy(value: Option<&Json>) -> bool {
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
fn js_equal(a: &Json, b: &Json) -> bool {
    match (a, b) {
        (Json::Number(a), Json::Number(b)) => a.as_f64() == b.as_f64(),
        (Json::Array(_) | Json::Object(_), _) | (_, Json::Array(_) | Json::Object(_)) => false,
        (a, b) => a == b,
    }
}

/// A value as JavaScript's template strings write it.
fn js_display(value: &Json) -> String {
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
fn supports_property_expression(spec: &Json) -> bool {
    matches!(
        spec["property-type"].as_str(),
        Some("data-driven" | "cross-faded-data-driven")
    )
}

/// util/properties.ts `supportsZoomExpression`
fn supports_zoom_expression(spec: &Json) -> bool {
    spec["expression"]["parameters"]
        .as_array()
        .is_some_and(|parameters| parameters.iter().any(|parameter| parameter == "zoom"))
}

/// util/properties.ts `supportsInterpolation`
fn supports_interpolation(spec: &Json) -> bool {
    truthy(spec["expression"].get("interpolated"))
}

// --- validate/validate_layer.ts, validate_property.ts and validate_filter.ts, with the filter
// classification from feature_filter/index.ts.

impl<'s> Validator<'s> {
    /// `index` is the layer's place in `layers`: an id may not repeat one before it.
    fn layer(&mut self, key: &str, layer: &'s Json, index: usize) {
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
    fn property(
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
    fn filter(&mut self, key: &str, value: &'s Json) {
        if matches!(classify_filter(value), FilterSyntax::Legacy) {
            self.legacy_filter(key, value);
        } else {
            self.expression(key, value, None, Context::Filter);
        }
    }

    fn legacy_filter(&mut self, key: &str, value: &'s Json) {
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
enum FilterSyntax {
    Expression,
    Legacy,
    /// Reads the same either way, so it says nothing about the tree around it.
    Neutral,
}

/// feature_filter/index.ts `classifyFilter`. Anything but legacy validates as an expression.
fn classify_filter(filter: &Json) -> FilterSyntax {
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
fn is_legacy_filter(filter: &Json) -> bool {
    matches!(classify_filter(filter), FilterSyntax::Legacy)
}

// --- validate/validate_source.ts and validate_raster_dem_source.ts.

const CUSTOM_ENCODING_KEYS: [&str; 4] = ["redFactor", "greenFactor", "blueFactor", "baseShift"];

impl<'s> Validator<'s> {
    fn source(&mut self, key: &str, value: &'s Json) {
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

    fn promote_id(&mut self, key: &str, value: &Json) {
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

    fn cluster_properties(&mut self, key: &str, source: &Json) {
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

    fn raster_dem_source(&mut self, key: &str, value: &'s Json) {
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
struct Stops<'v> {
    /// The spec of the property the function sets.
    value_spec: &'v Json,
    function_type: Option<&'v Json>,
    zoom_and_property: bool,
    key_type: Option<&'static str>,
    previous_value: Option<f64>,
    previous_zoom: Option<Json>,
    domain_values: Vec<Json>,
}

impl Stops<'_> {
    fn is(&self, function_type: &str) -> bool {
        self.function_type
            .is_some_and(|value| value == function_type)
    }
}

impl<'s> Validator<'s> {
    fn function(&mut self, key: &str, value: &'s Json, value_spec: &Json) {
        let function_type = value.get("type");
        let is_zoom_function = !function_type.is_some_and(|value| value == "categorical")
            && value.get("property").is_none();
        let stops_value = value.get("stops");
        let zoom_and_property = stops_value
            .and_then(Json::as_array)
            .and_then(|stops| stops.first())
            .and_then(Json::as_array)
            .and_then(|stop| stop.first())
            .is_some_and(Json::is_object);
        let mut stops = Stops {
            value_spec,
            function_type,
            zoom_and_property,
            key_type: None,
            previous_value: None,
            previous_zoom: None,
            domain_values: Vec::new(),
        };
        let function_spec = &self.spec["function"];
        self.object(
            key,
            value,
            function_spec,
            Some(Elements {
                named: &["stops", "default"],
                star: false,
                validate: &mut |validator, spec_key, _, key, value| {
                    if spec_key == "stops" {
                        validator.function_stops(&mut stops, key, value);
                    } else {
                        validator.validate(key, value, stops.value_spec, Context::None);
                    }
                },
            }),
        );

        let is = |name: &str| function_type.is_some_and(|value| value == name);
        if is("identity") && is_zoom_function {
            self.error(key, "missing required property \"property\"");
        }
        if !is("identity") && !truthy(stops_value) {
            self.error(key, "missing required property \"stops\"");
        }
        if is("exponential")
            && truthy(value_spec.get("expression"))
            && !supports_interpolation(value_spec)
        {
            self.error(key, "exponential functions not supported");
        }
        if !is_zoom_function && !supports_property_expression(value_spec) {
            self.error(key, "property functions not supported");
        } else if is_zoom_function && !supports_zoom_expression(value_spec) {
            self.error(key, "zoom functions not supported");
        }
        if (is("categorical") || zoom_and_property) && value.get("property").is_none() {
            self.error(key, "\"property\" property is required");
        }
    }

    fn function_stops(&mut self, stops: &mut Stops<'_>, key: &str, value: &'s Json) {
        if stops.is("identity") {
            self.error(key, "identity function may not have a \"stops\" property");
            return;
        }
        // validateArray with `validateFunctionStop` for each element: the stops spec has no length.
        let Some(list) = value.as_array() else {
            self.error(key, format!("array expected, {} found", type_name(value)));
            return;
        };
        for (index, stop) in list.iter().enumerate() {
            self.function_stop(stops, &format!("{key}[{index}]"), stop);
        }
        if list.is_empty() {
            self.error(key, "array must have at least one stop");
        }
    }

    fn function_stop(&mut self, stops: &mut Stops<'_>, key: &str, value: &'s Json) {
        let Some(stop) = value.as_array() else {
            self.error(key, format!("array expected, {} found", type_name(value)));
            return;
        };
        if stop.len() != 2 {
            self.error(
                key,
                format!("array length 2 expected, length {} found", stop.len()),
            );
            return;
        }
        if stops.zoom_and_property {
            let Some(domain) = stop[0].as_object() else {
                self.error(
                    key,
                    format!("object expected, {} found", type_name(&stop[0])),
                );
                return;
            };
            let Some(zoom) = domain.get("zoom") else {
                self.error(key, "object stop key must have zoom");
                return;
            };
            if !domain.contains_key("value") {
                self.error(key, "object stop key must have value");
                return;
            }
            let previous = stops.previous_zoom.as_ref().and_then(Json::as_f64);
            if let (Some(previous), Some(zoom)) = (previous, zoom.as_f64()) {
                if previous != 0.0 && previous > zoom {
                    self.error(key, "stop zoom values must appear in ascending order");
                    return;
                }
            }
            if !stops
                .previous_zoom
                .as_ref()
                .is_some_and(|previous| js_equal(previous, zoom))
            {
                stops.previous_zoom = Some(zoom.clone());
                stops.previous_value = None;
                stops.domain_values.clear();
            }
            self.object(
                &format!("{key}[0]"),
                &stop[0],
                &json!({ "zoom": {} }),
                Some(Elements {
                    named: &["zoom", "value"],
                    star: false,
                    validate: &mut |validator, spec_key, _, key, value| {
                        if spec_key == "zoom" {
                            validator.number(key, value, &json!({}));
                        } else {
                            validator.stop_domain_value(stops, key, value);
                        }
                    },
                }),
            );
        } else {
            self.stop_domain_value(stops, &format!("{key}[0]"), &stop[0]);
        }
        let output_key = format!("{key}[1]");
        if maplibre_expr::is_expression(&stop[1]) {
            self.error(
                &output_key,
                "expressions are not allowed in function stops.",
            );
            return;
        }
        self.validate(&output_key, &stop[1], stops.value_spec, Context::None);
    }

    fn stop_domain_value(&mut self, stops: &mut Stops<'_>, key: &str, value: &Json) {
        let value_type = type_name(value);
        match stops.key_type {
            None => stops.key_type = Some(value_type),
            Some(key_type) if key_type != value_type => {
                self.error(
                    key,
                    format!("{value_type} stop domain type must match previous stop domain type {key_type}"),
                );
                return;
            }
            Some(_) => {}
        }
        if !matches!(value_type, "number" | "string" | "boolean") {
            self.error(
                key,
                "stop domain value must be a number, string, or boolean",
            );
            return;
        }
        let categorical = stops.is("categorical");
        if value_type != "number" && !categorical {
            let mut message = format!("number expected, {value_type} found");
            if supports_property_expression(stops.value_spec) && stops.function_type.is_none() {
                message.push_str(
                    "\nIf you intended to use a categorical function, specify `\"type\": \"categorical\"`.",
                );
            }
            self.error(key, message);
            return;
        }
        let number = value.as_f64();
        if categorical
            && number.is_some_and(|number| !number.is_finite() || number.floor() != number)
        {
            self.error(
                key,
                format!("integer expected, found {}", js_display(value)),
            );
            return;
        }
        match (number, stops.previous_value) {
            (Some(number), Some(previous)) if !categorical && number < previous => {
                self.error(key, "stop domain values must appear in ascending order");
                return;
            }
            _ => stops.previous_value = number,
        }
        if categorical {
            if stops.domain_values.iter().any(|seen| js_equal(seen, value)) {
                self.error(key, "stop domain values must be unique");
                return;
            }
            stops.domain_values.push(value.clone());
        }
    }
}

// --- The style's top-level objects: validate/validate_light.ts, validate_sky.ts,
// validate_terrain.ts, validate_projection.ts, validate_projectiondefinition.ts,
// validate_sprite.ts, validate_glyphs_url.ts, validate_font_faces.ts and validate_state.ts.

const MAX_CODE_POINT: u32 = 0x10ffff;

impl<'s> Validator<'s> {
    fn light(&mut self, key: &str, value: &'s Json) {
        self.top_level_object(key, value, "light", true);
    }

    fn sky(&mut self, key: &str, value: &'s Json) {
        self.top_level_object(key, value, "sky", true);
    }

    fn terrain(&mut self, key: &str, value: &'s Json) {
        self.top_level_object(key, value, "terrain", false);
    }

    fn projection(&mut self, key: &str, value: &'s Json) {
        self.top_level_object(key, value, "projection", false);
    }

    /// An object whose every property has a spec in `section`. Transitionable ones also take a
    /// `-transition` property when `transitions` is set.
    fn top_level_object(&mut self, key: &str, value: &'s Json, section: &str, transitions: bool) {
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

    fn projection_definition(&mut self, key: &str, value: &Json) {
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

    fn sprite(&mut self, key: &str, value: &'s Json) {
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

    fn glyphs(&mut self, key: &str, value: &Json) {
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

    fn font_faces(&mut self, key: &str, value: &'s Json) {
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
    fn unicode_range(&mut self, key: &str, value: &Json) {
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

    fn state(&mut self, key: &str, value: &Json) {
        if !value.is_object() {
            self.error(key, format!("object expected, {} found", type_name(value)));
        }
    }
}

// --- validate/validate_expression.ts, with `createExpression` and `createPropertyExpression` from
// expression/index.ts. `maplibre-expr` parses and type-checks; the checks MapLibre makes on the
// parsed tree are ported here over its `Expr`.

const ZOOM_CURVE_NOT_TOP_LEVEL: &str =
    "\"zoom\" expression may only be used as input to a top-level \"step\" or \"interpolate\" expression.";
const CLUSTER_GLOBALS: &str =
    "\"zoom\" and \"feature-state\" expressions are not supported with cluster properties.";
/// What [`without_zoom`] reads instead of the zoom: a number, and not a constant.
const ZOOM_STAND_IN: &str = "\u{0}zoom";
const ONE_ZOOM_CURVE: &str =
    "Only one zoom-based \"step\" or \"interpolate\" subexpression may be used in an expression.";

impl Validator<'_> {
    /// `spec` is the property's, when there is one: it gives the type the expression must produce.
    fn expression(&mut self, key: &str, value: &Json, spec: Option<&Json>, context: Context<'_>) {
        let expected = spec.and_then(expected_type);
        let coerce_string = spec.is_some_and(|spec| spec["type"] == "string");
        let check = |value: &Json| {
            parse(value).and_then(|expr| typecheck(&expr, expected.as_ref(), coerce_string))
        };
        let parsed = match check(value) {
            // `maplibre-expr` places the zoom in every expression, MapLibre only in properties.
            Err(error)
                if matches!(error.kind, ParseErrorKind::Zoom(_))
                    && !matches!(context, Context::Property { .. }) =>
            {
                if matches!(context, Context::ClusterMap | Context::ClusterReduce) {
                    self.error(key, CLUSTER_GLOBALS);
                    return;
                }
                check(&without_zoom(value))
            }
            parsed => parsed,
        };
        let expr = match parsed {
            Ok(expr) => expr,
            Err(error) => {
                self.error(&format!("{key}{}", error.key), error.to_string());
                return;
            }
        };
        match context {
            Context::Property { layout, name } => {
                let spec = spec.expect("a property expression has its property's spec");
                if let Err(message) = property_expression(&expr, spec) {
                    self.error(key, message);
                } else if name == "text-font" && !output_defined(&expr) {
                    self.error(
                        key,
                        format!("Invalid data expression for \"{name}\". Output values must be contained as literals within the expression."),
                    );
                } else if layout && !global_constant(&expr, &["feature-state"]) {
                    self.error(
                        key,
                        "\"feature-state\" data expressions are not supported with layout properties.",
                    );
                }
            }
            Context::Filter => {
                if !global_constant(&expr, &["feature-state"]) {
                    self.error(
                        key,
                        "\"feature-state\" data expressions are not supported with filters.",
                    );
                }
            }
            Context::ClusterMap | Context::ClusterReduce => {
                if !global_constant(&expr, &["zoom", "feature-state"]) {
                    self.error(key, CLUSTER_GLOBALS);
                }
            }
            Context::None => {}
        }
    }
}

/// The expression with every `["zoom"]` read as a number from the feature instead, so it checks
/// as MapLibre checks a filter or a top-level value: with the zoom anywhere. Literals stay as written.
fn without_zoom(value: &Json) -> Json {
    match value {
        Json::Array(items) if items.len() == 1 && items[0] == "zoom" => {
            json!(["number", ["get", ZOOM_STAND_IN]])
        }
        Json::Array(items) if items.first().is_some_and(|head| head == "literal") => value.clone(),
        Json::Array(items) => Json::Array(items.iter().map(without_zoom).collect()),
        _ => value.clone(),
    }
}

/// What `createPropertyExpression` adds to `createExpression`.
fn property_expression(expr: &Expr, spec: &Json) -> Result<(), &'static str> {
    if !feature_constant(expr) && !supports_property_expression(spec) {
        return Err("data expressions not supported");
    }
    let zoom_constant = global_constant(expr, &["zoom"]);
    if !zoom_constant && !supports_zoom_expression(spec) {
        return Err("zoom expressions not supported");
    }
    match zoom_curve(expr) {
        None if !zoom_constant => Err(ZOOM_CURVE_NOT_TOP_LEVEL),
        Some(Err(message)) => Err(message),
        Some(Ok(Expr::Interpolate { .. })) if !supports_interpolation(spec) => {
            Err("\"interpolate\" expressions cannot be used with this property")
        }
        _ => Ok(()),
    }
}

/// expression/index.ts `getExpectedType`
fn expected_type(spec: &Json) -> Option<Type> {
    fn named(name: &str) -> Option<Type> {
        Some(match name {
            "color" => Type::Color,
            "string" | "enum" => Type::String,
            "number" => Type::Number,
            "boolean" => Type::Boolean,
            "formatted" => Type::Formatted,
            "padding" => Type::Padding,
            "numberArray" => Type::NumberArray,
            "colorArray" => Type::ColorArray,
            "projectionDefinition" => Type::ProjectionDefinition,
            "resolvedImage" => Type::ResolvedImage,
            "variableAnchorOffsetCollection" => Type::VariableAnchorOffsetCollection,
            _ => return None,
        })
    }
    match spec.get("type")?.as_str()? {
        "array" => Some(Type::array(
            spec["value"]
                .as_str()
                .and_then(named)
                .unwrap_or(Type::Value),
            spec["length"].as_u64().map(|length| length as usize),
        )),
        other => named(other),
    }
}

/// Every sub-expression, as MapLibre's `eachChild` visits them.
fn children(expr: &Expr) -> Vec<&Expr> {
    match expr {
        Expr::Literal(_) | Expr::Var(_) | Expr::Within(_) | Expr::Distance(_) => Vec::new(),
        Expr::Call { args, .. } => args.iter().collect(),
        Expr::Let { bindings, body } => bindings
            .iter()
            .map(|(_, value)| value)
            .chain([&**body])
            .collect(),
        Expr::Match {
            input,
            arms,
            default,
        } => [&**input]
            .into_iter()
            .chain(arms.iter().map(|(_, output)| output))
            .chain([&**default])
            .collect(),
        Expr::Step {
            input,
            output0,
            stops,
        } => [&**input, &**output0]
            .into_iter()
            .chain(stops.iter().map(|(_, output)| output))
            .collect(),
        Expr::Interpolate { input, stops, .. } => [&**input]
            .into_iter()
            .chain(stops.iter().map(|(_, output)| output))
            .collect(),
        Expr::Format(sections) => sections
            .iter()
            .flat_map(|section| {
                [Some(&section.content)]
                    .into_iter()
                    .chain([
                        section.scale.as_ref(),
                        section.font.as_ref(),
                        section.text_color.as_ref(),
                        section.vertical_align.as_ref(),
                    ])
                    .flatten()
            })
            .collect(),
        Expr::Collator {
            case_sensitive,
            diacritic_sensitive,
            locale,
        } => [case_sensitive, diacritic_sensitive, locale]
            .into_iter()
            .flatten()
            .map(|expr| &**expr)
            .collect(),
        Expr::NumberFormat {
            value,
            locale,
            currency,
            min_fraction_digits,
            max_fraction_digits,
            unit,
        } => [&**value]
            .into_iter()
            .chain(
                [
                    locale,
                    currency,
                    min_fraction_digits,
                    max_fraction_digits,
                    unit,
                ]
                .into_iter()
                .flatten()
                .map(|expr| &**expr),
            )
            .collect(),
        Expr::Assert(_, inner) | Expr::Coerce(_, inner) => vec![&**inner],
    }
}

/// compound_expression.ts `isFeatureConstant`
fn feature_constant(expr: &Expr) -> bool {
    match expr {
        Expr::Call { op, args } => {
            let reads_feature = (op == "get" && args.len() == 1)
                || op == "feature-state"
                || (op == "has" && args.len() == 1)
                || matches!(op.as_str(), "properties" | "geometry-type" | "id")
                || op.starts_with("filter-");
            !reads_feature && args.iter().all(feature_constant)
        }
        Expr::Within(_) | Expr::Distance(_) => false,
        _ => children(expr).into_iter().all(feature_constant),
    }
}

/// compound_expression.ts `isGlobalPropertyConstant`. With `["feature-state"]` it is
/// `isStateConstant`.
fn global_constant(expr: &Expr, properties: &[&str]) -> bool {
    match expr {
        Expr::Call { op, .. } if properties.contains(&op.as_str()) => false,
        _ => children(expr)
            .into_iter()
            .all(|child| global_constant(child, properties)),
    }
}

/// expression/index.ts `findZoomCurve`: the zoom-driven `step` or `interpolate`, or why the zoom is
/// used where it can't be.
fn zoom_curve(expr: &Expr) -> Option<Result<&Expr, &'static str>> {
    let mut result = match expr {
        Expr::Let { body, .. } => zoom_curve(body),
        Expr::Call { op, args } if op == "coalesce" => args.iter().find_map(zoom_curve),
        Expr::Step { input, .. } | Expr::Interpolate { input, .. } if matches!(&**input, Expr::Call { op, .. } if op == "zoom") => {
            Some(Ok(expr))
        }
        _ => None,
    };
    if let Some(Err(_)) = result {
        return result;
    }
    for child in children(expr) {
        match (result, zoom_curve(child)) {
            (_, Some(Err(message))) => result = Some(Err(message)),
            (None, Some(Ok(_))) => result = Some(Err(ZOOM_CURVE_NOT_TOP_LEVEL)),
            (Some(Ok(found)), Some(Ok(other))) if !std::ptr::eq(found, other) => {
                result = Some(Err(ONE_ZOOM_CURVE))
            }
            _ => {}
        }
    }
    result
}

/// Whether every value the expression can produce is written in it: MapLibre's `outputDefined`. A
/// constant sub-expression counts, as MapLibre has folded it into a literal by then.
fn output_defined(expr: &Expr) -> bool {
    if constant(expr) {
        return true;
    }
    match expr {
        Expr::Literal(_) => true,
        Expr::Assert(_, inner) | Expr::Coerce(_, inner) => output_defined(inner),
        Expr::Call { op, args } => match op.as_str() {
            "case" => args
                .iter()
                .enumerate()
                .filter(|(index, _)| index % 2 == 1 || *index == args.len() - 1)
                .all(|(_, output)| output_defined(output)),
            "coalesce" | "array" | "string" | "number" | "boolean" | "object" | "to-boolean"
            | "to-color" | "to-number" | "to-string" => args.iter().all(output_defined),
            _ => false,
        },
        Expr::Let { body, .. } => output_defined(body),
        Expr::Match { arms, default, .. } => {
            arms.iter().all(|(_, output)| output_defined(output)) && output_defined(default)
        }
        Expr::Step { output0, stops, .. } => {
            output_defined(output0) && stops.iter().all(|(_, output)| output_defined(output))
        }
        Expr::Interpolate { stops, .. } => stops.iter().all(|(_, output)| output_defined(output)),
        _ => false,
    }
}

/// compound_expression.ts `isExpressionConstant`, for a tree `maplibre-expr` has not folded.
fn constant(expr: &Expr) -> bool {
    fn evaluates_alone(expr: &Expr) -> bool {
        match expr {
            Expr::Var(_) | Expr::Within(_) | Expr::Distance(_) | Expr::Collator { .. } => false,
            Expr::Call { op, .. } if matches!(op.as_str(), "error" | "global-state") => false,
            _ => children(expr).into_iter().all(evaluates_alone),
        }
    }
    evaluates_alone(expr)
        && feature_constant(expr)
        && global_constant(
            expr,
            &[
                "zoom",
                "heatmap-density",
                "elevation",
                "line-progress",
                "accumulated",
                "is-supported-script",
            ],
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value as Json};

    const BUNDLED: [(&str, &str); 5] = [
        (
            "bright",
            include_str!("../../../assets/map-styles/bright.json"),
        ),
        ("dark", include_str!("../../../assets/map-styles/dark.json")),
        (
            "fiord",
            include_str!("../../../assets/map-styles/fiord.json"),
        ),
        (
            "liberty",
            include_str!("../../../assets/map-styles/liberty.json"),
        ),
        (
            "positron",
            include_str!("../../../assets/map-styles/positron.json"),
        ),
    ];

    fn style(layers: Json) -> Json {
        json!({
            "version": 8,
            "sources": {
                "openmaptiles": { "type": "vector", "url": "https://example.com/tiles.json" },
                "shade": { "type": "raster", "tiles": ["https://example.com/{z}/{x}/{y}.png"], "tileSize": 256 },
            },
            "glyphs": "https://example.com/{fontstack}/{range}.pbf",
            "layers": layers,
        })
    }

    fn fill(paint: Json) -> Json {
        style(
            json!([{ "id": "water", "type": "fill", "source": "openmaptiles", "source-layer": "water", "paint": paint }]),
        )
    }

    fn assert_valid(style: &Json) {
        let errors = style_errors(style);
        assert!(errors.is_empty(), "{errors:#?}");
    }

    #[track_caller]
    fn assert_error(style: &Json, expected: &str) {
        let errors = style_errors(style);
        assert!(
            errors.iter().any(|error| error.contains(expected)),
            "expected an error containing {expected:?}, got {errors:#?}"
        );
    }

    #[test]
    fn the_bundled_styles_are_valid() {
        for (id, style) in BUNDLED {
            let errors = style_errors(&serde_json::from_str(style).unwrap());
            assert!(errors.is_empty(), "{id}: {errors:#?}");
        }
    }

    #[test]
    fn the_root_needs_version_8_sources_and_layers() {
        assert_error(
            &json!({ "version": 7, "sources": {}, "layers": [] }),
            "version: expected one of [8], 7 found",
        );
        assert_error(
            &json!({ "version": "8", "sources": {}, "layers": [] }),
            "version: expected one of [8]",
        );
        assert_error(
            &json!({ "version": 8, "layers": [] }),
            "missing required property \"sources\"",
        );
        assert_error(
            &json!({ "version": 8, "sources": {} }),
            "missing required property \"layers\"",
        );
        assert_error(&json!([]), "object expected, array found");
        assert_valid(
            &json!({ "version": 8, "sources": {}, "layers": [], "anything": 1, "metadata": { "x": [1] } }),
        );
    }

    #[test]
    fn root_properties_follow_their_specs() {
        let mut glyphs = style(json!([]));
        glyphs["glyphs"] = json!("https://example.com/{range}.pbf");
        assert_error(
            &glyphs,
            "\"glyphs\" url must include a \"{fontstack}\" token",
        );

        let mut constants = style(json!([]));
        constants["constants"] = json!({ "@a": 1 });
        assert_error(&constants, "constants have been deprecated as of v8");

        let mut center = style(json!([]));
        center["center"] = json!([1]);
        assert_error(&center, "center: array length 2 expected, length 1 found");

        let mut light = style(json!([]));
        light["light"] = json!({ "intensity": 2, "color-transition": { "duration": 1 } });
        assert_error(&light, "2 is greater than the maximum value 1");

        let mut sprite = style(json!([]));
        sprite["sprite"] = json!([{ "id": "a", "url": "x" }, { "id": "a", "url": "y" }]);
        assert_error(
            &sprite,
            "all the sprites' ids must be unique, but a is duplicated",
        );

        let mut faces = style(json!([]));
        faces["font-faces"] =
            json!({ "Sans": [{ "url": "x", "unicode-range": ["U+0-7F", "U+4??", "U+110000"] }] });
        let errors = style_errors(&faces);
        assert_eq!(errors.len(), 1, "{errors:#?}");
        assert!(errors[0].contains("out of bounds"));
    }

    #[test]
    fn layers_follow_their_sources() {
        assert_error(
            &style(json!([{ "type": "fill", "source": "openmaptiles", "source-layer": "x" }])),
            "missing required property \"id\"",
        );
        assert_error(
            &style(json!([{ "id": "a", "type": "blob" }])),
            "layers[0].type: expected one of [",
        );
        assert_error(
            &style(json!([{ "id": "a", "type": "fill" }])),
            "missing required property \"source\"",
        );
        assert_error(
            &style(json!([{ "id": "a", "type": "fill", "source": "nope" }])),
            "source \"nope\" not found",
        );
        assert_error(
            &style(json!([{ "id": "a", "type": "fill", "source": "openmaptiles" }])),
            "must specify a \"source-layer\"",
        );
        assert_error(
            &style(json!([{ "id": "a", "type": "raster", "source": "openmaptiles" }])),
            "requires a raster source",
        );
        assert_error(
            &style(json!([{ "id": "a", "type": "fill", "source": "shade" }])),
            "requires a vector source",
        );
        assert_error(
            &style(
                json!([{ "id": "a", "type": "background" }, { "id": "a", "type": "background" }]),
            ),
            "duplicate layer id \"a\"",
        );
        assert_error(
            &style(json!([{ "id": "a", "type": "background", "minzoom": 30 }])),
            "30 is greater than the maximum value 24",
        );
        assert_valid(&style(json!([
            { "id": "a", "type": "background", "interactive": true },
            { "id": "b", "type": "raster", "source": "shade" },
        ])));
    }

    #[test]
    fn properties_follow_their_layer_type() {
        assert_error(
            &fill(json!({ "line-color": "red" })),
            "unknown property \"line-color\"",
        );
        assert_error(
            &fill(json!({ "fill-color": 42 })),
            "fill-color: color expected, number found",
        );
        assert_error(
            &fill(json!({ "fill-color": "not a colour" })),
            "color expected, \"not a colour\" found",
        );
        assert_error(
            &fill(json!({ "fill-opacity": 2 })),
            "2 is greater than the maximum value 1",
        );
        assert_error(
            &fill(json!({ "fill-color": "{colour}" })),
            "does not support interpolation syntax",
        );
        assert_valid(&fill(
            json!({ "fill-color": "hsl(35, 8%, 85%)", "fill-color-transition": { "duration": 0 } }),
        ));
        assert_error(
            &fill(json!({ "fill-antialias-transition": { "duration": 0 } })),
            "unknown property \"fill-antialias-transition\"",
        );
        let line = |layout: Json| {
            style(
                json!([{ "id": "a", "type": "line", "source": "openmaptiles", "source-layer": "x", "layout": layout }]),
            )
        };
        assert_error(
            &line(json!({ "line-cap": "pointy" })),
            "expected one of [butt, round, square], \"pointy\" found",
        );
        assert_valid(&line(json!({ "line-cap": "round", "visibility": "none" })));
    }

    #[test]
    fn expressions_are_parsed_against_their_property() {
        assert_valid(&fill(json!({
            "fill-color": ["interpolate", ["linear"], ["zoom"], 5, "red", 10, ["get", "colour"]],
            "fill-opacity": ["step", ["zoom"], 0, 10, 1],
        })));
        assert_error(
            &fill(json!({ "fill-color": ["+", 1, 2] })),
            "Expected color but found number",
        );
        assert_error(
            &fill(json!({ "fill-antialias": ["get", "aa"] })),
            "data expressions not supported",
        );
        assert_error(
            &fill(json!({ "fill-opacity": ["*", ["zoom"], 0.1] })),
            "\"zoom\" expression may only be used as input to a top-level \"step\" or \"interpolate\" expression.",
        );
        assert_error(
            &fill(
                json!({ "fill-opacity": ["interpolate", ["linear"], ["zoom"], 1, ["step", ["zoom"], 0, 2, 1], 3, 1] }),
            ),
            "Only one zoom-based",
        );
        let symbol = |layout: Json| {
            style(
                json!([{ "id": "a", "type": "symbol", "source": "openmaptiles", "source-layer": "x", "layout": layout }]),
            )
        };
        assert_error(
            &symbol(json!({ "text-font": ["get", "font"] })),
            "Invalid data expression for \"text-font\"",
        );
        assert_valid(&symbol(
            json!({ "text-font": ["step", ["zoom"], ["literal", ["A"]], 10, ["literal", ["B"]]] }),
        ));
        assert_error(
            &symbol(json!({ "text-size": ["feature-state", "size"] })),
            "\"feature-state\" data expressions are not supported with layout properties.",
        );
        assert_error(
            &symbol(json!({ "visibility": ["step", ["zoom"], "none", 10, "visible"] })),
            "zoom expressions not supported",
        );
    }

    #[test]
    fn legacy_functions_follow_their_stops() {
        assert_valid(&fill(
            json!({ "fill-opacity": { "base": 1.2, "stops": [[5, 0], [10, 1]] } }),
        ));
        assert_valid(&fill(
            json!({ "fill-color": { "property": "class", "type": "categorical", "stops": [["a", "red"], ["b", "blue"]] } }),
        ));
        assert_error(
            &fill(json!({ "fill-opacity": { "stops": [[10, 0], [5, 1]] } })),
            "stop domain values must appear in ascending order",
        );
        assert_error(
            &fill(json!({ "fill-opacity": { "stops": [] } })),
            "array must have at least one stop",
        );
        assert_error(
            &fill(json!({ "fill-opacity": { "stops": [[5, 2]] } })),
            "2 is greater than the maximum value 1",
        );
        assert_error(
            &fill(json!({ "fill-opacity": { "stops": [[5, ["get", "x"]]] } })),
            "expressions are not allowed in function stops.",
        );
        assert_error(
            &fill(json!({ "fill-antialias": { "property": "x", "stops": [[1, true]] } })),
            "property functions not supported",
        );
        assert_error(
            &fill(json!({ "fill-color": { "type": "categorical", "stops": [["a", "red"]] } })),
            "\"property\" property is required",
        );
        assert_error(
            &fill(
                json!({ "fill-color": { "property": "x", "type": "categorical", "stops": [[1.5, "red"]] } }),
            ),
            "integer expected, found 1.5",
        );
        assert_error(
            &fill(
                json!({ "fill-color": { "property": "x", "type": "categorical", "stops": [["a", "red"], ["a", "blue"]] } }),
            ),
            "stop domain values must be unique",
        );
        assert_error(
            &fill(json!({ "fill-opacity": { "stops": [[5, 1]], "bogus": 1 } })),
            "unknown property \"bogus\"",
        );
        assert_valid(&fill(
            json!({ "fill-opacity": { "property": "h", "stops": [[{ "zoom": 0, "value": 0 }, 0], [{ "zoom": 10, "value": 1 }, 1]] } }),
        ));
    }

    #[test]
    fn filters_are_legacy_or_expressions() {
        let filtered = |filter: Json| {
            style(
                json!([{ "id": "a", "type": "fill", "source": "openmaptiles", "source-layer": "x", "filter": filter }]),
            )
        };
        assert_valid(&filtered(json!([
            "all",
            ["==", "class", "lake"],
            ["!in", "brunnel", "tunnel"],
            ["has", "name"]
        ])));
        assert_valid(&filtered(json!([
            "all",
            ["==", ["get", "class"], "lake"],
            ["has", "name"]
        ])));
        assert_valid(&filtered(json!(true)));
        // A two-element comparison reads as an expression; only under `none` is it a legacy filter.
        assert_error(
            &filtered(json!(["==", "class"])),
            "Expected 2 or 3 arguments",
        );
        assert_error(
            &filtered(json!(["none", ["==", "class"]])),
            "filter array for operator \"==\" must have 3 elements",
        );
        assert_error(
            &filtered(json!(["<", "$type", "Point"])),
            "\"$type\" cannot be use with operator \"<\"",
        );
        assert_error(
            &filtered(json!(["==", "$type", "Circle"])),
            "Polygon], \"Circle\" found",
        );
        assert_error(
            &filtered(json!(["!has"])),
            "filter array for \"!has\" operator must have 2 elements",
        );
        assert_error(
            &filtered(json!(["==", ["get", "a"]])),
            "Expected 2 or 3 arguments",
        );
        assert_error(
            &filtered(json!(["==", ["feature-state", "a"], 1])),
            "not supported with filters",
        );

        assert!(is_legacy_filter(&json!(["none", ["has", "a"]])));
        assert!(!is_legacy_filter(&json!(["all", ["has", "a"]])));
        assert!(is_legacy_filter(&json!(["in", "class", "a", "b"])));
        assert!(!is_legacy_filter(&json!([
            "in",
            "class",
            ["literal", ["a"]]
        ])));
    }

    #[test]
    fn sources_follow_their_type() {
        let with_source =
            |source: Json| json!({ "version": 8, "sources": { "s": source }, "layers": [] });
        assert_error(
            &with_source(json!({ "url": "x" })),
            "sources.s: \"type\" is required",
        );
        assert_error(
            &with_source(json!({ "type": "canvas" })),
            "Please use runtime APIs",
        );
        assert_error(
            &with_source(json!({ "type": "tiles" })),
            "sources.s.type: expected one of [vector, raster",
        );
        assert_error(
            &with_source(json!({ "type": "raster", "tiles": "x" })),
            "array expected, string found",
        );
        assert_error(
            &with_source(json!({ "type": "geojson" })),
            "missing required property \"data\"",
        );
        assert_error(
            &with_source(json!({ "type": "raster-dem", "redFactor": 1 })),
            "only valid when \"encoding\" is set to \"custom\"",
        );
        assert_valid(&with_source(
            json!({ "type": "vector", "tiles": ["x"], "promoteId": { "a": "id" }, "custom": 1 }),
        ));
        assert_valid(&with_source(
            json!({ "type": "raster-dem", "encoding": "custom", "redFactor": 1 }),
        ));
    }
}
