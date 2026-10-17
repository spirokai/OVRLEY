//! Generate typed activity contracts from the shared widget manifests.

use serde_json::{Map, Value};
use std::{collections::BTreeMap, fmt::Write, fs, path::Path};

fn valid_name_byte(ch: u8) -> bool {
    ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == b'_'
}

fn variant(key: &str) -> String {
    assert!(
        key.bytes().all(valid_name_byte),
        "activity column must be snake_case: {key}"
    );
    let mut name = String::new();
    for word in key.split('_') {
        let mut chars = word.chars();
        name.extend(
            chars
                .next()
                .expect("activity column name must have nonempty words")
                .to_uppercase(),
        );
        name.extend(chars);
    }
    name
}

fn object<'a>(value: &'a Value, field: &str) -> &'a Map<String, Value> {
    value[field]
        .as_object()
        .expect("activity manifest field must be an object")
}

fn boolean(value: &Value, field: &str) -> bool {
    value[field]
        .as_bool()
        .expect("activity manifest field must be a boolean")
}

fn interpolation(value: &Value) -> &'static str {
    match value["interpolation"]
        .as_str()
        .expect("activity interpolation is required")
    {
        "linear" => "Linear",
        "angular" => "Angular",
        "hold" => "Hold",
        "preserve" => "Preserve",
        policy => panic!("unknown activity interpolation: {policy}"),
    }
}

fn idle_assignment(value: &Value) -> Option<&'static str> {
    match value["idleValue"]
        .as_str()
        .expect("numeric idleValue is required")
    {
        "preserve" => None,
        "zero" => Some("Some(0.0)"),
        "missing" => Some("None"),
        policy => panic!("unknown activity idleValue: {policy}"),
    }
}

fn optional_string(value: &Value, field: &str) -> String {
    match value
        .get(field)
        .expect("activity metadata field is required")
    {
        Value::Null => "None".into(),
        Value::String(text) => format!("Some({text:?})"),
        _ => panic!("activity {field} must be a string or null"),
    }
}

fn names(output: &mut String, fields: &BTreeMap<String, String>, kind: &str) {
    for (field, field_kind) in fields {
        if field_kind == kind {
            write!(output, "{field},").unwrap();
        }
    }
}

fn widget_collection<'a>(widget: &'a Value, manifest: &'a Value) -> &'a str {
    widget
        .get("collection")
        .unwrap_or(&manifest["widgetDefaults"]["collection"])
        .as_str()
        .expect("widget collection must be a string")
}

fn generate_metric_kinds(metrics: &Value, widgets: &Value, out: &Path) {
    let mut kinds = BTreeMap::new();
    for manifest in [metrics, widgets] {
        for widget in manifest["definitions"]
            .as_array()
            .expect("widget definitions must be an array")
        {
            let numeric = widget["activity"]["sources"]
                .as_array()
                .is_some_and(nonempty);
            if widget_collection(widget, manifest) == "values" || numeric {
                let key = widget["type"]
                    .as_str()
                    .expect("widget type must be a string");
                assert!(
                    kinds.insert(key.to_owned(), variant(key)).is_none(),
                    "duplicate metric kind: {key}"
                );
            }
        }
    }
    let mut generated = String::from("#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]\npub enum MetricKind {\n");
    for (key, name) in &kinds {
        writeln!(generated, "#[serde(rename = {key:?})] {name},").unwrap();
    }
    generated.push_str("}\nimpl MetricKind { pub fn as_str(self) -> &'static str { match self {\n");
    for (key, name) in &kinds {
        writeln!(generated, "Self::{name} => {key:?},").unwrap();
    }
    generated.push_str("} } }\n");
    fs::write(out.join("metric_kinds.rs"), generated).unwrap();
}

fn nonempty(values: &Vec<Value>) -> bool {
    !values.is_empty()
}

