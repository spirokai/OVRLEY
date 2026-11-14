//! The main window on Windows and Linux. It is built here rather than from the config so Windows can
//! choose WebView2's graphics backend before the webview starts and Linux can tune WebKitGTK's
//! scrolling; macOS still creates it from `tauri.macos.conf.json`.
use tauri::{App, WebviewWindow, WebviewWindowBuilder};

/// Without a hardware GPU, WebView2 turns WebGL off and the map preview cannot draw. These run its
/// WebGL on WARP, Direct3D's software rasterizer, and let it use that device. Linux needs nothing
/// like this: WebKitGTK takes WebGL from the system's Mesa, which falls back to llvmpipe on its own.
#[cfg(windows)]
const WARP_ARGS: &str = "--use-angle=d3d11-warp --ignore-gpu-blocklist";

/// Builds the window `tauri.conf.json` describes, which the config itself does not create.
pub(crate) fn create(app: &App) -> Result<WebviewWindow, Box<dyn std::error::Error>> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == "main")
        .ok_or("tauri.conf.json describes no main window")?;
    let builder = WebviewWindowBuilder::from_config(app.handle(), config)?;
    #[cfg(windows)]
    let builder = if needs_warp() {
        let base = config.additional_browser_args.as_deref().unwrap_or_default();
        log::warn!("No hardware GPU found; WebView2 draws WebGL on WARP");
        builder.additional_browser_args(&format!("{base} {WARP_ARGS}"))
    } else {
        builder
    };
    let window = builder.build()?;
    #[cfg(target_os = "linux")]
    window.with_webview(|webview| {
        use webkit2gtk::{glib::translate::ToGlibPtr, WebViewExt};
        let Some(settings) = webview.inner().settings() else {
            return;
        };
        // SAFETY: `settings` is a live WebKitSettings owned by this webview, on its GTK thread.
        let disabled = unsafe {
            webkit_features::disable(
                settings.to_glib_none().0,
                &[c"ThreadedScrolling", c"AsyncOverflowScrolling"],
            )
        };
        if let Err(reason) = disabled {
            log::warn!("Editor zoom may shake on this WebKitGTK: {reason}");
        }
    })?;
    Ok(window)
}

/// WebKitGTK moves scroll areas on its scrolling thread, in a different compositor frame from the
/// layer update that carries the page's own changes. The editor's cursor-anchored wheel zoom changes
/// the scene's scale and its viewport's scroll position together, so steps showed one frame at the
/// new scale and the old scroll. Main-thread scrolling paints both in one frame.
/// Feature toggles arrived in WebKitGTK 2.42, after the 2.40 bindings, so they are resolved at runtime
/// and an older system library keeps threaded scrolling instead of failing to load.
#[cfg(target_os = "linux")]
mod webkit_features {
    use std::ffi::{c_char, c_int, c_void, CStr};

    unsafe fn symbol<T: Copy>(name: &CStr) -> Result<T, String> {
        let pointer = libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr());
        if pointer.is_null() {
            return Err(format!(
                "{} is missing (WebKitGTK before 2.42)",
                name.to_string_lossy()
            ));
        }
        Ok(std::mem::transmute_copy(&pointer))
    }

    /// Turns off the features `identifiers` for every page using `settings`.
    pub(super) unsafe fn disable(
        settings: *mut impl Sized,
        identifiers: &[&CStr],
    ) -> Result<(), String> {
        let all_features: unsafe extern "C" fn() -> *mut c_void =
            symbol(c"webkit_settings_get_all_features")?;
        let length: unsafe extern "C" fn(*mut c_void) -> usize =
            symbol(c"webkit_feature_list_get_length")?;
        let get: unsafe extern "C" fn(*mut c_void, usize) -> *mut c_void =
            symbol(c"webkit_feature_list_get")?;
        let unref: unsafe extern "C" fn(*mut c_void) = symbol(c"webkit_feature_list_unref")?;
        let feature_identifier: unsafe extern "C" fn(*mut c_void) -> *const c_char =
            symbol(c"webkit_feature_get_identifier")?;
        let set_enabled: unsafe extern "C" fn(*mut c_void, *mut c_void, c_int) =
            symbol(c"webkit_settings_set_feature_enabled")?;
        let list = all_features();
        let mut missing = identifiers.to_vec();
        for index in 0..length(list) {
            let feature = get(list, index);
            let identifier = CStr::from_ptr(feature_identifier(feature));
            if let Some(position) = missing.iter().position(|&wanted| wanted == identifier) {
                set_enabled(settings.cast(), feature, 0);
                missing.swap_remove(position);
            }
        }
        unref(list);
        if missing.is_empty() {
            return Ok(());
        }
        Err(format!("WebKitGTK has no {missing:?} feature"))
    }
}

/// `OVRLEY_FORCE_WARP` tries the WARP path on a machine that has a GPU, in debug builds only.
#[cfg(windows)]
fn needs_warp() -> bool {
    (cfg!(debug_assertions) && std::env::var_os("OVRLEY_FORCE_WARP").is_some())
        || !has_hardware_gpu()
}

/// Whether DXGI lists an adapter that is not software. The Microsoft Basic Render Driver, which
/// Windows installs when no GPU driver is present, is flagged as software.
#[cfg(windows)]
fn has_hardware_gpu() -> bool {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE};
    // SAFETY: plain DXGI calls on a factory this function owns.
    let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else {
        return false;
    };
    (0..)
        .map_while(|index| unsafe { factory.EnumAdapters1(index) }.ok())
        .any(|adapter| {
            unsafe { adapter.GetDesc1() }
                .is_ok_and(|desc| desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 == 0)
        })
}
