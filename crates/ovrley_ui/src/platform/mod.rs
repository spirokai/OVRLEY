//! The platform layer: backend selection, window chrome, and the services `ovrley_app` defines as
//! traits (plan D3), implemented with Slint, winit, `rfd` and `arboard`.

#[cfg(target_os = "linux")]
pub mod appimage_integration;
pub mod gpu;
pub mod key_input;
#[cfg(target_os = "linux")]
pub mod wayland_drop;
pub mod window_events;
#[cfg(target_os = "macos")]
pub mod macos;

use crate::AppWindow;
use gpu::{GpuChoice, GpuDevice};
use ovrley_app::store::services::{Clipboard, DialogReply, Dialogs, FileDialogRequest, UiPost, WindowControl};
use ovrley_app::store::{DispatchHandle, SendAction};
use slint::winit_030::winit;
use slint::ComponentHandle;
use std::cell::RefCell;

/// Standard output as `--mcp` was given it, for the protocol alone. Everything else that writes
/// there goes to the null device from here on: MDK puts its banner into C's stdout as it loads,
/// before `main`, and C writes it out whenever it flushes.
pub fn take_standard_output() -> std::io::Result<std::fs::File> {
    #[cfg(unix)]
    let output = std::os::fd::AsFd::as_fd(&std::io::stdout()).try_clone_to_owned()?;
    #[cfg(windows)]
    let output = std::os::windows::io::AsHandle::as_handle(&std::io::stdout()).try_clone_to_owned()?;
    let null = if cfg!(windows) { c"NUL" } else { c"/dev/null" };
    // SAFETY: the path is a C string, and the descriptor is this function's own until it closes it.
    // On Windows they are the Universal CRT's, which MDK shares.
    unsafe {
        let null = libc::open(null.as_ptr(), libc::O_WRONLY);
        if null < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let redirected = libc::dup2(null, 1);
        let error = std::io::Error::last_os_error();
        libc::close(null);
        if redirected < 0 {
            return Err(error);
        }
    }
    Ok(output.into())
}

/// The Linux app id, which is the desktop entry's name, so the window finds its launcher and icon.
/// The Flatpak's entry has the sandbox's id (`FLATPAK_ID`, the runtime's documented variable).
#[cfg(any(test, all(unix, not(target_os = "macos"))))]
fn linux_app_id(flatpak_id: Option<String>) -> String {
    flatpak_id.unwrap_or_else(|| ovrley_services::runtime::APP_IDENTIFIER.to_string())
}

/// Selects Slint's backend, in the plan's order: the GPU device, the window-attribute hooks, the
/// renderer, `select()`. Returns the shared device, if any, for the compositor.
pub fn select_backend() -> Result<Option<GpuDevice>, String> {
    let gpu = gpu::open();
    let selector = slint::BackendSelector::new()
        .backend_name("winit".into())
        .with_winit_window_attributes_hook(window_attributes)
        .with_winit_custom_application_handler(window_events::AppEvents::default());
    let (selector, shared) = match gpu {
        GpuChoice::Shared(device) => {
            let selector = selector.require_wgpu_30(slint::wgpu_30::WGPUConfiguration::Manual {
                instance: device.instance.clone(),
                adapter: device.adapter.clone(),
                device: device.device.clone(),
                queue: device.queue.clone(),
            });
            (selector, Some(device))
        }
        GpuChoice::Software { .. } => (selector.renderer_name("skia-software".into()), None),
        GpuChoice::None => (require_platform_api(selector), None),
    };
    selector.select().map_err(|error| format!("Could not start the window system: {error}"))?;
    Ok(shared)
}

/// With no adapter, the platform's own API is required by name, so a silent fallback to the CPU
/// rasteriser becomes a visible startup failure.
fn require_platform_api(selector: slint::BackendSelector) -> slint::BackendSelector {
    if cfg!(target_vendor = "apple") {
        selector.require_metal()
    } else if cfg!(windows) {
        selector.require_d3d()
    } else {
        selector
    }
}

/// The window chrome per platform (D9; `concat-window-chrome.md`).
fn window_attributes(attributes: winit::window::WindowAttributes) -> winit::window::WindowAttributes {
    let attributes = attributes.with_title("OVRLEY").with_maximized(true).with_active(true);
    #[cfg(target_os = "macos")]
    let attributes = {
        use winit::platform::macos::WindowAttributesExtMacOS;
        // The native title bar stays, invisible, so the traffic lights stay native.
        attributes
            .with_titlebar_transparent(true)
            .with_title_hidden(true)
            .with_fullsize_content_view(true)
    };
    #[cfg(windows)]
    let attributes = {
        use winit::platform::windows::WindowAttributesExtWindows;
        attributes
            .with_decorations(false)
            .with_undecorated_shadow(true)
            .with_window_icon(window_icon())
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let attributes = {
        use winit::platform::wayland::WindowAttributesExtWayland;
        use winit::platform::x11::WindowAttributesExtX11;
        static APP_ID: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| linux_app_id(std::env::var("FLATPAK_ID").ok()));
        let attributes = WindowAttributesExtWayland::with_name(attributes, APP_ID.as_str(), APP_ID.as_str());
        WindowAttributesExtX11::with_name(attributes, APP_ID.as_str(), APP_ID.as_str())
            .with_decorations(false)
            .with_window_icon(window_icon())
    };
    attributes
}

