//! The description context and the shared sections of the widget editors: Position, Dimensions,
//! Font, Icon and Units. The cell builders resolve each form control's values.

use ovrley_widgets::model::document::Collection;
use serde_json::{Map, Value};
use crate::features::widgets::{ConfigWidget, NumericFieldOptions};
use crate::features::widgets::blocks::{full, grid, grid_end, heading, stack, when, Cell, CellKind, Node, OptionView, Rule, SwitchRowStyle};
use crate::features::widgets::rules::Named;
use crate::i18n::{catalog as translations, Locale};
use crate::store::Store;
use crate::store::template::ThemeColors;

/// What a description reads: the store, the widget as the inspector shows it, the language and
/// the theme's fallback colours.
pub struct Ctx<'a> {
    pub store: &'a Store,
    pub widget: &'a ConfigWidget,
    pub locale: Locale,
    pub theme: &'a ThemeColors,
}

impl<'a> Ctx<'a> {
    pub fn new(store: &'a Store, widget: &'a ConfigWidget) -> Self {
        Self {
            store,
            widget,
            locale: store.i18n.language,
            theme: store
                .theme
                .as_ref()
                .expect("ovrley_ui reads the theme before the first dispatch"),
        }
    }

    pub fn data(&self) -> &'a Map<String, Value> {
        &self.widget.data
    }

    /// `t(key, default)`.
    pub fn t(&self, key: &str, default: &str) -> String {
        translations().t_or(self.locale, key, default).to_string()
    }

    /// `(value, key, default)` triples as options.
    pub fn options(&self, options: &[(&str, &str, &str)]) -> Vec<OptionView> {
        options
            .iter()
            .map(|(value, key, default)| OptionView::new(*value, self.t(key, default)))
            .collect()
    }

    pub fn value(&self, path: &str) -> Option<&'a Value> {
        super::blocks::at(self.data(), path)
    }

    /// A numeric field; absent reads as 0, as the components' arithmetic would.
    pub fn num(&self, path: &str) -> f64 {
        self.value(path).and_then(Value::as_f64).unwrap_or_default()
    }

    pub fn text(&self, path: &str) -> String {
        self.value(path)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    pub fn flag(&self, path: &str) -> bool {
        self.value(path)
            .and_then(Value::as_bool)
            .unwrap_or_default()
    }

    /// The active display's variant path of `key`.
    pub fn var(&self, key: &str) -> String {
        format!("display_variants.{}.{key}", self.text("display_type"))
    }

    /// A slider over the field at `id`, showing its plain value.
    pub fn slider(
        &self,
        id: impl Into<String>,
        label: impl Into<String>,
        min: f64,
        max: f64,
    ) -> Cell {
        let id = id.into();
        let value = self.num(&id);
        Cell::new(
            id,
            label,
            CellKind::Slider {
                editable: true,
                value,
                min,
                max,
                step: 1.0,
                scale: 1.0,
                readout: crate::format::plain_number(value),
                suffix: String::new(),
            },
        )
    }

    /// The color field.
    pub fn color(&self, id: impl Into<String>, label: impl Into<String>) -> Cell {
        let id = id.into();
        let value = self.text(&id);
        Cell::new(id, label, CellKind::Color { value })
    }

    /// A select over a string field.
    pub fn select(
        &self,
        id: impl Into<String>,
        label: impl Into<String>,
        options: Vec<OptionView>,
    ) -> Cell {
        let id = id.into();
        let value = self.text(&id);
        Cell::new(id, label, CellKind::Select { value, options, resettable: false })
    }

    /// A heading's switch.
    pub fn switch(&self, id: impl Into<String>, label: impl Into<String>) -> Cell {
        let id = id.into();
        let checked = self.flag(&id);
        Cell::new(id, label, CellKind::Switch { checked, label_style: None })
    }

    /// A label and a switch filling a cell.
    pub fn switch_row(
        &self,
        id: impl Into<String>,
        label: impl Into<String>,
        style: SwitchRowStyle,
    ) -> Cell {
        let id = id.into();
        let checked = self.flag(&id);
        Cell::new(id, label, CellKind::SwitchRow { checked, style })
    }

    /// A number field with these options, showing the rounded value.
    pub fn number(
        &self,
        id: impl Into<String>,
        label: impl Into<String>,
        options: NumericFieldOptions,
    ) -> Cell {
        let id = id.into();
        let value = whole_number_text(self.value(&id));
        Cell::new(
            id,
            label,
            CellKind::Number {
                value,
                placeholder: String::new(),
                suffix: String::new(),
                resettable: false,
            },
        )
        .rule(Rule::Numeric(options))
    }

    /// The bundled options for this value.
    pub fn font(&self, id: impl Into<String>, label: impl Into<String>) -> Cell {
        let id = id.into();
        let value = self.text(&id);
        let (recommended, _) = super::font_options(self.store, &value);
        Cell::new(id, label, CellKind::Font { value, recommended, small_label: false })
    }
}

