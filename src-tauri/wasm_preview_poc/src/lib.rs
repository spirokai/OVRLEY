use skia_safe::{font::Edging, surfaces, Color, ColorType, Font, FontHinting, FontMgr, ImageInfo, Paint, Rect, Typeface};
use std::cell::RefCell;
use std::sync::OnceLock;

pub const WIDTH: u32 = 1280;
pub const HEIGHT: u32 = 480;
pub const BYTES_PER_PIXEL: usize = 4;
pub const RGBA_LEN: usize = WIDTH as usize * HEIGHT as usize * BYTES_PER_PIXEL;

pub const RENDER_OK: i32 = 0;
pub const RENDER_BAD_BUFFER: i32 = 1;
pub const RENDER_SURFACE_FAILED: i32 = 2;
pub const RENDER_FONT_NOT_LOADED: i32 = 3;

static FONT: OnceLock<Typeface> = OnceLock::new();
static DYNAMIC_BASE_LAYER: OnceLock<Vec<u8>> = OnceLock::new();

const BACKGROUND_COLOR: Color = Color::from_argb(255, 12, 14, 18);
const PANEL_COLOR: Color = Color::from_argb(255, 24, 28, 36);
const ACCENT_COLOR: Color = Color::from_argb(255, 64, 196, 255);
const MARKER_COLOR: Color = Color::from_argb(255, 238, 242, 246);
const VALUE_TEXT_COLOR: Color = Color::from_argb(255, 238, 242, 246);
const UNIT_TEXT_COLOR: Color = Color::from_argb(255, 150, 160, 170);

const ACCENT_RECT: (f32, f32, f32, f32) = (48.0, 48.0, 96.0, 384.0);
const MARKER_CENTER: (f32, f32) = (820.0, 240.0);
const MARKER_RADIUS: f32 = 156.0;
const VALUE_TEXT_X: f32 = 160.0;
const VALUE_TEXT_BASELINE_Y: f32 = 240.0;
const UNIT_TEXT_GAP_X: f32 = 20.0;

thread_local! {
    static FONT_MGR: RefCell<Option<FontMgr>> = const { RefCell::new(None) };
}

/// Allocates Wasm-owned bytes for browser-side canvas presentation.
///
/// The frontend mount slice can call this, pass the returned pointer to
/// `ovrley_wasm_preview_render_static_frame`, then copy the bytes into
/// ImageData for the widget-local preview canvas.
pub fn alloc(len: usize) -> *mut u8 {
    let mut bytes = Vec::<u8>::with_capacity(len);
    let ptr = bytes.as_mut_ptr();
    std::mem::forget(bytes);
    ptr
}

pub unsafe fn dealloc(ptr: *mut u8, len: usize) {
    if !ptr.is_null() && len > 0 {
        drop(Vec::from_raw_parts(ptr, 0, len));
    }
}

/// Loads a font from bytes provided by the frontend.
///
/// This allows the renderer to use bundled font bytes instead of native filesystem font lookup.
pub unsafe fn load_font_from_bytes(ptr: *const u8, len: usize) -> i32 {
    if ptr.is_null() || len == 0 {
        return RENDER_BAD_BUFFER;
    }

    let bytes = std::slice::from_raw_parts(ptr, len);
    let font_mgr = font_mgr();
    match font_mgr.new_from_data(bytes, None) {
        Some(typeface) => {
            let _ = FONT.set(typeface);
            RENDER_OK
        }
        None => RENDER_FONT_NOT_LOADED,
    }
}

pub unsafe fn render_static_frame(ptr: *mut u8, len: usize) -> i32 {
    if ptr.is_null() || len < RGBA_LEN {
        return RENDER_BAD_BUFFER;
    }

    let pixels = std::slice::from_raw_parts_mut(ptr, RGBA_LEN);
    match draw_static_frame(pixels) {
        Some(()) => RENDER_OK,
        None => RENDER_SURFACE_FAILED,
    }
}

/// Renders a dynamic text widget with a changing numeric value and optional unit label.
///
/// The value and unit are passed as strings from React state.
pub unsafe fn render_dynamic_text_widget(
    ptr: *mut u8,
    len: usize,
    value_ptr: *const u8,
    value_len: usize,
    unit_ptr: *const u8,
    unit_len: usize,
) -> i32 {
    if ptr.is_null() || len < RGBA_LEN {
        return RENDER_BAD_BUFFER;
    }

    let pixels = std::slice::from_raw_parts_mut(ptr, RGBA_LEN);
    let value = if value_ptr.is_null() || value_len == 0 {
        String::new()
    } else {
        let value_bytes = std::slice::from_raw_parts(value_ptr, value_len);
        String::from_utf8_lossy(value_bytes).to_string()
    };

    let unit = if unit_ptr.is_null() || unit_len == 0 {
        String::new()
    } else {
        let unit_bytes = std::slice::from_raw_parts(unit_ptr, unit_len);
        String::from_utf8_lossy(unit_bytes).to_string()
    };

    match draw_dynamic_text_widget(pixels, &value, &unit) {
        Some(()) => RENDER_OK,
        None => RENDER_SURFACE_FAILED,
    }
}

