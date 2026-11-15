//! Projection helpers (plan D4): a projection writes a property only when its value changed, so
//! Slint re-evaluates and repaints only what the action touched. Comparing with the property's
//! current value, not a cache, also restores what a live-preview reload reset (D17).

/// Calls `set` with `value` unless the property already holds it.
pub fn set_if_changed<T: PartialEq>(current: T, value: T, set: impl FnOnce(T)) {
    if current != value {
        set(value);
    }
}
