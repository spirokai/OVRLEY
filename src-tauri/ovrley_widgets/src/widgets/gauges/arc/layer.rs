//! Static track layers and dynamic fill rendering for arc gauges.
//!
//! Static rendering caches empty tracks, borders, shadows, labels, and units.
//! Dynamic rendering uses the same canonical track geometry for continuous
//! reveals and rounded segmented wedges, so fill cannot change the silhouette.

use super::geometry::{arc_point, ArcGaugeGeometry};
use super::path::{append_circular_arc, ArcTrackSpec};
use super::segment::{draw_segment, inner_segment_geometry, segment_geometries};
use super::ArcGaugeCache;
use crate::model::{scene::ValidatedScenePresentation, types::TrackFillStyle};
use crate::render::{
    shadow::shadow_paint,
    text::{parse_color, ResolvedTextStyle},
    track_fill::FillShape,
};
use crate::widgets::gauges::{
    bar_geometry::ResolvedBarGeometry, validation::ValidatedArcGaugeWidget,
};
use skia_safe::{
    paint::{Cap, Style},
    BlendMode, Canvas, Paint, PathBuilder,
};

/// GT ticks start this many track thicknesses inside the track's inner edge.
const GT_TICK_GAP_RATIO: f32 = 1.0;
/// GT tick stroke width relative to the track thickness.
const GT_TICK_WIDTH_RATIO: f32 = 0.2;
/// GT glow blur radius relative to the track thickness.
const GT_GLOW_SIGMA_RATIO: f32 = 0.6;

/// Blur sigma of the GT fill glow; also sizes the glow's raster padding.
pub(super) fn glow_sigma(track_thickness: f32) -> f32 {
    track_thickness * GT_GLOW_SIGMA_RATIO
}

/// Builds the padded, cached image containing every frame-invariant element.
pub(super) fn draw_static_layer(
    canvas: &Canvas,
    gauge: &ValidatedArcGaugeWidget,
    scene: &ValidatedScenePresentation,
    geometry: ArcGaugeGeometry,
    bars: Option<ResolvedBarGeometry>,
    scale: f32,
    text_style: &ResolvedTextStyle,
) {
    let track_thickness = gauge.track_thickness * scale;
    let border_thickness = gauge.track_border_thickness * scale;
    let shadow = if border_thickness > 0.0 {
        shadow_paint(
            &track_paint(parse_color(&gauge.track_border_color, text_style.opacity)),
            parse_color(&scene.shadow_color, text_style.opacity),
            scene.shadow_strength * scale,
        )
    } else {
        None
    };

    match bars {
        Some(bar_geometry) => {
            for segment in segment_geometries(geometry, bar_geometry) {
                draw_static_track(
                    canvas,
                    gauge,
                    segment,
                    track_thickness,
                    border_thickness,
                    scale,
                    text_style,
                    shadow
                        .as_ref()
                        .map(|paint| (paint, scene.shadow_distance * scale)),
                );
            }
        }
        None => draw_static_track(
            canvas,
            gauge,
            geometry,
            track_thickness,
            border_thickness,
            scale,
            text_style,
            shadow
                .as_ref()
                .map(|paint| (paint, scene.shadow_distance * scale)),
        ),
    }
    draw_gt_ticks(canvas, gauge, geometry, scale, text_style);
}

/// Value-less ticks evenly spaced along the sweep, inside the track.
fn draw_gt_ticks(
    canvas: &Canvas,
    gauge: &ValidatedArcGaugeWidget,
    geometry: ArcGaugeGeometry,
    scale: f32,
    text_style: &ResolvedTextStyle,
) {
    let Some(style) = &gauge.gt_style else {
        return;
    };
    let length = style.tick_length * scale;
    if length <= 0.0 {
        return;
    }
    let thickness = gauge.track_thickness * scale;
    let outer = geometry.radius
        - thickness * (0.5 + GT_TICK_GAP_RATIO)
        - gauge.track_border_thickness * scale;
    let inner = (outer - length).max(0.0);
    if outer <= inner {
        return;
    }
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(Style::Stroke);
    paint.set_stroke_cap(Cap::Round);
    paint.set_stroke_width((thickness * GT_TICK_WIDTH_RATIO).max(scale));
    paint.set_color(parse_color(&style.tick_color, text_style.opacity));
    let steps = (style.tick_count - 1) as f32;
    for index in 0..style.tick_count {
        let angle = geometry.start_angle + geometry.sweep_angle * index as f32 / steps;
        canvas.draw_line(
            arc_point(geometry.center_x, geometry.center_y, outer, angle),
            arc_point(geometry.center_x, geometry.center_y, inner, angle),
            &paint,
        );
    }
}

