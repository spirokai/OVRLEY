//! Circular compass dial with a heading readout.
use crate::activity::ActivitySampler;
use crate::model::catalog::{catalog, widget_font_weight, DisplayType, MetricKind};
use crate::model::scene::{
    validate_scene_presentation, Numerals, ScenePresentationConfig, ValidatedScenePresentation,
};
use crate::model::validation::require_non_negative_f32;
use crate::model::validation::{
    require_bool, require_f32, require_hex_color, require_percentage, require_positive_f32,
    require_string, require_unit_opacity,
};
use crate::preview::ResizeGesture;
use crate::preview::WidgetHost;
use crate::render::text::LINE_HEIGHT_RATIO;
use crate::render::text::{
    draw_text_blob, layout_text, measure_text_with_font, parse_color, resolve_styled_font,
    shadow_paint, text_selection_bounds, FontResolver, MeasuredText, ResolvedTextStyle,
};
use crate::render::tile::{surface, TileBounds, SHAPE_COLOR_TYPE};
use crate::render::tile::{Presented, Tile};
use crate::render::tile::{PreviewRasterScale, StaticRaster};
use crate::widgets::gauges::arc::ArcGaugeGeometry;
use crate::widgets::gauges::arc::{MAX_ARC_ANGLE_DEGREES, MIN_ARC_ANGLE_DEGREES};
use crate::widgets::gauges::track::rounded_segment_path;
use crate::widgets::value::layout::{
    metric_vertical_metrics_text, METRIC_WIDGET_UNITS_GAP_PX, METRIC_WIDGET_UNIT_RATIO,
    MIN_UNITS_FONT_SIZE,
};
use crate::widgets::value::ValueConfig;
use crate::{WidgetError, WidgetResult};
use serde::Serialize;
use skia_safe::paint::{Join, Style};
use skia_safe::{
    Canvas, Color, CubicResampler, FilterMode, Font, Image, Paint, Path, PathBuilder, Point, Rect,
    SamplingOptions, TextBlob,
};

/// Which part of the dial follows the heading; the other part stays fixed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompassRotationMode {
    /// The needle turns to the heading over a fixed north-up card.
    Indicator,
    /// The card turns so the heading sits under a fixed up-pointing needle.
    Compass,
}

#[derive(Clone, Debug)]
pub struct ValidatedHeadingCompass {
    pub x: f32,
    pub y: f32,
    pub diameter: f32,
    pub opacity: f32,
    pub rotation_mode: CompassRotationMode,
    pub arc_angle: f32,
    pub track_thickness: f32,
    pub track_corner_radius: f32,
    pub track_fill: String,
    pub indicator_size: f32,
    pub indicator_corner_radius: f32,
    pub indicator_color: String,
    pub show_cardinal_labels: bool,
    pub cardinal_font_size: f32,
    pub cardinal_label_color: String,
    pub font: String,
    pub show_value: bool,
    pub font_size: f32,
    pub color: String,
    pub unit_color: String,
    pub show_units: bool,
    pub value_offset_x: f32,
    pub value_offset_y: f32,
}

