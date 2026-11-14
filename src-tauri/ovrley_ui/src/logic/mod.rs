//! Binds the logic globals (D2) to `ovrley_app`, one binder per global. Binders only convert
//! between Slint types and `ovrley_app` types.

mod choice_list;
mod color_picker;
mod gradient_picker;
mod number_input;
mod placement;

use crate::{
    ChoiceListLogic, ColorPickerLogic, GradientPickerLogic, NumberInputLogic, PlacementLogic,
};
use slint::{ComponentHandle, Global};

/// Installs every logic global on a window. Every window runs it at start: the showcase, the
/// test harnesses and the app window.
pub fn install_component_logic<C>(window: &C)
where
    C: ComponentHandle,
    for<'a> ChoiceListLogic<'a>: Global<'a, C>,
    for<'a> ColorPickerLogic<'a>: Global<'a, C>,
    for<'a> GradientPickerLogic<'a>: Global<'a, C>,
    for<'a> NumberInputLogic<'a>: Global<'a, C>,
    for<'a> PlacementLogic<'a>: Global<'a, C>,
{
    choice_list::bind(&window.global::<ChoiceListLogic>());
    color_picker::bind(&window.global::<ColorPickerLogic>());
    gradient_picker::bind(&window.global::<GradientPickerLogic>());
    number_input::bind(&window.global::<NumberInputLogic>());
    placement::bind(&window.global::<PlacementLogic>());
}
