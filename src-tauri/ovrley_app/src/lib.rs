//! UI-independent application logic for the Slint frontend.
//!
//! `lib` and `hooks` mirror `app/src/lib` and `app/src/hooks`. The components themselves live in
//! ovrley_ui; their computation lives here.

pub mod hooks;
pub mod js;
// `src/lib.rs` is the crate root, so the `lib` module's directory is named explicitly.
#[path = "lib/mod.rs"]
pub mod lib;