fn draw_static_frame(pixels: &mut [u8]) -> Option<()> {
    let info = ImageInfo::new(
        (WIDTH as i32, HEIGHT as i32),
        ColorType::RGBA8888,
        skia_safe::AlphaType::Premul,
        None,
    );
    let row_bytes = WIDTH as usize * BYTES_PER_PIXEL;
    let mut surface = surfaces::wrap_pixels(&info, pixels, row_bytes, None)?;
    draw_widget_chrome(surface.canvas(), true);
    Some(())
}

fn draw_dynamic_text_widget(pixels: &mut [u8], value: &str, unit: &str) -> Option<()> {
    restore_dynamic_base_layer(pixels)?;

    let info = ImageInfo::new(
        (WIDTH as i32, HEIGHT as i32),
        ColorType::RGBA8888,
        skia_safe::AlphaType::Premul,
        None,
    );
    let row_bytes = WIDTH as usize * BYTES_PER_PIXEL;
    let mut surface = surfaces::wrap_pixels(&info, pixels, row_bytes, None)?;
    let canvas = surface.canvas();

    let typeface = FONT.get().cloned().or_else(|| {
        font_mgr()
            .legacy_make_typeface(Some("Arial"), skia_safe::FontStyle::normal())
    });

    if let Some(typeface) = typeface {
        let value_font = configure_text_font(Font::from_typeface(&typeface, 120.0));
        let mut value_paint = Paint::default();
        value_paint.set_anti_alias(true);
        value_paint.set_color(VALUE_TEXT_COLOR);

        canvas.draw_str(value, (VALUE_TEXT_X, VALUE_TEXT_BASELINE_Y), &value_font, &value_paint);

        if !unit.is_empty() {
            let unit_font = configure_text_font(Font::from_typeface(&typeface, 48.0));
            let mut unit_paint = Paint::default();
            unit_paint.set_anti_alias(true);
            unit_paint.set_color(UNIT_TEXT_COLOR);

            let value_width = value_font.measure_str(value, None).0;
            let unit_x = VALUE_TEXT_X + value_width + UNIT_TEXT_GAP_X;
            canvas.draw_str(unit, (unit_x, VALUE_TEXT_BASELINE_Y), &unit_font, &unit_paint);
        }
    }

    Some(())
}

fn draw_widget_chrome(canvas: &skia_safe::Canvas, include_marker: bool) {
    canvas.clear(BACKGROUND_COLOR);

    let mut panel = Paint::default();
    panel.set_anti_alias(true);
    panel.set_color(PANEL_COLOR);
    canvas.draw_rect(Rect::from_xywh(0.0, 0.0, WIDTH as f32, HEIGHT as f32), &panel);

    let mut accent = Paint::default();
    accent.set_anti_alias(true);
    accent.set_color(ACCENT_COLOR);
    canvas.draw_rect(Rect::from_xywh(ACCENT_RECT.0, ACCENT_RECT.1, ACCENT_RECT.2, ACCENT_RECT.3), &accent);

    if include_marker {
        let mut marker = Paint::default();
        marker.set_anti_alias(true);
        marker.set_color(MARKER_COLOR);
        canvas.draw_circle(MARKER_CENTER, MARKER_RADIUS, &marker);
    }
}

fn restore_dynamic_base_layer(pixels: &mut [u8]) -> Option<()> {
    let base_layer = DYNAMIC_BASE_LAYER.get_or_init(build_dynamic_base_layer);
    if base_layer.len() != RGBA_LEN {
        return None;
    }
    pixels.copy_from_slice(base_layer);
    Some(())
}

fn build_dynamic_base_layer() -> Vec<u8> {
    let mut pixels = vec![0; RGBA_LEN];
    let info = ImageInfo::new(
        (WIDTH as i32, HEIGHT as i32),
        ColorType::RGBA8888,
        skia_safe::AlphaType::Premul,
        None,
    );
    let row_bytes = WIDTH as usize * BYTES_PER_PIXEL;

    if let Some(mut surface) = surfaces::wrap_pixels(&info, pixels.as_mut_slice(), row_bytes, None) {
        draw_widget_chrome(surface.canvas(), false);
    }

    pixels
}

fn configure_text_font(mut font: Font) -> Font {
    font.set_subpixel(true);
    font.set_edging(Edging::SubpixelAntiAlias);
    font.set_hinting(FontHinting::Full);
    font
}

fn font_mgr() -> FontMgr {
    FONT_MGR.with(|cell| {
        let mut borrowed = cell.borrow_mut();
        if borrowed.is_none() {
            *borrowed = Some(FontMgr::default());
        }
        borrowed.as_ref().unwrap().clone()
    })
}
