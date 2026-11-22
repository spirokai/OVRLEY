//! UI-independent application logic for the Slint frontend.
//!
//! `lib`, `hooks`, `store` and `features` mirror the folders of the React app it was ported
//! from. The components themselves live in ovrley_ui; their state, actions and computation live here.

pub mod features;
pub mod hooks;
pub mod i18n;
pub mod js;
// `src/lib.rs` is the crate root, so the `lib` module's directory is named explicitly.
#[path = "lib/mod.rs"]
pub mod lib;
pub mod store;
