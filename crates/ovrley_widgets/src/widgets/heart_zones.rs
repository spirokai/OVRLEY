//! Five-zone heart-rate track with a moving indicator.
use crate::activity::ActivitySampler;
use crate::model::catalog::{heart_zone_model, DisplayType, MetricKind};
use crate::model::scene::{
    validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
};
use crate::model::validation::{
    require_bool, require_f32, require_hex_color, require_non_negative_f32, require_positive_f32,
    require_string, require_unit_opacity,
};
use crate::preview::ResizeGesture;
use crate::preview::WidgetHost;
use crate::render::text::{parse_color, resolve_styled_font, shadow_paint, FontResolver};
use crate::render::tile::{surface, TileBounds, SHAPE_COLOR_TYPE};
use crate::render::tile::{Presented, Tile};
use crate::render::tile::{PreviewRasterScale, StaticRaster};
use crate::widgets::value::ValueConfig;
use crate::{WidgetError, WidgetResult};
use serde::{Deserialize, Serialize};
use skia_safe::{
    Canvas, Color, FilterMode, Image, Paint, Path, PathBuilder, Point, RRect, Rect,
    SamplingOptions, TextBlob, Vector,
};

pub const ZONE_COUNT: usize = 5;

/// The `heart_zones` display's `zoneModel`, shared with the editor's manual-zone seeding.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartZoneModel {
    /// Zone boundaries as shares of LTHR. Coggan's Z1 and Z5 are open-ended, so the
    /// manifest also fixes where the track starts and ends.
    pub lthr_boundaries: [f32; ZONE_COUNT + 1],
    /// Max-HR mode estimates LTHR as this share of maximum heart rate.
    pub lthr_share_of_max_heartrate: f32,
}

/// One zone's heart-rate span in bpm.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
pub struct HeartZoneRange {
    pub from: f32,
    pub to: f32,
}

pub fn coggan_zones(lthr: f32) -> [HeartZoneRange; ZONE_COUNT] {
    let boundaries = &heart_zone_model().lthr_boundaries;
    std::array::from_fn(|index| HeartZoneRange {
        from: lthr * boundaries[index],
        to: lthr * boundaries[index + 1],
    })
}

/// LTHR estimated from maximum heart rate.
pub fn lthr_from_max_heartrate(max_heartrate: f32) -> f32 {
    max_heartrate * heart_zone_model().lthr_share_of_max_heartrate
}