fn generate_struct_macros(
    generated: &mut String,
    numeric: &BTreeMap<String, Value>,
    input: &BTreeMap<String, String>,
    samples: &BTreeMap<String, String>,
) {
    generated.push_str("#[macro_export]\nmacro_rules! numeric_activity_struct {\n");
    for mode in ["parsed", "plain"] {
        write!(generated, "({mode}, $(#[$meta:meta])* $visibility:vis struct $name:ident {{ $($fields:tt)* }}) => {{ $(#[$meta])* $visibility struct $name {{").unwrap();
        for (key, definition) in numeric {
            if mode == "parsed" {
                match definition["inputFormat"].as_str().expect("numeric inputFormat is required") {
                    "numeric" => generated.push_str("#[serde(default)]"),
                    "balance" => generated.push_str("#[serde(default, deserialize_with = \"deserialize_optional_numeric_or_balance_series\")]"),
                    format => panic!("unsupported numeric inputFormat: {format}"),
                }
            }
            write!(generated, "pub {key}: Vec<Option<f64>>,").unwrap();
        }
        generated.push_str("$($fields)* } };\n");
    }
    generated.push_str("}\n#[macro_export]\nmacro_rules! input_activity_struct {\n");
    for (mode, fields) in [("columns", input), ("samples", samples)] {
        write!(generated, "({mode}, $(#[$meta:meta])* $visibility:vis struct $name:ident {{ $($fields:tt)* }}) => {{ $(#[$meta])* $visibility struct $name {{").unwrap();
        for (key, kind) in fields {
            if mode == "samples" {
                write!(generated, "#[serde(default)] pub {key}: Option<{kind}>,").unwrap();
            } else {
                write!(generated, "pub {key}: Vec<Option<{kind}>>,").unwrap();
            }
        }
        generated.push_str("$($fields)* }");
        if mode == "samples" {
            generated.push_str("impl $name { pub(crate) fn apply_idle_values(&mut self) {");
            for (key, definition) in numeric {
                if samples.contains_key(key) {
                    if let Some(value) = idle_assignment(definition) {
                        write!(generated, "self.{key} = {value};").unwrap();
                    }
                }
            }
            generated.push_str("} }");
        }
        generated.push_str(" };\n");
    }
    generated.push_str("}\n");
}

fn collect_column(
    numeric: &mut Map<String, Value>,
    other: &mut Map<String, Value>,
    key: &str,
    column: &Value,
    policy: Option<&Value>,
) {
    let mut definition = column
        .as_object()
        .expect("activity column must be an object")
        .clone();
    if let Some(policy) = policy {
        assert!(
            definition
                .insert("interpolation".into(), policy.clone())
                .is_none(),
            "interpolation must have one owner"
        );
    }
    let columns = match definition.get("specialized") {
        None | Some(Value::Bool(false)) => numeric,
        Some(Value::Bool(true)) => other,
        _ => panic!("activity specialized must be a boolean"),
    };
    assert!(
        columns
            .insert(key.to_owned(), Value::Object(definition))
            .is_none(),
        "duplicate activity column: {key}"
    );
}

fn load_schema(metrics: &Value, widgets: &Value) -> Value {
    let mut raw = metrics["activitySeries"].clone();
    let mut numeric = Map::new();
    let mut other = Map::new();
    let mut coverage = Map::new();
    for (key, column) in object(&raw, "auxiliary") {
        collect_column(&mut numeric, &mut other, key, column, None);
    }
    for manifest in [metrics, widgets] {
        for widget in manifest["definitions"]
            .as_array()
            .expect("widget definitions must be an array")
        {
            if let Some(column) = widget["activity"].get("column") {
                let key = widget["type"]
                    .as_str()
                    .expect("widget type must be a string");
                collect_column(
                    &mut numeric,
                    &mut other,
                    key,
                    column,
                    widget.get("interpolation"),
                );
            }
            if let Some(source) = widget["activity"].get("coverageSource") {
                let key = widget["type"]
                    .as_str()
                    .expect("widget type must be a string");
                source.as_str().expect("coverageSource must be a string");
                assert!(
                    coverage.insert(key.into(), source.clone()).is_none(),
                    "duplicate coverage source"
                );
            }
        }
    }
    raw["numeric"] = Value::Object(numeric);
    raw["other"] = Value::Object(other);
    raw["coverageSources"] = Value::Object(coverage);
    raw
}

