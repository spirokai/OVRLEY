use crate::activity::schema::{DenseActivityReport, NumericSeries, ParsedActivity};
use crate::debug::RenderProfiler;
use crate::error::CoreResult;
use crate::normalize::{GForceAxis, ValidatedGForceWidget, ValidatedSceneConfig};
use crate::render::surface::create_surface;
use crate::render::text::parse_color;
use crate::render::widgets::common::{normalize_shadow_style_validated, static_layer_padding};
use crate::render::widgets::types::GForceWidgetCache;
use skia_safe::{
    image_filters, paint::Style, BlendMode, ClipOp, Paint, PathBuilder, PathFillType, Point,
};

const GUIDE_DEFAULT_THICKNESS_PX: f32 = 2.0;
const GUIDE_REFERENCE_DIAMETER_PX: f32 = 400.0;
const GUIDE_OPACITY: f32 = 0.4;
const GUIDE_RING_RADIUS_RATIOS: [f32; 2] = [0.33, 0.66];

fn activity_axis(activity: &ParsedActivity, axis: GForceAxis) -> &NumericSeries {
    match axis {
        GForceAxis::X => &activity.g_force_x,
        GForceAxis::Y => &activity.g_force_y,
        GForceAxis::Z => &activity.g_force_z,
    }
}

fn dense_axis(dense: &DenseActivityReport, axis: GForceAxis) -> &[Option<f64>] {
    match axis {
        GForceAxis::X => &dense.series.g_force_x,
        GForceAxis::Y => &dense.series.g_force_y,
        GForceAxis::Z => &dense.series.g_force_z,
    }
}

fn oriented_values(values: &[Option<f64>], invert: bool) -> Vec<Option<f64>> {
    let multiplier = if invert { -1.0 } else { 1.0 };
    values
        .iter()
        .map(|value| value.map(|value| value * multiplier))
        .collect()
}

pub fn derive_max_g(activity: &ParsedActivity, widget: &ValidatedGForceWidget) -> f64 {
    let horizontal = oriented_values(
        activity_axis(activity, widget.axis_horizontal),
        widget.invert_horizontal,
    );
    let vertical = oriented_values(
        activity_axis(activity, widget.axis_vertical),
        widget.invert_vertical,
    );
    if horizontal.is_empty() || vertical.is_empty() {
        return 0.0;
    }
    let mut magnitudes = Vec::with_capacity(horizontal.len());
    for (index, horizontal) in horizontal.iter().enumerate() {
        if let (Some(horizontal), Some(vertical)) = (horizontal, vertical[index]) {
            magnitudes.push(horizontal.hypot(vertical));
        }
    }
    if magnitudes.is_empty() {
        return 0.0;
    }

    let rank = ((widget.clip_percentile as f64 / 100.0) * magnitudes.len() as f64).ceil() as usize;
    let (_, value, _) = magnitudes
        .select_nth_unstable_by(rank.saturating_sub(1), |left, right| left.total_cmp(right));
    *value
}