pub fn validate_heading_compass(
    value: ValueConfig,
    index: usize,
) -> WidgetResult<ValidatedHeadingCompass> {
    let p = |field: &str| format!("values[{index}].{field}");
    if value.value != MetricKind::Heading || value.display_type != DisplayType::HeadingCompass {
        return Err(WidgetError::Config(format!(
            "values[{index}]: expected heading/heading_compass"
        )));
    }
    if value.width.is_some() || value.height.is_some() {
        return Err(WidgetError::Config(format!(
            "{} and {}: must be absent for diameter-based geometry",
            p("width"),
            p("height")
        )));
    }
    let rotation_mode = match require_string(value.rotation_mode, &p("rotation_mode"))?.as_str() {
        "indicator" => CompassRotationMode::Indicator,
        "compass" => CompassRotationMode::Compass,
        other => {
            return Err(WidgetError::Config(format!(
                "{}: invalid value '{other}'",
                p("rotation_mode")
            )))
        }
    };
    let arc_angle = require_f32(value.arc_angle, &p("arc_angle"))?;
    if !(MIN_ARC_ANGLE_DEGREES..=MAX_ARC_ANGLE_DEGREES).contains(&arc_angle) {
        return Err(WidgetError::Config(format!(
            "{}: must be {MIN_ARC_ANGLE_DEGREES}..={MAX_ARC_ANGLE_DEGREES}, got {arc_angle}",
            p("arc_angle")
        )));
    }
    let track_corner_radius =
        require_non_negative_f32(value.track_corner_radius, &p("track_corner_radius"))?;
    let font = require_string(value.font, &p("font"))?;
    if font.trim().is_empty() {
        return Err(WidgetError::Config(format!("{}: required", p("font"))));
    }
    let color = |raw: Option<String>, field: &str| require_hex_color(raw.as_deref(), &p(field));
    Ok(ValidatedHeadingCompass {
        x: require_f32(Some(value.x), &p("x"))?,
        y: require_f32(Some(value.y), &p("y"))?,
        diameter: require_positive_f32(value.diameter, &p("diameter"))?,
        opacity: require_unit_opacity(value.opacity, &p("opacity"))?,
        rotation_mode,
        arc_angle,
        track_thickness: require_positive_f32(value.track_thickness, &p("track_thickness"))?,
        track_corner_radius,
        track_fill: color(value.track_fill, "track_fill")?,
        indicator_size: require_positive_f32(value.indicator_size, &p("indicator_size"))?,
        indicator_corner_radius: require_percentage(
            value.indicator_corner_radius,
            &p("indicator_corner_radius"),
        )?,
        indicator_color: color(value.indicator_color, "indicator_color")?,
        show_cardinal_labels: require_bool(value.show_cardinal_labels, &p("show_cardinal_labels"))?,
        cardinal_font_size: require_positive_f32(
            value.cardinal_font_size,
            &p("cardinal_font_size"),
        )?,
        cardinal_label_color: color(value.cardinal_label_color, "cardinal_label_color")?,
        font,
        show_value: require_bool(value.show_value, &p("show_value"))?,
        font_size: require_positive_f32(value.font_size, &p("font_size"))?,
        color: color(value.color, "color")?,
        unit_color: color(value.unit_color, "unit_color")?,
        show_units: require_bool(value.show_units, &p("show_units"))?,
        value_offset_x: require_f32(value.value_offset_x, &p("value_offset_x"))?,
        value_offset_y: require_f32(value.value_offset_y, &p("value_offset_y"))?,
    })
}

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
    /// Track, fixed labels or needle around the compass center.
    pub(crate) static_layer: StaticRaster,
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
    raster_scale: PreviewRasterScale,
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

    let mut indicator_paint = Paint::default();
    indicator_paint.set_anti_alias(true);
    indicator_paint.set_color(parse_color(&widget.indicator_color, 1.0));
    // Dial-local coordinates: the compass center is the origin.
    let static_layer = StaticRaster::new(
        raster_scale,
        center,
        Rect::new(-half, -half, half, half),
        SHAPE_COLOR_TYPE,
        |canvas| {
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
            match widget.rotation_mode {
                // Fixed labels in the gap fade exactly as rotating ones would.
                CompassRotationMode::Indicator => draw_card(
                    canvas,
                    &card,
                    &layout,
                    Point::new(0.0, 0.0),
                    0.0,
                    shadow_offset,
                    1.0,
                ),
                CompassRotationMode::Compass => draw_with_shadow(
                    canvas,
                    &indicator_path(indicator_size, widget.indicator_corner_radius),
                    &indicator_paint,
                    shadow_color,
                    shadow_sigma,
                    shadow_offset,
                ),
            }
            Ok(())
        },
    )?;
    let (card, indicator) = match widget.rotation_mode {
        CompassRotationMode::Indicator => {
            let shadow = shadow_paint(&indicator_paint, shadow_color, shadow_sigma);
            (Vec::new(), Some((indicator_paint, shadow)))
        }
        CompassRotationMode::Compass => (card, None),
    };
    let mut dial_bounds = static_layer.bounds().with_offset(center);
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
        static_layer,
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
        canvas.save();
        canvas.translate(self.center);
        self.static_layer.draw(canvas, Some(&paint));
        canvas.restore();
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

