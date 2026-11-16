//! Projection helpers (plan D4): a projection writes a property only when its value changed, so
//! Slint re-evaluates and repaints only what the action touched. Comparing with the property's
//! current value, not a cache, also restores what a live-preview reload reset (D17).

use slint::{Model, ModelRc, VecModel};

/// Calls `set` with `value` unless the property already holds it.
pub fn set_if_changed<T: PartialEq>(current: T, value: T, set: impl FnOnce(T)) {
    if current != value {
        set(value);
    }
}

/// Writes `next` into the model. Rows of an unchanged length update in place, so the repeater
/// keeps their elements and their animations play (a lap's switch slides instead of jumping).
pub fn replace_if_changed<T: Clone + PartialEq + 'static>(current: ModelRc<T>, next: Vec<T>, set: impl FnOnce(ModelRc<T>)) {
    if current.row_count() != next.len() {
        set(ModelRc::new(VecModel::from(next)));
        return;
    }
    for (row, value) in next.into_iter().enumerate() {
        if current.row_data(row).as_ref() != Some(&value) {
            current.set_row_data(row, value);
        }
    }
}
