fn main() {}

#[no_mangle]
pub extern "C" fn ovrley_wasm_preview_alloc(len: usize) -> *mut u8 {
    wasm_preview_poc::alloc(len)
}

#[no_mangle]
pub unsafe extern "C" fn ovrley_wasm_preview_dealloc(ptr: *mut u8, len: usize) {
    wasm_preview_poc::dealloc(ptr, len);
}

#[no_mangle]
pub extern "C" fn ovrley_wasm_preview_width() -> u32 {
    wasm_preview_poc::WIDTH
}

#[no_mangle]
pub extern "C" fn ovrley_wasm_preview_height() -> u32 {
    wasm_preview_poc::HEIGHT
}

#[no_mangle]
pub extern "C" fn ovrley_wasm_preview_rgba_len() -> usize {
    wasm_preview_poc::RGBA_LEN
}

#[no_mangle]
pub extern "C" fn ovrley_wasm_preview_backend() -> u32 {
    // 1 = Skia software raster backend drawing into an RGBA8888 buffer.
    1
}

#[no_mangle]
pub unsafe extern "C" fn ovrley_wasm_preview_render_static_frame(ptr: *mut u8, len: usize) -> i32 {
    wasm_preview_poc::render_static_frame(ptr, len)
}

/// Loads a font from bytes provided by the frontend.
#[no_mangle]
pub unsafe extern "C" fn ovrley_wasm_preview_load_font_from_bytes(ptr: *const u8, len: usize) -> i32 {
    wasm_preview_poc::load_font_from_bytes(ptr, len)
}

/// Renders a dynamic text widget with a changing numeric value and optional unit label.
#[no_mangle]
pub unsafe extern "C" fn ovrley_wasm_preview_render_dynamic_text_widget(
    ptr: *mut u8,
    len: usize,
    value_ptr: *const u8,
    value_len: usize,
    unit_ptr: *const u8,
    unit_len: usize,
) -> i32 {
    wasm_preview_poc::render_dynamic_text_widget(ptr, len, value_ptr, value_len, unit_ptr, unit_len)
}