/// Active zone and the sample's 0..=1 progress across it. Samples between manual
/// zones belong to the lower one; samples outside the track clamp to its ends.
pub fn zone_position(zones: &[HeartZoneRange; ZONE_COUNT], heartrate: f32) -> (usize, f32) {
    let index = zones
        .iter()
        .rposition(|zone| heartrate >= zone.from)
        .unwrap_or(0);
    let zone = zones[index];
    (
        index,
        ((heartrate - zone.from) / (zone.to - zone.from)).clamp(0.0, 1.0),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeartZonesIndicatorStyle {
    /// A vertical line across the active zone.
    Line,
    /// Two triangles above and below the track, pointing at it.
    Arrow,
}

#[derive(Clone, Debug)]
pub struct ValidatedHeartZones {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub opacity: f32,
    pub zones: [HeartZoneRange; ZONE_COUNT],
    pub zone_colors: [String; ZONE_COUNT],
    pub zone_gap: f32,
    pub track_corner_radius: f32,
    pub indicator_style: HeartZonesIndicatorStyle,
    pub indicator_size: f32,
    pub indicator_color: String,
    pub show_label: bool,
    pub font: String,
    pub label_font_weight: f32,
    pub label_color: String,
}

pub fn validate_heart_zones(value: ValueConfig, index: usize) -> WidgetResult<ValidatedHeartZones> {
    let p = |field: &str| format!("values[{index}].{field}");
    if value.value != MetricKind::Heartrate || value.display_type != DisplayType::HeartZones {
        return Err(WidgetError::Config(format!(
            "values[{index}]: expected heartrate/heart_zones"
        )));
    }
    let width = require_positive_f32(value.width, &p("width"))?;
    let zone_gap = require_non_negative_f32(value.zone_gap, &p("zone_gap"))?;
    if zone_gap * (ZONE_COUNT - 1) as f32 >= width {
        return Err(WidgetError::Config(format!(
            "{}: {} gaps must be narrower than width {width}",
            p("zone_gap"),
            ZONE_COUNT - 1
        )));
    }
    let zones = match require_string(value.zone_calculation, &p("zone_calculation"))?.as_str() {
        "lthr" => coggan_zones(require_positive_f32(value.lthr, &p("lthr"))?),
        "max_heartrate" => coggan_zones(lthr_from_max_heartrate(require_positive_f32(
            value.max_heartrate,
            &p("max_heartrate"),
        )?)),
        "manual" => validate_manual_zones(value.manual_zones, &p("manual_zones"))?,
        other => {
            return Err(WidgetError::Config(format!(
                "{}: invalid value '{other}'",
                p("zone_calculation")
            )))
        }
    };
    let zone_colors = value
        .zone_colors
        .ok_or_else(|| WidgetError::Config(format!("{}: required", p("zone_colors"))))?;
    let zone_colors: [String; ZONE_COUNT] = zone_colors
        .iter()
        .enumerate()
        .map(|(i, color)| require_hex_color(Some(color), &p(&format!("zone_colors[{i}]"))))
        .collect::<WidgetResult<Vec<_>>>()?
        .try_into()
        .map_err(|_| {
            WidgetError::Config(format!(
                "{}: expected {ZONE_COUNT} colors",
                p("zone_colors")
            ))
        })?;
    let indicator_style =
        match require_string(value.indicator_style, &p("indicator_style"))?.as_str() {
            "line" => HeartZonesIndicatorStyle::Line,
            "arrow" => HeartZonesIndicatorStyle::Arrow,
            other => {
                return Err(WidgetError::Config(format!(
                    "{}: invalid value '{other}'",
                    p("indicator_style")
                )))
            }
        };
    let font = require_string(value.font, &p("font"))?;
    if font.trim().is_empty() {
        return Err(WidgetError::Config(format!("{}: required", p("font"))));
    }
    let color = |raw: Option<String>, field: &str| require_hex_color(raw.as_deref(), &p(field));
    Ok(ValidatedHeartZones {
        x: require_f32(Some(value.x), &p("x"))?,
        y: require_f32(Some(value.y), &p("y"))?,
        width,
        height: require_positive_f32(value.height, &p("height"))?,
        opacity: require_unit_opacity(value.opacity, &p("opacity"))?,
        zones,
        zone_colors,
        zone_gap,
        track_corner_radius: require_non_negative_f32(
            value.track_corner_radius,
            &p("track_corner_radius"),
        )?,
        indicator_style,
        indicator_size: require_positive_f32(value.indicator_size, &p("indicator_size"))?,
        indicator_color: color(value.indicator_color, "indicator_color")?,
        show_label: require_bool(value.show_label, &p("show_label"))?,
        font,
        label_font_weight: require_positive_f32(value.label_font_weight, &p("label_font_weight"))?,
        label_color: color(value.label_color, "label_color")?,
    })
}

/// Manual zones must be five ascending, non-overlapping spans.
fn validate_manual_zones(
    zones: Option<Vec<HeartZoneRange>>,
    field: &str,
) -> WidgetResult<[HeartZoneRange; ZONE_COUNT]> {
    let zones: [HeartZoneRange; ZONE_COUNT] = zones
        .ok_or_else(|| WidgetError::Config(format!("{field}: required")))?
        .try_into()
        .map_err(|_| WidgetError::Config(format!("{field}: expected {ZONE_COUNT} zones")))?;
    for (index, zone) in zones.iter().enumerate() {
        if !zone.from.is_finite() || !zone.to.is_finite() || zone.from < 0.0 || zone.from >= zone.to
        {
            return Err(WidgetError::Config(format!(
                "{field}[{index}]: requires 0 <= from < to, got {}..{}",
                zone.from, zone.to
            )));
        }
        if index > 0 && zone.from < zones[index - 1].to {
            return Err(WidgetError::Config(format!(
                "{field}[{index}]: must start at or above the previous zone's end {}",
                zones[index - 1].to
            )));
        }
    }
    Ok(zones)
}

/// The active zone grows by this share of the track height above and below; the gaps stay intact.
pub const ACTIVE_ZONE_GROWTH: f32 = 0.08;
/// The active zone brightens and saturates its configured color by these HSV factors.
pub const ACTIVE_ZONE_BRIGHTNESS: f32 = 1.7;
pub const ACTIVE_ZONE_SATURATION: f32 = 1.7;
/// Inactive zones wash out their configured color by this HSV saturation factor.
pub const INACTIVE_ZONE_SATURATION: f32 = 0.8;
/// The "Z1".."Z5" label's font size relative to the track height.
const LABEL_HEIGHT_RATIO: f32 = 0.546;
/// Line indicator width and its vertical inset from the active zone, relative to the track height.
const LINE_WIDTH_RATIO: f32 = 0.06;
const LINE_INSET_RATIO: f32 = 0.18;
/// Arrow depth relative to its base width (`indicator_size`).
const ARROW_DEPTH_RATIO: f32 = 0.75;

/// Five equal-width zones; the corner radius rounds only the track's outermost corners.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeartZonesLayout {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
    pub zone_width: f32,
    pub gap: f32,
    pub grow: f32,
    pub corner_radius: f32,
}

pub fn heart_zones_layout(
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    gap: f32,
    corner_radius: f32,
) -> HeartZonesLayout {
    let zone_width = (width - gap * (ZONE_COUNT - 1) as f32) / ZONE_COUNT as f32;
    HeartZonesLayout {
        left,
        top,
        width,
        height,
        zone_width,
        gap,
        grow: height * ACTIVE_ZONE_GROWTH,
        // The radius is capped at half the zone's smallest dimension.
        corner_radius: corner_radius.min(zone_width.min(height) * 0.5),
    }
}

impl HeartZonesLayout {
    pub fn zone_rect(&self, index: usize) -> Rect {
        Rect::from_xywh(
            self.left + index as f32 * (self.zone_width + self.gap),
            self.top,
            self.zone_width,
            self.height,
        )
    }

    pub fn active_rect(&self, index: usize) -> Rect {
        let mut rect = self.zone_rect(index);
        rect.outset((0.0, self.grow));
        rect
    }

    /// Rounds the left corners of the first zone and the right corners of the last.
    fn zone_rrect(&self, rect: Rect, index: usize) -> RRect {
        let left = if index == 0 { self.corner_radius } else { 0.0 };
        let right = if index == ZONE_COUNT - 1 {
            self.corner_radius
        } else {
            0.0
        };
        let radii = [
            Vector::new(left, left),
            Vector::new(right, right),
            Vector::new(right, right),
            Vector::new(left, left),
        ];
        RRect::new_rect_radii(rect, &radii)
    }

    /// Scene x of a sample at `progress` across zone `index`.
    pub fn indicator_x(&self, index: usize, progress: f32) -> f32 {
        self.zone_rect(index).left + progress * self.zone_width
    }
}

/// Indicator outline around x = 0 in scene y coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IndicatorShape {
    pub style: HeartZonesIndicatorStyle,
    pub half_width: f32,
    /// Line: top/bottom of the line. Arrow: the tips touching the active zone.
    pub top: f32,
    pub bottom: f32,
    /// Arrow depth beyond its tip; unused by lines.
    pub depth: f32,
}

