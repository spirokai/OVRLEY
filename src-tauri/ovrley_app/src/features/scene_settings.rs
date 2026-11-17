//! Scene settings (phase 5 D7, D8): `SidebarSettingsTab` with `useSceneSettingsState` and
//! `useFpsMode`. Both sections are descriptions on the D1 grammar, rendered by the inspector's
//! renderer; their edits come back here by field id.
//!
//! - **Overlay:** aspect ratio and resolution presets (`ASPECT_RATIOS`, `RESOLUTIONS`), the custom
//!   size, the frame rate (locked while a video sets it), the resolution mismatch note and the
//!   widget update rate.
//! - **Global settings:** the fonts, colours and sliders of the global defaults, with the reset
//!   button. Sliders draft and commit once; a scale draft moves every widget about the widgets'
//!   bounds, captured when the drag starts. Changing the labels' font, and the reset, wait for the
//!   family's capabilities (D6) and apply only if the document is still the same.

use crate::features::widget_editor::blocks::{
    flatten, full, grid, grid_gap, scene_heading, stack, when, Block, Cell, CellKind, Node,
    OptionView, Rule, SwitchRowStyle,
};
use crate::features::widget_editor::rules::{js_number, Input};
use crate::features::widget_editor::{font_options, request_font_capabilities};
use crate::i18n::{catalog as translations, Locale};
use crate::lib::update_rate::{
    fps_mode_value, normalize_update_rate_for_fps, sanitize_integer_fps, update_rate_options,
    PRESET_FPS_VALUES,
};
use crate::lib::video_program::collection::derive_compatibility;
use crate::store::services::Services;
use crate::store::slices::template::{
    reset_global_defaults, set_aspect_ratio_preset, set_custom_aspect_ratio, set_global_default,
};
use crate::store::Store;
use ovrley_core::normalize::template::template_manifest;
use ovrley_widgets::model::document::{js_round, number};
use serde_json::{Map, Value};

/// `ASPECT_RATIOS`: id and label key.
pub const ASPECT_RATIOS: [(&str, &str); 6] = [
    ("16:9", "scene-settings.widescreen"),
    ("9:16", "scene-settings.vertical"),
    ("1:1", "scene-settings.square"),
    ("4:3", "scene-settings.portrait"),
    ("21:9", "scene-settings.ultrawide"),
    ("custom", "scene-settings.custom"),
];

/// A resolution preset: id, label key, width, height.
pub type Resolution = (&'static str, &'static str, u32, u32);

/// `RESOLUTIONS` of an aspect ratio; none for `custom`.
pub fn resolutions(aspect_ratio: &str) -> &'static [Resolution] {
    match aspect_ratio {
        "16:9" => &[
            ("4k", "scene-settings.resolution4k", 3840, 2160),
            ("1080p", "scene-settings.resolution1080p", 1920, 1080),
            ("720p", "scene-settings.resolution720p", 1280, 720),
        ],
        "9:16" => &[
            ("4k-v", "scene-settings.resolution4kVertical", 2160, 3840),
            (
                "1080p-v",
                "scene-settings.resolution1080pVertical",
                1080,
                1920,
            ),
        ],
        "1:1" => &[
            ("1080s", "scene-settings.resolution1080pSquare", 1080, 1080),
            ("2160s", "scene-settings.resolution4kSquare", 2160, 2160),
        ],
        "4:3" => &[
            ("sxga", "scene-settings.resolutionSxga", 1400, 1050),
            ("uxga", "scene-settings.resolutionUxga", 1600, 1200),
            ("hires", "scene-settings.resolutionXga", 1920, 1440),
            ("qxga", "scene-settings.resolutionQxga", 2048, 1536),
        ],
        "21:9" => &[("ultra", "scene-settings.resolutionUltrawide", 3440, 1440)],
        _ => &[],
    }
}

const ALL_RATIOS: [&str; 5] = ["16:9", "9:16", "1:1", "4:3", "21:9"];

/// The global defaults the sliders draft.
const SLIDER_KEYS: [&str; 5] = [
    "opacity",
    "scale",
    "border_thickness",
    "shadow_strength",
    "shadow_distance",
];

/// A label-font edit waiting for the family's capabilities (D6).
#[derive(Clone, Debug, PartialEq)]
enum FontEdit {
    /// `setGlobalDefault(key, value)` for `font_text`.
    Set(String),
    /// `resetGlobalDefaults`.
    Reset,
}

/// Hears how a label-font edit ended: applied, or dropped (replaced, the document changed, or its
/// family failed to load). An agent's call answers through it.
pub type FontEditReply = Box<dyn FnOnce(&mut Store, Result<(), String>)>;

struct PendingFontEdit {
    edit: FontEdit,
    /// The labels' family the edit needs.
    family: String,
    document_revision: u64,
    reply: Option<FontEditReply>,
}

