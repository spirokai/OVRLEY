//! The platform layer: backend selection, window chrome, and the services `ovrley_app` defines as
//! traits (plan D3), implemented with Slint, winit, `rfd` and `arboard`.

pub mod gpu;
pub mod key_input;
#[cfg(target_os = "macos")]
pub mod macos;

use crate::AppWindow;
use gpu::{GpuChoice, GpuDevice};
use ovrley_app::store::services::{Clipboard, DialogReply, Dialogs, FileDialogRequest, UiPost, WindowControl};
use ovrley_app::store::{DispatchHandle, SendAction};
use slint::winit_030::winit;
use slint::winit_030 as winit_handler;
use slint::ComponentHandle;
use std::cell::RefCell;

/// The Linux app id: the desktop entry's name, so the running window groups with its launcher.
pub const LINUX_APP_ID: &str = "com.ovrley.desktop";

/// Selects Slint's backend, in the plan's order: the GPU device, the window-attribute hooks, the
/// renderer, `select()`. Returns the shared device, if any, for the compositor.
pub fn select_backend() -> Result<Option<GpuDevice>, String> {
    let gpu = gpu::open();
    let selector = slint::BackendSelector::new()
        .backend_name("winit".into())
        .with_winit_window_attributes_hook(window_attributes)
        .with_winit_custom_application_handler(AppEvents);
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
        let attributes = WindowAttributesExtWayland::with_name(attributes, LINUX_APP_ID, LINUX_APP_ID);
        WindowAttributesExtX11::with_name(attributes, LINUX_APP_ID, LINUX_APP_ID)
            .with_decorations(false)
            .with_window_icon(window_icon())
    };
    attributes
}

/// The window icon on Windows and Linux, from the Tauri app's icons.
#[cfg(not(target_os = "macos"))]
fn window_icon() -> Option<winit::window::Icon> {
    let decoded = image::load_from_memory_with_format(include_bytes!("../../../icons/128x128.png"), image::ImageFormat::Png)
        .map_err(|error| log::warn!("could not decode the window icon: {error}"))
        .ok()?
        .into_rgba8();
    let (width, height) = decoded.dimensions();
    winit::window::Icon::from_rgba(decoded.into_raw(), width, height)
        .map_err(|error| log::warn!("could not use the window icon: {error}"))
        .ok()
}

/// The raw winit events the app needs before Slint routes them. Installed at backend selection,
/// before the dispatcher exists, so it reaches the app through [`UI_DISPATCH`]. Step 8 adds the
/// file drops and the press-away blur here.
struct AppEvents;

impl winit_handler::CustomApplicationHandler for AppEvents {
    /// On macOS, ⌘Q ends the loop inside the system's terminate, and `window.run()` may never
    /// return; the preferences are flushed here, while everything is still alive.
    fn exiting(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) -> winit_handler::EventResult {
        UI_DISPATCH.with(|slot| {
            if let Some(dispatch) = slot.borrow().as_ref() {
                dispatch.flush_preferences();
            }
        });
        winit_handler::EventResult::Propagate
    }
}

thread_local! {
    /// The dispatcher's handle on the UI thread, for actions posted from other threads.
    static UI_DISPATCH: RefCell<Option<DispatchHandle>> = const { RefCell::new(None) };
}

/// Makes posted actions reach `dispatch`. Called once the dispatcher exists.
pub fn install_ui_dispatch(dispatch: DispatchHandle) {
    UI_DISPATCH.with(|slot| *slot.borrow_mut() = Some(dispatch));
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
                    add_filters(dialog, &filters).pick_file().await
                }
                FileDialogRequest::OpenDirectory { default_path } => {
                    if let Some(directory) = default_path {
                        dialog = dialog.set_directory(directory);
                    }
                    dialog.pick_folder().await
                }
                FileDialogRequest::Save { default_path, filters } => {
                    let path = std::path::Path::new(&default_path);
                    if let Some(directory) = path.parent() {
                        dialog = dialog.set_directory(directory);
                    }
                    if let Some(name) = path.file_name() {
                        dialog = dialog.set_file_name(name.to_string_lossy());
                    }
                    add_filters(dialog, &filters).save_file().await
                }
            };
            reply(selected.map(|handle| handle.path().to_string_lossy().into_owned()));
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
