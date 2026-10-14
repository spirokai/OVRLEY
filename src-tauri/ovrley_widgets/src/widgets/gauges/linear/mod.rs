//! Shared linear track cache and boundary-label layout.
mod geometry;
mod labels;
mod layer;

use super::{
    bar_geometry::{scale_bar_geometry, ResolvedBarGeometry},
    frame::{prepare_text, raster_bounds},
    validation::{ValidatedLinearGaugeOrientation, ValidatedLinearGaugeWidget},
};
use crate::{
    error::WidgetResult,
    model::{
        scene::ValidatedScenePresentation, standard_widgets::widget_font_weight,
        types::TrackFillStyle,
    },
    render::text::{parse_color, resolve_font, FontResolver, ResolvedTextStyle},
    widgets::value::preview::surface,
};
pub use geometry::{bar_fill_rect, bordered_bar_fill_rect};
use skia_safe::{Canvas, Color, Image, Rect};

#[derive(Clone, Debug)]
pub struct LinearGaugeCache {
    pub static_image: Image,
    pub static_image_x: f32,
    pub static_image_y: f32,
    pub content_bounds: Rect,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub orientation: ValidatedLinearGaugeOrientation,
    pub track_corner_radius: f32,
    pub track_border_thickness: f32,
    pub track_filled_color: String,
    pub track_filled_opacity: f32,
    pub track_fill_flat: bool,
    pub track_fill_style: TrackFillStyle,
    pub bar_geometry: Option<ResolvedBarGeometry>,
}

pub fn prepare_linear_gauge_cache(
    gauge: &ValidatedLinearGaugeWidget,
    scene: &ValidatedScenePresentation,
    range: (f64, f64),
    altitude_offset_m: f64,
    fonts: &dyn FontResolver,
) -> WidgetResult<LinearGaugeCache> {
    let scale = scene.scale;
    let width = gauge.width * scale;
    let height = gauge.height * scale;
    let mut content = Rect::from_xywh(0.0, 0.0, width, height);
    let mut bounds = content;
    if gauge.track_border_thickness > 0.0 && scene.shadow_strength > 0.0 {
        let mut shadow = content;
        shadow.outset((
            scene.shadow_strength * scale * 3.0,
            scene.shadow_strength * scale * 3.0,
        ));
        shadow.offset((scene.shadow_distance * scale, scene.shadow_distance * scale));
        bounds.join(shadow);
    }
    let mut texts = Vec::new();
    if gauge.show_min_max_labels {
        let size = gauge.min_max_label_font_size * scale;
        let font = resolve_font(fonts, Some(&gauge.min_max_label_font), size)?;
        let labels = labels::label_layout(
            gauge,
            width,
            height,
            scale,
            &font,
            range.0,
            range.1,
            altitude_offset_m,
        );
        for (text, origin) in [
            (&labels.min_label, labels.min_origin),
            (&labels.max_label, labels.max_origin),
        ] {
            let (_, ink) = font.measure_str(text, None);
            let style = ResolvedTextStyle {
                x: origin.x,
                y: origin.y + ink.top,
                font_name: Some(gauge.min_max_label_font.clone()),
                font_size: size,
                font_weight: widget_font_weight(),
                italic: false,
                letter_spacing: 0.0,
                line_height: ink.height(),
                color: parse_color(&gauge.min_max_label_color, scene.opacity),
                opacity: scene.opacity,
                shadow_color: None,
                shadow_strength: 0.0,
                shadow_distance: 0.0,
                border_color: None,
                border_thickness: 0.0,
            };
            let run = prepare_text(text, text, style, fonts)?;
            content.join(run.content_bounds);
            bounds.join(run.bounds);
            texts.push(run);
        }
    }
    let bounds = raster_bounds(bounds);
    let mut surface = surface(bounds.width() as u32, bounds.height() as u32)?;
    let canvas = surface.canvas();
    canvas.clear(Color::TRANSPARENT);
    canvas.translate((-bounds.left, -bounds.top));
    layer::draw_static_layer(canvas, gauge, scene, width, height, scale);
    for text in texts {
        text.draw(canvas);
    }
    content.offset((gauge.x, gauge.y));
    Ok(LinearGaugeCache {
        static_image: surface.image_snapshot(),
        static_image_x: gauge.x + bounds.left,
        static_image_y: gauge.y + bounds.top,
        content_bounds: content,
        x: gauge.x,
        y: gauge.y,
        width,
        height,
        rotation: gauge.rotation,
        orientation: gauge.orientation,
        track_corner_radius: gauge.track_corner_radius * scale,
        track_border_thickness: gauge.track_border_thickness * scale,
        track_filled_color: gauge.track_filled_color.clone(),
        track_filled_opacity: gauge.track_filled_opacity * scene.opacity,
        track_fill_flat: gauge.track_fill_flat,
        track_fill_style: gauge.track_fill_style,
        bar_geometry: scale_bar_geometry(gauge.bar_geometry, scale),
    })
}

impl LinearGaugeCache {
    pub fn raster_bounds(&self) -> Rect {
        Rect::from_xywh(
            self.static_image_x,
            self.static_image_y,
            self.static_image.width() as f32,
            self.static_image.height() as f32,
        )
    }
    pub fn draw(&self, canvas: &Canvas, fill01: f32) {
        canvas.draw_image(
            &self.static_image,
            (self.static_image_x, self.static_image_y),
            None,
        );
        match self.track_fill_style {
            TrackFillStyle::Bars => layer::draw_segmented_fill(canvas, self, fill01),
            TrackFillStyle::Fill => layer::draw_continuous_fill(canvas, self, fill01),
        }
    }
}