fn draw_static_track(
    canvas: &Canvas,
    gauge: &ValidatedArcGaugeWidget,
    geometry: ArcGaugeGeometry,
    track_thickness: f32,
    border_thickness: f32,
    scale: f32,
    text_style: &ResolvedTextStyle,
    shadow: Option<(&Paint, f32)>,
) {
    let inner_corner_radius = gauge.track_corner_radius * scale;
    let border_color = parse_color(&gauge.track_border_color, text_style.opacity);

    if let Some((paint, distance)) = shadow {
        canvas.save();
        canvas.translate((distance, distance));
        draw_segment(
            canvas,
            geometry,
            track_thickness,
            inner_corner_radius,
            paint,
        );
        canvas.restore();
    }
    if border_thickness > 0.0 {
        draw_segment(
            canvas,
            geometry,
            track_thickness,
            inner_corner_radius,
            &track_paint(border_color),
        );
        let inner_geo = inner_segment_geometry(geometry, border_thickness);
        let inner_thickness = (track_thickness - border_thickness * 2.0).max(0.0);
        let inner_cnr = (inner_corner_radius - border_thickness).max(0.0);
        draw_segment(
            canvas,
            inner_geo,
            inner_thickness,
            inner_cnr,
            &clear_track_paint(),
        );
        draw_segment(
            canvas,
            inner_geo,
            inner_thickness,
            inner_cnr,
            &track_paint(parse_color(
                &gauge.track_empty_color,
                gauge.track_empty_opacity * text_style.opacity,
            )),
        );
    } else {
        draw_segment(
            canvas,
            geometry,
            track_thickness,
            inner_corner_radius,
            &track_paint(parse_color(
                &gauge.track_empty_color,
                gauge.track_empty_opacity * text_style.opacity,
            )),
        );
    }
}

/// Dispatches the current fill to the continuous or segmented painting model.
pub(super) fn draw_fill(
    canvas: &Canvas,
    cache: &ArcGaugeCache,
    geometry: ArcGaugeGeometry,
    fill01: f32,
) {
    match cache.track_fill_style {
        TrackFillStyle::Bars => draw_segmented_fill(canvas, cache, geometry, fill01),
        TrackFillStyle::Fill => draw_continuous_fill(canvas, cache, geometry, fill01),
    }
}

/// Paints the first `count` border-inset bars of `geometry`.
fn draw_bars(
    canvas: &Canvas,
    cache: &ArcGaugeCache,
    geometry: ArcGaugeGeometry,
    bars: ResolvedBarGeometry,
    count: usize,
    paint: &Paint,
) {
    let inner_thickness = (cache.track_thickness - cache.track_border_thickness * 2.0).max(0.0);
    let inner_corner = (cache.track_corner_radius - cache.track_border_thickness).max(0.0);
    for segment in segment_geometries(geometry, bars).into_iter().take(count) {
        let inner_geo = inner_segment_geometry(segment, cache.track_border_thickness);
        draw_segment(canvas, inner_geo, inner_thickness, inner_corner, paint);
    }
}

