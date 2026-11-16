//! Pure helpers: ports of `app/src/lib`, plus the computation behind the shared components that
//! has no React file (`color_picker` from color-picker.jsx; `number_input` and `placement`
//! reproduce what the webview's number input and Radix's positioning do).
pub mod activity;
pub mod choice_list;
pub mod color_picker;
pub mod file_dialog;
pub mod fonts;
pub mod format;
pub mod gradient_stops;
pub mod keyboard_shortcuts;
pub mod number_input;
pub mod placement;
pub mod preferences_store;
pub mod template;
pub mod utils;
pub mod widget;
