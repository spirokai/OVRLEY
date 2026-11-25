//! The type editors (text, time, backdrop, gradient, raster, elevation, diving profile, route map,
//! tile map, rolling chart) and the sections they share (plot markers, route line), as
//! descriptions.

use ovrley_widgets::model::catalog::catalog;
use ovrley_widgets::model::document::js_round;
use serde_json::Value;
use crate::features::widgets::{starting_altitude_placeholder, NumericFieldOptions};
use crate::features::widgets::blocks::{full, grid, grid_end, heading, stack, when, Cell, CellKind, Node, OptionView, Rule, ShapeToggle, SwitchRowStyle};
use crate::features::widgets::displays::toggle_group;
use crate::features::widgets::rules::{chart_limits, format_window_seconds, max_edge_strip_thickness, unused_chart_metrics, variant, Named};
use crate::features::widgets::sections::{dimensions_section, fixed, font_section, icon_section, units_row, Ctx, FontSection, UnitsRow};

const TIME_FORMATS: [(&str, &str, &str); 19] = [
    (
        "date-dd-mm-yyyy",
        "widget-editor.dateFormatDdMmYyyy",
        "Date only (DD-MM-YYYY)",
    ),
    (
        "date-mm-dd-yyyy",
        "widget-editor.dateFormatMmDdYyyy",
        "Date only (MM-DD-YYYY)",
    ),
    (
        "date-yyyy-mm-dd",
        "widget-editor.dateFormatYyyyMmDd",
        "Date only (YYYY-MM-DD)",
    ),
    (
        "date-dd-mmm-yyyy",
        "widget-editor.dateFormatDdMmmYyyy",
        "Date only (DD MMM YYYY)",
    ),
    (
        "date-mmm-dd-yyyy",
        "widget-editor.dateFormatMmmDdYyyy",
        "Date only (MMM DD YYYY)",
    ),
    (
        "date-dd-mmmm-yyyy",
        "widget-editor.dateFormatDdMmmmYyyy",
        "Date only (DD MMMM YYYY)",
    ),
    (
        "date-mmmm-dd-yyyy",
        "widget-editor.dateFormatMmmmDdYyyy",
        "Date only (MMMM DD YYYY)",
    ),
    ("time-24", "widget-editor.timeFormat24h", "Time only (24h)"),
    (
        "time-24s",
        "widget-editor.timeFormat24hWithSeconds",
        "Time only (24h with seconds)",
    ),
    ("time-12", "widget-editor.timeFormat12h", "Time only (12h)"),
    (
        "time-12s",
        "widget-editor.timeFormat12hWithSeconds",
        "Time only (12h with seconds)",
    ),
    (
        "date-time-24",
        "widget-editor.dateTimeFormat24h",
        "Date + time (24h)",
    ),
    (
        "date-time-24s",
        "widget-editor.dateTimeFormat24hWithSeconds",
        "Date + time (24h with seconds)",
    ),
    (
        "date-time-12",
        "widget-editor.dateTimeFormat12h",
        "Date + time (12h)",
    ),
    (
        "date-time-12s",
        "widget-editor.dateTimeFormat12hWithSeconds",
        "Date + time (12h with seconds)",
    ),
    (
        "date-mmm-time-24",
        "widget-editor.dateTimeFormatDdMmm24h",
        "Date + time (DD MMM, 24h)",
    ),
    (
        "date-mmm-time-12",
        "widget-editor.dateTimeFormatDdMmm12h",
        "Date + time (DD MMM, 12h)",
    ),
    (
        "date-mmmm-time-24",
        "widget-editor.dateTimeFormatDdMmmm24h",
        "Date + time (DD MMMM, 24h)",
    ),
    (
        "date-mmmm-time-12",
        "widget-editor.dateTimeFormatDdMmmm12h",
        "Date + time (DD MMMM, 12h)",
    ),
];

/// The type's own editor; `None` for types without one.
pub fn type_editor(ctx: &Ctx) -> Option<Vec<Node>> {
    Some(match ctx.widget.kind.as_str() {
        "label" => text_editor(ctx),
        "time" => time_editor(ctx),
        "backdrop" => backdrop_editor(ctx),
        "gradient" => gradient_editor(ctx),
        "raster" => raster_editor(ctx),
        "elevation" => profile_editor(ctx, false),
        "diving_profile" => profile_editor(ctx, true),
        "course" => route_map_editor(ctx),
        "map" => map_editor(ctx),
        "rolling_chart" => rolling_chart_editor(ctx),
        _ => return None,
    })
}

/// A manifest choice list (`modes`, `elapsedOrigins`) as options, each label translated.
fn choices(
    ctx: &Ctx,
    choices: &[ovrley_widgets::model::catalog::ManifestChoice],
) -> Vec<OptionView> {
    choices
        .iter()
        .map(|choice| {
            OptionView::new(
                choice.value.clone(),
                ctx.t(
                    &choice.label_key,
                    choice
                        .extra
                        .get("defaultLabel")
                        .and_then(Value::as_str)
                        .unwrap_or(&choice.value),
                ),
            )
        })
        .collect()
}

/// A theme fallback colour (`value || getThemeColor(name)`).
fn color_or(ctx: &Ctx, id: &str, label: String, fallback: &str) -> Cell {
    let value = ctx
        .value(id)
        .and_then(Value::as_str)
        .filter(|color| !color.is_empty())
        .unwrap_or(fallback)
        .to_string();
    Cell::new(id, label, CellKind::Color { value })
}

/// The text widget's editor.
fn text_editor(ctx: &Ctx) -> Vec<Node> {
    vec![font_section(
        ctx,
        FontSection {
            title: Some(ctx.t("widget-editor.typography", "Typography")),
            show_text_input: true,
            color_label: Some(ctx.t("widget-editor.color", "Color")),
            ..FontSection::default()
        },
    )]
}

/// The time widget's editor.
fn time_editor(ctx: &Ctx) -> Vec<Node> {
    let definition = catalog()
        .require_widget("time")
        .expect("the catalog defines time");
    let elapsed = ctx.text("time_mode") == "elapsed";
    vec![
        stack(
            1.0,
            [
                full(toggle_group(
                    ctx,
                    "time_mode".into(),
                    ctx.t("widget-editor.timeMode", "Time Mode"),
                    choices(ctx, &definition.modes),
                )),
                when(elapsed, || {
                    stack(
                        1.0,
                        [
                            full(toggle_group(
                                ctx,
                                "elapsed_origin".into(),
                                ctx.t("widget-editor.elapsedFrom", "Elapsed From"),
                                choices(ctx, &definition.elapsed_origins),
                            )),
                            grid(
                                2,
                                [ctx.switch_row(
                                    "show_hundredths",
                                    ctx.t("widget-editor.hundredths", "Hundredths"),
                                    SwitchRowStyle::PADDED_LEFT,
                                )],
                            ),
                        ],
                    )
                }),
                when(!elapsed, || {
                    full(ctx.select(
                        "format",
                        ctx.t("widget-editor.format", "Format"),
                        ctx.options(&TIME_FORMATS),
                    ))
                }),
            ],
        ),
        font_section(
            ctx,
            FontSection {
                show_content_alignment: true,
                ..FontSection::default()
            },
        ),
        icon_section(ctx, Node::Empty),
    ]
}

