//! `MetricWidgetEditor` and its display sections (`components/metricWidget/`), as descriptions.

use super::blocks::{full, grid, Cell, CellKind, Node, OptionView, Rule, SwitchRowStyle};
use super::rules::Named;
use super::sections::{
    font_section, icon_section, units_row, Ctx, FontSectionProps, UnitsRowProps,
};
use super::NumericFieldOptions;
use crate::lib::widget::starting_altitude_placeholder;
use ovrley_widgets::model::catalog::{catalog, UnitOption, WidgetDefinition};
use serde_json::Value;

/// A unit option's label: its own label, else its translated key.
pub fn unit_label(ctx: &Ctx, option: &UnitOption) -> String {
    match (&option.label, &option.label_key) {
        (Some(label), _) => label.clone(),
        (None, Some(key)) => ctx.t(key, option.default_label.as_deref().unwrap_or_default()),
        (None, None) => panic!("unit option {} has no label", option.value),
    }
}

pub fn unit_options(ctx: &Ctx, definition: &WidgetDefinition) -> Vec<OptionView> {
    definition
        .supported_display_units
        .iter()
        .map(|option| OptionView::new(option.value.clone(), unit_label(ctx, option)))
        .collect()
}

pub fn definition(ctx: &Ctx) -> &'static WidgetDefinition {
    catalog()
        .require_widget(&ctx.widget.kind)
        .expect("listed widgets are in the catalog")
}

/// `MetricWidgetEditor`: the display type select (with more than one display), the display's
/// section, and "Altitude at start".
pub fn metric_widget_editor(ctx: &Ctx) -> Vec<Node> {
    let catalog = catalog();
    let definition = definition(ctx);
    let display_type = ctx.text("display_type");
    let display_options: Vec<OptionView> = definition
        .display_types
        .iter()
        .map(|value| {
            OptionView::new(
                value.clone(),
                ctx.t(definition.display_type_label_key(catalog, value), value),
            )
        })
        .collect();
    let mut nodes = Vec::new();
    if display_options.len() > 1 {
        nodes.push(full(
            ctx.select(
                "display_type",
                ctx.t("widget-editor.displayType", "Display Type"),
                display_options,
            )
            .rule(Rule::Named(Named::DisplayType)),
        ));
    }
    match display_type.as_str() {
        "text" => nodes.extend(text_display_section(ctx)),
        _ => {}
    }
    if ctx.widget.kind == "altitude" {
        // The activity driving widgets at the playhead (`useActivityContext`).
        let preview_activity = crate::features::widget_preview::preview_activity(ctx.store);
        let activity = preview_activity
            .activity()
            .map(|imported| imported.activity.as_ref());
        let placeholder = starting_altitude_placeholder(ctx.widget, activity)
            .map(|value| format!("{value}"))
            .unwrap_or_default();
        let options = NumericFieldOptions {
            optional: true,
            round: true,
            ..NumericFieldOptions::default()
        };
        let mut cell = ctx.number(
            "starting_altitude",
            ctx.t("widget-editor.altitudeAtStart", "Altitude at start"),
            options,
        );
        if let CellKind::Number {
            placeholder: shown,
            suffix,
            resettable,
            ..
        } = &mut cell.kind
        {
            *shown = placeholder;
            *suffix = ctx.text("display_unit");
            *resettable = true;
        }
        nodes.push(grid(2, [cell]));
    }
    nodes
}

/// `TextDisplaySection`: font, decimals or balance format, the distance, coordinate and ascent
/// options, then the icon and units.
pub fn text_display_section(ctx: &Ctx) -> Vec<Node> {
    let definition = definition(ctx);
    let kind = ctx.widget.kind.as_str();
    let hidden_units = definition.units_mode.as_deref().unwrap_or("hidden") == "hidden";
    let supports_selection = definition.supported_display_units.len() > 1;
    let is_coordinates = kind == "gps_coordinates";
    let mut nodes = vec![font_section(
        ctx,
        FontSectionProps {
            show_content_alignment: true,
            ..FontSectionProps::default()
        },
    )];
    if definition.has_decimal_control {
        nodes.push(grid(
            2,
            [ctx.slider(
                "decimals",
                ctx.t("widget-editor.decimals", "Decimals"),
                0.0,
                f64::from(definition.max_decimals),
            )],
        ));
    }
    if definition.formatter.as_deref() == Some("balance") {
        let options = [
            ("percent_label", "52%/48%"),
            ("plain", "52/48"),
            ("l_prefix", "L52/R48"),
            ("l_suffix", "52L/48R"),
        ]
        .map(|(value, label)| OptionView::new(value, label))
        .to_vec();
        nodes.push(full(ctx.select(
            "balance_format",
            ctx.t("widget-editor.balanceFormat", "Balance Format"),
            options,
        )));
    }
    if kind == "distance" {
        let checked = ctx
            .data()
            .get("show_full_distance")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let cell = Cell::new(
            "show_full_distance",
            ctx.t("widget-editor.showFullDistance", "Show Full Distance"),
            CellKind::SwitchRow {
                checked,
                style: SwitchRowStyle::Toggle,
            },
        );
        nodes.push(grid(2, [cell]));
    }
    if is_coordinates {
        nodes.push(grid(
            2,
            [
                ctx.select(
                    "display_unit",
                    ctx.t("widget-editor.format", "Format"),
                    unit_options(ctx, definition),
                ),
                ctx.select(
                    "coordinate_format",
                    ctx.t("widget-editor.coordinates", "Coordinates"),
                    ctx.options(&[
                        (
                            "dms",
                            "widget-editor.coordinateFormatDms",
                            "Deg / Min / Sec",
                        ),
                        ("ddm", "widget-editor.coordinateFormatDdm", "Deg / Dec Min"),
                    ]),
                ),
            ],
        ));
    }
    if kind == "total_ascent" {
        nodes.push(grid(
            2,
            [ctx.switch_row(
                "show_full_ascent",
                ctx.t("widget-editor.showFullAscent", "Show Full Ascent"),
                SwitchRowStyle::Toggle,
            )],
        ));
    }
    let units = if hidden_units {
        Node::Empty
    } else {
        units_row(
            ctx,
            UnitsRowProps {
                title: if supports_selection {
                    ctx.t("widget-editor.units", "Units")
                } else {
                    ctx.t("widget-editor.unit", "Unit")
                },
                show_toggle: !is_coordinates,
                color: ctx
                    .data()
                    .get("unit_color")
                    .map(|color| color.as_str().unwrap_or_default().to_string()),
                select_label: ctx.t("widget-editor.unit", "Unit"),
                options: if !is_coordinates && supports_selection {
                    unit_options(ctx, definition)
                } else {
                    Vec::new()
                },
            },
        )
    };
    nodes.push(icon_section(ctx, units));
    nodes
}
