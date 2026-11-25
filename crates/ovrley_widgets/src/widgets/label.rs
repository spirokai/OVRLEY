//! Static text labels with their own typography.
use crate::model::scene::{
    validate_scene_presentation, Numerals, ScenePresentationConfig, ValidatedScenePresentation,
};
use crate::model::validation::rgba_from_hex;
use crate::preview::ResizeGesture;
use crate::preview::WidgetHost;
use crate::render::text::LINE_HEIGHT_RATIO;
use crate::render::text::{
    baseline_for_top_with_line_height, draw_text_blob, layout_text, parse_color,
    resolve_styled_font, text_selection_bounds, FontResolver, ResolvedTextStyle,
};
use crate::render::tile::PreviewRasterScale;
use crate::render::tile::{Presented, Tile};
use crate::render::tile::{TileBounds, SHAPE_COLOR_TYPE};
use crate::{WidgetError, WidgetResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use skia_safe::{Canvas, Color, Rect, TextBlob};
use std::collections::BTreeMap;

/// One typography contract for render requests and saved-document ingress.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct LabelTypography {
    #[serde(deserialize_with = "deserialize_font_weight")]
    pub font_weight: f32,
    #[serde(deserialize_with = "deserialize_italic")]
    pub italic: bool,
    /// Percentage of font size between grapheme clusters; no trailing gap.
    #[serde(deserialize_with = "deserialize_letter_spacing")]
    pub letter_spacing: f32,
}

pub(crate) fn deserialize_font_weight<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<f32, D::Error> {
    let weight = f64::deserialize(deserializer)
        .map_err(|error| serde::de::Error::custom(format!("font_weight: {error}")))?;
    // Validate the original number before narrowing to Skia's f32 coordinates.
    if !weight.is_finite() || !(1.0..=1000.0).contains(&weight) {
        return Err(serde::de::Error::custom(
            "font_weight: must be a finite number from 1 to 1000",
        ));
    }
    Ok(weight as f32)
}

fn deserialize_italic<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    bool::deserialize(deserializer)
        .map_err(|error| serde::de::Error::custom(format!("italic: {error}")))
}

fn deserialize_letter_spacing<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<f32, D::Error> {
    let spacing = f64::deserialize(deserializer)
        .map_err(|error| serde::de::Error::custom(format!("letter_spacing: {error}")))?;
    if !spacing.is_finite() || !(spacing as f32).is_finite() {
        return Err(serde::de::Error::custom(
            "letter_spacing: must be a finite 32-bit number",
        ));
    }
    Ok(spacing as f32)
}

/// Static text label drawn onto the cached base layer.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct LabelConfig {
    #[serde(default)]
    pub text: String,
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub font: Option<String>,
    #[serde(default)]
    pub font_family: Option<String>,
    #[serde(default)]
    pub font_size: Option<f32>,
    #[serde(flatten)]
    pub typography: LabelTypography,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub opacity: Option<f32>,
    #[serde(default)]
    pub shadow_color: Option<String>,
    #[serde(default)]
    pub shadow_strength: Option<f32>,
    #[serde(default)]
    pub shadow_distance: Option<f32>,
    #[serde(default)]
    pub border_color: Option<String>,
    #[serde(default)]
    pub border_thickness: Option<f32>,
    #[serde(default)]
    pub border_strength: Option<f32>,
    #[serde(default)]
    pub border_distance: Option<f32>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Every output-affecting field for a static text label is explicit.
#[derive(Clone, Debug)]
pub struct ValidatedLabel {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub font_name: String,
    pub font_size: f32,
    pub typography: LabelTypography,
    pub color: [u8; 4],
    pub opacity: f32,
}