impl PendingFontEdit {
    fn answer(self, store: &mut Store, result: Result<(), String>) {
        if let Some(reply) = self.reply {
            reply(store, result);
        }
    }
}

impl std::fmt::Debug for PendingFontEdit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("PendingFontEdit").field("edit", &self.edit).field("family", &self.family).field("document_revision", &self.document_revision).finish()
    }
}

/// `useSceneSettingsState`'s and `useFpsMode`'s hook state.
#[derive(Debug, Default)]
pub struct SceneSettingsState {
    /// The scene size the user picked Custom resolution at (`customResolutionAnchor`).
    custom_resolution_anchor: Option<(u64, u64)>,
    /// The frame rate the user picked Custom fps at (`customFpsAnchor`).
    custom_fps_anchor: Option<u32>,
    /// The newest label-font edit, until its family's capabilities arrive.
    pending_font: Option<PendingFontEdit>,
}

fn t(locale: Locale, key: &str, default: &str) -> String {
    translations().t_or(locale, key, default).to_string()
}

fn scene_size(store: &Store) -> (u64, u64) {
    let size = |key: &str| {
        store
            .editor
            .config
            .scene
            .get(key)
            .and_then(Value::as_f64)
            .map_or(0, |value| value as u64)
    };
    (size("width"), size("height"))
}

/// The global defaults with their drafts.
fn globals(store: &Store) -> Map<String, Value> {
    store
        .drafts
        .apply_global_defaults(&store.template.global_defaults)
}

/// The videos' reference format (`deriveCompatibility(importedVideos).reference`): its frame rate
/// and resolution.
fn video_format(store: &Store) -> Option<(f64, (u32, u32))> {
    let clips = store.video_import.clips();
    let refs: Vec<_> = clips.iter().collect();
    derive_compatibility(&refs)
        .reference
        .map(|(resolution, fps)| (fps.as_f64(), (resolution.width, resolution.height)))
}

/// `getResolutionPresetId`, or Custom while the anchor holds the scene size.
fn resolution_id(store: &Store) -> String {
    let size = scene_size(store);
    if store.scene_settings.custom_resolution_anchor == Some(size) {
        return "custom".into();
    }
    ALL_RATIOS
        .iter()
        .flat_map(|ratio| resolutions(ratio))
        .find(|(_, _, width, height)| (u64::from(*width), u64::from(*height)) == size)
        .map_or_else(|| "custom".to_string(), |(id, ..)| id.to_string())
}

/// `useFpsMode`'s mode.
fn fps_mode(store: &Store) -> String {
    let fps = store.render_settings.fps;
    match store.scene_settings.custom_fps_anchor {
        Some(anchor) if f64::from(anchor) == fps => "custom".into(),
        _ => fps_mode_value(fps),
    }
}

fn select(id: &str, label: String, value: String, options: Vec<OptionView>) -> Cell {
    Cell::new(
        id,
        label,
        CellKind::Select {
            value,
            options,
            resettable: false,
        },
    )
}

fn number_cell(id: &str, label: String, value: f64) -> Cell {
    Cell::new(
        id,
        label,
        CellKind::Number {
            value: crate::features::widget_editor::sections::whole_number_text(Some(&number(
                value,
            ))),
            placeholder: String::new(),
            suffix: String::new(),
            resettable: false,
        },
    )
}

fn slider(
    id: &str,
    label: String,
    value: f64,
    (min, max, step): (f64, f64, f64),
    readout: String,
    suffix: &str,
) -> Cell {
    Cell::new(
        id,
        label,
        CellKind::Slider {
            editable: true,
            value,
            min,
            max,
            step,
            scale: 1.0,
            readout,
            suffix: suffix.into(),
        },
    )
}