/// A number field's shown value: empty for an absent value, else the rounded value.
pub fn whole_number_text(value: Option<&Value>) -> String {
    match value.and_then(Value::as_f64) {
        Some(number) => crate::format::plain_number(number.round()),
        None => String::new(),
    }
}

/// `value` with exactly `digits` decimals.
pub fn fixed(value: f64, digits: usize) -> String {
    format!("{value:.digits$}")
}

/// The slider values a description sets after building the cell.
impl Cell {
    fn slider_mut(
        &mut self,
    ) -> (
        &mut f64,
        &mut f64,
        &mut f64,
        &mut f64,
        &mut f64,
        &mut String,
        &mut String,
    ) {
        match &mut self.kind {
            CellKind::Slider {
                value,
                min,
                max,
                step,
                scale,
                readout,
                suffix,
                ..
            } => (value, min, max, step, scale, readout, suffix),
            other => panic!("{} is not a slider: {other:?}", self.id),
        }
    }

    /// A slider without `editable`: its readout is plain text.
    pub fn fixed(mut self) -> Self {
        if let CellKind::Slider { editable, .. } = &mut self.kind {
            *editable = false;
        }
        self
    }

    /// The font select's 9px label.
    pub fn small_label(mut self) -> Self {
        if let CellKind::Font { small_label, .. } = &mut self.kind {
            *small_label = true;
        }
        self
    }

    pub fn step(mut self, step: f64) -> Self {
        *self.slider_mut().3 = step;
        self
    }

    pub fn suffix(mut self, suffix: &str) -> Self {
        *self.slider_mut().6 = suffix.into();
        self
    }

    /// `suffix="px"`.
    pub fn px(self) -> Self {
        self.suffix("px")
    }

    /// The readout is the rounded value.
    pub fn int(mut self) -> Self {
        let (value, _, _, _, _, readout, _) = self.slider_mut();
        *readout = crate::format::plain_number((*value).round());
        self
    }

    pub fn readout(mut self, text: impl Into<String>) -> Self {
        *self.slider_mut().5 = text.into();
        self
    }

    /// The shown value, when it is not the field's (a clamped radius, a percent).
    pub fn shown(mut self, shown: f64) -> Self {
        *self.slider_mut().0 = shown;
        self
    }

    pub fn max(mut self, max: f64) -> Self {
        *self.slider_mut().2 = max;
        self
    }

    /// A 0–1 opacity slider: steps of 0.05, shown ×100, the readout in whole percent.
    pub fn percent(mut self) -> Self {
        let (value, _, _, step, scale, readout, suffix) = self.slider_mut();
        *step = 0.05;
        *scale = 100.0;
        *readout = crate::format::plain_number((*value * 100.0).round());
        *suffix = "%".into();
        self
    }
}

// ── The sections ────────────────────────────────────────────────────────────────────────────────

/// The position section, with the inspector's reset button in its heading.
pub fn position_section(ctx: &Ctx) -> Node {
    let widget = ctx.widget;
    let uses_horizontal_anchor = widget.collection == Collection::Values
        && ctx.text("display_type") == "text"
        && widget.kind != "gradient"
        && widget.kind != "lap_timer";
    let opacity = (ctx.num("opacity") * 100.0).round();
    let reset = Cell::new(
        "reset",
        String::new(),
        CellKind::IconButton {
            icon: "rotate-ccw",
            outline: false,
        },
    )
    .rule(Rule::None);
    stack(
        1.0,
        [
            heading(
                "move",
                ctx.t("widget-editor.general", "General"),
                Some(reset),
            ),
            grid(
                2,
                [
                    ctx.number(
                        "x",
                        if uses_horizontal_anchor {
                            ctx.t("widget-editor.horizontalAnchor", "Horizontal Anchor")
                        } else {
                            ctx.t("widget-editor.horizontalPosition", "Horizontal Position")
                        },
                        NumericFieldOptions::default(),
                    ),
                    ctx.number(
                        "y",
                        ctx.t("widget-editor.verticalPosition", "Vertical Position"),
                        NumericFieldOptions::default(),
                    ),
                ],
            )
            .pad_top(0.5),
            full(
                ctx.slider(
                    "opacity",
                    ctx.t("widget-editor.transparency", "Transparency"),
                    0.0,
                    100.0,
                )
                .shown(opacity)
                .readout(crate::format::plain_number(opacity))
                .suffix("%")
                .rule(Rule::Scaled(100.0)),
            ),
        ],
    )
}

