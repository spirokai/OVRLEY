//! A finished map frame that lands 1:1 is copied outside its corners. The pixels must stay exactly
//! those of blending the whole frame through its shape, over the atlas's translucent base.
use ovrley_core::fonts::NativeFonts;
use ovrley_widgets::model::scene::{validate_scene_presentation, ScenePresentationConfig};
use ovrley_widgets::render::preview::PreviewRasterScale;
use ovrley_widgets::widgets::map::{
    config::MapConfig, frame_provider::MapFramePixels, layout::MapLayout,
    validation::validate_map_widget,
};
use ovrley_map::{MapStyleBuildings, MapStyleId, MapStyleInfo};
use serde_json::json;
use skia_safe::{
    surfaces, AlphaType, Canvas, ColorType, FilterMode, IRect, Image, ImageInfo, Paint, RRect, Rect,
};
use std::path::PathBuf;

const WIDTH: u32 = 120;
const HEIGHT: u32 = 80;

fn layout(x: f32, radius: f32, opacity: f32) -> MapLayout {
    let config: MapConfig = serde_json::from_value(json!({
        "value": "map", "x": x, "y": 40, "opacity": opacity, "font": "Inter.ttf", "width": WIDTH, "height": HEIGHT,
        "map_style": "liberty", "show_buildings": true, "show_paths": true, "show_labels": true, "camera_mode": "follow", "orientation": "rider_up", "pitch": 45,
        "follow_zoom": 16, "fixed_zoom": 0, "fixed_pan_x": 0, "fixed_pan_y": 0, "msaa_samples": 4, "shape": "rectangle",
        "corner_radius": radius, "border_color": "#ffffff", "border_thickness": 0, "border_opacity": 1,
        "route_smoothing_m": 8, "show_full_activity": true, "line_width": 6,
        "completed_line_opacity": 100, "remaining_line_opacity": 60, "completed_line_color": "#ff4d1f",
        "remaining_line_color": "#1f7bff", "marker_variant": "single", "marker_variant_diameter": 44, "marker_size": 10,
        "marker_opacity": 100, "marker_color": "#ffffff"
    }))
    .unwrap();
    let scene: ScenePresentationConfig = serde_json::from_value(json!({
        "width": 1920, "height": 1080, "scale": 1, "opacity": 1, "shadow_strength": 0, "shadow_distance": 0,
        "shadow_color": "#000000", "border_thickness": 0, "border_color": "#000000"
    }))
    .unwrap();
    let fonts = [PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fonts")];
    let style = MapStyleInfo {
        id: MapStyleId::new("liberty").unwrap(),
        label: "Liberty".into(),
        background: [0xf8, 0xf4, 0xf0],
        buildings: MapStyleBuildings::Own,
    };
    MapLayout::new(
        &validate_map_widget(&config, 0).unwrap(),
        &style,
        &validate_scene_presentation(scene).unwrap(),
        PreviewRasterScale::NATIVE,
        &NativeFonts(&fonts),
    )
    .unwrap()
}

/// Noise cropped by the renderer's shape coverage, premultiplied like a real frame.
fn frame(radius: f32) -> MapFramePixels {
    let (half_x, half_y) = (WIDTH as f32 / 2.0, HEIGHT as f32 / 2.0);
    let mut pixels = Vec::new();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let corner = (
                (x as f32 + 0.5 - half_x).abs() - (half_x - radius),
                (y as f32 + 0.5 - half_y).abs() - (half_y - radius),
            );
            let distance = (corner.0.max(0.0).powi(2) + corner.1.max(0.0).powi(2)).sqrt()
                + corner.0.max(corner.1).min(0.0)
                - radius;
            let coverage = (0.5 - distance).clamp(0.0, 1.0);
            let noise = x.wrapping_mul(2654435761) ^ y.wrapping_mul(40503);
            for byte in &noise.to_be_bytes()[..3] {
                pixels.push((f32::from(*byte) * coverage).round() as u8);
            }
            pixels.push((255.0 * coverage).round() as u8);
        }
    }
    MapFramePixels::new(WIDTH, HEIGHT, pixels).unwrap()
}

/// Draws onto a translucent atlas holding a slot that may cut the frame.
fn atlas(slot: IRect, draw: impl FnOnce(&Canvas)) -> Vec<u8> {
    let (width, height) = (160u32, 110u32);
    let mut pixels: Vec<u8> = (0..width as usize * height as usize)
        .flat_map(|index| {
            [
                (index % 251) as u8,
                (index % 241) as u8,
                (index % 239) as u8,
                (index % 200) as u8,
            ]
        })
        .collect();
    let info = ImageInfo::new(
        (width as i32, height as i32),
        ColorType::RGBA8888,
        AlphaType::Unpremul,
        None,
    );
    let mut surface = surfaces::wrap_pixels(&info, &mut pixels, width as usize * 4, None).unwrap();
    let canvas = surface.canvas();
    canvas.save();
    canvas.clip_irect(slot, None);
    // Like a slot placed away from its scene position.
    canvas.translate((-16.0, -16.0));
    canvas.save();
    draw(canvas);
    canvas.restore();
    canvas.restore();
    drop(surface);
    pixels
}

fn blended(x: f32, radius: f32, image: &Image, slot: IRect) -> Vec<u8> {
    atlas(slot, |canvas| {
        let rect = Rect::from_xywh(x, 40.0, WIDTH as f32, HEIGHT as f32);
        canvas.clip_rrect(RRect::new_rect_xy(rect, radius, radius), None, true);
        canvas.draw_image_rect_with_sampling_options(
            image,
            None,
            rect,
            FilterMode::Linear,
            &Paint::default(),
        );
    })
}

#[test]
fn a_copied_frame_matches_blending_it_whole() {
    let slots = [
        IRect::from_wh(160, 110),
        // Cuts through the frame and its corners on every side.
        IRect::new(20, 30, 90, 70),
    ];
    for radius in [0.0, 1.0, 13.5, 24.0, 40.0] {
        let frame = frame(radius);
        let image = frame.image();
        let map = layout(32.0, radius, 1.0);
        for slot in slots {
            let copied = atlas(slot, |canvas| {
                map.clip_shape(canvas);
                map.draw_image(canvas, image);
            });
            assert!(
                copied == blended(32.0, radius, image, slot),
                "radius {radius}, slot {slot:?} changed the pixels"
            );
        }
    }
}

#[test]
fn a_frame_between_pixels_lands_on_whole_pixels() {
    let frame = frame(24.0);
    let image = frame.image();
    let map = layout(32.5, 24.0, 1.0);
    assert_eq!(map.frame.left, 33.0);
    let slot = IRect::from_wh(160, 110);
    let drawn = atlas(slot, |canvas| {
        map.clip_shape(canvas);
        map.draw_image(canvas, image);
    });
    assert!(drawn == blended(33.0, 24.0, image, slot));
}

#[test]
fn a_translucent_map_still_shows_what_is_beneath() {
    let frame = frame(0.0);
    let image = frame.image();
    let map = layout(32.0, 0.0, 0.5);
    let slot = IRect::from_wh(160, 110);
    let drawn = atlas(slot, |canvas| map.draw_image(canvas, image));
    let copied = atlas(slot, |canvas| {
        layout(32.0, 0.0, 1.0).draw_image(canvas, image)
    });
    assert!(drawn != copied, "half opacity must blend with the base");
}