/// `OverlaySettingsSection`.
fn overlay_section(store: &Store, locale: Locale) -> Node {
    let aspect_ratio = store.template.aspect_ratio.clone();
    let res_id = resolution_id(store);
    let (width, height) = scene_size(store);
    let fps = store.render_settings.fps;
    let mode = fps_mode(store);
    let video = video_format(store);
    let custom = t(locale, "scene-settings.custom", "Custom");
    let mut resolution_options: Vec<OptionView> = resolutions(&aspect_ratio)
        .iter()
        .map(|(id, key, ..)| OptionView::new(*id, t(locale, key, id)))
        .collect();
    resolution_options.push(OptionView::new("custom", custom.clone()));
    let framerate_label = t(locale, "scene-settings.framerate", "Framerate");
    let framerate = match video {
        Some((video_fps, _)) => Cell::new(
            "framerate",
            framerate_label,
            CellKind::Locked {
                text: translations().t_with(
                    locale,
                    "scene-settings.lockedToFps",
                    &[("fps", &js_round(video_fps).to_string())],
                ),
            },
        )
        .rule(Rule::None),
        None => select(
            "framerate",
            framerate_label,
            mode.clone(),
            vec![
                OptionView::new("24", "24 fps"),
                OptionView::new("30", "30 fps"),
                OptionView::new("60", "60 fps"),
                OptionView::new("custom", custom.clone()),
            ],
        ),
    };
    let mismatch = video.is_some_and(|(_, (video_width, video_height))| {
        width != 0
            && height != 0
            && (width, height) != (u64::from(video_width), u64::from(video_height))
    });
    let rates = update_rate_options(fps);
    stack(
        1.0,
        [
            scene_heading(
                "video",
                t(locale, "scene-settings.overlay", "Overlay"),
                None,
            ),
            grid(
                2,
                [
                    select(
                        "aspect_ratio",
                        t(locale, "scene-settings.aspectRatio", "Aspect Ratio"),
                        aspect_ratio.clone(),
                        ASPECT_RATIOS
                            .iter()
                            .map(|(id, key)| OptionView::new(*id, t(locale, key, id)))
                            .collect(),
                    ),
                    select(
                        "resolution",
                        t(locale, "scene-settings.resolution", "Resolution"),
                        res_id.clone(),
                        resolution_options,
                    )
                    .disabled(aspect_ratio == "custom"),
                ],
            )
            .pt(1.5),
            when(aspect_ratio == "custom" || res_id == "custom", || {
                grid(
                    2,
                    [
                        number_cell(
                            "width",
                            t(locale, "scene-settings.width", "Width"),
                            width as f64,
                        ),
                        number_cell(
                            "height",
                            t(locale, "scene-settings.height", "Height"),
                            height as f64,
                        ),
                    ],
                )
            }),
            grid(
                2,
                [
                    Some(framerate),
                    (video.is_none() && mode == "custom").then(|| {
                        number_cell(
                            "custom_fps",
                            t(locale, "scene-settings.customFps", "Custom FPS"),
                            fps,
                        )
                    }),
                ],
            ),
            when(mismatch, || {
                full(
                    Cell::new(
                        "mismatch",
                        "",
                        CellKind::Note {
                            text: t(
                                locale,
                                "scene-settings.resolutionMismatch",
                                "Overlay and video resolutions do not match",
                            ),
                        },
                    )
                    .rule(Rule::None),
                )
            }),
            Node::Panel {
                space: 0.75,
                children: vec![full(Cell::new(
                    "update_rate",
                    t(
                        locale,
                        "scene-settings.widgetUpdateRate",
                        "Widget Update Rate",
                    ),
                    CellKind::Tabs {
                        value: store.render_settings.widget_update_rate.to_string(),
                        options: rates
                            .iter()
                            .map(|rate| OptionView::new(rate.to_string(), format!("1/{rate}")))
                            .collect(),
                    },
                ))],
            },
        ],
    )
}

