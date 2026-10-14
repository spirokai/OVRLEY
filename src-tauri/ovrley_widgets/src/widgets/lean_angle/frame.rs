//! One prepared sector/value frame owns painting and text-layout interaction bounds.
use super::{
    format::{format_lean_angle_value, lean_angle_fill_sweep},
    layout::{
        annular_sector_inset_path, annular_sector_path, geometry_from_layout, inset_geometry,
        lean_angle_layout, polar_point, LeanAngleCache, LeanAngleGeometry,
    },
    validation::ValidatedLeanAngleWidget,
};
use crate::{
    error::{WidgetError, WidgetResult},
    model::{
        scene::ValidatedScenePresentation, standard_metrics::standard_metric_unit_label,
        standard_widgets::widget_font_weight, types::MetricKind,
    },
    render::text::{
        draw_text_blob, layout_text, measure_text_with_font, parse_color, resolve_styled_font,
        text_selection_bounds, FontResolver, ResolvedTextStyle,
    },
    widgets::value::layout::{
        metric_vertical_metrics_text, METRIC_WIDGET_LINE_HEIGHT, METRIC_WIDGET_UNITS_GAP_PX,
        METRIC_WIDGET_UNIT_RATIO, MIN_UNITS_FONT_SIZE,
    },
};
use skia_safe::{paint::Style, Canvas, ClipOp, Paint, Path, Point, Rect, TextBlob};

struct TextRun {
    blob: TextBlob,
    baseline: f32,
    style: ResolvedTextStyle,
}

pub struct LeanAngleFrame {
    value: TextRun,
    unit: Option<TextRun>,
    fill: Option<(Path, Paint)>,
    inner_track: Path,
    pub bounds: Rect,
    pub content_bounds: Rect,
    pub content_width: f32,
    pub fill_sweep: f32,
    pub marker: Point,
}

impl LeanAngleFrame {
    pub fn draw(&self, canvas: &Canvas, cache: &LeanAngleCache) {
        canvas.draw_image(
            &cache.static_image,
            (cache.static_image_x, cache.static_image_y),
            None,
        );
        if let Some((path, paint)) = &self.fill {
            canvas.save();
            canvas.clip_path(&self.inner_track, ClipOp::Intersect, true);
            canvas.draw_path(path, paint);
            canvas.restore();
        }
        draw_text_blob(
            canvas,
            &self.value.blob,
            self.value.baseline,
            &self.value.style,
        );
        if let Some(unit) = &self.unit {
            draw_text_blob(canvas, &unit.blob, unit.baseline, &unit.style);
        }
    }
}

