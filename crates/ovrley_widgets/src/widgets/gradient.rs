//! Gradient text: the signed percentage and a slope triangle.
use crate::activity::ActivitySampler;
use crate::model::catalog::{DisplayType, MetricKind};
use crate::model::scene::{
    validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
};
use crate::model::validation::require_non_negative_f32;
use crate::model::validation::{require_bool, require_f32};
use crate::model::validation::{require_precision, require_rgba, Precision};
use crate::preview::ResizeGesture;
use crate::preview::WidgetHost;
use crate::render::text::LINE_HEIGHT_RATIO;
use crate::render::text::{
    draw_text_with_vertical_metrics_text, measure_text, rgba_color, FontResolver, ResolvedTextStyle,
};
use crate::render::tile::PreviewRasterScale;
use crate::render::tile::{Presented, Tile};
use crate::render::tile::{TileBounds, TEXT_COLOR_TYPE};
use crate::widgets::value::format::format_number;
use crate::widgets::value::layout::text_bounds;
use crate::widgets::value::layout::{
    metric_vertical_metrics_text, METRIC_WIDGET_UNIT_RATIO, MIN_UNITS_FONT_SIZE,
};
use crate::widgets::value::{TextFields, TextPlacement, ValueConfig};
use crate::{WidgetError, WidgetResult};
use serde::Serialize;
use skia_safe::paint::Style;
use skia_safe::{Canvas, Paint, PaintCap, Path, PathBuilder, Point, Rect};

/// A validated gradient widget.
#[derive(Clone, Debug)]
pub struct ValidatedGradientWidget {
    pub x: f32,
    pub y: f32,
    pub font_name: String,
    pub font_size: f32,
    pub color: [u8; 4],
    pub opacity: f32,
    pub prefix: String,
    pub suffix: String,
    pub precision: Precision,
    pub show_sign: bool,
    pub show_triangle: bool,
    pub triangle_width: f32,
    pub value_offset: f32,
    pub unit_color: [u8; 4],
    pub triangle_positive_color: [u8; 4],
    pub triangle_negative_color: [u8; 4],
}

pub fn validate_gradient_widget(
    value: ValueConfig,
    index: usize,
) -> WidgetResult<ValidatedGradientWidget> {
    let p = |f: &str| format!("values[{index}].{f}");

    if value.value != MetricKind::Gradient {
        return Err(WidgetError::Config(format!(
            "{}: expected Gradient, got {:?}",
            p("value"),
            value.value
        )));
    }

    if value.display_type != DisplayType::Text {
        return Err(WidgetError::Config(format!(
            "{}: gradient requires text display",
            p("display_type")
        )));
    }
    let mut value = value;
    let text = TextFields::validate(&mut value, TextPlacement::Embedded, &p)?;
    let opacity = text.opacity;
    let precision = require_precision(value.decimals, value.decimal_rounding, p)?;

    let show_sign = require_bool(value.show_sign, &p("show_sign"))?;
    let show_triangle = require_bool(value.show_triangle, &p("show_triangle"))?;

    let triangle_width = require_non_negative_f32(value.triangle_width, &p("triangle_width"))?;

    let value_offset = require_f32(value.value_offset, &p("value_offset"))?;

    let unit_color = require_rgba(value.unit_color.as_deref(), &p("unit_color"), opacity)?;

    let triangle_positive_color = require_rgba(
        value.triangle_positive_color.as_deref(),
        &p("triangle_positive_color"),
        opacity,
    )?;

    let triangle_negative_color = require_rgba(
        value.triangle_negative_color.as_deref(),
        &p("triangle_negative_color"),
        opacity,
    )?;

    Ok(ValidatedGradientWidget {
        x: value.x,
        y: value.y,
        font_name: text.font_name,
        font_size: text.font_size,
        color: text.color,
        opacity,
        prefix: text.prefix,
        suffix: text.suffix,
        precision,
        show_sign,
        show_triangle,
        triangle_width,
        value_offset,
        unit_color,
        triangle_positive_color,
        triangle_negative_color,
    })
}