fn backdrop_border_max(ctx: &Ctx, display: &str) -> f64 {
    let size = |key: &str| {
        ctx.value(&format!("display_variants.{display}.{key}"))
            .or_else(|| ctx.value(key))
            .and_then(Value::as_f64)
            .unwrap_or(1.0)
    };
    let extent = if display == "circle" {
        size("diameter")
    } else {
        size("width").min(size("height"))
    };
    ((extent - 1.0) * 0.2).floor().max(0.0)
}

/// The backdrop widget's editor.
fn backdrop_editor(ctx: &Ctx) -> Vec<Node> {
    let display = ctx.text("display_type");
    let circle = display == "circle";
    let catalog = catalog();
    let definition = catalog
        .require_widget("backdrop")
        .expect("the catalog defines backdrop");
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
    let variant_id = |key: &str| format!("display_variants.{display}.{key}");
    let border_max = backdrop_border_max(ctx, &display);
    let border_thickness = ctx
        .value("border_thickness")
        .and_then(Value::as_f64)
        .unwrap_or_default();
    let border = || {
        ctx.slider(
            "border_thickness",
            ctx.t("widget-editor.borderThickness", "Border Thickness"),
            0.0,
            border_max,
        )
        .shown(border_thickness.min(border_max))
        .int()
        .px()
        .readout(crate::format::plain_number(border_thickness))
    };
    let size_cells = if circle {
        vec![ctx
            .number(
                variant_id("diameter"),
                ctx.t("widget-editor.diameter", "Diameter"),
                NumericFieldOptions::default(),
            )
            .rule(Rule::Named(Named::BackdropDiameter))]
    } else {
        vec![
            ctx.number(
                variant_id("width"),
                ctx.t("widget-editor.width", "Width"),
                NumericFieldOptions::default(),
            )
            .rule(Rule::Named(Named::BackdropSize)),
            ctx.number(
                variant_id("height"),
                ctx.t("widget-editor.height", "Height"),
                NumericFieldOptions::default(),
            )
            .rule(Rule::Named(Named::BackdropSize)),
        ]
    };
    let mut nodes = vec![
        stack(
            1.0,
            [
                full(
                    ctx.select(
                        "display_type",
                        ctx.t("widget-editor.displayType", "Display Type"),
                        display_options,
                    )
                    .rule(Rule::Named(Named::BackdropDisplay)),
                ),
                grid(2, size_cells),
            ],
        ),
        stack(
            1.0,
            [
                heading(
                    "palette",
                    ctx.t("widget-editor.backdropStyle", "Backdrop Style"),
                    None,
                ),
                grid(
                    2,
                    [
                        ctx.color("fill_color", ctx.t("widget-editor.fillColor", "Fill Color")),
                        ctx.color(
                            "border_color",
                            ctx.t("widget-editor.borderColor", "Border Color"),
                        ),
                    ],
                ),
                grid(
                    2,
                    [
                        ctx.slider(
                            "fill_opacity",
                            ctx.t("widget-editor.fillOpacity", "Fill Opacity"),
                            0.0,
                            1.0,
                        )
                        .percent(),
                        ctx.slider(
                            "border_opacity",
                            ctx.t("widget-editor.borderOpacity", "Border Opacity"),
                            0.0,
                            1.0,
                        )
                        .percent(),
                    ],
                ),
                when(circle, || full(border())),
            ],
        ),
    ];
    if display == "rectangle" {
        let rectangle = variant(ctx.data(), "rectangle");
        let number = |key: &str| rectangle.get(key).and_then(Value::as_f64);
        let radius_max = (number("width")
            .unwrap_or_default()
            .min(number("height").unwrap_or_default())
            * 0.5)
            .max(0.0);
        let corner_radius = number("corner_radius")
            .unwrap_or_default()
            .max(0.0)
            .min(radius_max);
        let strip_max = max_edge_strip_thickness(
            number("width").unwrap_or_default(),
            number("height").unwrap_or_default(),
        );
        let edges: Vec<&str> = rectangle
            .get("edge_strips")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let corner = |key: &str, label_key: &str, default: &str| ShapeToggle {
            key: key.into(),
            edge: false,
            active: rectangle
                .get(key)
                .and_then(Value::as_bool)
                .unwrap_or_default(),
            label: ctx.t(label_key, default),
        };
        let edge = |key: &str, label_key: &str, default: &str| ShapeToggle {
            key: key.into(),
            edge: true,
            active: edges.contains(&key),
            label: ctx.t(label_key, default),
        };
        // `SHAPE_GRID_CELLS` without the centre: corners at the corners, edges between them.
        let toggles = vec![
            corner(
                "round_top_left",
                "widget-editor.roundTopLeftCorner",
                "Round top left corner",
            ),
            edge("top", "widget-editor.topEdgeStrip", "Top edge strip"),
            corner(
                "round_top_right",
                "widget-editor.roundTopRightCorner",
                "Round top right corner",
            ),
            edge("left", "widget-editor.leftEdgeStrip", "Left edge strip"),
            edge("right", "widget-editor.rightEdgeStrip", "Right edge strip"),
            corner(
                "round_bottom_left",
                "widget-editor.roundBottomLeftCorner",
                "Round bottom left corner",
            ),
            edge(
                "bottom",
                "widget-editor.bottomEdgeStrip",
                "Bottom edge strip",
            ),
            corner(
                "round_bottom_right",
                "widget-editor.roundBottomRightCorner",
                "Round bottom right corner",
            ),
        ];
        let shape = Cell::new(
            "shape",
            ctx.t("widget-editor.cornersAndEdges", "Corners & edges"),
            CellKind::ShapeGrid { toggles },
        )
        .rule(Rule::Named(Named::ShapeToggle));
        nodes.push(stack(
            1.0,
            [
                heading("shapes", ctx.t("widget-editor.shape", "Shape"), None),
                Node::Aside {
                    cells: vec![
                        shape,
                        border(),
                        ctx.slider(
                            variant_id("corner_radius"),
                            ctx.t("widget-editor.cornerRadius", "Corner Radius"),
                            0.0,
                            radius_max,
                        )
                        .shown(corner_radius)
                        .readout(crate::format::plain_number(corner_radius))
                        .int()
                        .px()
                        .rule(Rule::Named(Named::Clamp {
                            min: 0.0,
                            max: radius_max,
                        })),
                    ],
                },
                grid(
                    2,
                    [
                        ctx.slider(
                            variant_id("edge_strip_thickness"),
                            ctx.t("widget-editor.stripThickness", "Strip Thickness"),
                            0.0,
                            strip_max,
                        )
                        .int()
                        .px()
                        .rule(Rule::Named(Named::Clamp {
                            min: f64::NEG_INFINITY,
                            max: strip_max,
                        })),
                        ctx.color(
                            variant_id("edge_strip_color"),
                            ctx.t("widget-editor.stripColor", "Strip Color"),
                        ),
                    ],
                ),
            ],
        ));
    }
    nodes
}