fn validate_widget_telemetry(
    metrics: &Value,
    widgets: &Value,
    numeric: &BTreeMap<String, Value>,
    other: &Map<String, Value>,
) {
    for (index, manifest) in [metrics, widgets].into_iter().enumerate() {
        for widget in manifest["definitions"]
            .as_array()
            .expect("widget definitions must be an array")
        {
            let required =
                index == 0 || widget["collection"] == "values" || widget["collection"] == "plots";
            let activity = match widget.get("activity") {
                Some(activity) => activity
                    .as_object()
                    .expect("widget activity must be an object"),
                None if !required => continue,
                None => panic!("activity metadata is required for telemetry widgets"),
            };
            for field in ["sources", "requires"] {
                for value in activity
                    .get(field)
                    .expect("widget telemetry field is required")
                    .as_array()
                    .expect("widget telemetry field must be an array")
                {
                    let key = value.as_str().expect("activity source must be a string");
                    assert!(
                        numeric.contains_key(key) || other.contains_key(key),
                        "unknown activity column: {key}"
                    );
                    assert!(
                        field != "sources" || numeric.contains_key(key),
                        "numeric source must refer to a numeric column: {key}"
                    );
                }
            }
            let mut policy = None;
            for source in activity["sources"].as_array().unwrap() {
                let source = &numeric[source.as_str().unwrap()];
                let next = (interpolation(source), boolean(source, "limitToObserved"));
                if let Some(policy) = policy {
                    assert_eq!(
                        policy, next,
                        "alternate sources must have the same sampling policy"
                    );
                }
                policy = Some(next);
            }
        }
    }
}