impl IndicatorShape {
    pub fn new(
        layout: &HeartZonesLayout,
        style: HeartZonesIndicatorStyle,
        indicator_size: f32,
    ) -> Self {
        let top = layout.top - layout.grow;
        let bottom = layout.top + layout.height + layout.grow;
        match style {
            HeartZonesIndicatorStyle::Line => {
                let inset = layout.height * LINE_INSET_RATIO;
                Self {
                    style,
                    half_width: (layout.height * LINE_WIDTH_RATIO).max(2.0) * 0.5,
                    top: top + inset,
                    bottom: bottom - inset,
                    depth: 0.0,
                }
            }
            HeartZonesIndicatorStyle::Arrow => Self {
                style,
                half_width: indicator_size * 0.5,
                top,
                bottom,
                depth: indicator_size * ARROW_DEPTH_RATIO,
            },
        }
    }

    /// Bounding box of the shape at x = 0.
    pub fn extent(&self) -> Rect {
        Rect::new(
            -self.half_width,
            self.top - self.depth,
            self.half_width,
            self.bottom + self.depth,
        )
    }

    pub fn path(&self) -> Path {
        let mut path = PathBuilder::new();
        match self.style {
            HeartZonesIndicatorStyle::Line => {
                let rect = Rect::new(-self.half_width, self.top, self.half_width, self.bottom);
                path.add_rrect(
                    RRect::new_rect_xy(rect, self.half_width, self.half_width),
                    None,
                    None,
                );
            }
            HeartZonesIndicatorStyle::Arrow => {
                for (tip, base) in [
                    (self.top, self.top - self.depth),
                    (self.bottom, self.bottom + self.depth),
                ] {
                    path.move_to((0.0, tip));
                    path.line_to((self.half_width, base));
                    path.line_to((-self.half_width, base));
                    path.close();
                }
            }
        }
        path.detach()
    }
}