/// The dimensions section.
pub fn dimensions_section(ctx: &Ctx, min: f64) -> Node {
    let options = NumericFieldOptions {
        min: Some(min),
        ..NumericFieldOptions::default()
    };
    stack(
        0.75,
        [
            heading(
                "trending-up",
                ctx.t("widget-editor.dimensions", "Dimensions"),
                None,
            ),
            grid(
                2,
                [
                    ctx.number("width", ctx.t("widget-editor.width", "Width"), options),
                    ctx.number("height", ctx.t("widget-editor.height", "Height"), options),
                ],
            ),
        ],
    )
}

/// The font section's options.
pub struct FontSection {
    pub title: Option<String>,
    pub show_text_input: bool,
    pub font_size_label: Option<String>,
    pub size_min: f64,
    pub size_max: f64,
    pub color_label: Option<String>,
    pub show_content_alignment: bool,
}

impl Default for FontSection {
    fn default() -> Self {
        Self {
            title: None,
            show_text_input: false,
            font_size_label: None,
            size_min: 20.0,
            size_max: 200.0,
            color_label: None,
            show_content_alignment: false,
        }
    }
}

/// The font section.
pub fn font_section(ctx: &Ctx, section: FontSection) -> Node {
    let is_label = ctx.widget.collection == Collection::Labels;
    // The label's family capabilities, once loaded.
    let family = is_label.then(|| ctx.store.widgets.fonts.capabilities(&ctx.text("font"))).flatten();
    let italic_supported = family.is_some_and(|family| ovrley_core::fonts::family_supports_italic(family).unwrap_or_default());
    let trailing = if is_label {
        Some(
            Cell::new("italic", ctx.t("widget-editor.italic", "Italic"), CellKind::Italic { pressed: ctx.flag("italic") && italic_supported })
                .disabled(!italic_supported)
                .tooltip(ctx.t("widget-editor.italic", "Italic"))
                .rule(Rule::Named(Named::ToggleItalic)),
        )
    } else if section.show_content_alignment {
        let options = ctx.options(&[
            ("left", "widget-editor.alignLeft", "Align left"),
            ("center", "widget-editor.alignCenter", "Align center"),
            ("right", "widget-editor.alignRight", "Align right"),
        ]);
        Some(Cell::new(
            "content_alignment",
            ctx.t("widget-editor.contentAlignment", "Content Alignment"),
            CellKind::Alignment {
                value: ctx.text("content_alignment"),
                options,
            },
        ))
    } else {
        None
    };
    let color = ctx
        .data()
        .get("color")
        .and_then(Value::as_str)
        .filter(|color| !color.is_empty())
        .unwrap_or(&ctx.theme.ice)
        .to_string();
    stack(
        1.0,
        [
            heading(
                "type",
                section
                    .title
                    .unwrap_or_else(|| ctx.t("widget-editor.typography", "Typography")),
                trailing,
            ),
            when(section.show_text_input, || {
                full(Cell::new(
                    "text",
                    ctx.t("widget-editor.text", "Text"),
                    CellKind::Text {
                        value: ctx.text("text"),
                        placeholder: String::new(),
                    },
                ))
            }),
            full(
                ctx.slider(
                    "font_size",
                    section
                        .font_size_label
                        .unwrap_or_else(|| ctx.t("widget-editor.fontSize", "Font Size")),
                    section.size_min,
                    section.size_max,
                )
                .int()
                .px(),
            ),
            grid_end(
                2,
                [
                    ctx.font("font", ctx.t("widget-editor.fontFamily", "Font Family"))
                        .rule(Rule::Named(Named::Font)),
                    Cell::new(
                        "color",
                        section
                            .color_label
                            .unwrap_or_else(|| ctx.t("widget-editor.fontColor", "Font Color")),
                        CellKind::Color { value: color },
                    ),
                ],
            ),
            when(is_label, || label_typography(ctx, family)),
        ],
    )
}