/// Paints the completely filled track in `geometry`'s frame for the cached gradient raster.
pub(super) fn draw_full_fill_layer(
    canvas: &Canvas,
    cache: &ArcGaugeCache,
    geometry: ArcGaugeGeometry,
    paint: &Paint,
) {
    match cache.bar_geometry {
        Some(bars) => draw_bars(canvas, cache, geometry, bars, bars.count as usize, paint),
        None => {
            let bt = cache.track_border_thickness;
            draw_segment(
                canvas,
                inner_segment_geometry(geometry, bt),
                (cache.track_thickness - bt * 2.0).max(0.0),
                (cache.track_corner_radius - bt).max(0.0),
                paint,
            );
        }
    }
}

fn draw_segmented_fill(
    canvas: &Canvas,
    cache: &ArcGaugeCache,
    geometry: ArcGaugeGeometry,
    fill01: f32,
) {
    let bars = cache
        .bar_geometry
        .expect("bars fill style must carry resolved bar geometry");
    let filled_count = super::super::metric::bar_fill_count(fill01, bars.count);
    if filled_count == 0 {
        return;
    }
    // Gaps are empty in the full-fill raster, so one wedge from the center, ending
    // mid-gap after the last filled bar, reveals whole bars.
    let direction = geometry.sweep_angle.signum();
    let degrees = |length: f32| direction * (length / geometry.radius).to_degrees();
    let start = geometry.start_angle - degrees(bars.gap * 0.5);
    let sweep = degrees(filled_count as f32 * (bars.extent + bars.gap));
    let radius = geometry.radius + cache.track_thickness;
    let mut wedge = PathBuilder::new();
    wedge.move_to((geometry.center_x, geometry.center_y));
    wedge.line_to(arc_point(
        geometry.center_x,
        geometry.center_y,
        radius,
        start,
    ));
    append_circular_arc(
        &mut wedge,
        geometry.center_x,
        geometry.center_y,
        radius,
        start,
        sweep,
    );
    wedge.close();
    cache
        .fill
        .draw(canvas, &FillShape::Path(wedge.detach()), |paint| {
            draw_bars(canvas, cache, geometry, bars, filled_count, paint)
        });
}

fn draw_continuous_fill(
    canvas: &Canvas,
    cache: &ArcGaugeCache,
    geometry: ArcGaugeGeometry,
    fill01: f32,
) {
    let bt = cache.track_border_thickness;
    let inner_thickness = (cache.track_thickness - bt * 2.0).max(0.0);
    let inner_geometry = inner_segment_geometry(geometry, bt);
    let inner_corner = (cache.track_corner_radius - bt).max(0.0);
    let end_corner = if cache.track_fill_flat {
        0.0
    } else {
        inner_corner
    };
    let reveal_spec = ArcTrackSpec::full(inner_geometry, inner_thickness, inner_corner)
        .with_end_corner_radius(end_corner);
    let Some(reveal) = reveal_spec.reveal_clip(fill01) else {
        return;
    };
    let Some(reveal_path) = reveal.path(&reveal_spec) else {
        return;
    };
    let reveal = FillShape::Path(reveal_path);
    cache.fill.draw(canvas, &reveal, |paint| {
        if let Some(glow) = glow_paint(cache, paint) {
            // The blur hides the tip's exact cap shape, so the glow needs no reveal clip.
            let glow_geometry = ArcGaugeGeometry {
                sweep_angle: inner_geometry.sweep_angle * fill01,
                ..inner_geometry
            };
            draw_segment(canvas, glow_geometry, inner_thickness, end_corner, &glow);
        }
        canvas.save();
        reveal.clip(canvas);
        draw_segment(canvas, inner_geometry, inner_thickness, inner_corner, paint);
        canvas.restore();
    });
}

/// Subtle blurred halo in the fill color, drawn beneath the fill.
fn glow_paint(cache: &ArcGaugeCache, fill: &Paint) -> Option<Paint> {
    if cache.glow <= 0.0 {
        return None;
    }
    let color = fill.color().with_a((cache.glow * 255.0).round() as u8);
    shadow_paint(fill, color, glow_sigma(cache.track_thickness))
}

fn track_paint(color: skia_safe::Color) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(Style::Fill);
    paint.set_color(color);
    paint
}

fn clear_track_paint() -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(Style::Fill);
    paint.set_blend_mode(BlendMode::Clear);
    paint
}