/// Formats the existing signed percentage and affixes. Missing telemetry shows `--%`.
pub fn format_validated_gradient(validated: &ValidatedGradientWidget, raw: Option<f64>) -> String {
    let Some(value) = raw else {
        return "--%".to_string();
    };
    let magnitude = format_number(value.abs(), validated.precision.decimals());
    let sign = if value > 0.0 {
        "+"
    } else if value < 0.0 {
        "-"
    } else {
        ""
    };
    let sign = if validated.show_sign { sign } else { "" };
    format!("{}{sign}{magnitude}%{}", validated.prefix, validated.suffix)
}

const TRIANGLE_GAP: f32 = 8.0;
const ZERO_EPSILON: f64 = 0.05;
const MAX_GRADIENT: f64 = 30.0;

pub fn validated_gradient_style(
    widget: &ValidatedGradientWidget,
    scene: &ValidatedScenePresentation,
    scale: f32,
) -> ResolvedTextStyle {
    ResolvedTextStyle {
        x: widget.x,
        y: widget.y,
        ..ResolvedTextStyle::widget(
            scene,
            scale,
            Some(widget.font_name.clone()),
            widget.font_size,
            rgba_color(widget.color, scene.opacity),
            widget.opacity * scene.opacity,
        )
    }
}

pub struct GradientFrame {
    value: (String, ResolvedTextStyle),
    unit: Option<ResolvedTextStyle>,
    triangle: Option<(Path, Paint)>,
    pub bounds: Rect,
    pub content_bounds: Rect,
    pub content_width: f32,
}

impl GradientFrame {
    pub fn draw(&self, canvas: &Canvas, fonts: &dyn FontResolver) -> WidgetResult<()> {
        let (text, style) = &self.value;
        draw_text_with_vertical_metrics_text(
            canvas,
            text,
            metric_vertical_metrics_text(text),
            style,
            fonts,
        )?;
        if let Some(style) = &self.unit {
            draw_text_with_vertical_metrics_text(
                canvas,
                "%",
                metric_vertical_metrics_text("%"),
                style,
                fonts,
            )?;
        }
        if let Some((path, paint)) = &self.triangle {
            canvas.draw_path(path, paint);
        }
        Ok(())
    }
}

