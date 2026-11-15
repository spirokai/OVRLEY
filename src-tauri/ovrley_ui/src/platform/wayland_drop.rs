//! Files dropped on the window under Wayland (D10; `concat-file-drop.md`, "Wayland").
//!
//! winit 0.30 never asks the seat for a data device, so it reports no drops on Wayland. This
//! listener joins winit's `wl_display` connection as a guest, on its own event queue and thread:
//! - it binds the seat and the data-device manager from a fresh registry and gets a data device;
//! - on enter, it accepts an offer that lists `text/uri-list` and reports hovering;
//! - on leave, it reports the hover cancelled;
//! - on drop, it reads the URI list through a pipe, decodes the `file:` URIs, finishes the offer
//!   and reports the paths as one drop.
//!
//! It shares the socket with winit the way libwayland intends (prepare to read, poll, read, then
//! dispatch its own queue), waking every 200 ms to check its stop flag. It never closes the
//! connection, which belongs to winit. The module depends only on Wayland and `libc` and reports
//! through a callback, so the platform layer decides how events reach the UI thread.

use super::window_events::decode_uri_list;
use std::collections::HashMap;
use std::io::Read;
use std::os::fd::AsFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use wayland_client::backend::{Backend, ObjectId};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{
    wl_data_device, wl_data_device_manager, wl_data_offer, wl_registry, wl_seat,
};
use wayland_client::{event_created_child, Connection, Dispatch, Proxy, QueueHandle};

const URI_LIST: &str = "text/uri-list";
const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// What the listener reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DropEvent {
    Hovering(bool),
    Dropped(Vec<PathBuf>),
}

struct State {
    connection: Connection,
    report: Box<dyn Fn(DropEvent) + Send>,
    /// The mime types each live offer announced.
    offers: HashMap<ObjectId, Vec<String>>,
    /// The offer being dragged over the window, when it carries a URI list.
    current: Option<wl_data_offer::WlDataOffer>,
}

impl State {
    fn read_uri_list(&self, offer: &wl_data_offer::WlDataOffer) -> Option<String> {
        let (mut reader, writer) = match std::io::pipe() {
            Ok(pipe) => pipe,
            Err(error) => {
                log::warn!("could not open a pipe for a drop: {error}");
                return None;
            }
        };
        offer.receive(URI_LIST.to_string(), writer.as_fd());
        drop(writer);
        if let Err(error) = self.connection.flush() {
            log::warn!("could not ask for the dropped files: {error}");
            return None;
        }
        let mut body = String::new();
        match reader.read_to_string(&mut body) {
            Ok(_) => Some(body),
            Err(error) => {
                log::warn!("could not read the dropped files: {error}");
                None
            }
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_seat::WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_data_device_manager::WlDataDeviceManager, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_data_device_manager::WlDataDeviceManager,
        _: wl_data_device_manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_data_offer::WlDataOffer, ()> for State {
    fn event(
        state: &mut Self,
        offer: &wl_data_offer::WlDataOffer,
        event: wl_data_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_data_offer::Event::Offer { mime_type } = event {
            state.offers.entry(offer.id()).or_default().push(mime_type);
        }
    }
}

impl Dispatch<wl_data_device::WlDataDevice, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_data_device::WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::DataOffer { id } => {
                state.offers.insert(id.id(), Vec::new());
            }
            wl_data_device::Event::Enter { serial, id, .. } => {
                let Some(offer) = id else { return };
                let carries_files = state
                    .offers
                    .get(&offer.id())
                    .is_some_and(|mimes| mimes.iter().any(|mime| mime == URI_LIST));
                if !carries_files {
                    offer.accept(serial, None);
                    return;
                }
                offer.accept(serial, Some(URI_LIST.to_string()));
                if offer.version() >= 3 {
                    offer.set_actions(
                        wl_data_device_manager::DndAction::Copy,
                        wl_data_device_manager::DndAction::Copy,
                    );
                }
                state.current = Some(offer);
                (state.report)(DropEvent::Hovering(true));
            }
            wl_data_device::Event::Leave => {
                if let Some(offer) = state.current.take() {
                    state.offers.remove(&offer.id());
                    offer.destroy();
                    (state.report)(DropEvent::Hovering(false));
                }
            }
            wl_data_device::Event::Drop => {
                let Some(offer) = state.current.take() else {
                    return;
                };
                let paths = state
                    .read_uri_list(&offer)
                    .map(|body| decode_uri_list(&body))
                    .unwrap_or_default();
                if offer.version() >= 3 {
                    offer.finish();
                }
                state.offers.remove(&offer.id());
                offer.destroy();
                (state.report)(DropEvent::Hovering(false));
                if !paths.is_empty() {
                    (state.report)(DropEvent::Dropped(paths));
                }
            }
            wl_data_device::Event::Selection { id: Some(offer) } => {
                // Clipboard offers are not this listener's business; forget them.
                state.offers.remove(&offer.id());
                offer.destroy();
            }
            _ => {}
        }
    }