/// Whole degrees in 0..360; a missing heading shows a placeholder.
pub fn format_heading_compass_value(raw: Option<f64>) -> String {
    raw.map_or_else(
        || "--".into(),
        |heading| (heading.round() as i64).rem_euclid(360).to_string(),
    )
}

struct TextRun {
    blob: TextBlob,
    baseline: f32,
    style: ResolvedTextStyle,
}

pub struct HeadingCompassFrame {
    heading: f32,
    value: Option<TextRun>,
    unit: Option<TextRun>,
    pub bounds: Rect,
    pub content_bounds: Rect,
    pub content_width: f32,
    pub marker: Point,
}

impl HeadingCompassFrame {
    pub fn draw(&self, canvas: &Canvas, cache: &HeadingCompassCache) {
        cache.draw_static(canvas);
        match cache.rotation_mode {
            CompassRotationMode::Indicator => cache.draw_indicator(canvas, self.heading),
            CompassRotationMode::Compass => cache.draw_card(canvas, -self.heading),
        }
        for run in [&self.value, &self.unit].into_iter().flatten() {
            draw_text_blob(canvas, &run.blob, run.baseline, &run.style);
        }
    }
}

impl HeadingCompassCache {
    pub fn prepare_frame(
        &self,
        raw: Option<f64>,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<HeadingCompassFrame> {
        let widget = &self.config;
        let scene = &self.scene;
        let scale = scene.scale;
        // Missing external heading deliberately displays the north-up dial.
        let heading = raw.unwrap_or(0.0) as f32;
        let needle_angle = match self.rotation_mode {
            CompassRotationMode::Indicator => heading,
            CompassRotationMode::Compass => 0.0,
        };
        let marker_angle = (needle_angle - 90.0).to_radians();
        let marker = Point::new(
            self.center.x + self.indicator_size * 0.5 * marker_angle.cos(),
            self.center.y + self.indicator_size * 0.5 * marker_angle.sin(),
        );
        let size = self.layout.size();
        let mut bounds = self.dial_bounds;
        let mut content_bounds = Rect::from_xywh(self.x, self.y, size, size);
        let (value, unit) = if widget.show_value {
            let opacity = self.opacity;
            let font_size = widget.font_size * scale;
            let mut value_style = ResolvedTextStyle::widget(
                scene,
                scale,
                Some(widget.font.clone()),
                widget.font_size,
                parse_color(&widget.color, opacity),
                opacity,
            );
            let value_text = format_heading_compass_value(raw);
            let value_font = resolve_styled_font(
                fonts,
                Some(&widget.font),
                font_size,
                widget_font_weight(),
                false,
            )?;
            let value = layout_text(&value_text, &value_font, 0.0, value_style.numerals);
            let value_vertical = measure_text_with_font(
                metric_vertical_metrics_text(&value_text),
                &value_font,
                Numerals::Default,
            );
            let unit = if raw.is_some() && widget.show_units {
                let unit_size =
                    (font_size * METRIC_WIDGET_UNIT_RATIO).max(MIN_UNITS_FONT_SIZE * scale);
                let font = resolve_styled_font(
                    fonts,
                    Some(&widget.font),
                    unit_size,
                    widget_font_weight(),
                    false,
                )?;
                let text = layout_text(
                    catalog().unit_label(MetricKind::Heading, None),
                    &font,
                    0.0,
                    Numerals::Default,
                );
                let style = ResolvedTextStyle {
                    font_size: unit_size,
                    line_height: unit_size * LINE_HEIGHT_RATIO,
                    color: parse_color(&widget.unit_color, opacity),
                    ..value_style.clone()
                };
                Some((text, style))
            } else {
                None
            };
            // The superscript degree sits half the usual unit gap after the digits.
            let unit_x = value.measurement.width + METRIC_WIDGET_UNITS_GAP_PX * scale * 0.5;
            // The value's leading ink edge starts at the gap anchor; digits set the vertical centre.
            let anchor = self.center
                + readout_anchor(&self.layout)
                + Point::new(widget.value_offset_x * scale, widget.value_offset_y * scale);
            value_style.x = anchor.x - value.measurement.bounds_left;
            let value_baseline =
                anchor.y - (value_vertical.bounds_top + value_vertical.bounds_bottom) * 0.5;
            let digit_top = value_baseline + value_vertical.bounds_top;
            let shadow_padding = if scene.shadow_strength > 0.0 {
                (scene.shadow_strength * 3.0 + scene.shadow_distance) * scale
            } else {
                0.0
            };
            let border = value_style.border_thickness * 0.5;
            let mut text_bounds = |ink: &MeasuredText, x: f32, baseline: f32| {
                let mut painted = Rect::new(
                    x + ink.bounds_left,
                    baseline + ink.bounds_top,
                    x + ink.bounds_right,
                    baseline + ink.bounds_bottom,
                );
                painted.outset((border + shadow_padding + 2.0, border + shadow_padding + 2.0));
                bounds.join(painted);
                let mut selection = text_selection_bounds(ink, x, baseline);
                selection.outset((border, border));
                content_bounds.join(selection);
            };
            text_bounds(&value.measurement, value_style.x, value_baseline);
            let unit = unit
                .map(|(text, mut style)| -> WidgetResult<TextRun> {
                    style.x = value_style.x + unit_x;
                    // Superscript: the degree's ink top meets the digits' ink top.
                    let baseline = digit_top - text.measurement.bounds_top;
                    text_bounds(&text.measurement, style.x, baseline);
                    Ok(TextRun {
                        blob: text.blob.ok_or_else(|| {
                            WidgetError::Render("unable to lay out heading compass unit".into())
                        })?,
                        baseline,
                        style,
                    })
                })
                .transpose()?;
            let value = TextRun {
                blob: value.blob.ok_or_else(|| {
                    WidgetError::Render("unable to lay out heading compass value".into())
                })?,
                baseline: value_baseline,
                style: value_style,
            };
            (Some(value), unit)
        } else {
            (None, None)
        };
        Ok(HeadingCompassFrame {
            heading,
            value,
            unit,
            bounds: Rect::new(
                bounds.left.floor(),
                bounds.top.floor(),
                bounds.right.ceil(),
                bounds.bottom.ceil(),
            ),
            content_bounds,
            content_width: size,
            marker,
        })
    }
}

#[derive(Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub display_variants: ResizeVariants,
}

