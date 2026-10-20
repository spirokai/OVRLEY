//! Prepare the fixed track plus whichever of card/needle stays still; playback only rotates the other.
use super::validation::{CompassRotationMode, ValidatedHeadingCompass};
use crate::{
    error::{WidgetError, WidgetResult},
    model::{scene::ValidatedScenePresentation, standard_widgets::widget_font_weight},
    render::{
        shadow::shadow_paint,
        text::{parse_color, resolve_styled_font, FontResolver},
    },
    widgets::{
        gauges::arc::{rounded_segment_path, ArcGaugeGeometry},
        value::preview::{surface, SHAPE_COLOR_TYPE},
    },
};
use skia_safe::{
    paint::{Join, Style},
    Canvas, Color, CubicResampler, FilterMode, Font, Image, Paint, Path, PathBuilder, Point, Rect,
    SamplingOptions, TextBlob,
};

/// The track opens towards the bottom-right, where the heading readout sits.
pub const GAP_CENTER_ANGLE: f32 = 45.0;
/// Rotating labels fade out over this many degrees past either track end.
const LABEL_FADE_DEGREES: f32 = 25.0;
/// Cardinal glyph centres sit this many label font sizes inside the track.
const CARDINAL_INSET_RATIO: f32 = 0.95;
/// Needle half-width and rear notch depth, relative to its tip-to-tail length.
const INDICATOR_HALF_WIDTH_RATIO: f32 = 0.36;
const INDICATOR_NOTCH_RATIO: f32 = 0.3;
const CARDINALS: [(f32, &str); 4] = [(0.0, "N"), (90.0, "E"), (180.0, "S"), (270.0, "W")];

/// Diameter sets the dial face; a thicker track grows outwards so labels and needle keep their room.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeadingCompassLayout {
    pub inner_radius: f32,
    pub outer_radius: f32,
    pub label_radius: f32,
    pub track_start_angle: f32,
    pub track_sweep_angle: f32,
}

impl HeadingCompassLayout {
    /// The widget origin is the top-left of the track's outer circle.
    pub fn size(&self) -> f32 {
        self.outer_radius * 2.0
    }
}

pub fn heading_compass_layout(
    diameter: f32,
    track_thickness: f32,
    cardinal_font_size: f32,
    arc_angle: f32,
) -> HeadingCompassLayout {
    let inner_radius = diameter * 0.5;
    HeadingCompassLayout {
        inner_radius,
        outer_radius: inner_radius + track_thickness,
        label_radius: inner_radius - cardinal_font_size * CARDINAL_INSET_RATIO,
        track_start_angle: GAP_CENTER_ANGLE + (360.0 - arc_angle) * 0.5,
        track_sweep_angle: arc_angle,
    }
}

/// Midpoint between the track's two end points, relative to the dial centre. The readout's
/// leading ink edge and vertical midpoint sit here, so it grows away from the dial.
pub fn readout_anchor(layout: &HeadingCompassLayout) -> Point {
    let radius = (layout.inner_radius + layout.outer_radius) * 0.5;
    let start = layout.track_start_angle.to_radians();
    let end = (layout.track_start_angle + layout.track_sweep_angle).to_radians();
    Point::new(
        radius * (start.cos() + end.cos()) * 0.5,
        radius * (start.sin() + end.sin()) * 0.5,
    )
}

/// Screen angle (Skia degrees, clockwise from +x) of a compass bearing on a card turned by `card_rotation`.
pub fn bearing_screen_angle(bearing: f32, card_rotation: f32) -> f32 {
    bearing + card_rotation - 90.0
}

/// Needle vertices pointing up around the origin: tip, right tail, rear notch, left tail.
pub fn indicator_vertices(size: f32) -> [Point; 4] {
    let half = size * 0.5;
    let half_width = size * INDICATOR_HALF_WIDTH_RATIO;
    [
        Point::new(0.0, -half),
        Point::new(half_width, half),
        Point::new(0.0, half - size * INDICATOR_NOTCH_RATIO),
        Point::new(-half_width, half),
    ]
}

/// Rounds every corner by the same share of the largest radius the shortest edges allow.
pub fn indicator_path(size: f32, corner_radius_pct: f32) -> Path {
    let vertices = indicator_vertices(size);
    let edge = |a: Point, b: Point| (b - a).length();
    let count = vertices.len();
    let max_cut = (0..count)
        .map(|i| edge(vertices[i], vertices[(i + 1) % count]) * 0.5)
        .fold(f32::INFINITY, f32::min);
    let cut = max_cut * corner_radius_pct / 100.0;
    let toward = |from: Point, to: Point| from + (to - from) * (cut / edge(from, to));
    let mut path = PathBuilder::new();
    if cut == 0.0 {
        path.move_to(vertices[0]);
        for vertex in &vertices[1..] {
            path.line_to(*vertex);
        }
    } else {
        path.move_to(toward(vertices[0], vertices[1]));
        for i in 1..=count {
            let vertex = vertices[i % count];
            path.line_to(toward(vertex, vertices[(i - 1) % count]));
            path.quad_to(vertex, toward(vertex, vertices[(i + 1) % count]));
        }
    }
    path.close();
    path.detach()
}