#[derive(Clone, Debug)]
pub struct HeartZonesCache {
    pub(crate) layout: HeartZonesLayout,
    pub(crate) zones: [HeartZoneRange; ZONE_COUNT],
    /// Index 0 shows no active zone; index `i + 1` shows zone `i` active and labelled.
    pub(crate) states: Vec<StaticRaster>,
    pub(crate) indicator: IndicatorShape,
    pub(crate) indicator_paint: Paint,
    /// Pre-blurred indicator shadow and its top-left relative to the indicator x.
    pub(crate) indicator_shadow: Option<(Image, Point)>,
    /// Stable raster bounds: the indicator never moves outside them.
    pub(crate) bounds: Rect,
    pub(crate) content_bounds: Rect,
    pub(crate) opacity: f32,
}

pub fn prepare_heart_zones_cache(
    widget: &ValidatedHeartZones,
    scene: &ValidatedScenePresentation,
    fonts: &dyn FontResolver,
    raster_scale: PreviewRasterScale,
) -> WidgetResult<HeartZonesCache> {
    let scale = scene.scale;
    let layout = heart_zones_layout(
        widget.x,
        widget.y,
        widget.width * scale,
        widget.height * scale,
        widget.zone_gap * scale,
        widget.track_corner_radius * scale,
    );
    let indicator = IndicatorShape::new(
        &layout,
        widget.indicator_style,
        widget.indicator_size * scale,
    );
    let shadow_sigma = scene.shadow_strength * scale;
    let shadow_offset = scene.shadow_distance * scale;
    let shadow_color = parse_color(&scene.shadow_color, 1.0);
    let padding = if shadow_sigma > 0.0 {
        shadow_sigma * 3.0 + shadow_offset
    } else {
        0.0
    } + 2.0;

    let mut track = layout.active_rect(0);
    track.join(layout.active_rect(ZONE_COUNT - 1));
    track.outset((padding, padding));
    let font = resolve_styled_font(
        fonts,
        Some(&widget.font),
        layout.height * LABEL_HEIGHT_RATIO,
        widget.label_font_weight,
        false,
    )?;
    let mut label_paint = Paint::default();
    label_paint.set_anti_alias(true);
    label_paint.set_color(parse_color(&widget.label_color, 1.0));
    let states = (0..=ZONE_COUNT)
        .map(|state| {
            StaticRaster::new(
                raster_scale,
                Point::default(),
                track,
                SHAPE_COLOR_TYPE,
                |canvas| {
                    let active = state.checked_sub(1);
                    for index in (0..ZONE_COUNT).filter(|index| Some(*index) != active) {
                        let mut paint = Paint::default();
                        paint.set_anti_alias(true);
                        paint.set_color(scale_hsv(
                            parse_color(&widget.zone_colors[index], 1.0),
                            1.0,
                            INACTIVE_ZONE_SATURATION,
                        ));
                        let rrect = layout.zone_rrect(layout.zone_rect(index), index);
                        draw_with_shadow(
                            canvas,
                            &rrect,
                            &paint,
                            shadow_color,
                            shadow_sigma,
                            shadow_offset,
                        );
                    }
                    if let Some(index) = active {
                        let mut paint = Paint::default();
                        paint.set_anti_alias(true);
                        paint.set_color(scale_hsv(
                            parse_color(&widget.zone_colors[index], 1.0),
                            ACTIVE_ZONE_BRIGHTNESS,
                            ACTIVE_ZONE_SATURATION,
                        ));
                        let rect = layout.active_rect(index);
                        let rrect = layout.zone_rrect(rect, index);
                        draw_with_shadow(
                            canvas,
                            &rrect,
                            &paint,
                            shadow_color,
                            shadow_sigma,
                            shadow_offset,
                        );
                        if !widget.show_label {
                            return Ok(());
                        }
                        let text = format!("Z{}", index + 1);
                        let blob = TextBlob::from_str(&text, &font).ok_or_else(|| {
                            WidgetError::Render("unable to lay out heart zone label".into())
                        })?;
                        let (_, ink) = font.measure_str(&text, None);
                        // Centre the painted glyphs, not their advance box, in the zone.
                        canvas.draw_text_blob(
                            &blob,
                            (
                                rect.center_x() - ink.center_x(),
                                rect.center_y() - ink.center_y(),
                            ),
                            &label_paint,
                        );
                    }
                    Ok(())
                },
            )
        })
        .collect::<WidgetResult<Vec<_>>>()?;

    let mut indicator_paint = Paint::default();
    indicator_paint.set_anti_alias(true);
    indicator_paint.set_color(parse_color(&widget.indicator_color, 1.0));
    let extent = indicator.extent();
    let indicator_shadow = shadow_paint(&indicator_paint, shadow_color, shadow_sigma)
        .map(|shadow| -> WidgetResult<(Image, Point)> {
            let mut sprite = extent;
            sprite.offset((shadow_offset, shadow_offset));
            sprite.outset((shadow_sigma * 3.0 + 1.0, shadow_sigma * 3.0 + 1.0));
            // Whole-pixel vertical placement; horizontal placement follows the sample.
            let origin = Point::new(sprite.left, sprite.top.floor());
            let mut raster = surface(
                (sprite.right - origin.x).ceil() as u32,
                (sprite.bottom.ceil() - origin.y) as u32,
                SHAPE_COLOR_TYPE,
            )?;
            let canvas = raster.canvas();
            canvas.clear(Color::TRANSPARENT);
            canvas.translate((shadow_offset - origin.x, shadow_offset - origin.y));
            canvas.draw_path(&indicator.path(), &shadow);
            Ok((raster.image_snapshot(), origin))
        })
        .transpose()?;

    let mut bounds = states[0].bounds();
    let mut travel = Rect::new(
        layout.left + extent.left,
        extent.top,
        layout.left + layout.width + extent.right,
        extent.bottom,
    );
    travel.outset((padding, padding));
    bounds.join(travel);
    Ok(HeartZonesCache {
        layout,
        zones: widget.zones,
        states,
        indicator,
        indicator_paint,
        indicator_shadow,
        bounds: Rect::new(
            bounds.left.floor(),
            bounds.top.floor(),
            bounds.right.ceil(),
            bounds.bottom.ceil(),
        ),
        content_bounds: Rect::from_xywh(layout.left, layout.top, layout.width, layout.height),
        opacity: widget.opacity * scene.opacity,
    })
}