/// The gradient widget's editor.
fn gradient_editor(ctx: &Ctx) -> Vec<Node> {
    let triangle = ctx.flag("show_triangle");
    let sign = ctx.switch_row(
        "show_sign",
        ctx.t("widget-editor.showSign", "Show sign"),
        SwitchRowStyle::Label {
            padding: [1.125 + 0.625, 0.0, 0.625, 2.0],
            label_top: 0.0,
        },
    );
    vec![
        font_section(
            ctx,
            FontSection {
                title: Some(ctx.t("widget-editor.typography", "Typography")),
                font_size_label: Some(ctx.t("widget-editor.fontSize", "Font Size")),
                color_label: Some(ctx.t("widget-editor.valueColor", "Value Color")),
                ..FontSection::default()
            },
        ),
        full(
            ctx.slider(
                "value_offset",
                ctx.t("widget-editor.valueOffset", "Value Offset"),
                -200.0,
                200.0,
            )
            .int()
            .px(),
        ),
        grid(
            2,
            [
                ctx.slider(
                    "decimals",
                    ctx.t("widget-editor.decimals", "Decimals"),
                    0.0,
                    2.0,
                ),
                sign,
            ],
        ),
        stack(
            1.0,
            [
                heading(
                    "trending-up",
                    ctx.t("widget-editor.indicator", "Indicator"),
                    Some(ctx.switch("show_triangle", String::new())),
                ),
                grid(
                    2,
                    [
                        color_or(
                            ctx,
                            "triangle_positive_color",
                            ctx.t("widget-editor.colorPositive", "Color Positive"),
                            &ctx.theme.aqua,
                        )
                        .disabled(!triangle),
                        color_or(
                            ctx,
                            "triangle_negative_color",
                            ctx.t("widget-editor.colorNegative", "Color Negative"),
                            &ctx.theme.accent,
                        )
                        .disabled(!triangle),
                    ],
                ),
                full(
                    ctx.slider(
                        "triangle_width",
                        ctx.t("widget-editor.width", "Width"),
                        0.0,
                        240.0,
                    )
                    .int()
                    .px()
                    .disabled(!triangle),
                ),
                units_row(
                    ctx,
                    UnitsRow {
                        title: ctx.t("widget-editor.unit", "Unit"),
                        show_toggle: false,
                        color: ctx
                            .data()
                            .get("unit_color")
                            .map(|color| color.as_str().unwrap_or_default().to_string()),
                        color_label: Some(ctx.t("widget-editor.percentColor", "Percent Color")),
                        select_label: ctx.t("widget-editor.unit", "Unit"),
                        options: Vec::new(),
                    },
                ),
            ],
        ),
    ]
}

/// The raster editor: the dimensions, the rotation, then the image picker, 0.5rem apart with 1rem
/// below the dimensions.
fn raster_editor(ctx: &Ctx) -> Vec<Node> {
    let rotation = crate::features::canvas::geometry::normalize_rotation(ctx.num("rotation"));
    let path = ctx.text("path");
    let picker = Cell::new(
        "path",
        ctx.t("raster.imageFiles", "Image files"),
        CellKind::RasterPicker {
            placeholder: ctx.t("raster.selectImage", "Select image"),
            button_label: if path.is_empty() {
                ctx.t("raster.selectImage", "Select image")
            } else {
                ctx.t("raster.replaceImage", "Replace image")
            },
            loading: super::raster::is_loading(ctx.store, &ctx.widget.id),
            error: super::raster::raster_error(ctx.store, &ctx.widget.id).unwrap_or_default(),
            path: path.clone(),
        },
    )
    .tooltip(path)
    .rule(Rule::Named(Named::RasterPick));
    vec![stack(
        0.5,
        [
            dimensions_section(ctx, 1.0),
            full(
                ctx.slider(
                    "rotation",
                    ctx.t("widget-editor.rotation", "Rotation"),
                    0.0,
                    360.0,
                )
                .shown(rotation)
                .int()
                .suffix("°"),
            )
            .pad_top(1.0),
            full(picker).pad_top(1.0),
        ],
    )]
}

/// The plot marker section.
pub fn plot_marker_section(
    ctx: &Ctx,
    show_color: bool,
    show_arrow: bool,
    disabled: bool,
    trailing: Option<Cell>,
) -> Node {
    let variant_value = ctx.text("marker_variant");
    let size = ctx.num("marker_size");
    let mut options = vec![
        ("single", "widget-editor.singleCircle", "Single Circle"),
        ("ring", "widget-editor.concentricRing", "Concentric Ring"),
        ("halo", "widget-editor.solidHalo", "Solid Halo"),
    ];
    if show_arrow {
        options.push(("caret", "widget-editor.markerArrow", "Arrow"));
    }
    stack(
        1.0,
        [
            heading("map", ctx.t("widget-editor.marker", "Marker"), trailing),
            grid(
                2,
                [
                    ctx.select(
                        "marker_variant",
                        ctx.t("widget-editor.markerType", "Marker Type"),
                        ctx.options(&options),
                    )
                    .disabled(disabled),
                    ctx.slider(
                        "marker_size",
                        ctx.t("widget-editor.size", "Size"),
                        1.0,
                        50.0,
                    )
                    .int()
                    .px()
                    .disabled(disabled),
                ],
            ),
            grid(
                2,
                [
                    show_color.then(|| {
                        ctx.color("marker_color", ctx.t("widget-editor.color", "Color"))
                            .disabled(disabled)
                    }),
                    Some(
                        ctx.slider(
                            "marker_opacity",
                            ctx.t("widget-editor.opacity", "Opacity"),
                            0.0,
                            100.0,
                        )
                        .suffix("%")
                        .disabled(disabled),
                    ),
                    (variant_value == "ring" || variant_value == "halo").then(|| {
                        let label = if variant_value == "ring" {
                            ctx.t("widget-editor.ringDiameter", "Ring Diameter")
                        } else {
                            ctx.t("widget-editor.haloDiameter", "Halo Diameter")
                        };
                        ctx.slider(
                            "marker_variant_diameter",
                            label,
                            js_round(size * 2.0).max(4.0),
                            120.0,
                        )
                        .int()
                        .px()
                        .disabled(disabled)
                    }),
                ],
            ),
        ],
    )
}

/// The route line section's slots: a thickness replacing the default, the detail and smoothing
/// sliders.
pub struct RouteLineSlots {
    pub thickness: Option<Cell>,
    pub detail: Option<Cell>,
    pub smoothing: Option<Cell>,
}