pub fn prepare_gradient_frame(
    widget: &ValidatedGradientWidget,
    base_style: &ResolvedTextStyle,
    raw: Option<f64>,
    scale: f32,
    scene_opacity: f32,
    fonts: &dyn FontResolver,
) -> WidgetResult<GradientFrame> {
    let formatted = format_validated_gradient(widget, raw);
    let (value_text, has_unit) = match formatted.strip_suffix('%') {
        Some(prefix) => (prefix.to_owned(), true),
        None => (formatted, false),
    };
    let line_height = base_style.font_size * LINE_HEIGHT_RATIO;
    let mut value_style = base_style.clone();
    value_style.line_height = line_height;
    let value_width = measure_text(&value_text, &value_style, fonts)?.width;
    // The unit is sized like metric widget units, but sits flush against the digits.
    let mut unit = has_unit.then(|| {
        let mut style = base_style.clone();
        style.font_size =
            (base_style.font_size * METRIC_WIDGET_UNIT_RATIO).max(MIN_UNITS_FONT_SIZE * scale);
        style.line_height = style.font_size * LINE_HEIGHT_RATIO;
        style.color = rgba_color(widget.unit_color, scene_opacity);
        style
    });
    let unit_width = match &unit {
        Some(style) => measure_text("%", style, fonts)?.width,
        None => 0.0,
    };
    let text_width = value_width + unit_width;
    let triangle_width = widget.triangle_width * scale;
    let show_triangle = widget.show_triangle && triangle_width > 0.0;
    let content_width = text_width.max(if show_triangle { triangle_width } else { 0.0 });
    // Indicator width changes its shape around this fixed baseline.
    let zero_baseline = base_style.y + line_height + TRIANGLE_GAP * scale;
    value_style.x = base_style.x + (content_width - text_width) * 0.5;
    // value_offset retains its existing scene-pixel meaning in native export.
    value_style.y = base_style.y - widget.value_offset;
    if let Some(style) = &mut unit {
        // Align the unit's reference ink bottom with the value's, as metric widgets do.
        let value_reference = measure_text(
            metric_vertical_metrics_text(&value_text),
            &value_style,
            fonts,
        )?;
        let bottom = value_style.y
            + (line_height + value_reference.bounds_bottom - value_reference.bounds_top) * 0.5;
        let unit_reference = measure_text(metric_vertical_metrics_text("%"), style, fonts)?;
        style.x = value_style.x + value_width;
        style.y = bottom
            - (style.line_height + unit_reference.bounds_bottom - unit_reference.bounds_top) * 0.5;
    }
    let (mut bounds, mut content_bounds) = text_bounds(
        &value_text,
        metric_vertical_metrics_text(&value_text),
        &value_style,
        fonts,
    )?;
    if let Some(style) = &unit {
        let (unit_bounds, unit_content) =
            text_bounds("%", metric_vertical_metrics_text("%"), style, fonts)?;
        bounds.join(unit_bounds);
        content_bounds.join(unit_content);
    }
    if base_style.border_color.is_some() {
        bounds.outset((
            base_style.border_thickness * 0.5,
            base_style.border_thickness * 0.5,
        ));
        content_bounds.outset((
            base_style.border_thickness * 0.5,
            base_style.border_thickness * 0.5,
        ));
    }
    let triangle = if show_triangle {
        let left = base_style.x + (content_width - triangle_width) * 0.5;
        let grade = raw.unwrap_or(0.0);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(rgba_color(
            if grade < 0.0 {
                widget.triangle_negative_color
            } else {
                widget.triangle_positive_color
            },
            scene_opacity,
        ));
        let mut path = PathBuilder::new();
        path.move_to(Point::new(left, zero_baseline));
        path.line_to(Point::new(left + triangle_width, zero_baseline));
        if grade.abs() <= ZERO_EPSILON {
            let stroke_width = scale.max(1.0);
            paint.set_style(Style::Stroke);
            paint.set_stroke_width(stroke_width);
            paint.set_stroke_cap(PaintCap::Round);
            let triangle_bounds = Rect::new(
                left - stroke_width * 0.5,
                zero_baseline - stroke_width * 0.5,
                left + triangle_width + stroke_width * 0.5,
                zero_baseline + stroke_width * 0.5,
            );
            bounds.join(triangle_bounds);
            content_bounds.join(triangle_bounds);
        } else {
            let height = gradient_triangle_height(raw, triangle_width);
            let tip = zero_baseline + if grade < 0.0 { height } else { -height };
            path.line_to(Point::new(left + triangle_width, tip));
            path.close();
            paint.set_style(Style::Fill);
            let triangle_bounds = Rect::new(
                left,
                zero_baseline.min(tip),
                left + triangle_width,
                zero_baseline.max(tip),
            );
            bounds.join(triangle_bounds);
            content_bounds.join(triangle_bounds);
        }
        Some((path.detach(), paint))
    } else {
        None
    };
    let padding = 2.0
        + if base_style.shadow_color.is_some() {
            base_style.shadow_strength * 3.0 + base_style.shadow_distance.abs()
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
    Ok(GradientFrame {
        value: (value_text, value_style),
        unit,
        triangle,
        bounds,
        content_bounds,
        content_width,
    })
}

/// Draws the percentage as an angle in degrees, exaggerating the slope for
/// readability, capped at 30. The text value is not capped.
pub fn gradient_triangle_height(raw: Option<f64>, width: f32) -> f32 {
    let magnitude = raw.unwrap_or(0.0).abs().min(MAX_GRADIENT);
    if width <= 0.0 || magnitude <= ZERO_EPSILON {
        return 0.0;
    }
    width * (magnitude as f32).to_radians().tan()
}

#[derive(Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub triangle_width: f32,
    pub value_offset: f32,
}