/// `GlobalSettingsSection`.
fn global_section(store: &Store, locale: Locale) -> Node {
    let globals = globals(store);
    let text = |key: &str, fallback: &str| {
        globals
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or(fallback)
            .to_string()
    };
    let num = |key: &str| globals.get(key).and_then(Value::as_f64).unwrap_or_default();
    let font = |key: &str, label: String| {
        let value = text(key, "");
        let (recommended, _) = font_options(store, &value);
        Cell::new(
            key,
            label,
            CellKind::Font {
                value,
                recommended,
                small_label: false,
            },
        )
    };
    let color = |key: &str, label: String, fallback: &str| {
        Cell::new(
            key,
            label,
            CellKind::Color {
                value: text(key, fallback),
            },
        )
    };
    let whole = |key: &str| crate::js::number_to_string(num(key));
    let opacity = num("opacity");
    let scale = num("scale");
    let reset = Cell::new(
        "reset_globals",
        String::new(),
        CellKind::IconButton {
            icon: "rotate-ccw",
            outline: false,
        },
    );
    stack(
        1.0,
        [
            scene_heading(
                "palette",
                t(locale, "scene-settings.globalSettings", "Global Settings"),
                Some(reset),
            ),
            grid(
                2,
                [
                    font(
                        "font_values",
                        t(locale, "scene-settings.fontValues", "Font - Values"),
                    ),
                    font(
                        "font_text",
                        t(locale, "scene-settings.fontLabels", "Font - Labels"),
                    ),
                ],
            )
            .pt(1.5),
            full(Cell::new(
                "fixed_width_digits",
                t(
                    locale,
                    "scene-settings.fixedWidthDigits",
                    "Fixed-Width Digits",
                ),
                CellKind::SwitchRow {
                    checked: globals
                        .get("fixed_width_digits")
                        .and_then(Value::as_bool)
                        .unwrap_or_default(),
                    style: SwitchRowStyle::Label {
                        padding: [0.0; 4],
                        label_top: 0.0,
                    },
                },
            )),
            grid_gap(
                3,
                0.75,
                [
                    color(
                        "color_values",
                        t(locale, "scene-settings.values", "Values"),
                        "",
                    ),
                    color(
                        "color_text",
                        t(locale, "scene-settings.labels", "Labels"),
                        "",
                    ),
                    color(
                        "color_icons",
                        t(locale, "scene-settings.icons", "Icons"),
                        "",
                    ),
                ],
            )
            .pt(1.0),
            grid_gap(
                3,
                0.75,
                [
                    color(
                        "border_color",
                        t(locale, "scene-settings.borders", "Borders"),
                        "#000000",
                    ),
                    color(
                        "shadow_color",
                        t(locale, "scene-settings.shadows", "Shadows"),
                        "#000000",
                    ),
                    color(
                        "color_units",
                        t(locale, "scene-settings.units", "Units"),
                        "",
                    ),
                ],
            ),
            stack(
                1.5,
                [
                    full({
                        let mut cell = slider(
                            "opacity",
                            t(locale, "scene-settings.transparency", "Transparency"),
                            opacity,
                            (0.0, 1.0, 0.01),
                            js_round(opacity * 100.0).to_string(),
                            "%",
                        );
                        if let CellKind::Slider { scale, .. } = &mut cell.kind {
                            *scale = 100.0;
                        }
                        cell
                    }),
                    full(slider(
                        "scale",
                        t(locale, "scene-settings.scale", "Scale"),
                        scale,
                        (0.5, 2.0, 0.01),
                        format!("{scale:.2}"),
                        "x",
                    )),
                    full(slider(
                        "border_thickness",
                        t(locale, "scene-settings.borderThickness", "Border Thickness"),
                        num("border_thickness"),
                        (0.0, 20.0, 1.0),
                        whole("border_thickness"),
                        "px",
                    )),
                    grid_gap(
                        2,
                        1.5,
                        [
                            slider(
                                "shadow_strength",
                                t(locale, "scene-settings.shadowStrength", "Shadow Strength"),
                                num("shadow_strength"),
                                (0.0, 20.0, 1.0),
                                whole("shadow_strength"),
                                "",
                            ),
                            slider(
                                "shadow_distance",
                                t(locale, "scene-settings.shadowDistance", "Shadow Distance"),
                                num("shadow_distance"),
                                (0.0, 20.0, 1.0),
                                whole("shadow_distance"),
                                "",
                            ),
                        ],
                    ),
                ],
            )
            .pt(1.5),
        ],
    )
}

/// `SidebarSettingsTab`: the two sections, 2rem apart (`space-y-8`).
pub fn describe(store: &Store) -> Vec<Block> {
    let locale = store.i18n.language;
    flatten(
        2.0,
        [
            overlay_section(store, locale),
            global_section(store, locale),
        ],
    )
}

/// The global defaults' keys, as the template manifest lists them.
pub fn global_default_keys() -> Vec<&'static str> {
    template_manifest().globals.keys().map(String::as_str).collect()
}

/// `sanitizeNumber`: commas dropped, then `parseInt(…, 10) || 0` (stripping leading zeros first
/// changes nothing `parseInt` reads).
pub fn sanitize_number(raw: &str) -> i64 {
    let text = raw.replace(',', "");
    let text = text.trim_start();
    let (sign, rest) = match text.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, text.strip_prefix('+').unwrap_or(text)),
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse::<i64>().map_or(0, |value| sign * value)
}

/// The scale's fixed point: the top-left of the widgets' presented bounds (`getScaleOrigin`). An
/// empty document has nothing to move.
pub fn scale_origin(store: &Store) -> (f64, f64) {
    use crate::features::overlay_editor::geometry::{group_bounds, ContentBox};
    let boxes: Vec<ContentBox> = store
        .preview
        .presented()
        .values()
        .map(|widget| ContentBox::of(&widget.geometry))
        .collect();
    if boxes.is_empty() {
        return (0.0, 0.0);
    }
    let bounds = group_bounds(&boxes);
    (bounds.left, bounds.top)
}

/// Labels resolve only when the labels' font changes, which waits for its family (D6).
fn no_label_change(_: bool, _: f64) -> Result<(bool, f64), String> {
    Err("only a labels' font change resolves the labels' styles".into())
}

/// The frame rate a video locks (`importedVideoFps`), none without videos.
pub fn locked_fps(store: &Store) -> Option<f64> {
    video_format(store).map(|(fps, _)| fps)
}

/// `onFpsChange`: a whole frame rate, the update rate normalized for it, as one undo step.
pub fn set_fps(store: &mut Store, fps: u32) -> Result<(), String> {
    let rate = normalize_update_rate_for_fps(
        f64::from(fps),
        f64::from(store.render_settings.widget_update_rate),
    );
    store
        .render_settings
        .set_fps_and_update_rate(f64::from(fps), rate)
}

