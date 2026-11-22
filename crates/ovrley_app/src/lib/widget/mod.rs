//! Port of `lib/widget/widget-presentation.js` and `altitude.js`'s activity part: widget lists,
//! translated names, sidebar order and activity-derived placeholders. The catalog-driven document
//! edits of `lib/widget` live in `ovrley_widgets::model::document` (Phase 3 findings).

use crate::i18n::{catalog as translations, Locale};
use crate::lib::format::compare_text;
use ovrley_core::activity::schema::ParsedActivity;
use ovrley_widgets::model::catalog::{catalog, AltitudeSource};
use ovrley_widgets::model::telemetry::{ActivitySeries, NumericActivity};
use ovrley_widgets::model::document::{convert_altitude_value, js_round, widget_id, Collection, OverlayConfig, WidgetData};
use serde_json::Value;

pub mod unit_system;

/// One widget of the document, as the editor lists it (`buildConfigWidgets`).
#[derive(Clone, Debug, PartialEq)]
pub struct ConfigWidget {
    pub id: String,
    pub kind: String,
    pub collection: Collection,
    /// The name field's value (a label's text), else the type.
    pub name: String,
    pub data: WidgetData,
}

/// `buildConfigWidgets`: every widget in document order. Ingress validated each type.
pub fn build_config_widgets(config: &OverlayConfig) -> Vec<ConfigWidget> {
    config
        .widgets()
        .map(|(collection, data)| {
            let kind = collection.widget_type(data).expect("ingress validated widget types");
            let definition = catalog().require_widget(&kind).expect("ingress validated widget types");
            let name = match &definition.name_field {
                None => kind.clone(),
                Some(field) => data.get(field).and_then(Value::as_str).unwrap_or_default().to_string(),
            };
            ConfigWidget {
                id: widget_id(data).to_string(),
                kind,
                collection,
                name,
                data: data.clone(),
            }
        })
        .collect()
}

/// `getWidgetTypeName`.
pub fn widget_type_name(kind: &str, locale: Locale) -> String {
    let definition = catalog().require_widget(kind).expect("a listed widget type is in the catalog");
    translations().t(locale, &definition.name_key).to_string()
}

/// `getActivityAttributeLabel`: a widget's name, else the attribute's own label, else the
/// attribute title-cased.
pub fn activity_attribute_label(attribute: &str, locale: Locale) -> String {
    if let Some(definition) = catalog().widget(attribute) {
        return translations().t(locale, &definition.name_key).to_string();
    }
    let words: Vec<String> = attribute
        .split('_')
        .map(|part| {
            if part == "gps" {
                return "GPS".to_string();
            }
            let mut chars = part.chars();
            chars.next().map(|first| first.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
        })
        .collect();
    let default = words.join(" ");
    translations().t_or(locale, &format!("widgets.activityAttributes.{attribute}"), &default).to_string()
}

/// `groupWidgetsForSidebar`: widgets grouped by type, groups ordered by sidebar priority then name;
/// a widget without a name field takes its type's name.
pub fn group_widgets_for_sidebar(widgets: &[ConfigWidget], locale: Locale) -> Vec<ConfigWidget> {
    struct Group {
        name: String,
        priority: i64,
        widgets: Vec<ConfigWidget>,
    }
    let mut groups: Vec<(String, Group)> = Vec::new();
    for widget in widgets {
        let definition = catalog().require_widget(&widget.kind).expect("listed widgets are in the catalog");
        let type_name = widget_type_name(&widget.kind, locale);
        let index = match groups.iter().position(|(kind, _)| *kind == widget.kind) {
            Some(index) => index,
            None => {
                groups.push((
                    widget.kind.clone(),
                    Group {
                        name: type_name.clone(),
                        priority: definition.sidebar_priority,
                        widgets: Vec::new(),
                    },
                ));
                groups.len() - 1
            }
        };
        let mut named = widget.clone();
        if definition.name_field.is_none() {
            named.name = type_name;
        }
        groups[index].1.widgets.push(named);
    }
    groups.sort_by(|(_, left), (_, right)| left.priority.cmp(&right.priority).then_with(|| compare_text(&left.name, &right.name, locale)));
    groups.into_iter().flat_map(|(_, group)| group.widgets).collect()
}

/// A numeric activity column by its manifest name. The manifest names only numeric columns and
/// `sample_elevations`.
fn activity_series<'a>(activity: &'a ParsedActivity, field: &str) -> &'a [Option<f64>] {
    if field == "sample_elevations" {
        return &activity.sample_elevations;
    }
    let series: ActivitySeries = serde_json::from_value(Value::String(field.to_string())).unwrap_or_else(|_| panic!("altitude source {field} is an activity column"));
    activity.numeric_series(series)
}