pub fn resize_gradient(
    widget: ValueConfig,
    scene: ScenePresentationConfig,
    factor: f32,
    gesture: ResizeGesture,
    activity: Option<&ActivitySampler>,
    fonts: &dyn FontResolver,
) -> WidgetResult<ResizeUpdate> {
    let ResizeGesture {
        fixed,
        point,
        second,
        ..
    } = gesture;
    let mut config = validate_gradient_widget(widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    config.font_size = (config.font_size * factor).clamp(8.0, 400.0);
    config.triangle_width = (config.triangle_width * factor).clamp(0.0, 600.0);
    config.value_offset *= factor;
    let style = validated_gradient_style(&config, &scene, scene.scale);
    let raw = activity.and_then(|activity| activity.gradient_at(second));
    let bounds = prepare_gradient_frame(&config, &style, raw, scene.scale, scene.opacity, fonts)?
        .content_bounds;
    Ok(ResizeUpdate {
        x: config.x + point[0] - (bounds.left + bounds.width() * fixed[0]),
        y: config.y + point[1] - (bounds.top + bounds.height() * fixed[1]),
        font_size: config.font_size,
        triangle_width: config.triangle_width,
        value_offset: config.value_offset,
    })
}

pub struct PreviewGradient {
    config: ValidatedGradientWidget,
    style: ResolvedTextStyle,
    scale: f32,
    preview_raster_scale: PreviewRasterScale,
    scene_opacity: f32,
    last_sample: Option<Option<f64>>,
    pub tile: Tile,
}

impl PreviewGradient {
    pub fn new(
        widget: ValueConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
    ) -> WidgetResult<Self> {
        let config = validate_gradient_widget(widget, 0)?;
        let host = WidgetHost::from_scene(&scene, raster_scale)?;
        Ok(Self::from_validated(
            config,
            &validate_scene_presentation(scene)?,
            &host,
        ))
    }

    pub fn from_validated(
        config: ValidatedGradientWidget,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
    ) -> Self {
        let preview_raster_scale = host.raster_scale;
        let style = validated_gradient_style(&config, scene, scene.scale);
        Self {
            config,
            style,
            scale: scene.scale,
            preview_raster_scale,
            scene_opacity: scene.opacity,
            last_sample: None,
            tile: Tile::default(),
        }
    }

    /// The sample the frame at `second` draws; equal samples draw identical pixels.
    pub fn frame_key(&self, activity: Option<&ActivitySampler>, second: f64) -> Option<f64> {
        activity.and_then(|activity| activity.gradient_at(second))
    }

    fn frame(&self, raw: Option<f64>, fonts: &dyn FontResolver) -> WidgetResult<GradientFrame> {
        prepare_gradient_frame(
            &self.config,
            &self.style,
            raw,
            self.scale,
            self.scene_opacity,
            fonts,
        )
    }

    /// Scene rectangle a render at `second` draws into, from text layout alone.
    pub fn layout_bounds(
        &self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let raw = activity.and_then(|activity| activity.gradient_at(second));
        Ok(self.frame(raw, fonts)?.bounds.into())
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(
        &self,
        canvas: &Canvas,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let raw = activity.and_then(|activity| activity.gradient_at(second));
        let frame = self.frame(raw, fonts)?;
        frame.draw(canvas, fonts)?;
        Ok(frame.bounds.into())
    }

    pub fn render(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<()> {
        let raw = self.frame_key(activity, second);
        // The triangle moves continuously even while the formatted percentage stays unchanged.
        if self.last_sample == Some(raw) {
            return Ok(());
        }
        let frame = self.frame(raw, fonts)?;
        let bounds = frame.bounds;
        self.tile.render(
            self.preview_raster_scale,
            TEXT_COLOR_TYPE,
            bounds,
            Presented::new(frame.content_bounds, frame.content_width),
            |canvas| frame.draw(canvas, fonts),
        )?;
        self.last_sample = Some(raw);
        Ok(())
    }
}