pub fn prepare_g_force_cache(
    widget: &ValidatedGForceWidget,
    scene: &ValidatedSceneConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    prepare_profiler: &mut RenderProfiler,
) -> CoreResult<GForceWidgetCache> {
    prepare_profiler.measure("g_force.prepare", || {
        let scale = scene.scale;
        let width = (widget.width as f32 * scale).round().max(1.0) as u32;
        let height = (widget.height as f32 * scale).round().max(1.0) as u32;
        let radius = widget.diameter * scale * 0.5;
        let border_thickness = widget.border_thickness * scale;
        let guide_thickness =
            GUIDE_DEFAULT_THICKNESS_PX * (widget.diameter / GUIDE_REFERENCE_DIAMETER_PX) * scale;
        let center_x = width as f32 * 0.5;
        let center_y = height as f32 * 0.5;
        let shadow = normalize_shadow_style_validated(
            &scene.shadow_color,
            scene.shadow_strength,
            scene.shadow_distance,
            scale,
        );
        let padding = static_layer_padding(border_thickness, shadow.as_ref());
        let mut surface = create_surface(
            width.saturating_add(padding * 2),
            height.saturating_add(padding * 2),
        )?;
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::TRANSPARENT);
        canvas.translate((padding as f32, padding as f32));

        if border_thickness > 0.0 {
            if let Some(filter) = shadow.as_ref().and_then(|shadow| {
                image_filters::drop_shadow_only(
                    (shadow.offset_x, shadow.offset_y),
                    (shadow.strength, shadow.strength),
                    parse_color(&shadow.color, widget.opacity),
                    None,
                    None,
                    None,
                )
            }) {
                let mut shadow_paint = Paint::default();
                shadow_paint.set_anti_alias(true);
                shadow_paint.set_image_filter(filter);
                let mut ring_path = PathBuilder::new_with_fill_type(PathFillType::EvenOdd);
                ring_path.add_circle(Point::new(center_x, center_y), radius, None);
                ring_path.add_circle(
                    Point::new(center_x, center_y),
                    radius - border_thickness,
                    None,
                );
                canvas.draw_path(&ring_path.detach(), &shadow_paint);
            }

            let mut border_paint = Paint::default();
            border_paint.set_anti_alias(true);
            border_paint.set_color(parse_color(
                &widget.border_color,
                widget.border_opacity * widget.opacity,
            ));
            canvas.draw_circle(Point::new(center_x, center_y), radius, &border_paint);

            let mut clear_paint = Paint::default();
            clear_paint.set_anti_alias(true);
            clear_paint.set_blend_mode(BlendMode::Clear);
            canvas.draw_circle(
                Point::new(center_x, center_y),
                radius - border_thickness,
                &clear_paint,
            );
        }

        let mut fill_paint = Paint::default();
        fill_paint.set_anti_alias(true);
        fill_paint.set_color(parse_color(
            &widget.fill_color,
            widget.fill_opacity * widget.opacity,
        ));
        canvas.draw_circle(
            Point::new(center_x, center_y),
            radius - border_thickness,
            &fill_paint,
        );

        let mut guide_paint = Paint::default();
        guide_paint.set_anti_alias(true);
        guide_paint.set_style(Style::Stroke);
        guide_paint.set_stroke_width(guide_thickness);
        guide_paint.set_color(parse_color(&widget.marker_color, 1.0));
        let mut guide_clip = PathBuilder::new();
        guide_clip.add_circle(
            Point::new(center_x, center_y),
            radius - border_thickness,
            None,
        );
        canvas.save();
        canvas.clip_path(&guide_clip.detach(), ClipOp::Intersect, true);
        canvas.save_layer_alpha_f(None, GUIDE_OPACITY * widget.opacity);
        let mut guide_path = PathBuilder::new();
        guide_path.move_to(Point::new(center_x - radius, center_y));
        guide_path.line_to(Point::new(center_x + radius, center_y));
        guide_path.move_to(Point::new(center_x, center_y - radius));
        guide_path.line_to(Point::new(center_x, center_y + radius));
        for ratio in GUIDE_RING_RADIUS_RATIOS {
            guide_path.add_circle(Point::new(center_x, center_y), radius * ratio, None);
        }
        canvas.draw_path(&guide_path.detach(), &guide_paint);
        canvas.restore();
        canvas.restore();

        Ok(GForceWidgetCache {
            parent_circle_image: surface.image_snapshot(),
            parent_circle_image_x: -(padding as f32),
            parent_circle_image_y: -(padding as f32),
            max_g: derive_max_g(activity, widget),
            x: widget.x,
            y: widget.y,
            width,
            height,
            center_x,
            center_y,
            radius,
            opacity: widget.opacity,
            marker_radius: widget.marker_size * scale * 0.5,
            marker_color: widget.marker_color.clone(),
            marker_opacity: widget.marker_opacity,
            label_font: widget.label_font.clone(),
            label_font_size: widget.label_font_size * scale,
            label_color: widget.label_color.clone(),
            label_decimals: widget.label_decimals,
            label_unit: widget.label_unit.clone(),
            label_unit_color: widget.label_unit_color.clone(),
            label_offset_x: widget.label_offset_x * scale,
            label_offset_y: widget.label_offset_y * scale,
            horizontal_values: oriented_values(
                dense_axis(dense_activity, widget.axis_horizontal),
                widget.invert_horizontal,
            ),
            vertical_values: oriented_values(
                dense_axis(dense_activity, widget.axis_vertical),
                widget.invert_vertical,
            ),
            shadow,
        })
    })
}