/// `changeGlobalDefault`: the labels' font waits for its family's capabilities, answering `reply`
/// once applied or dropped; the rest apply now. A scale set outright (an agent's) scales about the
/// widgets' bounds, as the slider does.
pub fn change_global_default(store: &mut Store, services: &mut Services, key: &str, value: Value, reply: Option<FontEditReply>) -> Result<(), String> {
    if key == "font_text" {
        let family = value.as_str().ok_or("font_text takes a font id")?.to_string();
        queue_font_edit(store, services, FontEdit::Set(family.clone()), family, reply);
        return Ok(());
    }
    let origin = (key == "scale").then(|| scale_origin(store));
    let result = set_global_default(&mut store.editor, &mut store.template, key, value, &no_label_change, origin);
    if let Some(reply) = reply {
        reply(store, result.clone());
    }
    result
}

/// `resetGlobalDefaults`, once the default labels' font is known, about the widgets' bounds.
pub fn reset_globals(store: &mut Store, services: &mut Services, reply: Option<FontEditReply>) {
    let family = template_manifest().globals.get("font_text").and_then(Value::as_str).expect("the manifest has a labels' font").to_string();
    queue_font_edit(store, services, FontEdit::Reset, family, reply);
}

/// Waits for the family; a newer edit replaces an older one, which is dropped.
fn queue_font_edit(store: &mut Store, services: &mut Services, edit: FontEdit, family: String, reply: Option<FontEditReply>) {
    request_font_capabilities(store, services, &family);
    if let Some(replaced) = store.scene_settings.pending_font.take() {
        replaced.answer(store, Err("A newer font change replaced this one".into()));
    }
    store.scene_settings.pending_font = Some(PendingFontEdit { edit, family, document_revision: store.editor.editor_document_revision, reply });
    apply_pending_font_edit(store);
}

/// Reconcile step: applies the pending label-font edit once its family's capabilities are in, if
/// the document is still the one it was made on. A failed load (reported by the request) drops it.
pub fn apply_pending_font_edit(store: &mut Store) {
    let Some(family_id) = store.scene_settings.pending_font.as_ref().map(|pending| pending.family.clone()) else {
        return;
    };
    let fonts = &store.widget_editor.fonts;
    if !fonts.capabilities_requested(&family_id) {
        let pending = store.scene_settings.pending_font.take().expect("checked above");
        pending.answer(store, Err(format!("The font {family_id} could not be read")));
        return;
    }
    let Some(family) = fonts.capabilities(&family_id).cloned() else {
        return;
    };
    let pending = store.scene_settings.pending_font.take().expect("checked above");
    if pending.document_revision != store.editor.editor_document_revision {
        pending.answer(store, Err("The template changed before the font was ready".into()));
        return;
    }
    let label_style = |italic: bool, weight: f64| {
        ovrley_core::fonts::label_font_style(&family, weight as f32, italic)
            .map(|(italic, weight)| (italic, f64::from(weight)))
            .map_err(|error| error.to_string())
    };
    let result = match &pending.edit {
        FontEdit::Set(value) => set_global_default(
            &mut store.editor,
            &mut store.template,
            "font_text",
            Value::String(value.clone()),
            &label_style,
            None,
        ),
        FontEdit::Reset => {
            let origin = scale_origin(store);
            reset_global_defaults(
                &mut store.editor,
                &mut store.template,
                &label_style,
                Some(origin),
            )
        }
    };
    if pending.reply.is_some() {
        pending.answer(store, result);
    } else if let Err(error) = result {
        store.media.set_error_message(error);
    }
}

/// `draftGlobalDefault`: a slider's live value. The first scale draft captures the widgets' bounds,
/// and each moves every widget about them, so the canvas follows the drag.
pub fn draft_global_default(store: &mut Store, key: &str, value: f64) -> Result<(), String> {
    if !SLIDER_KEYS.contains(&key) {
        return Err(format!("{key} is not a global default slider"));
    }
    if key == "scale" {
        if store.drafts.scale_origin().is_none() {
            let origin = scale_origin(store);
            store.drafts.set_scale_origin(Some(origin));
        }
        let origin = store.drafts.scale_origin().expect("captured above");
        let committed = store
            .template
            .global_defaults
            .get("scale")
            .and_then(Value::as_f64)
            .expect("global defaults carry a scale");
        let mut scaled = (*store.editor.config).clone();
        scaled.scale_widget_positions(value / committed, origin);
        let drafts: Vec<(String, Map<String, Value>)> =
            crate::lib::widget::build_config_widgets(&scaled)
                .into_iter()
                .map(|widget| {
                    let mut patch = Map::new();
                    for field in ["x", "y"] {
                        patch.insert(
                            field.into(),
                            widget.data.get(field).cloned().unwrap_or(Value::Null),
                        );
                    }
                    (widget.id, patch)
                })
                .collect();
        store.drafts.set_widget_drafts(&drafts);
    }
    store.drafts.set_global_default_draft(key, number(value));
    Ok(())
}

