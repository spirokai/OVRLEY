//! The main window on Windows and Linux. It is built here rather than from the config so Windows can
//! choose WebView2's graphics backend before the webview starts; macOS still creates it from
//! `tauri.macos.conf.json`.
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
    Ok(builder.build()?)
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