/// The route line section.
pub fn route_line_section(ctx: &Ctx, slots: RouteLineSlots) -> Node {
    let line_width = ctx
        .value("completed_line_width")
        .or_else(|| ctx.value("remaining_line_width"))
        .and_then(Value::as_f64)
        .unwrap_or_default();
    let has_thickness = slots.thickness.is_some();
    let smoothing = slots.smoothing.unwrap_or_else(|| {
        let tolerance = ctx.num("simplify_tolerance_px");
        ctx.slider(
            "simplify_tolerance_px",
            ctx.t("widget-editor.smoothing", "Smoothing"),
            0.0,
            4.0,
        )
        .step(0.05)
        .readout(fixed(tolerance, 2))
        .px()
        .rule(Rule::Named(Named::Fixed(2)))
    });
    stack(
        1.0,
        [
            heading(
                "palette",
                ctx.t("widget-editor.lineStyling", "Line Styling"),
                None,
            ),
            when(!has_thickness, || {
                full(
                    ctx.slider(
                        "completed_line_width",
                        ctx.t("widget-editor.thickness", "Thickness"),
                        0.0,
                        20.0,
                    )
                    .shown(line_width)
                    .readout(crate::format::plain_number(line_width))
                    .int()
                    .px()
                    .rule(Rule::Named(Named::BothLineWidths)),
                )
            }),
            grid(2, [slots.thickness, Some(smoothing), slots.detail]),
            grid(
                2,
                [
                    color_or(
                        ctx,
                        "completed_line_color",
                        ctx.t("widget-editor.finishedColor", "Finished Color"),
                        &ctx.theme.ice,
                    )
                    .rule(Rule::Named(Named::FinishedColor)),
                    color_or(
                        ctx,
                        "remaining_line_color",
                        ctx.t("widget-editor.remainingColor", "Remaining Color"),
                        &ctx.theme.teal,
                    ),
                ],
            ),
            grid(
                2,
                [
                    ctx.slider(
                        "completed_line_opacity",
                        ctx.t("widget-editor.finishedOpacity", "Finished Opacity"),
                        0.0,
                        100.0,
                    )
                    .suffix("%"),
                    ctx.slider(
                        "remaining_line_opacity",
                        ctx.t("widget-editor.remainingOpacity", "Remaining Opacity"),
                        0.0,
                        100.0,
                    )
                    .suffix("%"),
                ],
            ),
            grid(
                2,
                [ctx.switch_row(
                    "show_full_activity",
                    ctx.t("widget-editor.showFullActivity", "Show Full Activity"),
                    SwitchRowStyle::Label {
                        padding: [0.5, 0.0, 0.5, 0.25],
                        label_top: 0.0,
                    },
                )],
            ),
        ],
    )
}

/// The route map widget's editor.
fn route_map_editor(ctx: &Ctx) -> Vec<Node> {
    let rotation = crate::features::canvas::geometry::normalize_rotation(ctx.num("rotation"));
    let density = ctx.num("target_density");
    let detail = ctx
        .slider(
            "target_density",
            ctx.t("widget-editor.routeDetail", "Route Detail"),
            0.25,
            1.5,
        )
        .step(0.05)
        .readout(fixed(density, 2))
        .suffix("x")
        .rule(Rule::Named(Named::Fixed(2)));
    vec![
        dimensions_section(ctx, 0.0),
        full(
            ctx.slider(
                "rotation",
                ctx.t("widget-editor.mapRotation", "Map Rotation"),
                0.0,
                360.0,
            )
            .shown(rotation)
            .int()
            .suffix("°"),
        ),
        route_line_section(
            ctx,
            RouteLineSlots {
                thickness: None,
                detail: Some(detail),
                smoothing: None,
            },
        ),
        plot_marker_section(ctx, true, true, false, None),
    ]
}

/// The map widget's editor.
fn map_editor(ctx: &Ctx) -> Vec<Node> {
    use crate::features::maps::{gpu_alert, map_buildings_available, map_style_select};
    let chase = ctx.text("camera_mode") == "follow";
    let circle = ctx.text("shape") == "circle";
    let buildings_available = map_buildings_available(ctx.data());
    let radius_max = ctx.num("width").min(ctx.num("height")) / 2.0;
    let (style_options, style_value) = map_style_select(ctx.store, ctx.value("map_style").and_then(Value::as_str));
    let msaa = || {
        Cell::new(
            "msaa_samples",
            ctx.t("widget-editor.msaa", "Anti-aliasing (MSAA)"),
            CellKind::Select {
                value: crate::format::plain_number(ctx.num("msaa_samples")),
                options: vec![OptionView::new("4", "4x"), OptionView::new("1", ctx.t("widget-editor.msaaOff", "Off"))],
                resettable: false,
            },
        )
        .rule(Rule::NumberChoice)
    };
    let zoom = |id: &str, min: f64| {
        let value = ctx.num(id);
        ctx.slider(id, ctx.t("widget-editor.zoom", "Zoom"), min, 19.0)
            .step(0.1)
            .readout(fixed(value, 1))
            .rule(Rule::Named(Named::Fixed(1)))
    };
    let layer = |id: &str, label: String| ctx.switch_row(id, label, SwitchRowStyle::INLINE);
    let mut buildings = layer("show_buildings", ctx.t("widget-editor.mapBuildings", "Buildings")).disabled(!buildings_available);
    if let CellKind::SwitchRow { checked, .. } = &mut buildings.kind {
        *checked = buildings_available && *checked;
    }
    let style = Cell::new(
        "map_style",
        ctx.t("widget-editor.mapStyle", "Map Style"),
        CellKind::Select { value: style_value, options: style_options, resettable: false },
    );
    let mut nodes = vec![];
    if let Some(text) = gpu_alert(ctx.store, ctx.locale) {
        nodes.push(full(Cell::new("gpu_alert", "", CellKind::Alert { text }).rule(Rule::None)));
    }
    nodes.push(if circle {
        grid(
            2,
            [Cell::new(
                "width",
                ctx.t("widget-editor.diameter", "Diameter"),
                CellKind::Number {
                    value: super::sections::whole_number_text(ctx.value("width")),
                    placeholder: String::new(),
                    suffix: String::new(),
                    resettable: false,
                },
            )
            .rule(Rule::Named(Named::MapDiameter))],
        )
    } else {
        dimensions_section(ctx, 8.0)
    });
    nodes.push(stack(
        1.0,
        [
            heading("map", ctx.t("widget-editor.mapView", "Map View"), None),
            grid(
                2,
                [
                    style,
                    ctx.select(
                        "camera_mode",
                        ctx.t("widget-editor.cameraMode", "Camera Mode"),
                        ctx.options(&[
                            ("follow", "widget-editor.chaseCamera", "Chase Camera"),
                            ("fixed", "widget-editor.topDown", "Top Down"),
                        ]),
                    ),
                ],
            ),
            when(chase, || {
                stack(
                    1.0,
                    [
                        grid(
                            2,
                            [
                                ctx.select(
                                    "orientation",
                                    ctx.t("widget-editor.orientation", "Orientation"),
                                    ctx.options(&[
                                        ("rider_up", "widget-editor.riderUp", "Rider Up"),
                                        ("north_up", "widget-editor.northUp", "North Up"),
                                    ]),
                                ),
                                msaa(),
                            ],
                        ),
                        grid(
                            2,
                            [
                                zoom("follow_zoom", 14.0),
                                ctx.slider("pitch", ctx.t("widget-editor.pitch", "Pitch"), 0.0, 60.0).int().suffix("°"),
                            ],
                        ),
                    ],
                )
            }),
            when(!chase, || grid(2, [zoom("fixed_zoom", 2.0), msaa()])),
            grid(
                2,
                [
                    buildings,
                    layer("show_paths", ctx.t("widget-editor.mapPaths", "Paths")),
                    layer("show_labels", ctx.t("widget-editor.mapLabels", "Labels")),
                    layer("local_names", ctx.t("widget-editor.mapLocalNames", "Local Names")).disabled(!ctx.flag("show_labels")),
                ],
            ),
        ],
    ));
    nodes.push(stack(
        1.0,
        [
            heading("shapes", ctx.t("widget-editor.shape", "Shape"), None),
            grid(
                2,
                [
                    Some(
                        ctx.select(
                            "shape",
                            ctx.t("widget-editor.shape", "Shape"),
                            ctx.options(&[
                                ("rectangle", "widgets.backdropTypes.rectangle", "Rectangle"),
                                ("circle", "widgets.backdropTypes.circle", "Circle"),
                            ]),
                        )
                        .rule(Rule::Named(Named::MapShape)),
                    ),
                    (!circle).then(|| {
                        let radius = ctx.num("corner_radius").min(radius_max);
                        ctx.slider("corner_radius", ctx.t("widget-editor.cornerRadius", "Corner Radius"), 0.0, radius_max)
                            .shown(radius)
                            .readout(crate::format::plain_number(js_round(radius)))
                            .px()
                    }),
                ],
            ),
            grid(
                2,
                [
                    ctx.color("border_color", ctx.t("widget-editor.borderColor", "Border Color")),
                    ctx.slider("border_thickness", ctx.t("widget-editor.borderThickness", "Border Thickness"), 0.0, 50.0).int().px(),
                ],
            ),
            grid(2, [ctx.slider("border_opacity", ctx.t("widget-editor.borderOpacity", "Border Opacity"), 0.0, 1.0).percent()]),
        ],
    ));
    let smoothing = ctx.num("route_smoothing_m");
    nodes.push(route_line_section(
        ctx,
        RouteLineSlots {
            thickness: Some(ctx.slider("line_width", ctx.t("widget-editor.thickness", "Thickness"), 1.0, 20.0).int().px()),
            detail: None,
            smoothing: Some(
                ctx.slider("route_smoothing_m", ctx.t("widget-editor.smoothing", "Smoothing"), 1.0, 20.0)
                    .step(0.5)
                    .readout(fixed(smoothing, 1))
                    .suffix("m"),
            ),
        },
    ));
    nodes.push(plot_marker_section(ctx, true, true, false, None));
    nodes
}