/// Validates a label config and returns a `ValidatedLabel` with all
/// output-affecting fields explicit. Missing or invalid fields cause an
/// immediate error.
pub fn validate_label(label: &LabelConfig, index: usize) -> WidgetResult<ValidatedLabel> {
    let p = |f: &str| format!("labels[{index}].{f}");

    // Labels may be empty.
    let text = label.text.clone();

    if !label.x.is_finite() || !label.y.is_finite() {
        return Err(WidgetError::Config(
            "label coordinates must be finite".into(),
        ));
    }

    // The editor resolves the font from the label's font, its family or the scene font.
    let font_name = label
        .font
        .clone()
        .or_else(|| label.font_family.clone())
        .ok_or_else(|| WidgetError::Config(format!("{}: required field missing", p("font"))))?;

    if font_name.is_empty() {
        return Err(WidgetError::Config(format!(
            "{}: font name must not be empty",
            p("font")
        )));
    }

    // font_size: required
    let font_size = label.font_size.ok_or_else(|| {
        WidgetError::Config(format!("{}: required field missing", p("font_size")))
    })?;

    if !font_size.is_finite() || font_size <= 0.0 {
        return Err(WidgetError::Config(format!(
            "{}: must be positive, got {font_size}",
            p("font_size")
        )));
    }

    // color: required
    let color_hex = label
        .color
        .as_deref()
        .ok_or_else(|| WidgetError::Config(format!("{}: required field missing", p("color"))))?;

    let opacity = label
        .opacity
        .ok_or_else(|| WidgetError::Config(format!("{}: required field missing", p("opacity"))))?;

    if !(0.0..=1.0).contains(&opacity) {
        return Err(WidgetError::Config(format!(
            "{}: must be 0.0..=1.0, got {opacity}",
            p("opacity")
        )));
    }

    let color = rgba_from_hex(color_hex, &p("color"), opacity)?;

    Ok(ValidatedLabel {
        text,
        x: label.x,
        y: label.y,
        font_name,
        font_size,
        typography: label.typography.clone(),
        color,
        opacity,
    })
}

/// Resolves a text style from a validated label and scene config.
///
/// All output-affecting fields are already explicit in the validated type.
/// Shadow and border come from scene config (not part of the label contract).
pub fn validated_label_style(
    validated: &ValidatedLabel,
    scene: &ValidatedScenePresentation,
    scale: f32,
) -> ResolvedTextStyle {
    let opacity = validated.opacity;
    let color = Color::from_argb(
        validated.color[3],
        validated.color[0],
        validated.color[1],
        validated.color[2],
    );

    ResolvedTextStyle {
        x: validated.x,
        y: validated.y,
        font_name: Some(validated.font_name.clone()),
        font_size: validated.font_size * scale,
        font_weight: validated.typography.font_weight,
        italic: validated.typography.italic,
        letter_spacing: validated.font_size * scale * validated.typography.letter_spacing / 100.0,
        line_height: validated.font_size * scale * LINE_HEIGHT_RATIO,
        color,
        opacity,
        shadow_color: (!scene.shadow_color.is_empty())
            .then(|| parse_color(&scene.shadow_color, opacity)),
        shadow_strength: scene.shadow_strength * scale,
        shadow_distance: scene.shadow_distance * scale,
        border_color: (!scene.border_color.is_empty())
            .then(|| parse_color(&scene.border_color, opacity)),
        border_thickness: scene.border_thickness * scale,
        numerals: Numerals::Default,
    }
}

pub struct LabelFrame {
    pub bounds: Rect,
    pub content_bounds: Rect,
    pub content_width: f32,
    style: ResolvedTextStyle,
    blob: Option<TextBlob>,
    baseline: f32,
}

impl LabelFrame {
    pub fn draw(&self, canvas: &Canvas) -> WidgetResult<()> {
        // Empty text is an intentional label state, with no painted glyphs.
        if let Some(blob) = &self.blob {
            draw_text_blob(canvas, blob, self.baseline, &self.style);
        }
        Ok(())
    }
}

