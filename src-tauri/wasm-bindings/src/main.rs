//! Browser ABI. Pointers must refer to live allocations owned by this module.
use wasm_bindings::{Renderer, HEIGHT, RGBA_LEN, WIDTH};
mod widget_abi;

fn main() {}

#[no_mangle]
pub extern "C" fn ovrley_abi_version() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn ovrley_alloc(len: usize) -> *mut u8 {
    Box::into_raw(vec![0_u8; len].into_boxed_slice()) as *mut u8
}

/// # Safety
/// `ptr` and `len` must describe an allocation returned by `ovrley_alloc`.
#[no_mangle]
pub unsafe extern "C" fn ovrley_dealloc(ptr: *mut u8, len: usize) {
    drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)));
}

/// Copies font bytes into a renderer. Returns null if the font is invalid.
/// # Safety
/// `ptr` must point to `len` readable bytes in this module's memory.
#[no_mangle]
pub unsafe extern "C" fn ovrley_renderer_create(ptr: *const u8, len: usize) -> *mut Renderer {
    if ptr.is_null() || len == 0 {
        return std::ptr::null_mut();
    }
    match Renderer::new(std::slice::from_raw_parts(ptr, len)) {
        Some(renderer) => Box::into_raw(Box::new(renderer)),
        None => std::ptr::null_mut(),
    }
}

/// # Safety
/// `renderer` must be a live handle returned by `ovrley_renderer_create`.
#[no_mangle]
pub unsafe extern "C" fn ovrley_renderer_destroy(renderer: *mut Renderer) {
    drop(Box::from_raw(renderer));
}

#[no_mangle]
pub extern "C" fn ovrley_width() -> u32 {
    WIDTH
}
#[no_mangle]
pub extern "C" fn ovrley_height() -> u32 {
    HEIGHT
}
#[no_mangle]
pub extern "C" fn ovrley_rgba_len() -> usize {
    RGBA_LEN
}

/// Returns 0 on success, 1 for invalid input, 2 for a surface creation failure.
/// # Safety
/// `renderer` must be a live renderer; `pixels` must reference `len` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn ovrley_render_frame(
    renderer: *mut Renderer,
    pixels: *mut u8,
    len: usize,
    frame: u32,
) -> u32 {
    if renderer.is_null() || pixels.is_null() || len != RGBA_LEN {
        return 1;
    }
    match (*renderer).render(std::slice::from_raw_parts_mut(pixels, len), frame) {
        Some(()) => 0,
        None => 2,
    }
}