/// The elevation and diving profile editors, which differ in the second section, the
/// label toggles' fields and the starting elevation.
fn profile_editor(ctx: &Ctx, diving: bool) -> Vec<Node> {
    let line_width = if diving {
        ctx.num("completed_line_width")
    } else {
        ctx.value("completed_line_width")
            .or_else(|| ctx.value("remaining_line_width"))
            .and_then(Value::as_f64)
            .unwrap_or_default()
    };
    let tolerance = ctx.num("simplify_tolerance_px");
    let density = ctx.num("target_density");
    let y_scale = ctx.num("y_scale");
    let scene_font_size = ctx
        .store
        .editor
        .config
        .scene
        .get("font_size")
        .and_then(Value::as_f64);
    let label_size = ctx
        .value("point_label.font_size")
        .and_then(Value::as_f64)
        .or(if diving { None } else { scene_font_size })
        .unwrap_or(12.5);
    let (metric_key, imperial_key) = if diving {
        ("show_depth_metric", "show_depth_imperial")
    } else {
        ("show_elevation_metric", "show_elevation_imperial")
    };
    let label_toggle = |key: &str, label: String| {
        ctx.switch_row(
            key,
            label,
            SwitchRowStyle::Label {
                padding: [0.0, 0.0, 0.75, 0.25],
                label_top: 0.0,
            },
        )
    };
    let offset = |key: &str, label: String, enabled: bool| {
        ctx.slider(key, label, -100.0, 100.0)
            .int()
            .px()
            .disabled(!enabled)
    };
    let line_colors = if diving {
        vec![
            ctx.color(
                "completed_line_color",
                ctx.t("widget-editor.finishedColor", "Finished Color"),
            ),
            ctx.color(
                "remaining_line_color",
                ctx.t("widget-editor.remainingColor", "Remaining Color"),
            ),
        ]
    } else {
        vec![
            color_or(
                ctx,
                "completed_line_color",
                ctx.t("widget-editor.finishedColor", "Finished Color"),
                &ctx.theme.ice,
            )
            .rule(Rule::Named(Named::FinishedColor)),
            color_or(
                ctx,
                "remaining_line_color",
                ctx.t("widget-editor.remainingColor", "Remaining Color"),
                &ctx.theme.teal,
            ),
        ]
    };
    let second_section = if diving {
        stack(
            1.0,
            [
                heading(
                    "palette",
                    ctx.t(
                        "widget-editor.decompressionCeiling",
                        "Decompression Ceiling",
                    ),
                    None,
                ),
                grid(
                    2,
                    [
                        ctx.color(
                            "ceiling_line_color",
                            ctx.t("widget-editor.ceilingLineColor", "Ceiling Line Color"),
                        ),
                        ctx.color(
                            "ceiling_area_color",
                            ctx.t("widget-editor.ceilingAreaColor", "Ceiling Area Color"),
                        ),
                        ctx.slider(
                            "ceiling_line_width",
                            ctx.t("widget-editor.thickness", "Thickness"),
                            0.0,
                            20.0,
                        )
                        .px(),
                        ctx.slider(
                            "ceiling_line_opacity",
                            ctx.t("widget-editor.ceilingLineOpacity", "Ceiling Line Opacity"),
                            0.0,
                            100.0,
                        )
                        .suffix("%"),
                        ctx.slider(
                            "ceiling_area_opacity",
                            ctx.t("widget-editor.ceilingAreaOpacity", "Ceiling Area Opacity"),
                            0.0,
                            100.0,
                        )
                        .suffix("%"),
                    ],
                ),
            ],
        )
    } else {
        let mut gradient = ctx.switch(
            "area_gradient",
            ctx.t("widget-editor.areaGradient", "Gradient"),
        );
        gradient.kind = CellKind::Switch {
            checked: ctx.flag("area_gradient"),
            label_style: Some(SwitchRowStyle::INLINE_X),
        };
        stack(
            1.0,
            [
                heading(
                    "palette",
                    ctx.t("widget-editor.areaStyling", "Area Styling"),
                    Some(gradient),
                ),
                grid(
                    2,
                    [
                        color_or(
                            ctx,
                            "area_completed_color",
                            ctx.t("widget-editor.finishedColor", "Finished Color"),
                            &ctx.theme.ice,
                        ),
                        color_or(
                            ctx,
                            "area_remaining_color",
                            ctx.t("widget-editor.remainingColor", "Remaining Color"),
                            &ctx.theme.teal,
                        ),
                    ],
                ),
                grid(
                    2,
                    [
                        ctx.slider(
                            "area_completed_opacity",
                            ctx.t("widget-editor.finishedOpacity", "Finished Opacity"),
                            0.0,
                            100.0,
                        )
                        .suffix("%"),
                        ctx.slider(
                            "area_remaining_opacity",
                            ctx.t("widget-editor.remainingOpacity", "Remaining Opacity"),
                            0.0,
                            100.0,
                        )
                        .suffix("%"),
                    ],
                ),
            ],
        )
    };
    let metric = ctx.flag(metric_key);
    let imperial = ctx.flag(imperial_key);
    let mut nodes = vec![
        dimensions_section(ctx, 0.0),
        stack(
            1.0,
            [
                heading(
                    "palette",
                    ctx.t("widget-editor.lineStyling", "Line Styling"),
                    None,
                ),
                full(
                    ctx.slider(
                        "completed_line_width",
                        ctx.t("widget-editor.thickness", "Thickness"),
                        if diving { 1.0 } else { 0.0 },
                        20.0,
                    )
                    .shown(line_width)
                    .readout(crate::format::plain_number(line_width))
                    .int()
                    .px()
                    .rule(Rule::Named(Named::BothLineWidths)),
                ),
                grid(
                    2,
                    [
                        ctx.slider(
                            "simplify_tolerance_px",
                            ctx.t("widget-editor.smoothing", "Smoothing"),
                            0.0,
                            4.0,
                        )
                        .step(0.05)
                        .readout(fixed(tolerance, 2))
                        .px()
                        .rule(Rule::Named(Named::Fixed(2))),
                        ctx.slider(
                            "target_density",
                            ctx.t("widget-editor.profileDetail", "Profile Detail"),
                            0.25,
                            1.5,
                        )
                        .step(0.05)
                        .readout(fixed(density, 2))
                        .suffix("x")
                        .rule(Rule::Named(Named::Fixed(2))),
                    ],
                ),
                grid(2, line_colors),
                grid(
                    2,
                    [
                        ctx.slider(
                            "completed_line_opacity",
                            ctx.t("widget-editor.finishedOpacity", "Finished Opacity"),
                            0.0,
                            100.0,
                        )
                        .suffix("%"),
                        ctx.slider(
                            "remaining_line_opacity",
                            ctx.t("widget-editor.remainingOpacity", "Remaining Opacity"),
                            0.0,
                            100.0,
                        )
                        .suffix("%"),
                    ],
                ),
                grid(
                    2,
                    [
                        {
                            let (min, max) = if diving { (0.2, 4.0) } else { (0.1, 3.0) };
                            ctx.slider(
                                "y_scale",
                                ctx.t("widget-editor.verticalScale", "Vertical Scale"),
                                min,
                                max,
                            )
                            .step(0.05)
                            .readout(fixed(y_scale, 2))
                            .suffix("x")
                            .rule(Rule::Named(Named::Fixed(2)))
                        },
                        ctx.switch_row(
                            "show_full_activity",
                            ctx.t("widget-editor.showFullActivity", "Show Full Activity"),
                            SwitchRowStyle::Label {
                                padding: [1.5, 0.25, 0.0, 0.25],
                                label_top: 0.0,
                            },
                        ),
                    ],
                ),
                second_section,
            ],
        ),
        plot_marker_section(ctx, true, false, false, None),
    ];
    let mut labels = vec![
        heading("mountain", ctx.t("widget-editor.labels", "Labels"), None),
        full(
            Cell::new(
                "point_label.font_size",
                ctx.t("widget-editor.labelSize", "Label Size"),
                CellKind::Slider {
                    editable: true,
                    value: label_size,
                    min: 5.0,
                    max: 50.0,
                    step: 1.0,
                    scale: 1.0,
                    readout: crate::format::plain_number(js_round(label_size)),
                    suffix: "px".into(),
                },
            )
            .rule(Rule::Named(Named::ObjectField)),
        ),
        // Two columns of a toggle and its offsets, as rows.
        grid(
            2,
            [
                label_toggle(
                    metric_key,
                    ctx.t("widget-editor.metricLabel", "Metric Label"),
                ),
                label_toggle(
                    imperial_key,
                    ctx.t("widget-editor.imperialLabel", "Imperial Label"),
                ),
                offset(
                    "metric_label_offset_x",
                    ctx.t("widget-editor.metricOffsetX", "Metric Offset X"),
                    metric,
                ),
                offset(
                    "imperial_label_offset_x",
                    ctx.t("widget-editor.imperialOffsetX", "Imperial Offset X"),
                    imperial,
                ),
                offset(
                    "metric_label_offset_y",
                    ctx.t("widget-editor.metricOffsetY", "Metric Offset Y"),
                    metric,
                ),
                offset(
                    "imperial_label_offset_y",
                    ctx.t("widget-editor.imperialOffsetY", "Imperial Offset Y"),
                    imperial,
                ),
            ],
        )
        .pad_top(0.5),
    ];
    if !diving {
        let preview_activity = crate::features::widgets::preview::preview_activity(ctx.store);
        let activity = preview_activity
            .activity()
            .map(|imported| imported.activity.as_ref());
        let placeholder = starting_altitude_placeholder(ctx.widget, activity)
            .map(|value| format!("{value}"))
            .unwrap_or_default();
        let mut start = ctx.number(
            "starting_altitude",
            ctx.t("widget-editor.elevationAtStart", "Elevation at start"),
            NumericFieldOptions {
                optional: true,
                round: true,
                ..NumericFieldOptions::default()
            },
        );
        if let CellKind::Number {
            placeholder: shown, ..
        } = &mut start.kind
        {
            *shown = placeholder;
        }
        let unit = Cell::new(
            "starting_altitude_unit",
            "",
            CellKind::Select {
                value: ctx.text("starting_altitude_unit"),
                options: vec![OptionView::new("m", "m"), OptionView::new("ft", "ft")],
                resettable: true,
            },
        )
        .rule(Rule::Named(Named::AltitudeInputUnit));
        labels.push(grid(2, [start, unit]));
    }
    nodes.push(stack(1.0, labels));
    nodes
}