/// Labels fade out over the gap so a rotating card never clashes with the readout.
pub fn label_fade(layout: &HeadingCompassLayout, screen_angle: f32) -> f32 {
    let gap = 360.0 - layout.track_sweep_angle;
    if gap <= 0.0 {
        return 1.0;
    }
    let track_end = layout.track_start_angle + layout.track_sweep_angle;
    let into_gap = (screen_angle - track_end).rem_euclid(360.0);
    if into_gap >= gap {
        return 1.0;
    }
    let fade = LABEL_FADE_DEGREES.min(gap * 0.5);
    1.0 - (into_gap.min(gap - into_gap) / fade).min(1.0)
}

/// One upright cardinal letter with its own blurred shadow. Rotating four small
/// sprites touches far fewer pixels than rotating a raster of the whole card.
#[derive(Clone, Debug)]
pub(crate) struct CardLabel {
    pub image: Image,
    pub shadow: Option<Image>,
    /// Sprite top-left relative to the dial centre on an unrotated card.
    pub origin: Point,
    /// Screen angle of the label on an unrotated card.
    pub angle: f32,
}

#[derive(Clone, Debug)]
pub struct HeadingCompassCache {
    pub(crate) config: ValidatedHeadingCompass,
    pub(crate) scene: ValidatedScenePresentation,
    pub x: f32,
    pub y: f32,
    pub layout: HeadingCompassLayout,
    pub center: Point,
    pub rotation_mode: CompassRotationMode,
    pub(crate) static_image: Image,
    pub(crate) static_origin: Point,
    /// Stable raster bounds: rotation never changes the surface size.
    pub(crate) dial_bounds: Rect,
    pub(crate) opacity: f32,
    pub(crate) shadow_offset: f32,
    /// Compass mode: the cardinal labels turned with the card each frame.
    pub(crate) card: Vec<CardLabel>,
    /// Indicator mode: the needle paint and its shadow paint. Paths are not
    /// shareable by export workers, so the four-vertex needle is rebuilt per frame.
    pub(crate) indicator: Option<(Paint, Option<Paint>)>,
    pub(crate) indicator_corner_radius: f32,
    pub(crate) indicator_size: f32,
}