pub fn generate(metrics: &Value, widgets: &Value, out: &Path) {
    generate_metric_kinds(metrics, widgets, out);
    let raw = load_schema(metrics, widgets);
    let mut numeric = BTreeMap::new();
    let mut input = BTreeMap::new();
    let mut samples = BTreeMap::new();
    for (key, kind) in object(&raw, "inputColumns") {
        variant(key);
        let kind = kind
            .as_str()
            .expect("input column type must be a string")
            .to_owned();
        assert!(
            matches!(kind.as_str(), "f64" | "String" | "i64"),
            "unsupported activity input type"
        );
        input.insert(key.clone(), kind.clone());
        samples.insert(key.clone(), kind);
    }
    for (key, definition) in object(&raw, "numeric") {
        variant(key);
        let mut resolved = object(&raw, "numericDefaults").clone();
        resolved.extend(
            definition
                .as_object()
                .expect("numeric series definition must be an object")
                .clone(),
        );
        let resolved = Value::Object(resolved);
        interpolation(&resolved);
        boolean(&resolved, "limitToObserved");
        boolean(&resolved, "finalize");
        optional_string(&resolved, "unit");
        optional_string(&resolved, "smoothing");
        idle_assignment(&resolved);
        if boolean(&resolved, "input") {
            input.insert(key.clone(), "f64".into());
        }
        if boolean(&resolved, "rawSample") {
            assert!(
                boolean(&resolved, "input"),
                "raw sample field requires an input column"
            );
            samples.insert(key.clone(), "f64".into());
        }
        numeric.insert(key.clone(), resolved);
    }
    for (key, definition) in object(&raw, "other") {
        variant(key);
        interpolation(definition);
        optional_string(definition, "unit");
        match &definition["inputType"] {
            Value::String(kind) => {
                assert!(
                    matches!(kind.as_str(), "String" | "i64"),
                    "unsupported activity input type"
                );
                input.insert(key.clone(), kind.clone());
                if boolean(definition, "rawSample") {
                    samples.insert(key.clone(), kind.clone());
                }
            }
            Value::Null => assert!(
                !boolean(definition, "rawSample"),
                "raw sample field requires an input column"
            ),
            _ => panic!("activity inputType must be a string or null"),
        }
    }

    validate_widget_telemetry(metrics, widgets, &numeric, object(&raw, "other"));

    let mut generated = String::from("#[macro_export]\nmacro_rules! activity_series { ($consumer:ident $(, $args:tt)*) => { $consumer! { $($args)* numeric {\n");
    for key in numeric.keys() {
        write!(generated, "{key}: {},", variant(key)).unwrap();
    }
    generated.pop();
    generated.push_str("} other {\n");
    for key in object(&raw, "other").keys() {
        write!(generated, "{key}: {},", variant(key)).unwrap();
    }
    generated.pop();
    generated.push_str("} } }; }\n");

    // Rust macros expand complete struct expressions; callers still supply all nonnumeric fields.
    generated.push_str("#[macro_export]\nmacro_rules! numeric_activity { ($kind:ident, $context:expr, $method:ident, { $($field:ident: $value:expr),* $(,)? }) => { $kind {\n");
    for key in numeric.keys() {
        write!(
            generated,
            "{key}: ($context).$method($crate::model::telemetry::ActivitySeries::{}),",
            variant(key)
        )
        .unwrap();
    }
    generated.push_str("$($field: $value,)* } }; }\n");

    generated.push_str("#[macro_export]\nmacro_rules! activity_input_columns { ($consumer:ident) => { $consumer! {\n");
    for (group, fields) in [("", &input), ("samples", &samples)] {
        if !group.is_empty() {
            generated.push_str("samples {");
        }
        for (label, kind) in [("numeric", "f64"), ("text", "String"), ("integer", "i64")] {
            write!(generated, "{label} {{").unwrap();
            names(&mut generated, fields, kind);
            generated.push('}');
        }
        if !group.is_empty() {
            generated.push('}');
        }
    }
    generated.push_str("sources {");
    for key in numeric.keys() {
        if input.contains_key(key) {
            write!(generated, "{key}: {},", variant(key)).unwrap();
        }
    }
    generated.push_str("} absent {");
    for key in input.keys() {
        if !samples.contains_key(key) {
            write!(generated, "{key},").unwrap();
        }
    }
    generated.push_str("} } }; }\n");

    generated.push_str("impl ActivitySeries {\npub const ALL: &'static [Self] = &[\n");
    for key in numeric.keys().chain(object(&raw, "other").keys()) {
        write!(generated, "Self::{},", variant(key)).unwrap();
    }
    generated.push_str("];\npub fn as_str(self) -> &'static str { match self {\n");
    for key in numeric.keys().chain(object(&raw, "other").keys()) {
        writeln!(generated, "Self::{} => {key:?},", variant(key)).unwrap();
    }
    generated.push_str("} }\npub fn interpolation(self) -> InterpolationKind { match self {\n");
    for (key, definition) in numeric.iter().chain(object(&raw, "other").iter()) {
        writeln!(
            generated,
            "Self::{} => InterpolationKind::{},",
            variant(key),
            interpolation(definition)
        )
        .unwrap();
    }
    generated.push_str("} }\npub fn limit_to_observed(self) -> bool { match self {\n");
    for (key, definition) in &numeric {
        writeln!(
            generated,
            "Self::{} => {},",
            variant(key),
            boolean(definition, "limitToObserved")
        )
        .unwrap();
    }
    generated
        .push_str("_ => panic!(\"observed bounds apply only to numeric activity columns\"), } }\n");
    generated.push_str("pub fn finalized(self) -> bool { match self {\n");
    for (key, definition) in &numeric {
        writeln!(
            generated,
            "Self::{} => {},",
            variant(key),
            boolean(definition, "finalize")
        )
        .unwrap();
    }
    generated
        .push_str("_ => false, } }\npub fn unit(self) -> Option<&'static str> { match self {\n");
    for (key, definition) in numeric.iter().chain(object(&raw, "other").iter()) {
        writeln!(
            generated,
            "Self::{} => {},",
            variant(key),
            optional_string(definition, "unit")
        )
        .unwrap();
    }
    generated.push_str("} }\npub fn smoothing(self) -> Option<&'static str> { match self {\n");
    for (key, definition) in &numeric {
        writeln!(
            generated,
            "Self::{} => {},",
            variant(key),
            optional_string(definition, "smoothing")
        )
        .unwrap();
    }
    generated.push_str("_ => None, } }\n}\n");
    generated.push_str("pub const ZERO_AVAILABLE_ACTIVITY_SERIES: &[&str] = &[\n");
    for (key, definition) in &numeric {
        if boolean(definition, "zeroIsAvailable") {
            writeln!(generated, "{key:?},").unwrap();
        }
    }
    generated.push_str("];\n");
    for (group, constant) in [
        ("core", "CORE_ACTIVITY_ATTRIBUTES"),
        ("extended", "EXTENDED_ACTIVITY_ATTRIBUTES"),
    ] {
        write!(generated, "pub const {constant}: &[&str] = &[").unwrap();
        for attribute in raw["attributes"][group]
            .as_array()
            .expect("activity attributes must be arrays")
        {
            write!(
                generated,
                "{:?},",
                attribute
                    .as_str()
                    .expect("activity attribute must be a string")
            )
            .unwrap();
        }
        generated.push_str("];\n");
    }
    generated.push_str("pub const ACTIVITY_COVERAGE_SOURCES: &[(&str, &str)] = &[");
    for (key, source) in object(&raw, "coverageSources") {
        let source = source.as_str().expect("coverageSource must be a string");
        assert!(
            numeric.contains_key(source) || object(&raw, "other").contains_key(source),
            "unknown coverage source: {source}"
        );
        write!(generated, "({key:?}, {source:?}),").unwrap();
    }
    generated.push_str("];\n");
    generate_struct_macros(&mut generated, &numeric, &input, &samples);
    fs::write(out.join("activity_series.rs"), generated).unwrap();
}