/// Scales HSV value and saturation, each capped at full; hue and alpha are unchanged.
pub fn scale_hsv(color: Color, value_factor: f32, saturation_factor: f32) -> Color {
    let peak = f32::from(color.r().max(color.g()).max(color.b()));
    if peak == 0.0 {
        return color;
    }
    let floor = f32::from(color.r().min(color.g()).min(color.b()));
    let value = (peak * value_factor).min(255.0);
    let saturation = ((peak - floor) / peak * saturation_factor).min(1.0);
    // Each channel keeps its relative position between the darkest and brightest channel.
    let channel = |raw: u8| {
        let depth = if peak > floor {
            (peak - f32::from(raw)) / (peak - floor)
        } else {
            0.0
        };
        (value * (1.0 - saturation * depth)).round() as u8
    };
    Color::from_argb(
        color.a(),
        channel(color.r()),
        channel(color.g()),
        channel(color.b()),
    )
}

fn draw_with_shadow(
    canvas: &Canvas,
    rrect: &RRect,
    paint: &Paint,
    shadow_color: Color,
    sigma: f32,
    offset: f32,
) {
    if let Some(shadow) = shadow_paint(paint, shadow_color, sigma) {
        canvas.save();
        canvas.translate((offset, offset));
        canvas.draw_rrect(rrect, &shadow);
        canvas.restore();
    }
    canvas.draw_rrect(rrect, paint);
}

