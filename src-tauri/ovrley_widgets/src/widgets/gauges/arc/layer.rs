//! Static track layers and dynamic fill rendering for arc gauges.
//!
//! Static rendering caches empty tracks, borders, shadows, labels, and units.
//! Dynamic rendering uses the same canonical track geometry for continuous
//! reveals and rounded segmented wedges, so fill cannot change the silhouette.

use super::geometry::ArcGaugeGeometry;
use super::path::ArcTrackSpec;
use super::segment::{draw_segment, inner_segment_geometry, segment_geometries};
use super::ArcGaugeCache;
use crate::model::{scene::ValidatedScenePresentation, types::TrackFillStyle};
use crate::render::{
    shadow::shadow_paint,
    text::{parse_color, ResolvedTextStyle},
};
use crate::widgets::gauges::{
    bar_geometry::scale_bar_geometry, validation::ValidatedArcGaugeWidget,
};
use skia_safe::{paint::Style, BlendMode, Canvas, ClipOp, Paint};

/// Builds the padded, cached image containing every frame-invariant element.
pub(super) fn draw_static_layer(
    canvas: &Canvas,
    gauge: &ValidatedArcGaugeWidget,
    scene: &ValidatedScenePresentation,
    geometry: ArcGaugeGeometry,
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

    match scale_bar_geometry(gauge.bar_geometry, scale) {
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
    let paint = fill_paint(cache);
    let inner_thickness = (cache.track_thickness - cache.track_border_thickness * 2.0).max(0.0);
    let inner_corner = (cache.track_corner_radius - cache.track_border_thickness).max(0.0);
    for segment in segment_geometries(geometry, bars)
        .into_iter()
        .take(filled_count)
    {
        let inner_geo = inner_segment_geometry(segment, cache.track_border_thickness);
        draw_segment(canvas, inner_geo, inner_thickness, inner_corner, &paint);
    }
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

    canvas.save();
    canvas.clip_path(&reveal_path, ClipOp::Intersect, true);
    draw_segment(
        canvas,
        inner_geometry,
        inner_thickness,
        inner_corner,
        &fill_paint(cache),
    );
    canvas.restore();
}

fn fill_paint(cache: &ArcGaugeCache) -> Paint {
    track_paint(parse_color(
        &cache.track_filled_color,
        cache.track_filled_opacity * cache.text_style.opacity,
    ))
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