#[derive(Serialize)]
pub struct ResizeVariants {
    pub heading_compass: HeadingCompassResizeVariant,
}

/// Every pixel-sized field scales; the relative needle corner radius needs no change.
#[derive(Serialize)]
pub struct HeadingCompassResizeVariant {
    pub diameter: f32,
    pub track_thickness: f32,
    pub track_corner_radius: f32,
    pub indicator_size: f32,
    pub cardinal_font_size: f32,
    pub value_offset_x: f32,
    pub value_offset_y: f32,
}

pub fn resize_heading_compass(
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
    let mut config = validate_heading_compass(widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    config.diameter = (config.diameter * factor).clamp(8.0, 10_000.0);
    config.font_size = (config.font_size * factor).clamp(6.0, 400.0);
    config.cardinal_font_size = (config.cardinal_font_size * factor).clamp(4.0, 400.0);
    config.track_thickness = (config.track_thickness * factor).clamp(1.0, 10_000.0);
    config.track_corner_radius = (config.track_corner_radius * factor).clamp(0.0, 10_000.0);
    config.indicator_size = (config.indicator_size * factor).clamp(1.0, 10_000.0);
    config.value_offset_x = (config.value_offset_x * factor).clamp(-10_000.0, 10_000.0);
    config.value_offset_y = (config.value_offset_y * factor).clamp(-10_000.0, 10_000.0);
    let raw = activity.and_then(|activity| activity.heading_at(second));
    let bounds = prepare_heading_compass_cache(&config, &scene, fonts, PreviewRasterScale::NATIVE)?
        .prepare_frame(raw, fonts)?
        .content_bounds;
    Ok(ResizeUpdate {
        x: config.x + point[0] - (bounds.left + bounds.width() * fixed[0]),
        y: config.y + point[1] - (bounds.top + bounds.height() * fixed[1]),
        font_size: config.font_size,
        display_variants: ResizeVariants {
            heading_compass: HeadingCompassResizeVariant {
                diameter: config.diameter,
                track_thickness: config.track_thickness,
                track_corner_radius: config.track_corner_radius,
                indicator_size: config.indicator_size,
                cardinal_font_size: config.cardinal_font_size,
                value_offset_x: config.value_offset_x,
                value_offset_y: config.value_offset_y,
            },
        },
    })
}

pub struct PreviewHeadingCompass {
    cache: HeadingCompassCache,
    preview_raster_scale: PreviewRasterScale,
    last_sample: Option<Option<f64>>,
    pub tile: Tile,
}

impl PreviewHeadingCompass {
    pub fn new(
        widget: ValueConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let widget = validate_heading_compass(widget, 0)?;
        let host = WidgetHost::from_scene(&scene, raster_scale)?;
        Self::from_validated(&widget, &validate_scene_presentation(scene)?, &host, fonts)
    }

    pub fn from_validated(
        widget: &ValidatedHeadingCompass,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        Ok(Self {
            cache: prepare_heading_compass_cache(widget, scene, fonts, host.raster_scale)?,
            preview_raster_scale: host.raster_scale,
            last_sample: None,
            tile: Tile::default(),
        })
    }

    /// The sample the frame at `second` draws; equal samples draw identical pixels.
    pub fn frame_key(&self, activity: Option<&ActivitySampler>, second: f64) -> Option<f64> {
        activity.and_then(|activity| activity.heading_at(second))
    }

    /// Scene rectangle a render at `second` draws into, from readout text layout alone.
    pub fn layout_bounds(
        &self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let raw = activity.and_then(|activity| activity.heading_at(second));
        Ok(self.cache.prepare_frame(raw, fonts)?.bounds.into())
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(
        &self,
        canvas: &Canvas,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let raw = activity.and_then(|activity| activity.heading_at(second));
        let frame = self.cache.prepare_frame(raw, fonts)?;
        frame.draw(canvas, &self.cache);
        Ok(frame.bounds.into())
    }

    pub fn render(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<()> {
        let raw = activity.and_then(|activity| activity.heading_at(second));
        // The dial turns continuously, even between integer-degree readouts.
        if self.last_sample == Some(raw) {
            return Ok(());
        }
        let frame = self.cache.prepare_frame(raw, fonts)?;
        let bounds = frame.bounds;
        self.tile.render(
            self.preview_raster_scale,
            SHAPE_COLOR_TYPE,
            bounds,
            Presented::new(frame.content_bounds, frame.content_width),
            |canvas| {
                frame.draw(canvas, &self.cache);
                Ok(())
            },
        )?;
        self.last_sample = Some(raw);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        bearing_screen_angle, format_heading_compass_value, heading_compass_layout, indicator_path,
        indicator_vertices, label_fade, readout_anchor, GAP_CENTER_ANGLE,
    };

    #[test]
    fn heading_compass_readout_wraps_whole_degrees_and_marks_missing_samples() {
        assert_eq!(format_heading_compass_value(Some(23.4)), "23");
        assert_eq!(format_heading_compass_value(Some(359.6)), "0");
        assert_eq!(format_heading_compass_value(Some(-10.0)), "350");
        assert_eq!(format_heading_compass_value(None), "--");
    }

    #[test]
    fn heading_compass_track_grows_outwards_and_opens_around_the_readout() {
        let thin = heading_compass_layout(240.0, 4.0, 20.0, 270.0);
        let thick = heading_compass_layout(240.0, 30.0, 20.0, 270.0);

        assert_eq!(thin.inner_radius, thick.inner_radius);
        assert_eq!(thin.label_radius, thick.label_radius);
        assert_eq!(thick.outer_radius, 150.0);
        assert_eq!(thick.size(), 300.0);
        assert_eq!(thick.track_sweep_angle, 270.0);
        // The 90° gap is centred on the readout angle.
        assert_eq!(thick.track_start_angle, GAP_CENTER_ANGLE + 45.0);
        assert_eq!(
            (thick.track_start_angle + thick.track_sweep_angle) % 360.0,
            GAP_CENTER_ANGLE - 45.0
        );
    }

    #[test]
    fn heading_compass_bearings_map_north_up_and_follow_the_card() {
        assert_eq!(bearing_screen_angle(0.0, 0.0), -90.0);
        assert_eq!(bearing_screen_angle(90.0, 0.0), 0.0);
        // Turning the card by -heading brings that heading to the top.
        assert_eq!(bearing_screen_angle(120.0, -120.0), -90.0);
    }

    #[test]
    fn heading_compass_needle_rounding_stays_inside_its_sharp_outline() {
        let sharp = indicator_path(80.0, 0.0).compute_tight_bounds();
        let round = indicator_path(80.0, 100.0).compute_tight_bounds();
        let tip = indicator_vertices(80.0)[0];

        assert_eq!(sharp.top, tip.y);
        assert!(round.top > sharp.top);
        assert!(round.left >= sharp.left && round.right <= sharp.right);
        assert!(round.bottom <= sharp.bottom);
    }

    #[test]
    fn heading_compass_labels_fade_across_the_gap_and_stay_opaque_over_the_track() {
        // 270° track: the 90° gap spans screen angles 0°..90°, fading over 25° from each end.
        let layout = heading_compass_layout(240.0, 10.0, 20.0, 270.0);

        assert_eq!(label_fade(&layout, -90.0), 1.0);
        assert_eq!(label_fade(&layout, 0.0), 1.0);
        assert_eq!(label_fade(&layout, 90.0), 1.0);
        assert_eq!(label_fade(&layout, 45.0), 0.0);
        assert!((label_fade(&layout, 12.5) - 0.5).abs() < 1e-5);
        assert!((label_fade(&layout, 360.0 + 77.5) - 0.5).abs() < 1e-5);
        assert_eq!(
            label_fade(&heading_compass_layout(240.0, 10.0, 20.0, 360.0), 45.0),
            1.0
        );
    }

    #[test]
    fn heading_compass_readout_starts_at_the_gap_opening() {
        // 270° track ends at 0° and 90° on the 125px centre line: the opening's midpoint.
        let layout = heading_compass_layout(240.0, 10.0, 20.0, 270.0);
        let anchor = readout_anchor(&layout);
        assert!((anchor.x - 62.5).abs() < 1e-3 && (anchor.y - 62.5).abs() < 1e-3);
        // The opening's midpoint stays on the gap's bisector for any track angle.
        let narrow = readout_anchor(&heading_compass_layout(240.0, 10.0, 20.0, 340.0));
        assert!((narrow.y.atan2(narrow.x).to_degrees() - GAP_CENTER_ANGLE).abs() < 1e-3);
    }
}