pub struct HeartZonesFrame {
    /// Index into the cached states; 0 when the heart rate is missing.
    state: usize,
    indicator_x: Option<f32>,
}

impl HeartZonesCache {
    /// Missing external heart rate shows the resting track without an indicator.
    pub fn prepare_frame(&self, raw: Option<f64>) -> HeartZonesFrame {
        match raw {
            Some(heartrate) => {
                let (index, progress) = zone_position(&self.zones, heartrate as f32);
                HeartZonesFrame {
                    state: index + 1,
                    indicator_x: Some(self.layout.indicator_x(index, progress)),
                }
            }
            None => HeartZonesFrame {
                state: 0,
                indicator_x: None,
            },
        }
    }
}

impl HeartZonesFrame {
    pub fn draw(&self, canvas: &Canvas, cache: &HeartZonesCache) {
        let mut paint = Paint::default();
        paint.set_alpha_f(cache.opacity);
        cache.states[self.state].draw(canvas, Some(&paint));
        let Some(x) = self.indicator_x else {
            return;
        };
        if let Some((shadow, origin)) = &cache.indicator_shadow {
            canvas.draw_image_with_sampling_options(
                shadow,
                (x + origin.x, origin.y),
                SamplingOptions::from(FilterMode::Linear),
                Some(&paint),
            );
        }
        let mut indicator = cache.indicator_paint.clone();
        indicator.set_alpha_f(indicator.alpha_f() * cache.opacity);
        canvas.save();
        canvas.translate((x, 0.0));
        canvas.draw_path(&cache.indicator.path(), &indicator);
        canvas.restore();
    }
}

#[derive(Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub display_variants: ResizeVariants,
}

#[derive(Serialize)]
pub struct ResizeVariants {
    pub heart_zones: ResizeVariant,
}

#[derive(Serialize)]
pub struct ResizeVariant {
    pub width: f32,
    pub height: f32,
}

