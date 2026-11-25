//! The metric widget's editor and its display sections, as descriptions.

use ovrley_widgets::model::catalog::{catalog, UnitOption, WidgetDefinition};
use ovrley_widgets::model::document::js_round;
use serde_json::Value;
use crate::features::widgets::{starting_altitude_placeholder, NumericFieldOptions};
use crate::features::widgets::blocks::{full, grid, grid_end, heading, stack, when, Cell, CellKind, Node, OptionView, Rule, StopView, SwitchRowStyle};
use crate::features::widgets::rules::{bar_gap_max, corner_radius_max, split_border_max, split_corner_radius_max, variant, Named, Track};
use crate::features::widgets::sections::{font_section, icon_section, units_row, Ctx, FontSection, UnitsRow};

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

/// The display type select (with more than one display), the display's
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
    // `DISPLAY_SECTION`.
    match display_type.as_str() {
        "text" => nodes.extend(text_display_section(ctx)),
        "altitude_tape" => nodes.extend(altitude_tape_section(ctx, definition)),
        "heading_tape" => nodes.extend(heading_tape_section(ctx)),
        "heading_compass" => nodes.extend(heading_compass_section(ctx)),
        "heart_zones" => nodes.extend(heart_zones_section(ctx)),
        "linear" => nodes.extend(linear_section(ctx)),
        "arc" | "corner" | "gt_gauge" => nodes.extend(arc_section(ctx, definition)),
        "lean_angle" => nodes.extend(lean_angle_section(ctx)),
        "g_force" => nodes.extend(g_force_editor(ctx)),
        "lap_timer" => nodes.extend(lap_timer_section(ctx)),
        "splits_run" => nodes.extend(splits_run_section(ctx)),
        other => panic!("{other} has no display section"),
    }
    if ctx.widget.kind == "altitude" {
        // The activity driving widgets at the playhead.
        let preview_activity = crate::features::widgets::preview::preview_activity(ctx.store);
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

/// Font, decimals or balance format, the distance, coordinate and ascent
/// options, then the icon and units.
pub fn text_display_section(ctx: &Ctx) -> Vec<Node> {
    let definition = definition(ctx);
    let kind = ctx.widget.kind.as_str();
    let hidden_units = definition.units_mode.as_deref().unwrap_or("hidden") == "hidden";
    let supports_selection = definition.supported_display_units.len() > 1;
    let is_coordinates = kind == "gps_coordinates";
    let mut nodes = vec![font_section(
        ctx,
        FontSection {
            show_content_alignment: true,
            ..FontSection::default()
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
            UnitsRow {
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
                color_label: None,
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

// ── Shared display controls ─────────────────────────────────────────────────────────────────────

/// `display_variants.<display>.<key>`.
fn v(display: &str, key: &str) -> String {
    format!("display_variants.{display}.{key}")
}

/// The track colours, opacities and border; with `supports_gradient`, the
/// heading switch swaps the filled colour for a gradient.
fn track_style_section(ctx: &Ctx, display: &str, border_max: f64, supports_gradient: bool) -> Node {
    let gradient = supports_gradient && ctx.flag(&v(display, "track_filled_gradient"));
    let trailing = supports_gradient.then(|| {
        let mut cell = ctx.switch(v(display, "track_filled_gradient"), ctx.t("widget-editor.fillGradient", "Gradient"));
        cell.kind = CellKind::Switch { checked: ctx.flag(&v(display, "track_filled_gradient")), label_style: Some(SwitchRowStyle::INLINE) };
        cell
    });
    stack(
        1.0,
        [
            heading("palette", ctx.t("widget-editor.trackStyle", "Track Style"), trailing),
            grid(
                2,
                [
                    ctx.color(v(display, "track_empty_color"), ctx.t("widget-editor.emptyColor", "Empty Color")),
                    ctx.slider(v(display, "track_empty_opacity"), ctx.t("widget-editor.emptyOpacity", "Empty Opacity"), 0.0, 1.0).percent(),
                ],
            ),
            grid(
                2,
                [
                    ctx.color(v(display, "track_filled_color"), ctx.t("widget-editor.filledColor", "Filled Color")).disabled(gradient),
                    ctx.slider(v(display, "track_filled_opacity"), ctx.t("widget-editor.filledOpacity", "Filled Opacity"), 0.0, 1.0).percent(),
                ],
            ),
            grid(
                2,
                [
                    ctx.color(v(display, "track_border_color"), ctx.t("widget-editor.borderColor", "Border Color")),
                    ctx.slider(v(display, "track_border_thickness"), ctx.t("widget-editor.border", "Border"), 0.0, border_max).int().px(),
                ],
            ),
            when(gradient, || full(gradient_cell(ctx, &v(display, "track_filled_gradient_stops")))),
        ],
    )
}

/// The gradient picker over a stops field.
pub fn gradient_cell(ctx: &Ctx, id: &str) -> Cell {
    let stops = ctx
        .value(id)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|stop| StopView {
            offset: stop.get("offset").and_then(Value::as_f64).unwrap_or_default(),
            color: stop.get("color").and_then(Value::as_str).unwrap_or_default().to_string(),
        })
        .collect();
    Cell::new(id, ctx.t("components.gradient", "Gradient"), CellKind::Gradient { stops })
}

/// The bar fill style field.
fn bar_fill_style_field(ctx: &Ctx, track: &Track, disabled: bool) -> Cell {
    let display = track.display();
    let value = ctx.value(&v(display, "track_fill_style")).and_then(Value::as_str).unwrap_or("fill").to_string();
    let options = ctx.options(&[("fill", "widget-editor.continuousFill", "Continuous Fill"), ("bars", "widget-editor.segmentedBars", "Segmented Bars")]);
    Cell::new(v(display, "track_fill_style"), ctx.t("widget-editor.trackStyle", "Track Style"), CellKind::Select { value, options, resettable: false })
        .disabled(disabled)
        .rule(Rule::Named(Named::BarFillStyle(track.clone())))
}

/// Flat Track while continuous, else the bar count and gap.
fn bar_fill_style_details(ctx: &Ctx, track: &Track) -> Vec<Cell> {
    let display = track.display();
    if ctx.value(&v(display, "track_fill_style")).and_then(Value::as_str) != Some("bars") {
        return vec![ctx.switch_row(v(display, "track_fill_flat"), ctx.t("widget-editor.flatTrack", "Flat Track"), SwitchRowStyle::PADDED)];
    }
    let gap_max = bar_gap_max(track, &variant(ctx.data(), display)).unwrap_or_default();
    vec![
        ctx.slider(v(display, "bar_count"), ctx.t("widget-editor.barCount", "Bar Count"), 2.0, 64.0).int().rule(Rule::Named(Named::TrackGeometry(track.clone()))),
        ctx.slider(v(display, "bar_gap"), ctx.t("widget-editor.barGap", "Bar Gap"), 0.0, gap_max).int().px().rule(Rule::Named(Named::TrackGeometry(track.clone()))),
    ]
}

/// The bounded inner offsets: the slider runs ±50, a typed value ±10000.
fn bounded_offset(ctx: &Ctx, id: String, label: String) -> Cell {
    ctx.slider(id, label, -50.0, 50.0).int().px().rule(Rule::Named(Named::Clamp { min: -10_000.0, max: 10_000.0 }))
}

/// The Min/Max Labels section of the linear and arc gauges, with the position select for linear.
fn min_max_labels(ctx: &Ctx, display: &str, position: Option<Vec<OptionView>>) -> Node {
    let disabled = !ctx.flag(&v(display, "show_min_max_labels"));
    let color = ctx.color(v(display, "min_max_label_color"), ctx.t("widget-editor.labelColor", "Label Color")).disabled(disabled);
    stack(
        1.0,
        [
            heading("tags", ctx.t("widget-editor.minmaxLabels", "Min/Max Labels"), Some(ctx.switch(v(display, "show_min_max_labels"), String::new()))),
            grid(
                2,
                [
                    ctx.font(v(display, "min_max_label_font"), ctx.t("widget-editor.labelFont", "Label Font")).disabled(disabled),
                    ctx.slider(v(display, "min_max_label_font_size"), ctx.t("widget-editor.fontSize", "Font Size"), 6.0, 50.0).int().px().disabled(disabled),
                ],
            ),
            match position {
                Some(options) => grid(2, [color, ctx.select(v(display, "min_max_label_position"), ctx.t("widget-editor.position", "Position"), options).disabled(disabled)]),
                None => full(color),
            },
        ],
    )
}

/// The decimals slider of gauges.
fn decimals_row(ctx: &Ctx, definition: &WidgetDefinition) -> Node {
    when(definition.has_decimal_control, || grid(2, [ctx.slider("decimals", ctx.t("widget-editor.decimals", "Decimals"), 0.0, f64::from(definition.max_decimals))]))
}

/// The gauges' units row: always titled "Unit".
fn gauge_units(ctx: &Ctx, definition: &WidgetDefinition) -> Node {
    if definition.units_mode.as_deref().unwrap_or("hidden") == "hidden" {
        return Node::Empty;
    }
    let supports_selection = definition.supported_display_units.len() > 1;
    units_row(
        ctx,
        UnitsRow {
            title: ctx.t("widget-editor.unit", "Unit"),
            show_toggle: true,
            color: ctx.data().get("unit_color").map(|color| color.as_str().unwrap_or_default().to_string()),
            color_label: None,
            select_label: ctx.t("widget-editor.unit", "Unit"),
            options: if supports_selection { unit_options(ctx, definition) } else { Vec::new() },
        },
    )
}

// ── The display sections ────────────────────────────────────────────────────────────────────────

/// The linear display's section.
fn linear_section(ctx: &Ctx) -> Vec<Node> {
    let track = Track::Linear;
    let data = variant(ctx.data(), "linear");
    let vertical = data.get("orientation").and_then(Value::as_str) == Some("vertical");
    let (width_bounds, height_bounds) = if vertical { ((8.0, 100.0), (20.0, 600.0)) } else { ((20.0, 600.0), (8.0, 100.0)) };
    let corner_max = corner_radius_max(&track, &data).unwrap_or_default();
    let geometry = || Rule::Named(Named::TrackGeometry(Track::Linear));
    let positions: Vec<OptionView> = ctx
        .options(&[("top", "widget-editor.top", "Top"), ("bottom", "widget-editor.bottom", "Bottom"), ("left", "widget-editor.left", "Left"), ("right", "widget-editor.right", "Right")])
        .into_iter()
        .filter(|option| if vertical { option.value == "left" || option.value == "right" } else { option.value == "top" || option.value == "bottom" })
        .collect();
    let mut second_row = vec![
        ctx.select(v("linear", "orientation"), ctx.t("widget-editor.orientation", "Orientation"), ctx.options(&[("horizontal", "widget-editor.horizontal", "Horizontal"), ("vertical", "widget-editor.vertical", "Vertical")]))
            .rule(Rule::Named(Named::LinearOrientation)),
        bar_fill_style_field(ctx, &track, false),
    ];
    second_row.extend(bar_fill_style_details(ctx, &track));
    second_row.push(ctx.slider(v("linear", "track_corner_radius"), ctx.t("widget-editor.cornerRadius", "Corner Radius"), 0.0, corner_max).int().px().rule(Rule::Named(Named::Clamp { min: 0.0, max: corner_max })));
    vec![
        stack(
            1.0,
            [
                heading("sliders-horizontal", ctx.t("widget-editor.gaugeTrack", "Gauge Track"), None),
                grid(
                    2,
                    [
                        ctx.slider(v("linear", "width"), ctx.t("widget-editor.width", "Width"), width_bounds.0, width_bounds.1).int().px().rule(geometry()),
                        ctx.slider(v("linear", "height"), ctx.t("widget-editor.height", "Height"), height_bounds.0, height_bounds.1).int().px().rule(geometry()),
                    ],
                ),
                grid(2, second_row).pad_top(0.5),
            ],
        ),
        track_style_section(ctx, "linear", 6.0, true),
        min_max_labels(ctx, "linear", Some(positions)),
    ]
}

/// The arc display's section, for the arc, corner and GT gauges.
fn arc_section(ctx: &Ctx, definition: &WidgetDefinition) -> Vec<Node> {
    let display = ctx.text("display_type");
    let corner = display == "corner";
    let gt = display == "gt_gauge";
    let track = Track::Arc(display.clone());
    let data = variant(ctx.data(), &display);
    let corner_max = corner_radius_max(&track, &data).unwrap_or_default();
    let size = ctx.value("width").or_else(|| data.get("width")).and_then(Value::as_f64).unwrap_or_default();
    let geometry = || Rule::Named(Named::TrackGeometry(Track::Arc(display.clone())));
    let first = if corner {
        ctx.select(v(&display, "corner_orientation"), ctx.t("widget-editor.cornerOrientation", "Corner Orientation"), ctx.options(&[("bottom-left", "widget-editor.bottomLeft", "Bottom Left"), ("bottom-right", "widget-editor.bottomRight", "Bottom Right")]))
    } else if gt {
        ctx.slider(v(&display, "tick_length"), ctx.t("widget-editor.tickLength", "Tick Length"), 0.0, 100.0).int().px()
    } else {
        ctx.slider(v(&display, "arc_angle"), ctx.t("widget-editor.arcAngle", "Arc Angle"), 30.0, 360.0).step(5.0).suffix("°")
    };
    let mut style_row = vec![bar_fill_style_field(ctx, &track, gt), ctx.slider(v(&display, "track_corner_radius"), ctx.t("widget-editor.cornerRadius", "Corner Radius"), 0.0, corner_max).int().px()];
    style_row.extend(bar_fill_style_details(ctx, &track));
    if gt {
        style_row.push(ctx.slider(v(&display, "glow"), ctx.t("widget-editor.glow", "Glow"), 0.0, 1.0).percent());
    }
    vec![
        stack(
            1.0,
            [
                heading("sliders-horizontal", if corner { ctx.t("widget-editor.cornerTrack", "Corner Track") } else { ctx.t("widget-editor.arcTrack", "Arc Track") }, None),
                grid(1, [size_slider(ctx, size, 30.0, 600.0)]),
                grid(2, [first, ctx.slider(v(&display, "track_thickness"), ctx.t("widget-editor.thickness", "Thickness"), 1.0, 100.0).int().px().rule(geometry())]).pad_top(0.5),
                grid(2, style_row),
            ],
        ),
        track_style_section(ctx, &display, 6.0, !gt),
        stack(
            1.0,
            [
                font_section(ctx, FontSection { title: Some(ctx.t("widget-editor.label", "Label")), font_size_label: Some(ctx.t("widget-editor.fontSize", "Font Size")), ..FontSection::default() }),
                grid(
                    2,
                    [
                        bounded_offset(ctx, v(&display, "inner_widget_offset_x"), ctx.t("widget-editor.horizontalOffset", "Horizontal Offset")),
                        bounded_offset(ctx, v(&display, "inner_widget_offset_y"), ctx.t("widget-editor.verticalOffset", "Vertical Offset")),
                    ],
                ),
            ],
        ),
        decimals_row(ctx, definition),
        gauge_units(ctx, definition),
        min_max_labels(ctx, &display, None),
    ]
}

/// The Size slider over the widget frame.
fn size_slider(ctx: &Ctx, size: f64, min: f64, max: f64) -> Cell {
    Cell::new(
        "size",
        ctx.t("widget-editor.size", "Size"),
        CellKind::Slider { editable: true, value: size, min, max, step: 1.0, scale: 1.0, readout: crate::format::plain_number(js_round(size)), suffix: "px".into() },
    )
    .rule(Rule::Named(Named::UniformSize))
}

/// The altitude tape display's section.
fn altitude_tape_section(ctx: &Ctx, definition: &WidgetDefinition) -> Vec<Node> {
    let d = "altitude_tape";
    let unit = ctx.text("display_unit");
    let show_speed = ctx.flag(&v(d, "show_vertical_speed"));
    let intervals: Vec<f64> = catalog().require_display(d).major_tick_intervals.as_ref().and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_f64).collect();
    let interval = ctx.num(&v(d, "major_tick_interval"));
    let index = intervals.iter().position(|value| *value == interval).map_or(-1.0, |index| index as f64);
    let interval_cell = Cell::new(
        v(d, "major_tick_interval"),
        ctx.t("widget-editor.majorInterval", "Interval"),
        CellKind::Slider {
            editable: false,
            value: index,
            min: 0.0,
            max: intervals.len() as f64 - 1.0,
            step: 1.0,
            scale: 1.0,
            readout: format!("{} {unit}", crate::format::plain_number(interval)),
            suffix: String::new(),
        },
    )
    .rule(Rule::Named(Named::Index(intervals)));
    let vertical_speed = catalog().require_widget("vertical_speed").expect("the catalog defines vertical_speed");
    let font = |id: &str, label: String| ctx.font(id, label);
    vec![
        stack(
            1.0,
            [
                heading("mountain", ctx.t("widget-editor.tapeScale", "Tape Scale"), None),
                grid(
                    2,
                    [
                        ctx.slider(v(d, "width"), ctx.t("widget-editor.width", "Width"), 40.0, 600.0).int().px(),
                        ctx.slider(v(d, "height"), ctx.t("widget-editor.height", "Height"), 120.0, 2000.0).int().px(),
                    ],
                ),
                grid(2, [interval_cell, ctx.slider(v(d, "major_tick_spacing"), ctx.t("widget-editor.majorSpacing", "Spacing"), 20.0, 300.0).int().px()]),
                grid(
                    2,
                    [
                        toggle_group(ctx, v(d, "tape_orientation"), ctx.t("widget-editor.orientation", "Orientation"), ctx.options(&[("left", "widget-editor.left", "Left"), ("right", "widget-editor.right", "Right")])),
                        ctx.select("display_unit", ctx.t("widget-editor.unit", "Unit"), unit_options(ctx, definition)).rule(Rule::Named(Named::MetricUnit)),
                    ],
                ),
            ],
        ),
        stack(
            1.0,
            [
                heading("ruler", ctx.t("widget-editor.ticks", "Ticks"), None),
                grid(
                    2,
                    [
                        ctx.slider(v(d, "major_tick_length_pct"), ctx.t("widget-editor.majorLength", "Major Length"), 5.0, 100.0).int().suffix("%"),
                        ctx.slider(v(d, "minor_tick_length_pct"), ctx.t("widget-editor.minorLength", "Minor Length"), 5.0, 100.0).int().suffix("%"),
                        ctx.slider(v(d, "major_tick_thickness"), ctx.t("widget-editor.majorThickness", "Major Thickness"), 0.5, 8.0).step(0.5).px(),
                        ctx.slider(v(d, "minor_tick_thickness"), ctx.t("widget-editor.minorThickness", "Minor Thickness"), 0.5, 8.0).step(0.5).px(),
                        ctx.color(v(d, "major_tick_color"), ctx.t("widget-editor.majorTickColor", "Major Color")),
                        ctx.color(v(d, "minor_tick_color"), ctx.t("widget-editor.minorTickColor", "Minor Color")),
                    ],
                ),
            ],
        ),
        stack(
            1.0,
            [
                heading("type", ctx.t("widget-editor.labels", "Labels"), Some(ctx.switch(v(d, "show_tick_labels"), ctx.t("widget-editor.showLabels", "Show Labels")))),
                full(font(&v(d, "label_font"), ctx.t("widget-editor.labelFont", "Label Font"))),
                grid(2, [ctx.slider(v(d, "label_font_size"), ctx.t("widget-editor.fontSize", "Font Size"), 6.0, 48.0).int().px(), ctx.color(v(d, "label_color"), ctx.t("widget-editor.labelColor", "Label Color"))]),
            ],
        ),
        stack(
            1.0,
            [
                heading("square-chevron-left", ctx.t("widget-editor.liveValue", "Value"), Some(ctx.switch(v(d, "show_value_decimal"), ctx.t("widget-editor.decimal", "Decimal")))),
                full(font("font", ctx.t("widget-editor.font", "Font"))),
                grid(
                    2,
                    [
                        ctx.slider("font_size", ctx.t("widget-editor.fontSize", "Font Size"), 8.0, 120.0).int().px(),
                        ctx.color("color", ctx.t("widget-editor.fontColor", "Font Color")),
                        ctx.color(v(d, "value_border_color"), ctx.t("widget-editor.borderColor", "Border Color")),
                        ctx.color(v(d, "value_background_color"), ctx.t("widget-editor.backgroundColor", "Background")),
                    ],
                ),
            ],
        ),
        stack(
            1.0,
            [
                heading("arrow-up-down", ctx.t("widget-editor.verticalSpeed", "Vert. Speed"), Some(ctx.switch(v(d, "show_vertical_speed"), ctx.t("widget-editor.verticalSpeed", "Vert. Speed")))),
                grid(
                    2,
                    [
                        ctx.select(v(d, "vertical_speed_unit"), ctx.t("widget-editor.unit", "Unit"), unit_options(ctx, vertical_speed)).disabled(!show_speed),
                        ctx.switch_row(v(d, "show_vertical_speed_decimal"), ctx.t("widget-editor.decimal", "Decimal"), SwitchRowStyle::BOTTOM).disabled(!show_speed).end(),
                        ctx.slider(v(d, "vertical_speed_trend_seconds"), ctx.t("widget-editor.trendSeconds", "Trend"), 1.0, 60.0).int().suffix("s").disabled(!show_speed),
                        ctx.slider(v(d, "vertical_speed_bar_thickness"), ctx.t("widget-editor.thickness", "Thickness"), 1.0, 20.0).int().px().disabled(!show_speed),
                        ctx.slider(v(d, "vertical_speed_font_size"), ctx.t("widget-editor.fontSize", "Font Size"), 6.0, 48.0).int().px().disabled(!show_speed),
                        Cell::empty("vertical_speed_spacer"),
                        ctx.color(v(d, "vertical_speed_positive_color"), ctx.t("widget-editor.climbColor", "Climb")).disabled(!show_speed),
                        ctx.color(v(d, "vertical_speed_negative_color"), ctx.t("widget-editor.descentColor", "Descent")).disabled(!show_speed),
                    ],
                ),
            ],
        ),
    ]
}

/// A labelled single toggle group.
pub fn toggle_group(ctx: &Ctx, id: String, label: String, options: Vec<OptionView>) -> Cell {
    let value = ctx.text(&id);
    Cell::new(id, label, CellKind::ToggleGroup { value, options, labelled: true, multiple: false, pressed: Vec::new() })
}

/// The heading tape display's section.
fn heading_tape_section(ctx: &Ctx) -> Vec<Node> {
    let d = "heading_tape";
    let major = ctx.flag(&v(d, "show_major_ticks"));
    let minor = ctx.flag(&v(d, "show_minor_ticks"));
    let chevron = ctx.text(&v(d, "indicator_style")) == "chevron";
    vec![
        stack(
            1.0,
            [
                heading("compass", ctx.t("widget-editor.tapeScale", "Tape Scale"), None),
                full(ctx.slider(v(d, "pixels_per_degree"), ctx.t("widget-editor.pixelsPerDegree", "Pixels per Degree"), 1.0, 20.0).step(0.5).int().px()),
            ],
        ),
        stack(
            1.0,
            [
                heading("ruler", ctx.t("widget-editor.ticks", "Ticks"), None),
                // Two columns of the same three controls, 1rem apart, as rows.
                grid(
                    2,
                    [
                        ctx.switch_row(v(d, "show_major_ticks"), ctx.t("widget-editor.majorTicks", "Major Ticks"), SwitchRowStyle::BOTTOM),
                        ctx.switch_row(v(d, "show_minor_ticks"), ctx.t("widget-editor.minorTicks", "Minor Ticks"), SwitchRowStyle::BOTTOM),
                        ctx.slider(v(d, "major_tick_length_pct"), ctx.t("widget-editor.majorLength", "Major Length"), 5.0, 100.0).suffix("%").disabled(!major),
                        ctx.slider(v(d, "minor_tick_length_pct"), ctx.t("widget-editor.minorLength", "Minor Length"), 5.0, 100.0).suffix("%").disabled(!minor),
                        ctx.slider(v(d, "major_tick_thickness"), ctx.t("widget-editor.majorThickness", "Major Thickness"), 0.5, 8.0).step(0.5).int().px().disabled(!major),
                        ctx.slider(v(d, "minor_tick_thickness"), ctx.t("widget-editor.minorThickness", "Minor Thickness"), 0.5, 8.0).step(0.5).int().px().disabled(!minor),
                    ],
                ),
                grid(2, [ctx.color(v(d, "tick_color"), ctx.t("widget-editor.tickColor", "Tick Color")), ctx.color(v(d, "cardinal_tick_color"), ctx.t("widget-editor.cardinalColor", "Cardinal Color"))]),
                full(ctx.select(v(d, "tick_alignment"), ctx.t("widget-editor.alignment", "Alignment"), ctx.options(&[("below", "widget-editor.alignmentBelow", "Below"), ("centered", "widget-editor.alignmentCentered", "Centered")]))),
            ],
        ),
        stack(
            1.0,
            [
                heading("type", ctx.t("widget-editor.labels", "Labels"), None),
                full(ctx.font(v(d, "label_font"), ctx.t("widget-editor.labelFont", "Label Font"))),
                grid(
                    2,
                    [
                        ctx.switch_row(v(d, "show_minor_labels"), ctx.t("widget-editor.minorLabels", "Minor Labels"), SwitchRowStyle::INLINE_X),
                        ctx.switch_row(v(d, "show_major_labels"), ctx.t("widget-editor.majorLabels", "Major Labels"), SwitchRowStyle::INLINE_X),
                    ],
                )
                .pad_y(0.5),
                grid(2, [ctx.color(v(d, "label_color"), ctx.t("widget-editor.labelColor", "Label Color")), ctx.color(v(d, "cardinal_label_color"), ctx.t("widget-editor.cardinalColor", "Cardinal Color"))]),
                grid(
                    2,
                    [
                        ctx.slider(v(d, "label_font_size"), ctx.t("widget-editor.fontSize", "Font Size"), 6.0, 36.0).int().px(),
                        ctx.slider(v(d, "label_offset"), ctx.t("widget-editor.offset", "Offset"), 0.0, 20.0).int().px(),
                    ],
                ),
            ],
        ),
        stack(
            1.0,
            [
                heading("target", ctx.t("widget-editor.indicator", "Indicator"), None),
                full(ctx.switch_row(v(d, "show_indicator"), ctx.t("widget-editor.showIndicator", "Show Indicator"), SwitchRowStyle::INLINE_X)),
                grid(
                    2,
                    [
                        ctx.select(v(d, "indicator_style"), ctx.t("widget-editor.style", "Style"), ctx.options(&[("chevron", "widget-editor.indicatorStyleChevron", "Chevron"), ("highlight_bar", "widget-editor.indicatorStyleHighlightBar", "Highlight Bar")])),
                        ctx.select(v(d, "indicator_placement"), ctx.t("widget-editor.placement", "Placement"), ctx.options(&[("top", "widget-editor.top", "Top"), ("bottom", "widget-editor.bottom", "Bottom"), ("both", "widget-editor.both", "Both")])).disabled(!chevron),
                    ],
                ),
                grid(2, [ctx.color(v(d, "indicator_color"), ctx.t("widget-editor.indicatorColor", "Indicator Color")), ctx.slider(v(d, "indicator_size"), ctx.t("widget-editor.indicatorSize", "Indicator Size"), 4.0, 40.0).int().px()]),
            ],
        ),
    ]
}

/// The heading compass display's section.
fn heading_compass_section(ctx: &Ctx) -> Vec<Node> {
    let d = "heading_compass";
    let diameter = ctx.num(&v(d, "diameter"));
    let thickness = ctx.num(&v(d, "track_thickness"));
    let labels = ctx.flag(&v(d, "show_cardinal_labels"));
    let value = ctx.flag(&v(d, "show_value"));
    let draft_switch = |id: String, label: String| ctx.switch(id, label).rule(Rule::Named(Named::DraftOnly));
    vec![
        stack(
            1.0,
            [
                heading("compass", ctx.t("widget-editor.compassDial", "Dial"), None),
                full(toggle_group(ctx, v(d, "rotation_mode"), ctx.t("widget-editor.rotationMode", "Rotation Mode"), ctx.options(&[("indicator", "widget-editor.indicator", "Indicator"), ("compass", "widget-editor.compass", "Compass")]))),
                grid(2, [size_slider(ctx, diameter, 60.0, 800.0), ctx.color(v(d, "track_fill"), ctx.t("widget-editor.trackFill", "Track Fill"))]),
                grid(
                    2,
                    [
                        ctx.slider(v(d, "track_thickness"), ctx.t("widget-editor.thickness", "Thickness"), 1.0, 60.0).int().px(),
                        ctx.slider(v(d, "track_corner_radius"), ctx.t("widget-editor.cornerRadius", "Corner Radius"), 0.0, (thickness / 2.0).floor()).int().px(),
                    ],
                ),
            ],
        ),
        stack(
            1.0,
            [
                heading("target", ctx.t("widget-editor.indicator", "Indicator"), None),
                grid(
                    2,
                    [
                        ctx.slider(v(d, "indicator_size"), ctx.t("widget-editor.indicatorSize", "Indicator Size"), 8.0, js_round(diameter)).int().px(),
                        ctx.slider(v(d, "indicator_corner_radius"), ctx.t("widget-editor.cornerRadius", "Corner Radius"), 0.0, 100.0).int().suffix("%"),
                    ],
                ),
                grid(2, [ctx.color(v(d, "indicator_color"), ctx.t("widget-editor.indicatorColor", "Indicator Color"))]),
            ],
        ),
        stack(
            1.0,
            [
                heading("type", ctx.t("widget-editor.cardinalLabels", "Cardinal Labels"), Some(draft_switch(v(d, "show_cardinal_labels"), ctx.t("widget-editor.showLabels", "Show Labels")))),
                full(ctx.slider(v(d, "cardinal_font_size"), ctx.t("widget-editor.fontSize", "Font Size"), 6.0, 120.0).int().px().disabled(!labels)),
                grid(2, [ctx.font("font", ctx.t("widget-editor.font", "Font")), ctx.color(v(d, "cardinal_label_color"), ctx.t("widget-editor.labelColor", "Label Color")).disabled(!labels)]),
            ],
        ),
        stack(
            1.0,
            [
                heading("hash", ctx.t("widget-editor.headingValue", "Value"), Some(draft_switch(v(d, "show_value"), ctx.t("widget-editor.showValue", "Show Value")))),
                full(ctx.slider("font_size", ctx.t("widget-editor.fontSize", "Font Size"), 6.0, 200.0).int().px().disabled(!value)),
                grid(2, [ctx.color("color", ctx.t("widget-editor.valueColor", "Value Color")).disabled(!value), ctx.color("unit_color", ctx.t("widget-editor.unitColor", "Unit Color")).disabled(!value)]),
                grid(
                    2,
                    [
                        ctx.slider(v(d, "value_offset_x"), ctx.t("widget-editor.horizontalOffset", "Horizontal Offset"), -200.0, 200.0).int().px().disabled(!value),
                        ctx.slider(v(d, "value_offset_y"), ctx.t("widget-editor.verticalOffset", "Vertical Offset"), -200.0, 200.0).int().px().disabled(!value),
                    ],
                ),
            ],
        ),
    ]
}

/// The lean angle display's section.
fn lean_angle_section(ctx: &Ctx) -> Vec<Node> {
    let d = "lean_angle";
    let diameter = ctx.num(&v(d, "diameter"));
    let thickness = ctx.num(&v(d, "track_thickness"));
    vec![
        stack(
            1.0,
            [
                heading("sliders-horizontal", ctx.t("widget-editor.angleTrack", "Angle Track"), None),
                grid(2, [size_slider(ctx, diameter, 30.0, 600.0), ctx.slider(v(d, "track_thickness"), ctx.t("widget-editor.thickness", "Thickness"), 1.0, ((diameter - 1.0) / 2.0).floor()).int().px()]),
            ],
        ),
        track_style_section(ctx, d, 8.0_f64.min(((thickness - 1.0) / 2.0).floor()), true),
        stack(
            1.0,
            [
                heading("type", ctx.t("widget-editor.label", "Label"), None),
                grid(2, [ctx.font("font", ctx.t("widget-editor.labelFont", "Label Font")), ctx.slider("font_size", ctx.t("widget-editor.fontSize", "Font Size"), 6.0, 200.0).int().px()]),
                grid(2, [ctx.color("color", ctx.t("widget-editor.labelColor", "Label Color")), ctx.color("unit_color", ctx.t("widget-editor.unitColor", "Unit Color"))]),
                grid(
                    2,
                    [
                        ctx.slider(v(d, "value_offset_x"), ctx.t("widget-editor.horizontalOffset", "Horizontal Offset"), -50.0, 50.0).int().px(),
                        ctx.slider(v(d, "value_offset_y"), ctx.t("widget-editor.verticalOffset", "Vertical Offset"), -50.0, 50.0).int().px(),
                    ],
                ),
            ],
        ),
    ]
}

/// `GForceWidgetEditor`: `GForceDisplaySection` and the axis mapping.
fn g_force_editor(ctx: &Ctx) -> Vec<Node> {
    let d = "g_force";
    let diameter = ctx.num(&v(d, "diameter"));
    let border_max = ((diameter - 1.0) / 2.0).floor().min(8.0);
    let size = Cell::new(
        v(d, "diameter"),
        ctx.t("widget-editor.size", "Size"),
        CellKind::Slider { editable: true, value: diameter, min: 20.0, max: 600.0, step: 1.0, scale: 1.0, readout: crate::format::plain_number(js_round(diameter)), suffix: "px".into() },
    )
    .rule(Rule::Named(Named::GForceDiameter));
    let axis = |key: &str, invert: &str, horizontal: bool, label: String| {
        Cell::new(
            v(d, key),
            ctx.t("widget-editor.labelAxis", "{{label}} axis").replace("{{label}}", &label),
            CellKind::GForceAxis { axis: ctx.text(&v(d, key)), inverted: ctx.flag(&v(d, invert)), invert_label: ctx.t("widget-editor.invertSign", "Invert sign") },
        )
        .rule(Rule::Named(Named::GForceAxis { horizontal }))
    };
    vec![
        stack(
            1.0,
            [
                heading("circle-gauge", ctx.t("widget-editor.gforcePlot", "G-Force Plot"), None),
                full(size),
                grid(2, [ctx.color(v(d, "fill_color"), ctx.t("widget-editor.fillColor", "Fill Color")), ctx.slider(v(d, "fill_opacity"), ctx.t("widget-editor.fillOpacity", "Fill Opacity"), 0.0, 1.0).percent()]),
                grid(2, [ctx.color(v(d, "border_color"), ctx.t("widget-editor.borderColor", "Border Color")), ctx.slider(v(d, "border_thickness"), ctx.t("widget-editor.borderThickness", "Border Thickness"), 0.0, border_max).int().px()]),
                grid(2, [ctx.color(v(d, "marker_color"), ctx.t("widget-editor.markerColor", "Marker Color")), ctx.slider(v(d, "marker_size"), ctx.t("widget-editor.markerSize", "Marker Size"), 2.0, 48.0).int().px()]),
                grid(
                    2,
                    [
                        ctx.slider(v(d, "marker_opacity"), ctx.t("widget-editor.markerOpacity", "Marker Opacity"), 0.0, 1.0).percent(),
                        ctx.slider(v(d, "border_opacity"), ctx.t("widget-editor.borderOpacity", "Border Opacity"), 0.0, 1.0).percent(),
                    ],
                ),
            ],
        ),
        stack(
            1.0,
            [
                heading("type", ctx.t("widget-editor.label", "Label"), None),
                grid(2, [ctx.font(v(d, "label_font"), ctx.t("widget-editor.labelFont", "Label Font")), ctx.slider(v(d, "label_font_size"), ctx.t("widget-editor.fontSize", "Font Size"), 6.0, 100.0).int().px()]),
                grid(2, [ctx.color(v(d, "label_color"), ctx.t("widget-editor.labelColor", "Label Color")), ctx.color(v(d, "label_unit_color"), ctx.t("widget-editor.unitColor", "Unit Color"))]),
                grid(
                    2,
                    [
                        ctx.slider(v(d, "label_offset_x"), ctx.t("widget-editor.horizontalOffset", "Horizontal Offset"), -100.0, 100.0).int().px(),
                        ctx.slider(v(d, "label_offset_y"), ctx.t("widget-editor.verticalOffset", "Vertical Offset"), -100.0, 100.0).int().px(),
                    ],
                ),
            ],
        ),
        stack(
            1.0,
            [
                heading("circle-gauge", ctx.t("widget-editor.axisMapping", "Axis Mapping"), None),
                grid(
                    1,
                    [
                        axis("axis_horizontal", "invert_horizontal", true, ctx.t("widget-editor.horizontal", "Horizontal")),
                        axis("axis_vertical", "invert_vertical", false, ctx.t("widget-editor.vertical", "Vertical")),
                    ],
                ),
            ],
        ),
    ]
}

/// The heart zones display's section.
fn heart_zones_section(ctx: &Ctx) -> Vec<Node> {
    let d = "heart_zones";
    let data = variant(ctx.data(), d);
    let corner_max = (((field_of(&data, "width") - field_of(&data, "zone_gap") * 4.0) / 5.0).min(field_of(&data, "height")) / 2.0).floor();
    let calculation = ctx.text(&v(d, "zone_calculation"));
    let show_label = ctx.flag(&v(d, "show_label"));
    let bpm = |id: String, label: String, rule: Named| {
        let mut cell = ctx.number(id, label, NumericFieldOptions::default()).rule(Rule::Named(rule));
        if let CellKind::Number { suffix, .. } = &mut cell.kind {
            *suffix = "bpm".into();
        }
        cell
    };
    let mut zones = vec![
        heading("heart-pulse", ctx.t("widget-editor.heartZones", "Zones"), None),
        grid(2, [ctx.slider(v(d, "width"), ctx.t("widget-editor.width", "Width"), 60.0, 1600.0).int().px(), ctx.slider(v(d, "height"), ctx.t("widget-editor.height", "Height"), 8.0, 200.0).int().px()]),
        grid(
            2,
            [ctx.slider(v(d, "track_corner_radius"), ctx.t("widget-editor.cornerRadius", "Corner Radius"), 0.0, corner_max).shown(ctx.num(&v(d, "track_corner_radius")).min(corner_max)).int().px()],
        ),
        grid(
            2,
            [
                Some(
                    ctx.select(
                        v(d, "zone_calculation"),
                        ctx.t("widget-editor.zoneCalculation", "Zone Calculation"),
                        ctx.options(&[("lthr", "widget-editor.zoneCalculationLthr", "Lactate Threshold HR"), ("max_heartrate", "widget-editor.zoneCalculationMaxHeartrate", "Max Heart Rate"), ("manual", "widget-editor.zoneCalculationManual", "Manual")]),
                    )
                    .rule(Rule::Named(Named::ZoneCalculation)),
                ),
                (calculation == "lthr").then(|| bpm(v(d, "lthr"), ctx.t("widget-editor.lthr", "Lactate Threshold HR"), Named::AtLeast(1.0))),
                (calculation == "max_heartrate").then(|| bpm(v(d, "max_heartrate"), ctx.t("widget-editor.maxHeartrate", "Max Heart Rate"), Named::AtLeast(1.0))),
            ],
        ),
    ];
    if calculation == "manual" {
        let count = data.get("manual_zones").and_then(Value::as_array).map_or(0, Vec::len);
        for index in 0..count {
            let cells: Vec<Cell> = ["from", "to"]
                .iter()
                .map(|bound| {
                    let label = format!("Z{} {}", index + 1, if *bound == "from" { ctx.t("widget-editor.zoneFrom", "From") } else { ctx.t("widget-editor.zoneTo", "To") });
                    bpm(v(d, &format!("manual_zones.{index}.{bound}")), label, Named::ManualZone { index, bound: (*bound).into() })
                })
                .collect();
            zones.push(grid(2, cells));
        }
    }
    vec![
        stack(1.0, zones),
        stack(
            1.0,
            [
                heading("type", ctx.t("widget-editor.zoneLabel", "Zone Label"), Some(ctx.switch(v(d, "show_label"), ctx.t("widget-editor.zoneLabel", "Zone Label")))),
                full(ctx.font("font", ctx.t("widget-editor.font", "Font")).disabled(!show_label)),
                full(ctx.color(v(d, "label_color"), ctx.t("widget-editor.fontColor", "Font Color")).disabled(!show_label)),
            ],
        ),
        stack(
            1.0,
            [
                heading("target", ctx.t("widget-editor.indicator", "Indicator"), None),
                full({
                    let options = ctx.options(&[("line", "widget-editor.indicatorLine", "Line"), ("arrow", "widget-editor.indicatorArrow", "Arrow")]);
                    Cell::new(v(d, "indicator_style"), ctx.t("widget-editor.indicator", "Indicator"), CellKind::ToggleGroup { value: ctx.text(&v(d, "indicator_style")), options, labelled: false, multiple: false, pressed: Vec::new() })
                }),
                grid(
                    2,
                    [
                        ctx.color(v(d, "indicator_color"), ctx.t("widget-editor.indicatorColor", "Indicator Color")),
                        ctx.slider(v(d, "indicator_size"), ctx.t("widget-editor.indicatorSize", "Indicator Size"), 4.0, 80.0).int().px().disabled(ctx.text(&v(d, "indicator_style")) != "arrow"),
                    ],
                ),
            ],
        ),
    ]
}

fn field_of(map: &serde_json::Map<String, Value>, key: &str) -> f64 {
    map.get(key).and_then(Value::as_f64).unwrap_or_default()
}

/// The lap timer display's section.
fn lap_timer_section(ctx: &Ctx) -> Vec<Node> {
    let mode = ctx.text("lap_timer_mode");
    let lap_log = mode == "lap_log";
    let modes: Vec<OptionView> = catalog()
        .require_display("lap_timer")
        .modes
        .iter()
        .map(|mode| {
            let key = mode.raw.get("labelKey").and_then(Value::as_str).unwrap_or_default();
            let label = mode.raw.get("label").and_then(Value::as_str).unwrap_or_default();
            OptionView::new(mode.value.clone(), ctx.t(key, label))
        })
        .collect();
    vec![
        full(ctx.select("lap_timer_mode", ctx.t("widget-editor.readout", "Readout"), modes).rule(Rule::Named(Named::DisplayMode))),
        when(lap_log, || full(ctx.switch_row("renumber_laps", ctx.t("widget-editor.renumberLaps", "Renumber laps"), SwitchRowStyle::INLINE))),
        font_section(ctx, FontSection::default()),
        stack(
            1.0,
            [
                heading("type", ctx.t("widget-editor.label", "Label"), (!lap_log).then(|| ctx.switch("show_label", String::new()))),
                when(!lap_log, || full(Cell::new("label", ctx.t("widget-editor.label", "Label"), CellKind::Text { value: ctx.text("label"), placeholder: String::new() }))),
                full(ctx.slider("label_font_size", ctx.t("widget-editor.labelFontSize", "Label Font Size"), 6.0, 100.0).step(0.5).int().px()),
                grid_end(2, [ctx.font("label_font", ctx.t("widget-editor.labelFont", "Label Font")), ctx.color("label_color", ctx.t("widget-editor.labelColor", "Label Color"))]),
            ],
        ),
        when(mode == "delta" || lap_log, || {
            grid(
                2,
                [
                    ctx.color("positive_delta_color", ctx.t("widget-editor.positiveDeltaColor", "Positive Delta Color")),
                    ctx.color("negative_delta_color", ctx.t("widget-editor.negativeDeltaColor", "Negative Delta Color")),
                ],
            )
        }),
    ]
}

/// The splits run display's section.
fn splits_run_section(ctx: &Ctx) -> Vec<Node> {
    let data = ctx.data();
    let columns: Vec<String> = data.get("visible_columns").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
    let show_bar = columns.iter().any(|column| column == "bar");
    let corner_max = split_corner_radius_max(data);
    let border_max = split_border_max(data);
    let gradient = ctx.flag("track_filled_gradient");
    let mut distance = ctx.number("split_distance", ctx.t("widget-editor.splitLength", "Split Length"), NumericFieldOptions::default()).rule(Rule::Named(Named::SplitDistance));
    if let CellKind::Number { value, .. } = &mut distance.kind {
        // `integerDisplay={false}`: the length as it is.
        *value = ctx.value("split_distance").and_then(Value::as_f64).map(crate::format::plain_number).unwrap_or_default();
    }
    let column_options = ctx.options(&[("split", "widget-editor.splitColumnSplit", "Split"), ("bar", "widget-editor.splitColumnBar", "Bar"), ("pace", "widget-editor.splitColumnPace", "Pace")]);
    let border = ctx.num("track_border_thickness").min(border_max);
    vec![
        font_section(ctx, FontSection::default()),
        stack(
            1.0,
            [
                heading("rows-3", ctx.t("widget-editor.splits", "Splits"), None),
                grid_end(
                    2,
                    [distance, ctx.select("split_unit", ctx.t("widget-editor.unit", "Unit"), ["m", "km", "ft", "mi"].iter().map(|unit| OptionView::new(*unit, *unit)).collect())],
                ),
                full(
                    Cell::new("visible_columns", ctx.t("widget-editor.splitColumns", "Columns"), CellKind::ToggleGroup { value: String::new(), options: column_options, labelled: true, multiple: true, pressed: columns })
                        .rule(Rule::Named(Named::SplitColumn)),
                ),
                grid(
                    2,
                    [ctx.slider("max_splits", ctx.t("widget-editor.maxSplits", "Max Splits"), 1.0, 30.0).int(), ctx.slider("row_gap", ctx.t("widget-editor.rowGap", "Row Gap"), 0.0, 100.0).int().px()],
                ),
            ],
        ),
        stack(
            1.0,
            [
                heading("type", ctx.t("widget-editor.label", "Label"), None),
                full(Cell::new("label", ctx.t("widget-editor.label", "Label"), CellKind::Text { value: ctx.text("label"), placeholder: String::new() })),
                full(ctx.slider("label_font_size", ctx.t("widget-editor.labelFontSize", "Label Font Size"), 6.0, 100.0).step(0.5).int().px()),
                grid_end(2, [ctx.font("label_font", ctx.t("widget-editor.labelFont", "Label Font")).small_label(), ctx.color("label_color", ctx.t("widget-editor.labelColor", "Label Color"))]),
            ],
        ),
        when(show_bar, || {
            let mut trailing = ctx.switch("track_filled_gradient", ctx.t("widget-editor.fillGradient", "Gradient"));
            trailing.kind = CellKind::Switch { checked: gradient, label_style: Some(SwitchRowStyle::Tall) };
            stack(
                1.0,
                [
                    heading("chart-no-axes-gantt", ctx.t("widget-editor.bar", "Bar"), Some(trailing)),
                    grid(
                        2,
                        [
                            ctx.slider("bar_width", ctx.t("widget-editor.barWidth", "Bar Width"), 10.0, 1600.0).int().px(),
                            ctx.slider("track_thickness", ctx.t("widget-editor.barHeight", "Bar Height"), 2.0, 200.0).int().px().rule(Rule::Named(Named::SplitBarHeight)),
                        ],
                    ),
                    grid(
                        2,
                        [
                            ctx.slider("track_border_thickness", ctx.t("widget-editor.border", "Border"), 0.0, border_max).shown(border).readout(crate::format::plain_number(border)).int().px(),
                            ctx.slider("track_corner_radius", ctx.t("widget-editor.cornerRadius", "Corner Radius"), 0.0, corner_max).shown(ctx.num("track_corner_radius").min(corner_max)).int().px(),
                        ],
                    ),
                    grid(2, [ctx.color("track_filled_color", ctx.t("widget-editor.filledColor", "Filled Color")).disabled(gradient), ctx.slider("track_filled_opacity", ctx.t("widget-editor.filledOpacity", "Filled Opacity"), 0.0, 1.0).percent()]),
                    grid(2, [ctx.color("track_border_color", ctx.t("widget-editor.borderColor", "Border Color")), ctx.slider("track_border_opacity", ctx.t("widget-editor.borderOpacity", "Border Opacity"), 0.0, 1.0).percent()]),
                    when(gradient, || full(gradient_cell(ctx, "track_filled_gradient_stops"))),
                ],
            )
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::super::blocks::{at, cells, find};
    use super::super::rules::{derive_lthr_zones, Input};
    use super::super::tests::{cell, edit, editor, inspector, patch, slider_value, widget};
    use super::super::{describe, update_widget_data};
    use super::*;
    use crate::store::template::ThemeColors;
    use crate::features::widgets::build_config_widgets;
    use crate::store::Store;
    use ovrley_core::normalize::template::template_manifest;
    use ovrley_widgets::model::document::{create_widget_defaults, OverlayConfig, WidgetCreationOptions, WidgetSelection};
    use serde_json::json;

    fn with_display(kind: &str, display: &str, id: &str) -> Value {
        // A display with modes is created in one of them, as the drawer offers it.
        let mode = catalog().require_display(display).modes.first().map(|mode| mode.value.clone());
        let selection = WidgetSelection { kind: kind.into(), display_type: Some(display.into()), mode, ..WidgetSelection::default() };
        let mut data = create_widget_defaults(&selection, &template_manifest().globals, &WidgetCreationOptions::default()).unwrap();
        data.insert("id".into(), json!(id));
        Value::Object(data)
    }

    #[test]
    fn every_metric_display_describes_and_its_fields_resolve() {
        for definition in catalog().widgets().iter().filter(|definition| definition.metric && definition.current) {
            for display in &definition.display_types {
                let mut store = Store { theme: Some(ThemeColors { ice: "#86aaaa".into(), aqua: "#40e0d0".into(), teal: "#005b5b".into(), accent: "#ad5013".into() }), ..Store::default() };
                store.editor.set_config(OverlayConfig::from_value(&json!({"values": [with_display(&definition.kind, display, "w")]})).unwrap());
                let widget = build_config_widgets(&store.editor.config).remove(0);
                let blocks = describe(&store, &widget);
                // Optional fields may be absent: the display's conditional keys, the type's optional
                // fields, and the full-distance switch (on when absent).
                let optional = |id: &str| {
                    let key = id.rsplit('.').next().unwrap_or(id);
                    catalog().require_display(display).conditional_keys.iter().any(|known| known == key)
                        || definition.optional_fields.iter().any(|known| known == key)
                        || id == "show_full_distance"
                };
                for cell in cells(&blocks) {
                    if cell.rule == Rule::Field && !matches!(cell.kind, CellKind::Empty) {
                        assert!(at(&widget.data, &cell.id).is_some() || optional(&cell.id), "{} {display}: {} has no data", definition.kind, cell.id);
                    }
                }
            }
        }
    }

    #[test]
    fn switching_a_gauge_to_bars_seeds_its_geometry() {
        let (dispatcher, _) = editor(vec![with_display("speed", "linear", "g")]);
        dispatcher.dispatch(|store, _| store.editor.set_selected_widget_id(Some("g")));
        edit(&dispatcher, "g", "display_variants.linear.track_fill_style", Input::Text("bars".into()));
        let linear = widget(&dispatcher, "g", |data| data["display_variants"]["linear"].clone());
        assert_eq!(linear["track_fill_style"], "bars");
        assert!(linear["bar_count"].as_f64().unwrap() >= 3.0 && linear["bar_gap"] == json!(4));
        assert!(cell(&dispatcher, "g", "display_variants.linear.bar_gap").is_some(), "bars show their count and gap");
        assert!(cell(&dispatcher, "g", "display_variants.linear.track_fill_flat").is_none());
        let count = linear["bar_count"].clone();
        edit(&dispatcher, "g", "display_variants.linear.track_fill_style", Input::Text("bars".into()));
        assert_eq!(widget(&dispatcher, "g", |data| data["display_variants"]["linear"]["bar_count"].clone()), count, "already bars: nothing is seeded");
    }

    #[test]
    fn a_thinner_arc_track_clamps_its_corner_radius() {
        let (dispatcher, _) = editor(vec![with_display("speed", "arc", "a")]);
        dispatcher.dispatch(|store, _| {
            store.editor.set_selected_widget_id(Some("a"));
            update_widget_data(store, "a", patch(json!({"display_variants": {"arc": {"track_thickness": 40, "track_corner_radius": 20}}}))).unwrap();
        });
        edit(&dispatcher, "a", "display_variants.arc.track_thickness", Input::Number(10.0));
        let arc = widget(&dispatcher, "a", |data| data["display_variants"]["arc"].clone());
        assert_eq!((arc["track_thickness"].clone(), arc["track_corner_radius"].clone()), (json!(10), json!(5)));
        assert_eq!(slider_value(&cell(&dispatcher, "a", "display_variants.arc.track_corner_radius").unwrap()).1, 5.0, "the radius slider's maximum follows");
    }

    #[test]
    fn the_linear_orientation_swaps_the_frame_and_label_side() {
        let (dispatcher, _) = editor(vec![with_display("speed", "linear", "g")]);
        dispatcher.dispatch(|store, _| store.editor.set_selected_widget_id(Some("g")));
        let before = widget(&dispatcher, "g", |data| data["display_variants"]["linear"].clone());
        let next = if before["orientation"] == "horizontal" { "vertical" } else { "horizontal" };
        edit(&dispatcher, "g", "display_variants.linear.orientation", Input::Text(next.into()));
        let after = widget(&dispatcher, "g", |data| data["display_variants"]["linear"].clone());
        assert_eq!((after["width"].clone(), after["height"].clone()), (before["height"].clone(), before["width"].clone()));
        assert_ne!(after["min_max_label_position"], before["min_max_label_position"]);
        let history = dispatcher.with_store(|store| store.history.past.len());
        edit(&dispatcher, "g", "display_variants.linear.orientation", Input::Text(next.into()));
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), history, "the same orientation changes nothing");
    }

    #[test]
    fn g_force_axes_swap_and_invert() {
        let (dispatcher, _) = editor(vec![with_display("g_force", "g_force", "f")]);
        dispatcher.dispatch(|store, _| {
            store.editor.set_selected_widget_id(Some("f"));
            update_widget_data(store, "f", patch(json!({"display_variants": {"g_force": {"axis_horizontal": "x", "axis_vertical": "y", "invert_horizontal": false}}}))).unwrap();
        });
        edit(&dispatcher, "f", "display_variants.g_force.axis_horizontal", Input::Text("y".into()));
        let axes = widget(&dispatcher, "f", |data| data["display_variants"]["g_force"].clone());
        assert_eq!((axes["axis_horizontal"].clone(), axes["axis_vertical"].clone()), (json!("y"), json!("x")));
        edit(&dispatcher, "f", "display_variants.g_force.axis_horizontal", Input::Flag(true));
        assert_eq!(widget(&dispatcher, "f", |data| data["display_variants"]["g_force"]["invert_horizontal"].clone()), json!(true));
    }

    #[test]
    fn the_g_force_size_scales_every_dimension() {
        let (dispatcher, _) = editor(vec![with_display("g_force", "g_force", "f")]);
        dispatcher.dispatch(|store, _| store.editor.set_selected_widget_id(Some("f")));
        let before = widget(&dispatcher, "f", |data| data["display_variants"]["g_force"].clone());
        let diameter = before["diameter"].as_f64().unwrap();
        dispatcher.dispatch(move |store, services| super::super::edit_field(store, services, "f", "display_variants.g_force.diameter", Input::Number(diameter * 2.0), super::super::Edit::Draft).unwrap());
        let draft = dispatcher.with_store(|store| store.drafts.widget_draft("f").cloned().unwrap());
        assert_eq!(draft["display_variants"]["g_force"]["diameter"].as_f64().unwrap(), (diameter * 2.0).round());
        assert_eq!(draft["width"].as_f64().unwrap(), (before["width"].as_f64().unwrap() * 2.0).round());
    }

    #[test]
    fn heart_zones_derive_whole_ascending_bpm_and_manual_bounds_stay_apart() {
        let defaults = catalog().require_display("heart_zones").variant_defaults["manual_zones"].clone();
        assert_eq!(Value::Array(derive_lthr_zones(165.0)), defaults);
        let tiny: Vec<(f64, f64)> = derive_lthr_zones(3.0).iter().map(|zone| (zone["from"].as_f64().unwrap(), zone["to"].as_f64().unwrap())).collect();
        assert_eq!(tiny, [(1.0, 2.0), (2.0, 3.0), (3.0, 4.0), (4.0, 5.0), (5.0, 6.0)]);

        let (dispatcher, _) = editor(vec![with_display("heartrate", "heart_zones", "h")]);
        dispatcher.dispatch(|store, _| {
            store.editor.set_selected_widget_id(Some("h"));
            update_widget_data(store, "h", patch(json!({"display_variants": {"heart_zones": {"zone_calculation": "max_heartrate", "max_heartrate": 200}}}))).unwrap();
        });
        edit(&dispatcher, "h", "display_variants.heart_zones.zone_calculation", Input::Text("manual".into()));
        let zones = widget(&dispatcher, "h", |data| data["display_variants"]["heart_zones"]["manual_zones"].clone());
        assert_eq!(zones, Value::Array(derive_lthr_zones(180.0)), "manual seeds from the estimated LTHR");
        let to = zones[0]["to"].as_f64().unwrap();
        edit(&dispatcher, "h", "display_variants.heart_zones.manual_zones.1.from", Input::Text("10".into()));
        assert_eq!(widget(&dispatcher, "h", |data| data["display_variants"]["heart_zones"]["manual_zones"][1]["from"].as_f64().unwrap()), to, "a bound stays above the previous zone");
        edit(&dispatcher, "h", "display_variants.heart_zones.manual_zones.0.from", Input::Text("-5".into()));
        assert_eq!(widget(&dispatcher, "h", |data| data["display_variants"]["heart_zones"]["manual_zones"][0]["from"].clone()), json!(0));
    }

    #[test]
    fn splits_keep_column_order_a_positive_length_and_bars_within_their_height() {
        let (dispatcher, _) = editor(vec![with_display("splits_run", "splits_run", "s")]);
        dispatcher.dispatch(|store, _| {
            store.editor.set_selected_widget_id(Some("s"));
            update_widget_data(store, "s", patch(json!({"visible_columns": ["split", "pace"], "track_thickness": 20, "track_corner_radius": 8, "track_border_thickness": 6}))).unwrap();
        });
        edit(&dispatcher, "s", "visible_columns", Input::Text("bar".into()));
        let draft = |dispatcher: &crate::store::Dispatcher| dispatcher.with_store(|store| store.drafts.widget_draft("s").and_then(|draft| draft.get("visible_columns").cloned()));
        assert_eq!(draft(&dispatcher), Some(json!(["split", "bar", "pace"])), "toggling a column drafts it in drawn order");

        dispatcher.dispatch(|store, _| update_widget_data(store, "s", patch(json!({"visible_columns": ["pace"]}))).unwrap());
        edit(&dispatcher, "s", "visible_columns", Input::Text("pace".into()));
        assert_eq!(widget(&dispatcher, "s", |data| data["visible_columns"].clone()), json!(["pace"]), "the last column stays");

        for (raw, expected) in [("0.5", json!(0.5)), ("0", json!(0.5)), ("", json!(0.5)), ("abc", json!(0.5))] {
            edit(&dispatcher, "s", "split_distance", Input::Text(raw.into()));
            assert_eq!(widget(&dispatcher, "s", |data| data["split_distance"].clone()), expected, "{raw}");
        }

        dispatcher.dispatch(|store, _| update_widget_data(store, "s", patch(json!({"visible_columns": ["split", "bar", "pace"]}))).unwrap());
        edit(&dispatcher, "s", "track_thickness", Input::Number(6.0));
        widget(&dispatcher, "s", |data| assert_eq!((data["track_thickness"].clone(), data["track_corner_radius"].clone(), data["track_border_thickness"].clone()), (json!(6), json!(3), json!(2))));
    }

    #[test]
    fn heading_tape_controls_follow_their_toggles() {
        let (dispatcher, _) = editor(vec![with_display("heading", "heading_tape", "t")]);
        dispatcher.dispatch(|store, _| {
            store.editor.set_selected_widget_id(Some("t"));
            update_widget_data(store, "t", patch(json!({"display_variants": {"heading_tape": {"show_major_ticks": false, "indicator_style": "highlight_bar"}}}))).unwrap();
        });
        let blocks = inspector(&dispatcher, "t");
        assert!(find(&blocks, "display_variants.heading_tape.major_tick_length_pct").unwrap().disabled);
        assert!(!find(&blocks, "display_variants.heading_tape.minor_tick_length_pct").unwrap().disabled);
        assert!(find(&blocks, "display_variants.heading_tape.indicator_placement").unwrap().disabled);
        edit(&dispatcher, "t", "display_variants.heading_tape.show_major_ticks", Input::Flag(true));
        assert_eq!(widget(&dispatcher, "t", |data| data["display_variants"]["heading_tape"]["show_major_ticks"].clone()), json!(true));
    }
}