pub fn prepare_heading_compass_cache(
    widget: &ValidatedHeadingCompass,
    scene: &ValidatedScenePresentation,
    fonts: &dyn FontResolver,
) -> WidgetResult<HeadingCompassCache> {
    let scale = scene.scale;
    let thickness = widget.track_thickness * scale;
    let layout = heading_compass_layout(
        widget.diameter * scale,
        thickness,
        widget.cardinal_font_size * scale,
        widget.arc_angle,
    );
    let center = Point::new(
        widget.x + layout.outer_radius,
        widget.y + layout.outer_radius,
    );
    let shadow_sigma = scene.shadow_strength * scale;
    let shadow_offset = scene.shadow_distance * scale;
    let shadow_color = parse_color(&scene.shadow_color, 1.0);
    let shadow_padding = if shadow_sigma > 0.0 {
        shadow_sigma * 3.0 + shadow_offset
    } else {
        0.0
    };
    let indicator_size = widget.indicator_size * scale;
    // The needle's farthest tail corner bounds it at every rotation.
    let indicator_extent = indicator_vertices(indicator_size)
        .iter()
        .map(|point| point.length())
        .fold(0.0, f32::max);
    let font = resolve_styled_font(
        fonts,
        Some(&widget.font),
        widget.cardinal_font_size * scale,
        widget_font_weight(),
        false,
    )?;
    let labels = widget
        .show_cardinal_labels
        .then(|| cardinal_runs(&font, layout.label_radius))
        .transpose()?
        .unwrap_or_default();
    let border_padding = scene.border_thickness * scale * 0.5;
    let label_style = LabelPaints::new(widget, scene, scale);
    let card = labels
        .iter()
        .map(|(blob, origin, bounds, angle)| {
            label_style.sprite(
                blob,
                *origin,
                *bounds,
                *angle,
                border_padding + shadow_sigma * 3.0 + 2.0,
            )
        })
        .collect::<WidgetResult<Vec<_>>>()?;
    // The farthest sprite corner bounds the card at every rotation.
    let card_extent = card
        .iter()
        .map(|label| {
            let (width, height) = (label.image.width() as f32, label.image.height() as f32);
            [(0.0, 0.0), (width, 0.0), (0.0, height), (width, height)]
                .iter()
                .map(|(x, y)| Point::new(label.origin.x + x, label.origin.y + y).length())
                .fold(0.0, f32::max)
        })
        .fold(0.0, f32::max);
    let half = (layout.outer_radius.max(indicator_extent).max(card_extent)
        + border_padding
        + shadow_padding
        + 2.0)
        .ceil();

    let size = (half * 2.0) as u32;
    let mut static_surface = surface(size, size, SHAPE_COLOR_TYPE)?;
    let canvas = static_surface.canvas();
    canvas.clear(Color::TRANSPARENT);
    canvas.translate((half, half));
    let mut track_paint = Paint::default();
    track_paint.set_anti_alias(true);
    track_paint.set_color(parse_color(&widget.track_fill, 1.0));
    if let Some(track) = rounded_segment_path(
        ArcGaugeGeometry {
            center_x: 0.0,
            center_y: 0.0,
            inner_widget_center_x: 0.0,
            inner_widget_center_y: 0.0,
            radius: layout.inner_radius + thickness * 0.5,
            start_angle: layout.track_start_angle,
            sweep_angle: layout.track_sweep_angle,
        },
        thickness,
        widget.track_corner_radius * scale,
    ) {
        draw_with_shadow(
            canvas,
            &track,
            &track_paint,
            shadow_color,
            shadow_sigma,
            shadow_offset,
        );
    }
    let mut indicator_paint = Paint::default();
    indicator_paint.set_anti_alias(true);
    indicator_paint.set_color(parse_color(&widget.indicator_color, 1.0));
    let needle = indicator_path(indicator_size, widget.indicator_corner_radius);
    let (card, indicator) = match widget.rotation_mode {
        CompassRotationMode::Indicator => {
            // Fixed labels in the gap fade exactly as rotating ones would.
            draw_card(
                canvas,
                &card,
                &layout,
                Point::new(0.0, 0.0),
                0.0,
                shadow_offset,
                1.0,
            );
            let shadow = shadow_paint(&indicator_paint, shadow_color, shadow_sigma);
            (Vec::new(), Some((indicator_paint, shadow)))
        }
        CompassRotationMode::Compass => {
            draw_with_shadow(
                canvas,
                &needle,
                &indicator_paint,
                shadow_color,
                shadow_sigma,
                shadow_offset,
            );
            (card, None)
        }
    };
    let static_origin = Point::new(center.x - half, center.y - half);
    let mut dial_bounds = Rect::from_xywh(static_origin.x, static_origin.y, half * 2.0, half * 2.0);
    dial_bounds.join(Rect::from_xywh(
        center.x - layout.outer_radius,
        center.y - layout.outer_radius,
        layout.size(),
        layout.size(),
    ));
    Ok(HeadingCompassCache {
        config: widget.clone(),
        scene: scene.clone(),
        x: widget.x,
        y: widget.y,
        layout,
        center,
        rotation_mode: widget.rotation_mode,
        static_image: static_surface.image_snapshot(),
        static_origin,
        dial_bounds,
        opacity: widget.opacity * scene.opacity,
        shadow_offset,
        card,
        indicator,
        indicator_size,
        indicator_corner_radius: widget.indicator_corner_radius,
    })
}

impl HeadingCompassCache {
    pub(crate) fn draw_static(&self, canvas: &Canvas) {
        let mut paint = Paint::default();
        paint.set_alpha_f(self.opacity);
        canvas.draw_image(&self.static_image, self.static_origin, Some(&paint));
    }

    pub(crate) fn draw_card(&self, canvas: &Canvas, card_rotation: f32) {
        draw_card(
            canvas,
            &self.card,
            &self.layout,
            self.center,
            card_rotation,
            self.shadow_offset,
            self.opacity,
        );
    }

    pub(crate) fn draw_indicator(&self, canvas: &Canvas, heading: f32) {
        let Some((paint, shadow)) = &self.indicator else {
            return;
        };
        let path = &indicator_path(self.indicator_size, self.indicator_corner_radius);
        let mut paint = paint.clone();
        paint.set_alpha_f(self.opacity);
        if let Some(shadow) = shadow {
            let mut shadow = shadow.clone();
            shadow.set_alpha_f(shadow.alpha_f() * self.opacity);
            canvas.save();
            canvas.translate((
                self.center.x + self.shadow_offset,
                self.center.y + self.shadow_offset,
            ));
            canvas.rotate(heading, None);
            canvas.draw_path(path, &shadow);
            canvas.restore();
        }
        canvas.save();
        canvas.translate(self.center);
        canvas.rotate(heading, None);
        canvas.draw_path(path, &paint);
        canvas.restore();
    }
}