/// `size` is the requested scene-pixel track size; the content rectangle is the track itself.
pub fn resize_heart_zones(
    widget: ValueConfig,
    scene: ScenePresentationConfig,
    size: [f32; 2],
    gesture: ResizeGesture,
) -> WidgetResult<ResizeUpdate> {
    let ResizeGesture { fixed, point, .. } = gesture;
    if size.iter().any(|value| !value.is_finite() || *value <= 0.0) {
        return Err(WidgetError::Config(
            "invalid heart zones resize size".into(),
        ));
    }
    let config = validate_heart_zones(widget, 0)?;
    let scale = validate_scene_presentation(scene)?.scale;
    // Every zone keeps at least a pixel between the fixed gaps.
    let min_width = config.zone_gap * (ZONE_COUNT - 1) as f32 + ZONE_COUNT as f32;
    let width = (size[0] / scale).clamp(min_width.max(8.0), 10_000.0);
    let height = (size[1] / scale).clamp(4.0, 10_000.0);
    Ok(ResizeUpdate {
        x: point[0] - width * scale * fixed[0],
        y: point[1] - height * scale * fixed[1],
        display_variants: ResizeVariants {
            heart_zones: ResizeVariant { width, height },
        },
    })
}

pub struct PreviewHeartZones {
    cache: HeartZonesCache,
    preview_raster_scale: PreviewRasterScale,
    last_sample: Option<Option<f64>>,
    pub tile: Tile,
}

impl PreviewHeartZones {
    pub fn new(
        widget: ValueConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let widget = validate_heart_zones(widget, 0)?;
        let host = WidgetHost::from_scene(&scene, raster_scale)?;
        Self::from_validated(&widget, &validate_scene_presentation(scene)?, &host, fonts)
    }

    pub fn from_validated(
        widget: &ValidatedHeartZones,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        Ok(Self {
            cache: prepare_heart_zones_cache(widget, scene, fonts, host.raster_scale)?,
            preview_raster_scale: host.raster_scale,
            last_sample: None,
            tile: Tile::default(),
        })
    }

    /// The sample the frame at `second` draws; equal samples draw identical pixels.
    pub fn frame_key(&self, activity: Option<&ActivitySampler>, second: f64) -> Option<f64> {
        activity.and_then(|activity| activity.heartrate_at(second))
    }

