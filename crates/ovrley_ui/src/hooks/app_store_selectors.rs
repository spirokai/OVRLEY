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

/// Keeps a `VecModel` in step with `next` by key (plan D1): rows whose key stays are kept, and
/// `update` rewrites the ones whose value changed; rows are inserted and removed at their position.
/// So a row that stays keeps its element, a dragged slider its pointer and an input its focus.
/// `previous` is what the model holds now, as the values it was built from; comparing those, not
/// the Slint rows, keeps unchanged rows from being rewritten.
pub fn sync_keyed<R: PartialEq, T: Clone + 'static>(
    model: &VecModel<T>,
    previous: &[R],
    next: &[R],
    key: impl Fn(&R) -> &str,
    create: impl Fn(&R) -> T,
    mut update: impl FnMut(usize, &R, &R),
) {
    debug_assert_eq!(model.row_count(), previous.len(), "the model holds `previous`");
    let mut current: Vec<&R> = previous.iter().collect();
    for (index, item) in next.iter().enumerate() {
        match (index..current.len()).find(|&row| key(current[row]) == key(item)) {
            Some(row) => {
                for _ in index..row {
                    model.remove(index);
                    current.remove(index);
                }
                if current[index] != item {
                    update(index, current[index], item);
                }
                current[index] = item;
            }
            None => {
                model.insert(index, create(item));
                current.insert(index, item);
            }
        }
    }
    while current.len() > next.len() {
        model.remove(next.len());
        current.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sync(model: &VecModel<String>, previous: &[(&str, i32)], next: &[(&str, i32)]) -> Vec<usize> {
        let mut updated = Vec::new();
        sync_keyed(model, previous, next, |row| row.0, |row| format!("{}={}", row.0, row.1), |index, _, row| {
            updated.push(index);
            model.set_row_data(index, format!("{}={}", row.0, row.1));
        });
        updated
    }

    fn rows(model: &VecModel<String>) -> Vec<String> {
        model.iter().collect()
    }

    #[test]
    fn keyed_rows_update_in_place_and_insert_or_remove_at_their_position() {
        let model = VecModel::default();
        let first = [("a", 1), ("b", 1), ("c", 1)];
        assert!(sync(&model, &[], &first).is_empty(), "new rows are created, not updated");
        assert_eq!(rows(&model), ["a=1", "b=1", "c=1"]);

        // A value change rewrites only its row; the others are left alone.
        let changed = [("a", 1), ("b", 2), ("c", 1)];
        assert_eq!(sync(&model, &first, &changed), [1]);
        assert_eq!(rows(&model), ["a=1", "b=2", "c=1"]);

        // A row shown by a toggle is inserted where it belongs; a hidden one is removed.
        let inserted = [("a", 1), ("x", 1), ("b", 2), ("c", 1)];
        assert!(sync(&model, &changed, &inserted).is_empty());
        assert_eq!(rows(&model), ["a=1", "x=1", "b=2", "c=1"]);
        let removed = [("a", 1), ("c", 1)];
        assert!(sync(&model, &inserted, &removed).is_empty());
        assert_eq!(rows(&model), ["a=1", "c=1"]);
        assert!(sync(&model, &removed, &[]).is_empty());
        assert_eq!(model.row_count(), 0);
    }
}