/// `commitGlobalDefault`: the drafted values become one undo step; a scale commits about the origin
/// its drag captured.
pub fn commit_global_defaults(store: &mut Store) -> Result<(), String> {
    let drafts = store.drafts.global_default_drafts().clone();
    let origin = store.drafts.scale_origin();
    let ids = store.editor.config.widget_ids();
    store.drafts.clear_global_default_drafts();
    store.drafts.set_scale_origin(None);
    if drafts.contains_key("scale") {
        store.drafts.clear_widget_drafts(&ids);
    }
    for (key, value) in drafts {
        let origin = (key == "scale").then_some(origin).flatten();
        set_global_default(
            &mut store.editor,
            &mut store.template,
            &key,
            value,
            &no_label_change,
            origin,
        )?;
    }
    Ok(())
}

/// An edit of a scene settings field, by its id (D1). A slider's live value is a draft.
pub fn edit(
    store: &mut Store,
    services: &mut Services,
    id: &str,
    input: Input,
    draft: bool,
) -> Result<(), String> {
    let text = |input: &Input| match input {
        Input::Text(text) => Ok(text.clone()),
        other => Err(format!("{id} takes text, not {other:?}")),
    };
    if draft {
        return match input {
            Input::Number(value) => draft_global_default(store, id, value),
            other => Err(format!("{id} drafts a number, not {other:?}")),
        };
    }
    match id {
        "aspect_ratio" => {
            let ratio = text(&input)?;
            if ratio == "custom" {
                set_custom_aspect_ratio(&mut store.template);
                return Ok(());
            }
            let &(_, _, width, height) = resolutions(&ratio)
                .first()
                .ok_or_else(|| format!("Unknown aspect ratio \"{ratio}\""))?;
            set_aspect_ratio_preset(
                &mut store.editor,
                &mut store.template,
                &ratio,
                width,
                height,
            );
            Ok(())
        }
        "resolution" => {
            let resolution = text(&input)?;
            if resolution == "custom" {
                store.scene_settings.custom_resolution_anchor = Some(scene_size(store));
                return Ok(());
            }
            store.scene_settings.custom_resolution_anchor = None;
            let ratio = store.template.aspect_ratio.clone();
            let &(_, _, width, height) = resolutions(&ratio)
                .iter()
                .find(|(preset, ..)| *preset == resolution)
                .ok_or_else(|| {
                    format!(
                        "Unknown resolution preset \"{resolution}\" for aspect ratio \"{ratio}\""
                    )
                })?;
            set_scene_size(store, u64::from(width), u64::from(height));
            Ok(())
        }
        "width" | "height" => {
            let value = sanitize_number(&text(&input)?);
            store.editor.edit_config(|config| {
                config.scene.insert(id.into(), Value::from(value));
            });
            Ok(())
        }
        "framerate" => {
            let mode = text(&input)?;
            if mode == "custom" {
                store.scene_settings.custom_fps_anchor =
                    Some(sanitize_integer_fps(store.render_settings.fps));
                return Ok(());
            }
            store.scene_settings.custom_fps_anchor = None;
            set_fps(store, sanitize_integer_fps(js_number(&mode)))
        }
        "custom_fps" => {
            let fps = sanitize_integer_fps(js_number(&text(&input)?));
            store.scene_settings.custom_fps_anchor =
                (!PRESET_FPS_VALUES.contains(&fps)).then_some(fps);
            set_fps(store, fps)
        }
        "update_rate" => {
            let rate: u32 = text(&input)?
                .parse()
                .map_err(|_| format!("{id} takes a whole rate"))?;
            store.render_settings.set_widget_update_rate(rate)
        }
        "reset_globals" => {
            reset_globals(store, services, None);
            Ok(())
        }
        "fixed_width_digits" => match input {
            Input::Flag(checked) => {
                change_global_default(store, services, id, Value::Bool(checked), None)
            }
            other => Err(format!("{id} takes a flag, not {other:?}")),
        },
        "font_values" | "font_text" | "color_values" | "color_text" | "color_icons"
        | "border_color" | "shadow_color" | "color_units" => {
            change_global_default(store, services, id, Value::String(text(&input)?), None)
        }
        key if SLIDER_KEYS.contains(&key) => match input {
            // A typed value commits at once, as the slider's release does.
            Input::Number(value) => {
                draft_global_default(store, key, value)?;
                commit_global_defaults(store)
            }
            other => Err(format!("{id} takes a number, not {other:?}")),
        },
        other => Err(format!("Scene settings have no field {other}")),
    }
}