    /// Scene rectangle a render draws into; the indicator travels inside fixed bounds.
    pub fn layout_bounds(&self) -> TileBounds {
        self.cache.bounds.into()
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(
        &self,
        canvas: &Canvas,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> TileBounds {
        self.cache
            .prepare_frame(self.frame_key(activity, second))
            .draw(canvas, &self.cache);
        self.cache.bounds.into()
    }

    pub fn render(&mut self, activity: Option<&ActivitySampler>, second: f64) -> WidgetResult<()> {
        let raw = self.frame_key(activity, second);
        // The indicator moves continuously, even between whole-bpm samples.
        if self.last_sample == Some(raw) {
            return Ok(());
        }
        let frame = self.cache.prepare_frame(raw);
        let bounds = self.cache.bounds;
        self.tile.render(
            self.preview_raster_scale,
            SHAPE_COLOR_TYPE,
            bounds,
            Presented::content(self.cache.content_bounds),
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
        coggan_zones, heart_zones_layout, scale_hsv, zone_position, HeartZoneRange,
        HeartZonesIndicatorStyle, IndicatorShape,
    };
    use skia_safe::Color;

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1e-3, "{actual} != {expected}");
    }

    #[test]
    fn coggan_zones_span_forty_to_one_hundred_fifteen_percent_lthr() {
        let zones = coggan_zones(200.0);
        let bounds: Vec<(f32, f32)> = zones.iter().map(|zone| (zone.from, zone.to)).collect();
        let expected = [
            (80.0, 138.0),
            (138.0, 168.0),
            (168.0, 190.0),
            (190.0, 212.0),
            (212.0, 230.0),
        ];
        for ((from, to), (expected_from, expected_to)) in bounds.into_iter().zip(expected) {
            assert_close(from, expected_from);
            assert_close(to, expected_to);
        }
    }

    #[test]
    fn zone_position_tracks_progress_and_clamps_outside_the_track() {
        let zones = coggan_zones(200.0);
        assert_eq!(zone_position(&zones, 153.0), (1, 0.5));
        assert_eq!(zone_position(&zones, 168.0), (2, 0.0));
        assert_eq!(zone_position(&zones, 60.0), (0, 0.0));
        assert_eq!(zone_position(&zones, 109.0), (0, 0.5));
        assert_eq!(zone_position(&zones, 260.0), (4, 1.0));
    }

    #[test]
    fn zone_position_assigns_manual_gaps_to_the_lower_zone() {
        let zone = |from, to| HeartZoneRange { from, to };
        let zones = [
            zone(80.0, 120.0),
            zone(121.0, 140.0),
            zone(141.0, 160.0),
            zone(161.0, 175.0),
            zone(176.0, 195.0),
        ];
        assert_eq!(zone_position(&zones, 120.5), (0, 1.0));
        assert_eq!(zone_position(&zones, 121.0), (1, 0.0));
    }

    #[test]
    fn heart_zones_layout_splits_equal_zones_and_caps_the_outer_radius() {
        let layout = heart_zones_layout(10.0, 20.0, 520.0, 40.0, 5.0, 100.0);
        assert_eq!(layout.zone_width, 100.0);
        assert_eq!(layout.corner_radius, 20.0);
        assert_eq!(layout.zone_rect(4).right, 530.0);
        assert_eq!(layout.indicator_x(2, 0.25), 10.0 + 2.0 * 105.0 + 25.0);
        let active = layout.active_rect(0);
        assert_eq!(active.top, 20.0 - layout.grow);
        // Enlarging is vertical only, so the gaps between zones are preserved.
        assert_eq!(active.left, 10.0);
        assert_eq!(
            layout.active_rect(1).left - layout.active_rect(0).right,
            5.0
        );
    }

    #[test]
    fn arrow_indicators_point_at_the_enlarged_active_zone_from_outside() {
        let layout = heart_zones_layout(0.0, 0.0, 500.0, 40.0, 4.0, 8.0);
        let arrow = IndicatorShape::new(&layout, HeartZonesIndicatorStyle::Arrow, 16.0);
        let line = IndicatorShape::new(&layout, HeartZonesIndicatorStyle::Line, 16.0);
        assert_eq!(arrow.top, -layout.grow);
        assert_eq!(arrow.bottom, 40.0 + layout.grow);
        assert_eq!(arrow.extent().top, -layout.grow - 12.0);
        assert!(line.top > -layout.grow && line.bottom < 40.0 + layout.grow);
        assert_eq!(arrow.extent().width(), 16.0);
    }

    #[test]
    fn zone_colors_scale_value_and_saturation_without_shifting_hue_or_alpha() {
        let brighten = |color| scale_hsv(color, 1.45, 1.3);
        assert_eq!(
            brighten(Color::from_argb(255, 40, 60, 100)),
            Color::from_argb(255, 32, 70, 145)
        );
        // Value and saturation both cap at full; alpha is untouched.
        assert_eq!(
            brighten(Color::from_argb(200, 200, 100, 50)),
            Color::from_argb(200, 255, 89, 6)
        );
        // Greys have no hue to saturate and only brighten.
        assert_eq!(
            brighten(Color::from_argb(255, 128, 128, 128)),
            Color::from_argb(255, 186, 186, 186)
        );
        assert_eq!(brighten(Color::BLACK), Color::BLACK);
    }

    #[test]
    fn inactive_zones_wash_out_saturation_and_keep_value() {
        // Saturation 0.6 -> 0.48 at value 100: the darkest channel rises from 40 to 52.
        assert_eq!(
            scale_hsv(Color::from_argb(255, 40, 60, 100), 1.0, 0.8),
            Color::from_argb(255, 52, 68, 100)
        );
    }
}