pub fn prepare_label_frame(
    label: &ValidatedLabel,
    scene: &ValidatedScenePresentation,
    fonts: &dyn FontResolver,
) -> WidgetResult<LabelFrame> {
    let style = validated_label_style(label, scene, scene.scale);
    let font = resolve_styled_font(
        fonts,
        Some(&label.font_name),
        style.font_size,
        style.font_weight,
        style.italic,
    )?;
    let layout = layout_text(&label.text, &font, style.letter_spacing, Numerals::Default);
    let ink = layout.measurement;
    let height = ink.bounds_bottom - ink.bounds_top;
    let baseline = if height <= f32::EPSILON {
        baseline_for_top_with_line_height(style.y, &font, style.line_height)
    } else {
        style.y + (style.line_height - height) * 0.5 - ink.bounds_top
    };
    let mut content_bounds = Rect::new(
        style.x + ink.bounds_left,
        baseline + ink.bounds_top,
        style.x + ink.bounds_right,
        baseline + ink.bounds_bottom,
    );
    if !content_bounds.is_empty() && style.border_color.is_some() {
        content_bounds.outset((style.border_thickness * 0.5, style.border_thickness * 0.5));
    }
    let mut bounds = content_bounds;
    if !label.text.is_empty() {
        content_bounds = text_selection_bounds(&ink, style.x, baseline);
        if style.border_color.is_some() {
            content_bounds.outset((style.border_thickness * 0.5, style.border_thickness * 0.5));
        }
    }
    let padding = 2.0
        + if style.shadow_color.is_some() {
            style.shadow_strength * 3.0 + style.shadow_distance.abs()
        } else {
            0.0
        };
    bounds.outset((padding, padding));
    bounds = Rect::new(
        bounds.left.floor(),
        bounds.top.floor(),
        bounds.right.ceil(),
        bounds.bottom.ceil(),
    );
    Ok(LabelFrame {
        bounds,
        content_bounds,
        content_width: ink.width,
        style,
        blob: layout.blob,
        baseline,
    })
}

#[derive(Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
}

pub fn resize_label(
    widget: LabelConfig,
    scene: ScenePresentationConfig,
    factor: f32,
    gesture: ResizeGesture,
    fonts: &dyn FontResolver,
) -> WidgetResult<ResizeUpdate> {
    let ResizeGesture { fixed, point, .. } = gesture;
    let mut label = validate_label(&widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    label.font_size = (label.font_size * factor).clamp(8.0, 400.0);
    let bounds = prepare_label_frame(&label, &scene, fonts)?.content_bounds;
    Ok(ResizeUpdate {
        x: label.x + point[0] - (bounds.left + bounds.width() * fixed[0]),
        y: label.y + point[1] - (bounds.top + bounds.height() * fixed[1]),
        font_size: label.font_size,
    })
}

/// Static pixels are produced only when the handle is created or its config changes.
pub struct PreviewLabel {
    pub tile: Tile,
    frame: LabelFrame,
}

impl PreviewLabel {
    pub fn new(
        widget: LabelConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let label = validate_label(&widget, 0)?;
        let host = WidgetHost::from_scene(&scene, raster_scale)?;
        Self::from_validated(&label, &validate_scene_presentation(scene)?, &host, fonts)
    }

    pub fn from_validated(
        label: &ValidatedLabel,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let frame = prepare_label_frame(label, scene, fonts)?;
        let mut tile = Tile::default();
        tile.render(
            host.raster_scale,
            SHAPE_COLOR_TYPE,
            frame.bounds,
            Presented::new(frame.content_bounds, frame.content_width),
            |canvas| frame.draw(canvas),
        )?;
        Ok(Self { tile, frame })
    }

    /// Static text: every sample draws into the same rectangle.
    pub fn layout_bounds(&self) -> TileBounds {
        self.frame.bounds.into()
    }

    /// Draws the label onto a host canvas in scene coordinates.
    pub fn draw(&self, canvas: &Canvas) -> WidgetResult<TileBounds> {
        self.frame.draw(canvas)?;
        Ok(self.layout_bounds())
    }
}