    event_created_child!(State, wl_data_device::WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (wl_data_offer::WlDataOffer, ()),
    ]);
}

/// The running listener. Dropping it stops the thread.
pub struct WaylandDropListener {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for WaylandDropListener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                log::error!("the Wayland drop listener panicked");
            }
        }
    }
}

/// Starts the listener on winit's display connection.
///
/// # Safety
/// `display` must be the live `wl_display` of the window's connection, and stay alive while the
/// listener runs (winit keeps it for the event loop's lifetime).
pub unsafe fn start(
    display: *mut std::ffi::c_void,
    report: impl Fn(DropEvent) + Send + 'static,
) -> Result<WaylandDropListener, String> {
    // SAFETY: the caller passes winit's live display; the listener only joins it as a guest.
    let backend = unsafe { Backend::from_foreign_display(display.cast()) };
    let connection = Connection::from_backend(backend);
    let (globals, mut queue) =
        registry_queue_init::<State>(&connection).map_err(|error| error.to_string())?;
    let handle = queue.handle();
    let seat: wl_seat::WlSeat = globals
        .bind(&handle, 1..=7, ())
        .map_err(|error| format!("no seat: {error}"))?;
    let manager: wl_data_device_manager::WlDataDeviceManager = globals
        .bind(&handle, 1..=3, ())
        .map_err(|error| format!("no data-device manager: {error}"))?;
    let _device = manager.get_data_device(&seat, &handle, ());

    let mut state = State {
        connection: connection.clone(),
        report: Box::new(report),
        offers: HashMap::new(),
        current: None,
    };
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let thread = std::thread::Builder::new()
        .name("wayland-drop".into())
        .spawn(move || {
            let _device = _device;
            while !stopping.load(Ordering::SeqCst) {
                if let Err(error) = queue.dispatch_pending(&mut state) {
                    log::warn!("the Wayland drop listener stopped: {error}");
                    return;
                }
                if let Err(error) = queue.flush() {
                    log::warn!("the Wayland drop listener stopped: {error}");
                    return;
                }
                let Some(guard) = queue.prepare_read() else {
                    continue;
                };
                let mut poll = libc::pollfd {
                    fd: std::os::fd::AsRawFd::as_raw_fd(&guard.connection_fd()),
                    events: libc::POLLIN,
                    revents: 0,
                };
                // SAFETY: one valid pollfd for the connection's socket.
                let ready =
                    unsafe { libc::poll(&mut poll, 1, POLL_INTERVAL.as_millis() as libc::c_int) };
                if ready > 0 {
                    if let Err(error) = guard.read() {
                        log::warn!("the Wayland drop listener stopped: {error}");
                        return;
                    }
                }
            }
        })
        .map_err(|error| error.to_string())?;
    Ok(WaylandDropListener {
        stop,
        thread: Some(thread),
    })
}
