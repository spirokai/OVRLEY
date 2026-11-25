//! The logic behind the shared form controls: list navigation, colour and gradient editing,
//! number and seconds inputs, and popup placement. Slint draws the controls and asks these for
//! every result it cannot compute in markup.
pub mod choice;
pub mod color;
pub mod gradient;
pub mod number;
pub mod placement;
pub mod seconds;