/// The labels' weight and letter spacing. The weight
/// slider shows once the family's capabilities are in: over the matched face's weight axis, step
/// 100, disabled without one.
fn label_typography(ctx: &Ctx, family: Option<&ovrley_core::fonts::FontFamily>) -> Node {
    let weight = family.and_then(|family| ovrley_core::fonts::weight_control(family, ctx.num("font_weight") as f32, ctx.flag("italic")).ok());
    let letter_spacing = ctx.num("letter_spacing");
    grid_end(
        2,
        [
            weight.map(|control| {
                let (min, max) = control.axis.map_or((1.0, 1000.0), |(min, max)| (f64::from(min), f64::from(max)));
                let mut cell = ctx.slider("font_weight", ctx.t("widget-editor.fontWeight", "Font Weight"), min, max).shown(f64::from(control.weight)).step(100.0).int();
                cell = cell.readout(crate::format::plain_number(f64::from(control.weight).round()));
                cell.disabled(control.axis.is_none())
            }),
            Some(ctx.slider("letter_spacing", ctx.t("widget-editor.letterSpacing", "Letter Spacing"), -35.0, 35.0).step(0.5).readout(fixed(letter_spacing, 1)).suffix("%")),
        ],
    )
    .pad_top(0.5)
}

/// The icon section with its units row, as the text display section composes it.
pub fn icon_section(ctx: &Ctx, units: Node) -> Node {
    let disabled = !ctx.flag("show_icon");
    let icon_color = ctx
        .data()
        .get("icon_color")
        .and_then(Value::as_str)
        .filter(|color| !color.is_empty())
        .unwrap_or(&ctx.theme.aqua)
        .to_string();
    stack(
        1.0,
        [
            heading(
                "palette",
                ctx.t("widget-editor.icon", "Icon"),
                Some(ctx.switch("show_icon", ctx.t("widget-editor.icon", "Icon"))),
            ),
            grid(
                2,
                [
                    Cell::new(
                        "icon_color",
                        ctx.t("widget-editor.color", "Color"),
                        CellKind::Color { value: icon_color },
                    )
                    .disabled(disabled),
                    ctx.slider("icon_size", ctx.t("widget-editor.size", "Size"), 0.0, 100.0)
                        .int()
                        .px()
                        .disabled(disabled),
                ],
            ),
            grid(
                2,
                [
                    ctx.number(
                        "icon_offset_x",
                        ctx.t("widget-editor.horizontalOffset", "Horizontal Offset"),
                        NumericFieldOptions::default(),
                    )
                    .disabled(disabled),
                    ctx.number(
                        "icon_offset_y",
                        ctx.t("widget-editor.verticalOffset", "Vertical Offset"),
                        NumericFieldOptions::default(),
                    )
                    .disabled(disabled),
                ],
            ),
            units,
        ],
    )
}

/// The units control row's options.
pub struct UnitsRow {
    pub title: String,
    pub show_toggle: bool,
    pub color: Option<String>,
    /// The colour field's label; "Color" when absent.
    pub color_label: Option<String>,
    pub select_label: String,
    /// The unit select's options; empty hides it.
    pub options: Vec<OptionView>,
}

/// The units heading with its switch, then the unit colour and unit select,
/// disabled while the switch is off.
pub fn units_row(ctx: &Ctx, row: UnitsRow) -> Node {
    let controls_disabled = row.show_toggle && !ctx.flag("show_units");
    let toggle = row
        .show_toggle
        .then(|| ctx.switch("show_units", row.title.clone()));
    stack(
        0.5,
        [
            heading("ruler", row.title, toggle),
            Node::Grid {
                columns: 2,
                gap: 1.0,
                align: super::blocks::Align::Start,
                cells: [
                    row.color.map(|value| {
                        Cell::new(
                            "unit_color",
                            row.color_label.clone().unwrap_or_else(|| ctx.t("widget-editor.color", "Color")),
                            CellKind::Color { value },
                        )
                        .disabled(controls_disabled)
                    }),
                    (!row.options.is_empty()).then(|| {
                        Cell::new(
                            "display_unit",
                            row.select_label,
                            CellKind::Select {
                                value: ctx.text("display_unit"),
                                options: row.options,
                                resettable: false,
                            },
                        )
                        .disabled(controls_disabled)
                        .rule(Rule::Named(Named::MetricUnit))
                    }),
                ]
                .into_iter()
                .flatten()
                .collect(),
            },
        ],
    )
    .pad_top(0.5)
}