/// The rolling chart widget's editor.
fn rolling_chart_editor(ctx: &Ctx) -> Vec<Node> {
    let series: Vec<Value> = ctx
        .data()
        .get("series")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let unused = unused_chart_metrics(&series);
    let area = ctx.flag("area_fill");
    let legend = ctx.flag("show_legend");
    let catalog = catalog();
    let metric_name = |metric: &str| {
        ctx.t(
            &catalog
                .require_widget(metric)
                .expect("plotted metrics are in the catalog")
                .name_key,
            metric,
        )
    };
    let window_text = ctx
        .store
        .widgets
        .window_drafts
        .get(&ctx.widget.id)
        .cloned();
    let window = Cell::new(
        "window_seconds",
        ctx.t("widget-editor.timeWindow", "Time Window"),
        CellKind::WindowSeconds {
            invalid: window_text.is_some(),
            text: window_text.unwrap_or_else(|| format_window_seconds(ctx.num("window_seconds"))),
        },
    )
    .rule(Rule::Named(Named::WindowSeconds));
    let add = Cell::new(
        "series_add",
        ctx.t("widget-editor.addMetric", "Add metric"),
        CellKind::IconButton {
            icon: "plus",
            outline: true,
        },
    )
    .disabled(series.len() >= chart_limits().max_series || unused.is_empty())
    .rule(Rule::Named(Named::ChartAdd));
    let mut metrics = vec![heading(
        "chart-line",
        ctx.t("widget-editor.chartMetrics", "Metrics"),
        Some(add),
    )];
    for (index, item) in series.iter().enumerate() {
        let metric = item
            .get("metric")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let options: Vec<OptionView> = std::iter::once(metric.clone())
            .chain(unused.iter().cloned())
            .map(|value| OptionView::new(value.clone(), metric_name(&value)))
            .collect();
        let row = Cell::new(
            format!("series.{index}"),
            ctx.t("widget-editor.chartMetric", "Metric"),
            CellKind::ChartSeries {
                metric,
                options,
                color: item
                    .get("color")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                first: index == 0,
                last: index == series.len() - 1,
                only: series.len() == 1,
            },
        )
        .rule(Rule::Named(Named::ChartSeries(index)));
        metrics.push(full(row));
    }
    let mut fill = ctx.switch("area_fill", ctx.t("widget-editor.fill", "Fill"));
    fill.kind = CellKind::Switch {
        checked: area,
        label_style: Some(SwitchRowStyle::Tall),
    };
    let mut marker_switch = ctx.switch("show_marker", ctx.t("widget-editor.marker", "Marker"));
    marker_switch.label = ctx.t("widget-editor.marker", "Marker");
    vec![
        dimensions_section(ctx, 8.0),
        full(window),
        stack(0.75, metrics),
        stack(
            1.0,
            [
                heading(
                    "palette",
                    ctx.t("widget-editor.lineStyling", "Line Styling"),
                    Some(fill),
                ),
                grid(
                    2,
                    [
                        ctx.slider(
                            "line_width",
                            ctx.t("widget-editor.thickness", "Thickness"),
                            1.0,
                            20.0,
                        )
                        .int()
                        .px(),
                        ctx.slider(
                            "line_opacity",
                            ctx.t("widget-editor.opacity", "Opacity"),
                            0.0,
                            100.0,
                        )
                        .suffix("%"),
                    ],
                ),
                full(
                    ctx.slider(
                        "line_curve",
                        ctx.t("widget-editor.smoothing", "Smoothing"),
                        0.0,
                        100.0,
                    )
                    .step(5.0)
                    .suffix("%"),
                ),
                grid_end(
                    2,
                    [
                        ctx.slider(
                            "area_opacity",
                            ctx.t("widget-editor.areaOpacity", "Area Opacity"),
                            0.0,
                            100.0,
                        )
                        .suffix("%")
                        .disabled(!area),
                        ctx.switch_row(
                            "area_gradient",
                            ctx.t("widget-editor.areaGradient", "Gradient"),
                            SwitchRowStyle::Tall,
                        )
                        .disabled(!area),
                    ],
                ),
            ],
        ),
        plot_marker_section(
            ctx,
            false,
            false,
            !ctx.flag("show_marker"),
            Some(marker_switch),
        ),
        stack(
            1.0,
            [
                heading(
                    "type",
                    ctx.t("widget-editor.legend", "Legend"),
                    Some(ctx.switch("show_legend", ctx.t("widget-editor.legend", "Legend"))),
                ),
                grid_end(
                    2,
                    [
                        ctx.font("legend_font", ctx.t("widget-editor.font", "Font"))
                            .small_label()
                            .disabled(!legend),
                        ctx.slider(
                            "legend_font_size",
                            ctx.t("widget-editor.fontSize", "Font Size"),
                            6.0,
                            100.0,
                        )
                        .int()
                        .px()
                        .disabled(!legend),
                    ],
                ),
                grid(
                    2,
                    [ctx.color(
                        "legend_color",
                        ctx.t("widget-editor.fontColor", "Font Color"),
                    )
                    .disabled(!legend)],
                ),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::super::blocks::{at, cells, Rule};
    use super::super::rules::{format_window_seconds, parse_window_seconds, Input};
    use super::super::tests::{cell, edit, editor_with, patch, widget};
    use super::super::{describe, update_widget_data};
    use super::*;
    use crate::store::template::ThemeColors;
    use crate::features::widgets::build_config_widgets;
    use crate::store::Store;
    use ovrley_core::normalize::template::template_manifest;
    use ovrley_widgets::model::document::{create_widget_defaults, OverlayConfig, WidgetCreationOptions, WidgetSelection};
    use serde_json::json;

    /// `kind`'s defaults in its collection, as `{collection: [data]}` with id `w`.
    fn document(kind: &str, display: Option<&str>) -> Value {
        let catalog = catalog();
        let definition = catalog.require_widget(kind).unwrap();
        let mode = display.and_then(|display| catalog.require_display(display).modes.first().map(|mode| mode.value.clone()));
        let selection = WidgetSelection { kind: kind.into(), display_type: display.map(str::to_string), mode, ..WidgetSelection::default() };
        let mut data = create_widget_defaults(&selection, &template_manifest().globals, &WidgetCreationOptions::default()).unwrap();
        data.insert("id".into(), json!("w"));
        json!({ definition.collection.as_str(): [data] })
    }

    fn open(kind: &str, display: Option<&str>) -> crate::store::Dispatcher {
        let (dispatcher, _) = editor_with(document(kind, display));
        dispatcher.dispatch(|store, _| store.editor.set_selected_widget_id(Some("w")));
        dispatcher
    }

    #[test]
    fn every_type_editor_describes_and_its_fields_resolve() {
        for definition in catalog().widgets().iter().filter(|definition| definition.current && !definition.metric) {
            let displays: Vec<Option<&str>> = if definition.display_types.is_empty() { vec![None] } else { definition.display_types.iter().map(|display| Some(display.as_str())).collect() };
            for display in displays {
                let mut store = Store { theme: Some(ThemeColors { ice: "#1".into(), aqua: "#2".into(), teal: "#3".into(), accent: "#4".into() }), ..Store::default() };
                store.editor.set_config(OverlayConfig::from_value(&document(&definition.kind, display)).unwrap());
                let widget = build_config_widgets(&store.editor.config).remove(0);
                let blocks = describe(&store, &widget);
                assert!(type_editor(&Ctx::new(&store, &widget)).is_some(), "{} has an editor", definition.kind);
                for cell in cells(&blocks) {
                    let optional = definition.optional_fields.iter().any(|key| cell.id.ends_with(key.as_str()));
                    if cell.rule == Rule::Field && !matches!(cell.kind, CellKind::Empty) {
                        assert!(at(&widget.data, &cell.id).is_some() || optional, "{} {display:?}: {} has no data", definition.kind, cell.id);
                    }
                }
            }
        }
    }

    #[test]
    fn a_smaller_backdrop_keeps_its_radius_and_strips_inside() {
        let dispatcher = open("backdrop", Some("rectangle"));
        dispatcher.dispatch(|store, _| {
            update_widget_data(store, "w", patch(json!({"display_variants": {"rectangle": {"width": 300, "height": 200, "corner_radius": 80, "edge_strip_thickness": 90, "edge_strips": ["left"]}}}))).unwrap();
        });
        edit(&dispatcher, "w", "display_variants.rectangle.height", Input::Text("100".into()));
        let rectangle = widget(&dispatcher, "w", |data| data["display_variants"]["rectangle"].clone());
        assert_eq!((rectangle["height"].clone(), rectangle["corner_radius"].clone(), rectangle["edge_strip_thickness"].clone()), (json!(100), json!(50), json!(50)));
        edit(&dispatcher, "w", "shape", Input::Text("top".into()));
        assert_eq!(widget(&dispatcher, "w", |data| data["display_variants"]["rectangle"]["edge_strips"].clone()), json!(["top", "left"]), "edges stay in canonical order");
        edit(&dispatcher, "w", "shape", Input::Text("round_top_left".into()));
        let corner = widget(&dispatcher, "w", |data| data["display_variants"]["rectangle"]["round_top_left"].clone());
        edit(&dispatcher, "w", "shape", Input::Text("round_top_left".into()));
        assert_ne!(widget(&dispatcher, "w", |data| data["display_variants"]["rectangle"]["round_top_left"].clone()), corner, "a corner toggles");
    }

    #[test]
    fn backdrop_display_changes_seed_the_variant_non_destructively() {
        let dispatcher = open("backdrop", Some("rectangle"));
        let rectangle = widget(&dispatcher, "w", |data| data["display_variants"]["rectangle"].clone());
        edit(&dispatcher, "w", "display_type", Input::Text("circle".into()));
        widget(&dispatcher, "w", |data| {
            assert_eq!(data["display_type"], "circle");
            assert!(data["display_variants"]["circle"]["diameter"].is_number());
            assert_eq!(data["display_variants"]["rectangle"], rectangle, "the rectangle variant stays");
        });
        edit(&dispatcher, "w", "display_variants.circle.diameter", Input::Text("64.9".into()));
        assert_eq!(widget(&dispatcher, "w", |data| data["display_variants"]["circle"]["diameter"].clone()), json!(64));
    }

    #[test]
    fn chart_windows_parse_seconds_or_minutes() {
        assert_eq!(parse_window_seconds("45"), Some(45.0));
        assert_eq!(parse_window_seconds(" 1:30 "), Some(90.0));
        assert_eq!(parse_window_seconds("5:00"), Some(300.0));
        for text in ["4", "5:01", "1:5", "1:75", "", "abc", "-10", "1.5"] {
            assert_eq!(parse_window_seconds(text), None, "{text}");
        }
        assert_eq!((format_window_seconds(30.0), format_window_seconds(125.0)), ("0:30".to_string(), "2:05".to_string()));
    }

    #[test]
    fn chart_series_rows_edit_the_series_and_the_window_commits_only_when_it_parses() {
        let dispatcher = open("rolling_chart", None);
        let before = widget(&dispatcher, "w", |data| data["series"].as_array().unwrap().clone());
        edit(&dispatcher, "w", "series_add", Input::Press);
        let added = widget(&dispatcher, "w", |data| data["series"].as_array().unwrap().clone());
        assert_eq!(added.len(), before.len() + 1);
        let used: Vec<&Value> = before.iter().map(|item| &item["color"]).collect();
        assert!(!used.contains(&&added[before.len()]["color"]), "a new series takes an unused colour");
        edit(&dispatcher, "w", "series.0", Input::Text("down".into()));
        assert_eq!(widget(&dispatcher, "w", |data| data["series"][1].clone()), added[0], "moving swaps neighbours");
        edit(&dispatcher, "w", "series.1", Input::Text("color:#123456".into()));
        assert_eq!(widget(&dispatcher, "w", |data| data["series"][1]["color"].clone()), json!("#123456"));
        edit(&dispatcher, "w", "series.0", Input::Text("remove".into()));
        assert_eq!(widget(&dispatcher, "w", |data| data["series"].as_array().unwrap().len()), added.len() - 1);

        edit(&dispatcher, "w", "window_seconds", Input::Text("1:30".into()));
        assert_eq!(widget(&dispatcher, "w", |data| data["window_seconds"].clone()), json!(90));
        edit(&dispatcher, "w", "window_seconds", Input::Text("nope".into()));
        assert_eq!(widget(&dispatcher, "w", |data| data["window_seconds"].clone()), json!(90), "an invalid window changes nothing");
        let window = cell(&dispatcher, "w", "window_seconds").unwrap();
        assert!(matches!(window.kind, CellKind::WindowSeconds { invalid: true, ref text } if text == "nope"), "and stays as an invalid draft");
        edit(&dispatcher, "w", "window_seconds", Input::Clear);
        assert!(matches!(cell(&dispatcher, "w", "window_seconds").unwrap().kind, CellKind::WindowSeconds { invalid: false, ref text } if text == "1:30"));
    }

    #[test]
    fn elevation_units_convert_the_start_and_the_reset_clears_it() {
        let dispatcher = open("elevation", None);
        dispatcher.dispatch(|store, _| update_widget_data(store, "w", patch(json!({"starting_altitude": 100, "starting_altitude_unit": "m"}))).unwrap());
        edit(&dispatcher, "w", "starting_altitude_unit", Input::Text("ft".into()));
        widget(&dispatcher, "w", |data| assert_eq!((data["starting_altitude"].clone(), data["starting_altitude_unit"].clone()), (json!(328), json!("ft"))));
        edit(&dispatcher, "w", "starting_altitude_unit", Input::Clear);
        assert_eq!(widget(&dispatcher, "w", |data| data["starting_altitude"].clone()), Value::Null);
        edit(&dispatcher, "w", "completed_line_color", Input::Text("#abcdef".into()));
        widget(&dispatcher, "w", |data| assert_eq!((data["completed_line_color"].clone(), data["color"].clone()), (json!("#abcdef"), json!("#abcdef"))));
        dispatcher.dispatch(|store, services| super::super::edit_field(store, services, "w", "completed_line_width", Input::Number(7.0), super::super::Edit::Commit).unwrap());
        widget(&dispatcher, "w", |data| assert_eq!((data["completed_line_width"].clone(), data["remaining_line_width"].clone()), (json!(7), json!(7))));
    }

    #[test]
    fn labels_toggle_italic_and_time_shows_its_mode_controls() {
        let dispatcher = open("label", None);
        let italic = cell(&dispatcher, "w", "italic").unwrap();
        assert!(matches!(italic.kind, CellKind::Italic { .. }));
        assert!(cell(&dispatcher, "w", "text").is_some() && cell(&dispatcher, "w", "letter_spacing").is_some());
        let time = open("time", None);
        time.dispatch(|store, _| update_widget_data(store, "w", patch(json!({"time_mode": "elapsed"}))).unwrap());
        assert!(cell(&time, "w", "elapsed_origin").is_some() && cell(&time, "w", "show_hundredths").is_some());
        assert!(cell(&time, "w", "format").is_none());
    }
}
