//! Port of `store/slices/createMediaSlice.js`. Phase 2 ports only the error message, which the
//! shell's error alert shows; Phase 3 adds the rest of the slice.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MediaSlice {
    pub error_message: Option<String>,
}

impl MediaSlice {
    pub fn set_error_message(&mut self, message: String) {
        self.error_message = Some(message);
    }

    pub fn clear_error(&mut self) {
        self.error_message = None;
    }
}