/// Turns each cached label about the dial centre; shadows keep their screen-space offset.
/// Cubic sampling keeps rotated glyph edges crisp; blurred shadows need only bilinear.
fn draw_card(
    canvas: &Canvas,
    card: &[CardLabel],
    layout: &HeadingCompassLayout,
    center: Point,
    card_rotation: f32,
    shadow_offset: f32,
    opacity: f32,
) {
    for label in card {
        let alpha = label_fade(layout, label.angle + card_rotation);
        if alpha == 0.0 {
            continue;
        }
        let mut paint = Paint::default();
        paint.set_alpha_f(alpha * opacity);
        let passes = [
            (
                label.shadow.as_ref(),
                shadow_offset,
                SamplingOptions::from(FilterMode::Linear),
            ),
            (
                Some(&label.image),
                0.0,
                SamplingOptions::from(CubicResampler::catmull_rom()),
            ),
        ];
        for (image, offset, sampling) in passes {
            let Some(image) = image else { continue };
            canvas.save();
            canvas.translate((center.x + offset, center.y + offset));
            canvas.rotate(card_rotation, None);
            canvas.draw_image_with_sampling_options(image, label.origin, sampling, Some(&paint));
            canvas.restore();
        }
    }
}

fn draw_with_shadow(
    canvas: &Canvas,
    path: &Path,
    paint: &Paint,
    shadow_color: Color,
    sigma: f32,
    offset: f32,
) {
    if let Some(shadow) = shadow_paint(paint, shadow_color, sigma) {
        canvas.save();
        canvas.translate((offset, offset));
        canvas.draw_path(path, &shadow);
        canvas.restore();
    }
    canvas.draw_path(path, paint);
}

/// Each upright cardinal blob, its origin relative to the dial centre, its ink bounds and screen angle.
fn cardinal_runs(font: &Font, radius: f32) -> WidgetResult<Vec<(TextBlob, Point, Rect, f32)>> {
    CARDINALS
        .iter()
        .map(|(bearing, text)| {
            let blob = TextBlob::from_str(text, font).ok_or_else(|| {
                WidgetError::Render("unable to lay out heading compass label".into())
            })?;
            let (_, bounds) = font.measure_str(text, None);
            let angle = bearing_screen_angle(*bearing, 0.0);
            let radians = angle.to_radians();
            // Centre the painted glyph, not its advance box, on the label circle.
            let origin = Point::new(
                radius * radians.cos() - bounds.center_x(),
                radius * radians.sin() - bounds.center_y(),
            );
            Ok((blob, origin, bounds, angle))
        })
        .collect()
}

struct LabelPaints {
    fill: Paint,
    border: Option<Paint>,
    shadow: Option<Paint>,
}

impl LabelPaints {
    fn new(
        widget: &ValidatedHeadingCompass,
        scene: &ValidatedScenePresentation,
        scale: f32,
    ) -> Self {
        let mut fill = Paint::default();
        fill.set_anti_alias(true);
        fill.set_color(parse_color(&widget.cardinal_label_color, 1.0));
        let border = (scene.border_thickness > 0.0).then(|| {
            let mut paint = fill.clone();
            paint.set_color(parse_color(&scene.border_color, 1.0));
            paint.set_style(Style::Stroke);
            paint.set_stroke_width(scene.border_thickness * scale);
            paint.set_stroke_join(Join::Round);
            paint
        });
        Self {
            shadow: shadow_paint(
                &fill,
                parse_color(&scene.shadow_color, 1.0),
                scene.shadow_strength * scale,
            ),
            border,
            fill,
        }
    }

    /// Rasterizes one label and, when shadows are enabled, its unshifted shadow.
    fn sprite(
        &self,
        blob: &TextBlob,
        origin: Point,
        ink: Rect,
        angle: f32,
        padding: f32,
    ) -> WidgetResult<CardLabel> {
        let mut bounds = ink;
        bounds.offset(origin);
        bounds.outset((padding, padding));
        let left = bounds.left.floor();
        let top = bounds.top.floor();
        let width = (bounds.right.ceil() - left) as u32;
        let height = (bounds.bottom.ceil() - top) as u32;
        let raster = |paints: &[&Paint]| -> WidgetResult<Image> {
            let mut sprite = surface(width, height, SHAPE_COLOR_TYPE)?;
            let canvas = sprite.canvas();
            canvas.clear(Color::TRANSPARENT);
            for paint in paints {
                canvas.draw_text_blob(blob, origin - Point::new(left, top), paint);
            }
            Ok(sprite.image_snapshot())
        };
        let foreground: Vec<&Paint> = [self.border.as_ref(), Some(&self.fill)]
            .into_iter()
            .flatten()
            .collect();
        Ok(CardLabel {
            image: raster(&foreground)?,
            shadow: self
                .shadow
                .as_ref()
                .map(|shadow| raster(&[shadow]))
                .transpose()?,
            origin: Point::new(left, top),
            angle,
        })
    }
}
