//! Production widget ABI. All input pointers reference live `_ovrley_alloc` allocations.
//! Output pointers are borrowed until the next session mutation or destruction.
use ovrley_widgets::{
    error::{WidgetError, WidgetResult},
    preview::PreviewSession,
    preview_widget::WidgetRequest,
};

#[derive(Default)]
pub struct Session {
    preview: PreviewSession,
    error: Vec<u8>,
    resize_update: Vec<u8>,
}

impl Session {
    fn status(&mut self, result: WidgetResult<()>) -> u32 {
        match result {
            Ok(()) => {
                self.error.clear();
                0
            }
            Err(error) => {
                self.error = error.to_string().into_bytes();
                1
            }
        }
    }
}

unsafe fn bytes<'a>(ptr: *const u8, len: usize) -> WidgetResult<&'a [u8]> {
    if len == 0 {
        return Ok(&[]);
    }
    if ptr.is_null() {
        return Err(WidgetError::Config("missing ABI input buffer".into()));
    }
    Ok(std::slice::from_raw_parts(ptr, len))
}

unsafe fn request(ptr: *const u8, len: usize) -> WidgetResult<WidgetRequest> {
    serde_json::from_slice(bytes(ptr, len)?).map_err(|e| WidgetError::Config(e.to_string()))
}

#[no_mangle]
pub extern "C" fn ovrley_preview_create() -> *mut Session {
    Box::into_raw(Box::default())
}

#[no_mangle]
pub unsafe extern "C" fn ovrley_preview_destroy(session: *mut Session) {
    drop(Box::from_raw(session));
}

#[no_mangle]
pub unsafe extern "C" fn ovrley_preview_activity(
    session: *mut Session,
    ptr: *const u8,
    len: usize,
) -> u32 {
    let session = &mut *session;
    let result = bytes(ptr, len).and_then(|b| session.preview.replace_activity(b));
    session.status(result)
}

#[no_mangle]
pub unsafe extern "C" fn ovrley_preview_font(
    session: *mut Session,
    id: *const u8,
    id_len: usize,
    ptr: *const u8,
    len: usize,
) -> u32 {
    let session = &mut *session;
    let result = (|| {
        let request = serde_json::from_slice(bytes(id, id_len)?)
            .map_err(|e| WidgetError::Config(e.to_string()))?;
        session
            .preview
            .fonts
            .register(request, bytes(ptr, len)?)
    })();
    session.status(result)
}

#[no_mangle]
pub unsafe extern "C" fn ovrley_preview_widget_create(
    session: *mut Session,
    ptr: *const u8,
    len: usize,
) -> u32 {
    let session = &mut *session;
    match request(ptr, len).and_then(|r| session.preview.create_widget(r)) {
        Ok(id) => {
            session.error.clear();
            id
        }
        Err(error) => {
            session.status(Err(error));
            0
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn ovrley_preview_widget_update(
    session: *mut Session,
    id: u32,
    ptr: *const u8,
    len: usize,
) -> u32 {
    let session = &mut *session;
    let result = request(ptr, len).and_then(|r| session.preview.update_widget(id, r));
    session.status(result)
}

#[no_mangle]
pub unsafe extern "C" fn ovrley_preview_widget_destroy(session: *mut Session, id: u32) {
    (*session)
        .preview
        .widgets
        .remove(&id)
        .expect("live widget handle");
}

#[no_mangle]
pub unsafe extern "C" fn ovrley_preview_render(session: *mut Session, second: f64) -> u32 {
    let session = &mut *session;
    let result = session.preview.render(second);
    session.status(result)
}

#[no_mangle]
pub unsafe extern "C" fn ovrley_preview_error_ptr(session: *const Session) -> *const u8 {
    (*session).error.as_ptr()
}
#[no_mangle]
pub unsafe extern "C" fn ovrley_preview_error_len(session: *const Session) -> usize {
    (*session).error.len()
}
#[no_mangle]
pub unsafe extern "C" fn ovrley_preview_pixels(session: *const Session, id: u32) -> *const u8 {
    (&(*session).preview.widgets)[&id].pixels().as_ptr()
}
#[no_mangle]
pub unsafe extern "C" fn ovrley_preview_geometry_ptr(
    session: *const Session,
    id: u32,
) -> *const u8 {
    (&(*session).preview.widgets)[&id].geometry().as_ptr()
}
#[no_mangle]
pub unsafe extern "C" fn ovrley_preview_geometry_len(session: *const Session, id: u32) -> usize {
    (&(*session).preview.widgets)[&id].geometry().len()
}
#[no_mangle]
pub unsafe extern "C" fn ovrley_preview_revision(session: *const Session, id: u32) -> u32 {
    (&(*session).preview.widgets)[&id].revision()
}

#[no_mangle]
pub unsafe extern "C" fn ovrley_preview_resize(
    session: *mut Session,
    ptr: *const u8,
    len: usize,
) -> u32 {
    let session = &mut *session;
    let result = (|| {
        let request = serde_json::from_slice(bytes(ptr, len)?)
            .map_err(|e| WidgetError::Config(e.to_string()))?;
        let update = session.preview.resize_widget(request)?;
        session.resize_update =
            serde_json::to_vec(&update).map_err(|e| WidgetError::Render(e.to_string()))?;
        Ok(())
    })();
    session.status(result)
}

#[no_mangle]
pub unsafe extern "C" fn ovrley_preview_resize_ptr(session: *const Session) -> *const u8 {
    (*session).resize_update.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn ovrley_preview_resize_len(session: *const Session) -> usize {
    (*session).resize_update.len()
}