fn set_scene_size(store: &mut Store, width: u64, height: u64) {
    store.editor.edit_config(|config| {
        config.scene.insert("width".into(), Value::from(width));
        config.scene.insert("height".into(), Value::from(height));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::widget_editor::tests::{editor_with, value};
    use crate::store::services::fakes::Fakes;
    use crate::store::Dispatcher;
    use serde_json::json;
    use std::time::Duration;

    fn settings(collections: Value) -> (Dispatcher, Fakes) {
        editor_with(collections)
    }

    fn run(dispatcher: &Dispatcher, id: &'static str, input: Input) {
        dispatcher.dispatch(move |store, services| edit(store, services, id, input, false).unwrap());
    }

    fn cell(dispatcher: &Dispatcher, id: &str) -> Option<Cell> {
        dispatcher.with_store(|store| crate::features::widget_editor::blocks::find(&describe(store), id).cloned())
    }

    fn selected(dispatcher: &Dispatcher, id: &str) -> String {
        match cell(dispatcher, id).unwrap().kind {
            CellKind::Select { value, .. } => value,
            other => panic!("{id} is not a select: {other:?}"),
        }
    }

    /// Runs worker results until `done`.
    fn settle(dispatcher: &Dispatcher, fakes: &Fakes, done: impl Fn(&Store) -> bool) {
        for _ in 0..400 {
            fakes.ui_post.drain(dispatcher);
            if dispatcher.with_store(&done) {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("timed out");
    }

    #[test]
    fn custom_modes_hold_across_unrelated_edits_and_leave_when_the_value_changes() {
        let (dispatcher, _) = settings(json!({}));
        dispatcher.dispatch(|store, _| store.editor.edit_config(|config| {
            config.scene.insert("width".into(), json!(1920));
            config.scene.insert("height".into(), json!(1080));
        }));
        assert_eq!(selected(&dispatcher, "resolution"), "1080p");
        assert!(cell(&dispatcher, "width").is_none(), "a preset hides the size fields");
        run(&dispatcher, "resolution", Input::Text("custom".into()));
        run(&dispatcher, "framerate", Input::Text("custom".into()));
        assert_eq!((selected(&dispatcher, "resolution"), selected(&dispatcher, "framerate")), ("custom".into(), "custom".into()));
        assert!(cell(&dispatcher, "width").is_some() && cell(&dispatcher, "custom_fps").is_some());
        dispatcher.dispatch(|store, _| store.editor.edit_config(|config| {
            config.scene.insert("start".into(), json!(12));
        }));
        assert_eq!((selected(&dispatcher, "resolution"), selected(&dispatcher, "framerate")), ("custom".into(), "custom".into()), "unrelated edits keep the modes");

        run(&dispatcher, "width", Input::Text("1,280".into()));
        run(&dispatcher, "height", Input::Text("0720".into()));
        dispatcher.dispatch(|store, _| set_fps(store, 60).unwrap());
        assert_eq!((selected(&dispatcher, "resolution"), selected(&dispatcher, "framerate")), ("720p".into(), "60".into()), "a real change returns to the presets");
    }

    #[test]
    fn presets_follow_the_aspect_ratio_and_a_custom_ratio_keeps_the_size() {
        let (dispatcher, _) = settings(json!({}));
        run(&dispatcher, "aspect_ratio", Input::Text("9:16".into()));
        dispatcher.with_store(|store| assert_eq!((store.template.aspect_ratio.as_str(), scene_size(store)), ("9:16", (2160, 3840))));
        run(&dispatcher, "resolution", Input::Text("1080p-v".into()));
        dispatcher.with_store(|store| assert_eq!(scene_size(store), (1080, 1920)));
        run(&dispatcher, "aspect_ratio", Input::Text("custom".into()));
        dispatcher.with_store(|store| assert_eq!((store.template.aspect_ratio.as_str(), scene_size(store)), ("custom", (1080, 1920))));
        assert!(cell(&dispatcher, "resolution").unwrap().disabled, "a custom ratio has no presets");
        assert!(cell(&dispatcher, "width").is_some());
    }

    #[test]
    fn the_frame_rate_normalizes_the_update_rate_and_custom_fps_anchors() {
        let (dispatcher, _) = settings(json!({}));
        run(&dispatcher, "framerate", Input::Text("60".into()));
        run(&dispatcher, "update_rate", Input::Text("6".into()));
        dispatcher.with_store(|store| assert_eq!((store.render_settings.fps, store.render_settings.widget_update_rate), (60.0, 6)));
        run(&dispatcher, "framerate", Input::Text("24".into()));
        dispatcher.with_store(|store| assert_eq!((store.render_settings.fps, store.render_settings.widget_update_rate), (24.0, 1), "24 fps offers no 1/6"));
        run(&dispatcher, "custom_fps", Input::Text("50".into()));
        assert_eq!(selected(&dispatcher, "framerate"), "custom");
        run(&dispatcher, "custom_fps", Input::Text("30".into()));
        assert_eq!(selected(&dispatcher, "framerate"), "30", "a preset typed in leaves custom");
        let CellKind::Tabs { options, .. } = cell(&dispatcher, "update_rate").unwrap().kind else { panic!() };
        assert_eq!(options.iter().map(|option| option.label.as_str()).collect::<Vec<_>>(), ["1/1", "1/2", "1/3", "1/5"]);
    }

    #[test]
    fn a_scale_drag_moves_the_widgets_live_and_commits_once_about_their_bounds() {
        let (dispatcher, _) = settings(json!({"values": [value("speed", "a", 100.0), value("speed", "b", 300.0)]}));
        let before = dispatcher.with_store(|store| store.history.past.len());
        let origin = dispatcher.with_store(scale_origin);
        dispatcher.dispatch(|store, services| edit(store, services, "scale", Input::Number(1.5), true).unwrap());
        dispatcher.dispatch(|store, services| edit(store, services, "scale", Input::Number(2.0), true).unwrap());
        dispatcher.with_store(|store| {
            assert_eq!(store.drafts.scale_origin(), Some(origin), "the first draft captures the bounds");
            let x = store.drafts.widget_draft("b").unwrap()["x"].as_f64().unwrap();
            assert_eq!(x, origin.0 + (300.0 - origin.0) * 2.0, "the drafts scale about the bounds");
            assert_eq!(store.editor.config.widget("b").unwrap()["x"].as_f64(), Some(300.0), "nothing commits while dragging");
        });
        dispatcher.dispatch(|store, _| commit_global_defaults(store).unwrap());
        dispatcher.with_store(|store| {
            assert_eq!(store.template.global_defaults["scale"], json!(2));
            assert_eq!(store.editor.config.widget("b").unwrap()["x"].as_f64().unwrap(), origin.0 + (300.0 - origin.0) * 2.0);
            assert!(store.drafts.widget_draft("b").is_none() && store.drafts.scale_origin().is_none());
            assert_eq!(store.history.past.len(), before + 1, "one undo step");
        });
    }

    #[test]
    fn the_labels_font_waits_for_its_family_and_the_latest_edit_wins() {
        let (dispatcher, fakes) = settings(json!({"labels": [{"id": "label", "text": "A", "x": 1, "y": 2, "font": "Oxanium.ttf", "font_weight": 900, "italic": true, "letter_spacing": -1.25}]}));
        run(&dispatcher, "font_text", Input::Text("Arial.ttf".into()));
        run(&dispatcher, "font_text", Input::Text("Rajdhani.ttf".into()));
        assert_eq!(dispatcher.with_store(|store| store.editor.config.widget("label").unwrap()["font"].clone()), json!("Oxanium.ttf"), "nothing applies before the family is read");
        settle(&dispatcher, &fakes, |store| store.scene_settings.pending_font.is_none());
        dispatcher.with_store(|store| {
            let label = store.editor.config.widget("label").unwrap();
            assert_eq!((label["font"].clone(), label["italic"].clone(), label["letter_spacing"].clone()), (json!("Rajdhani.ttf"), json!(false), json!(-1.25)));
            assert_eq!(store.template.global_defaults["font_text"], json!("Rajdhani.ttf"));
            assert_eq!(store.media.error_message, None);
        });
        settle(&dispatcher, &fakes, |_| true);
        assert_eq!(dispatcher.with_store(|store| store.template.global_defaults["font_text"].clone()), json!("Rajdhani.ttf"), "the replaced edit never applies");
    }

    #[test]
    fn colours_apply_at_once_and_the_reset_restores_the_manifest() {
        let (dispatcher, fakes) = settings(json!({"values": [value("speed", "a", 100.0)]}));
        run(&dispatcher, "color_values", Input::Text("#ffaa00".into()));
        dispatcher.with_store(|store| {
            assert_eq!(store.template.global_defaults["color_values"].as_str().map(str::to_ascii_lowercase).as_deref(), Some("#ffaa00"));
            assert!(matches!(crate::features::widget_editor::blocks::find(&describe(store), "color_values").unwrap().kind, CellKind::Color { ref value } if value.eq_ignore_ascii_case("#ffaa00")));
        });
        run(&dispatcher, "reset_globals", Input::Press);
        settle(&dispatcher, &fakes, |store| store.scene_settings.pending_font.is_none());
        dispatcher.with_store(|store| assert_eq!(*store.template.global_defaults, template_manifest().globals));
    }

    #[test]
    fn scene_numbers_sanitize_as_react_does() {
        for (raw, expected) in [("1,920", 1920), ("007", 7), ("", 0), ("abc", 0), ("12px", 12), ("-5", -5), ("0", 0)] {
            assert_eq!(sanitize_number(raw), expected, "{raw}");
        }
    }
}