/// Whether this is Windows before 11 (build 22000), which draws a frameless window without an
/// edge; Windows 11's DWM edges and rounds it.
#[cfg(windows)]
pub fn is_windows_10() -> bool {
    use windows::Wdk::System::SystemServices::RtlGetVersion;
    use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
    let mut version = OSVERSIONINFOW {
        dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // SAFETY: `version` is an OSVERSIONINFOW with its size set, as RtlGetVersion requires. Unlike
    // `GetVersionExW`, it reports the real version to an app without a compatibility manifest.
    if let Err(error) = unsafe { RtlGetVersion(&mut version) }.ok() {
        log::warn!("could not read the Windows version: {error}");
        return false;
    }
    version.dwBuildNumber < 22000
}

/// The window opens maximised, and leaving that restores the 1920×1080 preferred size, which winit
/// (unlike Tauri's tao) does not constrain to a smaller screen. A restored window larger than its
/// monitor shrinks to 90% of it, centred.
pub fn fit_restored_window(window: &winit::window::Window) {
    if window.is_maximized() || window.fullscreen().is_some() {
        return;
    }
    let Some(monitor) = window.current_monitor() else { return };
    let screen = monitor.size();
    let outer = window.outer_size();
    if outer.width <= screen.width && outer.height <= screen.height {
        return;
    }
    let width = screen.width * 9 / 10;
    let height = screen.height * 9 / 10;
    let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(width, height));
    let origin = monitor.position();
    window.set_outer_position(winit::dpi::PhysicalPosition::new(
        origin.x + ((screen.width - width) / 2) as i32,
        origin.y + ((screen.height - height) / 2) as i32,
    ));
}

/// The window icon on Windows and Linux, from the Tauri app's icons.
#[cfg(not(target_os = "macos"))]
fn window_icon() -> Option<winit::window::Icon> {
    let decoded = image::load_from_memory_with_format(include_bytes!("../../../../assets/icons/128x128.png"), image::ImageFormat::Png)
        .map_err(|error| log::warn!("could not decode the window icon: {error}"))
        .ok()?
        .into_rgba8();
    let (width, height) = decoded.dimensions();
    winit::window::Icon::from_rgba(decoded.into_raw(), width, height)
        .map_err(|error| log::warn!("could not use the window icon: {error}"))
        .ok()
}

thread_local! {
    /// The dispatcher's handle on the UI thread, for actions posted from other threads.
    pub(crate) static UI_DISPATCH: RefCell<Option<DispatchHandle>> = const { RefCell::new(None) };
    /// The app window, for the raw-event handler, which exists before the window does.
    pub(crate) static APP_WINDOW: RefCell<Option<slint::Weak<AppWindow>>> = const { RefCell::new(None) };
}

/// Makes posted actions and raw window events reach `dispatch` and `window`. Called once both
/// exist; a drop or a press cannot happen before the window is up.
pub fn install_ui_dispatch(dispatch: DispatchHandle, window: slint::Weak<AppWindow>) {
    UI_DISPATCH.with(|slot| *slot.borrow_mut() = Some(dispatch));
    APP_WINDOW.with(|slot| *slot.borrow_mut() = Some(window));
}

/// The window, as the MCP socket's threads reach it.
pub struct AgentWindow;

impl ovrley_mcp::link::Window for AgentWindow {
    fn post(&self, job: ovrley_mcp::link::UiJob) -> Result<(), ovrley_mcp::link::WindowClosed> {
        slint::invoke_from_event_loop(move || {
            UI_DISPATCH.with(|slot| match slot.borrow().as_ref() {
                Some(dispatch) => job(dispatch),
                None => log::error!("an agent reached the window before the dispatcher existed"),
            });
        })
        .map_err(|error| {
            log::debug!("the event loop takes no agent work: {error}");
            ovrley_mcp::link::WindowClosed
        })
    }
}

/// Dispatches into the app from code that runs outside Slint's callbacks (raw window events).
pub(crate) fn dispatch_ui(action: impl FnOnce(&mut ovrley_app::store::Store, &mut ovrley_app::store::services::Services) + 'static) {
    UI_DISPATCH.with(|slot| {
        if let Some(dispatch) = slot.borrow().as_ref() {
            dispatch.dispatch(action);
        }
    });
}

/// Delivers actions to the UI thread through Slint's event loop.
pub struct SlintUiPost;