pub fn prepare_lean_angle_frame(
    widget: &ValidatedLeanAngleWidget,
    scene: &ValidatedScenePresentation,
    raw: Option<f64>,
    fonts: &dyn FontResolver,
) -> WidgetResult<LeanAngleFrame> {
    let scale = scene.scale;
    let layout = lean_angle_layout(
        widget.diameter * scale,
        widget.track_thickness * scale,
        widget.font_size * scale,
    );
    let geometry = LeanAngleGeometry {
        center_x: widget.x + layout.center_x,
        center_y: widget.y + layout.center_y,
        ..geometry_from_layout(layout)
    };
    let border = widget.track_border_thickness * scale;
    let fill_sweep = lean_angle_fill_sweep(raw);
    let fill = if fill_sweep != 0.0 {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(Style::Fill);
        paint.set_color(parse_color(
            &widget.track_filled_color,
            widget.track_filled_opacity * widget.opacity,
        ));
        Some((
            annular_sector_path(LeanAngleGeometry {
                start_angle: 270.0,
                sweep_angle: fill_sweep,
                ..inset_geometry(geometry, border)
            }),
            paint,
        ))
    } else {
        None
    };
    let font_size = widget.font_size * scale;
    let mut value_style = ResolvedTextStyle {
        x: 0.0,
        y: 0.0,
        font_name: Some(widget.font.clone()),
        font_size,
        font_weight: widget_font_weight(),
        italic: false,
        letter_spacing: 0.0,
        line_height: font_size * METRIC_WIDGET_LINE_HEIGHT,
        color: parse_color(&widget.color, widget.opacity),
        opacity: widget.opacity,
        shadow_color: (scene.shadow_strength > 0.0)
            .then(|| parse_color(&scene.shadow_color, widget.opacity)),
        shadow_strength: scene.shadow_strength * scale,
        shadow_distance: scene.shadow_distance * scale,
        border_color: Some(parse_color(&scene.border_color, widget.opacity)),
        border_thickness: scene.border_thickness * scale,
    };
    let value_text = format_lean_angle_value(raw);
    let value_font = resolve_styled_font(
        fonts,
        Some(&widget.font),
        font_size,
        widget_font_weight(),
        false,
    )?;
    let value = layout_text(&value_text, &value_font, 0.0);
    let value_vertical =
        measure_text_with_font(metric_vertical_metrics_text(&value_text), &value_font);
    let unit = if raw.is_some() && widget.show_units {
        let unit_text = standard_metric_unit_label(MetricKind::LeanAngle, None);
        let unit_size = (font_size * METRIC_WIDGET_UNIT_RATIO).max(MIN_UNITS_FONT_SIZE * scale);
        let font = resolve_styled_font(
            fonts,
            Some(&widget.font),
            unit_size,
            widget_font_weight(),
            false,
        )?;
        let text = layout_text(unit_text, &font, 0.0);
        // The degree glyph shares the existing degree/C baseline reference.
        let vertical = measure_text_with_font(metric_vertical_metrics_text("\u{00b0}C"), &font);
        let style = ResolvedTextStyle {
            font_size: unit_size,
            line_height: unit_size * METRIC_WIDGET_LINE_HEIGHT,
            color: parse_color(&widget.unit_color, widget.opacity),
            ..value_style.clone()
        };
        Some((text, vertical, style))
    } else {
        None
    };
    let units_width = unit.as_ref().map_or(0.0, |(text, _, _)| {
        METRIC_WIDGET_UNITS_GAP_PX * scale + text.measurement.width
    });
    let group_width = value.measurement.width + units_width;
    let group_height = unit
        .as_ref()
        .map_or(value_style.line_height, |(_, _, style)| {
            value_style.line_height.max(style.line_height)
        });
    let degree_offset = if unit.is_some() { font_size * 0.1 } else { 0.0 };
    value_style.x =
        geometry.center_x + widget.value_offset_x * scale + degree_offset - group_width * 0.5;
    let group_bottom = geometry.center_y + widget.value_offset_y * scale + group_height * 0.5;
    let value_baseline = group_bottom - value_vertical.bounds_bottom;
    let mut content_bounds = annular_sector_path(geometry).compute_tight_bounds();
    let mut bounds = content_bounds;
    let text_bounds = |ink: &crate::render::text::MeasuredText, x: f32, baseline: f32| {
        let mut bounds = Rect::new(
            x + ink.bounds_left,
            baseline + ink.bounds_top,
            x + ink.bounds_right,
            baseline + ink.bounds_bottom,
        );
        bounds.outset((
            value_style.border_thickness * 0.5,
            value_style.border_thickness * 0.5,
        ));
        let mut content = text_selection_bounds(ink, x, baseline);
        content.outset((
            value_style.border_thickness * 0.5,
            value_style.border_thickness * 0.5,
        ));
        (bounds, content)
    };
    let (value_bounds, value_content) =
        text_bounds(&value.measurement, value_style.x, value_baseline);
    bounds.join(value_bounds);
    content_bounds.join(value_content);
    let unit = unit
        .map(|(text, vertical, mut style)| -> WidgetResult<TextRun> {
            style.x = value_style.x + value.measurement.width + METRIC_WIDGET_UNITS_GAP_PX * scale;
            let baseline = group_bottom - vertical.bounds_bottom;
            let (unit_bounds, unit_content) = text_bounds(&text.measurement, style.x, baseline);
            bounds.join(unit_bounds);
            content_bounds.join(unit_content);
            Ok(TextRun {
                blob: text.blob.ok_or_else(|| {
                    WidgetError::Render("unable to lay out lean-angle unit".into())
                })?,
                baseline,
                style,
            })
        })
        .transpose()?;
    let shadow_padding = if scene.shadow_strength > 0.0 {
        (scene.shadow_strength * 3.0 + scene.shadow_distance) * scale
    } else {
        0.0
    };
    let padding = 2.0 + border * 0.5 + shadow_padding;
    bounds.outset((padding, padding));
    bounds = Rect::new(
        bounds.left.floor(),
        bounds.top.floor(),
        bounds.right.ceil(),
        bounds.bottom.ceil(),
    );
    Ok(LeanAngleFrame {
        value: TextRun {
            blob: value
                .blob
                .ok_or_else(|| WidgetError::Render("unable to lay out lean-angle value".into()))?,
            baseline: value_baseline,
            style: value_style,
        },
        unit,
        fill,
        inner_track: annular_sector_inset_path(geometry, border),
        bounds,
        content_bounds,
        content_width: layout.width,
        fill_sweep,
        marker: polar_point(
            geometry.center_x,
            geometry.center_y,
            geometry.outer_radius,
            270.0 + fill_sweep,
        ),
    })
}
