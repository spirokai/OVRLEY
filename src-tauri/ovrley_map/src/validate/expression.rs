//! The validator's legacy functions and expressions: MapLibre's `validate_function.ts` and
//! `validate_expression.ts`, with the expression checks of `style-spec/expression`.

use super::spec::*;
use maplibre_expr::{parse, typecheck, Expr, ParseErrorKind, Type};
use serde_json::{json, Value as Json};

pub(super) struct Stops<'v> {
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
    pub(super) fn is(&self, function_type: &str) -> bool {
        self.function_type
            .is_some_and(|value| value == function_type)
    }
}

impl<'s> Validator<'s> {
    pub(super) fn function(&mut self, key: &str, value: &'s Json, value_spec: &Json) {
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

    pub(super) fn function_stops(&mut self, stops: &mut Stops<'_>, key: &str, value: &'s Json) {
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

    pub(super) fn function_stop(&mut self, stops: &mut Stops<'_>, key: &str, value: &'s Json) {
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

    pub(super) fn stop_domain_value(&mut self, stops: &mut Stops<'_>, key: &str, value: &Json) {
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

pub(super) const ZOOM_CURVE_NOT_TOP_LEVEL: &str =
    "\"zoom\" expression may only be used as input to a top-level \"step\" or \"interpolate\" expression.";
pub(super) const CLUSTER_GLOBALS: &str =
    "\"zoom\" and \"feature-state\" expressions are not supported with cluster properties.";
/// What [`without_zoom`] reads instead of the zoom: a number, and not a constant.
pub(super) const ZOOM_STAND_IN: &str = "\u{0}zoom";
pub(super) const ONE_ZOOM_CURVE: &str =
    "Only one zoom-based \"step\" or \"interpolate\" subexpression may be used in an expression.";

impl Validator<'_> {
    /// `spec` is the property's, when there is one: it gives the type the expression must produce.
    pub(super) fn expression(
        &mut self,
        key: &str,
        value: &Json,
        spec: Option<&Json>,
        context: Context<'_>,
    ) {
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
/// as MapLibre checks a filter or a top-level value: with the zoom anywhere. Literals and `match`
/// labels are data, not expressions, so they stay as written.
pub(super) fn without_zoom(value: &Json) -> Json {
    match value {
        Json::Array(items) if items.len() == 1 && items[0] == "zoom" => {
            json!(["number", ["get", ZOOM_STAND_IN]])
        }
        Json::Array(items) if items.first().is_some_and(|head| head == "literal") => value.clone(),
        // `["match", input, label, output, ..., fallback]`: labels sit at even places before the last.
        Json::Array(items) if items.first().is_some_and(|head| head == "match") => Json::Array(
            items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    if index >= 2 && index % 2 == 0 && index + 1 < items.len() {
                        item.clone()
                    } else {
                        without_zoom(item)
                    }
                })
                .collect(),
        ),
        Json::Array(items) => Json::Array(items.iter().map(without_zoom).collect()),
        _ => value.clone(),
    }
}

/// What `createPropertyExpression` adds to `createExpression`.
pub(super) fn property_expression(expr: &Expr, spec: &Json) -> Result<(), &'static str> {
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
pub(super) fn expected_type(spec: &Json) -> Option<Type> {
    pub(super) fn named(name: &str) -> Option<Type> {
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
pub(super) fn children(expr: &Expr) -> Vec<&Expr> {
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
pub(super) fn feature_constant(expr: &Expr) -> bool {
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
pub(super) fn global_constant(expr: &Expr, properties: &[&str]) -> bool {
    match expr {
        Expr::Call { op, .. } if properties.contains(&op.as_str()) => false,
        _ => children(expr)
            .into_iter()
            .all(|child| global_constant(child, properties)),
    }
}

/// expression/index.ts `findZoomCurve`: the zoom-driven `step` or `interpolate`, or why the zoom is
/// used where it can't be.
pub(super) fn zoom_curve(expr: &Expr) -> Option<Result<&Expr, &'static str>> {
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
pub(super) fn output_defined(expr: &Expr) -> bool {
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
pub(super) fn constant(expr: &Expr) -> bool {
    pub(super) fn evaluates_alone(expr: &Expr) -> bool {
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