impl UiPost for SlintUiPost {
    fn post(&self, action: SendAction) {
        let posted = slint::invoke_from_event_loop(move || {
            UI_DISPATCH.with(|slot| match slot.borrow().as_ref() {
                Some(dispatch) => dispatch.dispatch(move |store, services| action(store, services)),
                None => log::error!("an action was posted before the dispatcher existed"),
            });
        });
        if let Err(error) = posted {
            // The event loop has ended: the app is closing.
            log::debug!("dropped an action posted after the event loop ended: {error}");
        }
    }
}

/// The native window's buttons and close.
pub struct SlintWindowControl {
    pub window: slint::Weak<AppWindow>,
}

impl WindowControl for SlintWindowControl {
    fn close(&self) {
        if let Some(window) = self.window.upgrade() {
            if let Err(error) = window.hide() {
                log::warn!("could not hide the window: {error}");
            }
        }
        if let Err(error) = slint::quit_event_loop() {
            log::warn!("could not end the event loop: {error}");
        }
    }

    fn minimize(&self) {
        if let Some(window) = self.window.upgrade() {
            window.window().set_minimized(true);
        }
    }

    fn toggle_maximize(&self) {
        if let Some(window) = self.window.upgrade() {
            let window = window.window();
            window.set_maximized(!window.is_maximized());
        }
    }
}

/// Text on the system clipboard. Slint 1.18 has no public clipboard API. The `arboard` handle is
/// kept, because on Linux the clipboard owner must stay alive to serve what it copied.
#[derive(Default)]
pub struct SystemClipboard {
    clipboard: RefCell<Option<arboard::Clipboard>>,
}

impl Clipboard for SystemClipboard {
    fn write_text(&self, text: &str) -> Result<(), String> {
        let mut slot = self.clipboard.borrow_mut();
        if slot.is_none() {
            *slot = Some(arboard::Clipboard::new().map_err(|error| error.to_string())?);
        }
        slot.as_mut()
            .expect("opened above")
            .set_text(text.to_string())
            .map_err(|error| error.to_string())
    }
}

/// Native file pickers through `rfd`'s async dialogs, driven by Slint's event loop and parented to
/// the window, so the window keeps repainting while one is open.
pub struct NativeDialogs {
    pub window: slint::Weak<AppWindow>,
}

impl Dialogs for NativeDialogs {
    fn show(&self, request: FileDialogRequest, reply: DialogReply) {
        let window = self.window.clone();
        let spawned = slint::spawn_local(async move {
            let mut dialog = rfd::AsyncFileDialog::new();
            if let Some(window) = window.upgrade() {
                dialog = dialog.set_parent(&window.window().window_handle());
            }
            let add_filters = |mut dialog: rfd::AsyncFileDialog, filters: &[ovrley_app::store::services::DialogFilter]| {
                for filter in filters {
                    dialog = dialog.add_filter(&filter.name, &filter.extensions);
                }
                dialog
            };
            let selected = match request {
                FileDialogRequest::OpenFile { filters, default_path } => {
                    if let Some(directory) = default_path {
                        dialog = dialog.set_directory(directory);
                    }
                    add_filters(dialog, &filters).pick_file().await.into_iter().collect()
                }
                FileDialogRequest::OpenFiles { filters, default_path } => {
                    if let Some(directory) = default_path {
                        dialog = dialog.set_directory(directory);
                    }
                    add_filters(dialog, &filters).pick_files().await.unwrap_or_default()
                }
                FileDialogRequest::OpenDirectory { default_path } => {
                    if let Some(directory) = default_path {
                        dialog = dialog.set_directory(directory);
                    }
                    dialog.pick_folder().await.into_iter().collect()
                }
                FileDialogRequest::Save { default_path, filters } => {
                    let path = std::path::Path::new(&default_path);
                    if let Some(directory) = path.parent() {
                        dialog = dialog.set_directory(directory);
                    }
                    if let Some(name) = path.file_name() {
                        dialog = dialog.set_file_name(name.to_string_lossy());
                    }
                    add_filters(dialog, &filters).save_file().await.into_iter().collect()
                }
            };
            let selected: Vec<rfd::FileHandle> = selected;
            reply(selected.iter().map(|handle| handle.path().to_string_lossy().into_owned()).collect());
        });
        if let Err(error) = spawned {
            log::error!("could not show a file dialog: {error}");
        }
    }
}

/// A startup failure before the window exists, in a native message dialog: a GUI build has no
/// console.
pub fn show_startup_failure(message: &str) {
    log::error!("{message}");
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("OVRLEY could not start")
        .set_description(message)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

#[cfg(test)]
mod app_id_tests {
    #[test]
    fn the_app_id_is_the_desktop_entrys() {
        assert_eq!(super::linux_app_id(None), "com.ovrley.desktop");
        assert_eq!(super::linux_app_id(Some("cc.ovrley.OVRLEY".into())), "cc.ovrley.OVRLEY");
    }
}
