//! Prepare one raster per active-zone state; playback picks one and moves the indicator over it.
use super::{
    validation::{HeartZonesIndicatorStyle, ValidatedHeartZones},
    zones::{HeartZoneRange, ZONE_COUNT},
};
use crate::{
    error::{WidgetError, WidgetResult},
    model::scene::ValidatedScenePresentation,
    render::{
        preview::{PreviewRasterScale, StaticRaster},
        shadow::shadow_paint,
        text::{parse_color, resolve_styled_font, FontResolver},
    },
    widgets::value::preview::{surface, SHAPE_COLOR_TYPE},
};
use skia_safe::{
    Canvas, Color, Image, Paint, Path, PathBuilder, Point, RRect, Rect, TextBlob, Vector,
};

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