/// `getStartingAltitude`: the first altitude of the source's series, in meters.
pub fn starting_altitude(activity: Option<&ParsedActivity>, source: &AltitudeSource) -> Option<f64> {
    let activity = activity?;
    for series in &source.series {
        let values = activity_series(activity, &series.field);
        if values.is_empty() {
            continue;
        }
        if let Some(first) = values.iter().flatten().next() {
            return Some(*first);
        }
        if !series.require_value {
            return None;
        }
    }
    None
}

/// `withAltitudeEditorPresentation`'s placeholder: the activity's starting altitude in the
/// widget's unit, when the widget has an altitude source and no starting altitude of its own.
pub fn starting_altitude_placeholder(widget: &ConfigWidget, activity: Option<&ParsedActivity>) -> Option<f64> {
    let source = catalog().widget(&widget.kind)?.altitude_source.as_ref()?;
    if widget.data.get("starting_altitude").is_some_and(|value| !value.is_null()) {
        return None;
    }
    let meters = starting_altitude(activity, source)?;
    let unit = widget.data.get(&source.unit_field).and_then(Value::as_str).unwrap_or("m");
    Some(js_round(convert_altitude_value(meters, "m", unit)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn config(value: Value) -> OverlayConfig {
        OverlayConfig::from_value(&value).unwrap()
    }

    #[test]
    fn widgets_list_in_collection_order_with_their_names() {
        let widgets = build_config_widgets(&config(json!({
            "plots": [{"id": "p", "value": "course"}], "values": [{"id": "v", "value": "speed"}],
            "labels": [{"id": "l", "text": "Label"}], "backdrops": [{"id": "b", "display_type": "rectangle"}]
        })));
        assert_eq!(widgets.iter().map(|widget| widget.collection).collect::<Vec<_>>(), [Collection::Backdrops, Collection::Labels, Collection::Values, Collection::Plots]);
        assert_eq!(widgets[1].name, "Label");
        assert_eq!(widgets[2].kind, "speed");
    }

    #[test]
    fn the_sidebar_groups_by_type_and_sorts_by_priority_and_name() {
        let widgets = build_config_widgets(&config(json!({
            "backdrops": [{"id": "b", "display_type": "rectangle"}],
            "labels": [{"id": "l1", "text": "Beta"}, {"id": "l2", "text": "Alpha"}],
            "values": [{"id": "s", "value": "speed"}, {"id": "h", "value": "heartrate"}]
        })));
        let sorted = group_widgets_for_sidebar(&widgets, Locale::En);
        assert_eq!(sorted[0].id, "b", "backdrops have the lowest priority");
        let labels: Vec<_> = sorted.iter().filter(|widget| widget.kind == "label").map(|widget| widget.name.as_str()).collect();
        assert_eq!(labels, ["Beta", "Alpha"], "label text stays, in document order");
        let speed = sorted.iter().find(|widget| widget.id == "s").unwrap();
        assert_eq!(speed.name, widget_type_name("speed", Locale::En));
    }

    #[test]
    fn attribute_labels_fall_back_to_title_case() {
        assert_eq!(activity_attribute_label("speed", Locale::En), widget_type_name("speed", Locale::En));
        assert_eq!(activity_attribute_label("gps_fix_quality_xyz", Locale::En), "GPS Fix Quality Xyz");
    }

    #[test]
    fn altitude_placeholders_come_from_the_first_activity_value() {
        let activity = ParsedActivity {
            elevation: vec![None, Some(100.0)],
            ..Default::default()
        };
        let widgets = build_config_widgets(&config(json!({"values": [{"id": "a", "value": "altitude", "display_unit": "ft"}]})));
        assert_eq!(starting_altitude_placeholder(&widgets[0], Some(&activity)), Some(328.0));
        assert_eq!(starting_altitude_placeholder(&widgets[0], None), None);
    }
}
