//! What every feature's projection shares: writes that happen only when a value changed, models
//! kept in step row by row, images built from pixels, icons, and the conversions between Slint's
//! generated enums and the app's.
//!
//! A projection writes a property only when its value changed, so Slint re-evaluates and repaints
//! only what the action touched. Comparing with the property's current value, not a cache, also
//! restores what a live-preview reload reset to its default.

use crate::Icons;
use ovrley_app::store::services::Services;
use ovrley_app::store::{DispatchHandle, Store};
use slint::{Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};
use std::cell::RefCell;
use std::collections::HashMap;

/// Calls `set` with `value` unless the property already holds it.
pub fn set_if_changed<T: PartialEq>(current: T, value: T, set: impl FnOnce(T)) {
    if current != value {
        set(value);
    }
}

/// Writes `next` into a repeater's model. Rows of an unchanged count update in place, so the
/// repeater keeps their elements, their textures and their animations (a dragged widget moves,
/// a lap's switch slides). A changed count resets the model.
pub fn replace_if_changed<T: Clone + PartialEq + 'static>(
    current: ModelRc<T>,
    next: Vec<T>,
    set: impl FnOnce(ModelRc<T>),
) {
    let Some(model) = current.as_any().downcast_ref::<VecModel<T>>() else {
        set(ModelRc::new(VecModel::from(next)));
        return;
    };
    if model.row_count() != next.len() {
        model.set_vec(next);
        return;
    }
    for (row, value) in next.into_iter().enumerate() {
        if model.row_data(row).as_ref() != Some(&value) {
            model.set_row_data(row, value);
        }
    }
}

/// Whether two models hold equal rows. Models compare by pointer, so a struct that holds one
/// compares its models with this.
pub fn models_equal<T: PartialEq + 'static>(left: &ModelRc<T>, right: &ModelRc<T>) -> bool {
    left.row_count() == right.row_count() && left.iter().eq(right.iter())
}

/// Strings as a model.
pub fn strings<S: AsRef<str>>(values: &[S]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        values
            .iter()
            .map(|value| SharedString::from(value.as_ref()))
            .collect::<Vec<_>>(),
    ))
}

/// Keeps a `VecModel` in step with `next` by key: rows whose key stays are kept, and `update`
/// rewrites the ones whose value changed; rows are inserted and removed at their position. So a
/// row that stays keeps its element, a dragged slider its pointer and an input its focus.
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
    debug_assert_eq!(
        model.row_count(),
        previous.len(),
        "the model holds `previous`"
    );
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

/// Straight RGBA pixels as an image.
pub fn rgba_image(pixels: &[u8], width: u32, height: u32) -> Image {
    Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        pixels, width, height,
    ))
}

/// A normalized `#rrggbb` string as a colour.
///
/// # Panics
///
/// On any other string: callers pass colours the app normalized.
pub fn hex_color(hex: &str) -> slint::Color {
    let value = hex
        .strip_prefix('#')
        .filter(|digits| digits.len() == 6)
        .and_then(|digits| u32::from_str_radix(digits, 16).ok())
        .unwrap_or_else(|| panic!("a normalized #rrggbb colour, got {hex:?}"));
    let [_, red, green, blue] = value.to_be_bytes();
    slint::Color::from_rgb_u8(red, green, blue)
}

/// PNG images by key, each decoded once per content.
#[derive(Default)]
pub struct PngImages {
    images: RefCell<HashMap<String, (Vec<u8>, Image)>>,
}

impl PngImages {
    /// The image of `bytes`, decoded unless `key` last held the same bytes. `None` when they are no
    /// PNG.
    pub fn image(&self, key: &str, bytes: &[u8]) -> Option<Image> {
        let mut images = self.images.borrow_mut();
        if let Some((cached, image)) = images.get(key) {
            if cached.as_slice() == bytes {
                return Some(image.clone());
            }
        }
        // A thumbnail that is no PNG shows none, as a file without one does.
        let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).ok()?.into_rgba8();
        let image = rgba_image(decoded.as_raw(), decoded.width(), decoded.height());
        images.insert(key.to_string(), (bytes.to_vec(), image.clone()));
        Some(image)
    }
}

/// Runs `run` as an action and logs its error. For commands whose errors are bugs in what the UI
/// offered: there is nothing to show the user.
pub fn dispatch_logged(
    dispatch: &DispatchHandle,
    context: &'static str,
    run: impl FnOnce(&mut Store, &mut Services) -> Result<(), String> + 'static,
) {
    dispatch.dispatch(move |store, services| {
        if let Err(error) = run(store, services) {
            log::error!("{context}: {error}");
        }
    });
}

/// `From` both ways between a Slint enum and the app's enum of the same variants. Slint generates
/// its own types, which `ovrley_app` cannot name, so this is the one place they meet.
macro_rules! convert_enum {
    ($ui:ident <=> $app:ident { $($variant:ident),+ $(,)? }) => {
        impl From<$app> for $ui {
            fn from(value: $app) -> Self {
                match value {
                    $($app::$variant => Self::$variant),+
                }
            }
        }

        impl From<$ui> for $app {
            fn from(value: $ui) -> Self {
                match value {
                    $($ui::$variant => Self::$variant),+
                }
            }
        }
    };
}
pub(crate) use convert_enum;

/// The `Icons` global's glyph for a lucide name.
///
/// # Panics
///
/// On a name the global lacks: `examples/fetch_icons.rs` generates it with every name the app uses.
pub fn glyph(icons: &Icons<'_>, name: &str) -> SharedString {
    match name {
        "activity" => icons.get_activity(),
        "align-center" => icons.get_align_center(),
        "align-left" => icons.get_align_left(),
        "align-right" => icons.get_align_right(),
        "arrow-up-down" => icons.get_arrow_up_down(),
        "ban" => icons.get_ban(),
        "bell" => icons.get_bell(),
        "blocks" => icons.get_blocks(),
        "calendar-days" => icons.get_calendar_days(),
        "chart-line" => icons.get_chart_line(),
        "chart-no-axes-gantt" => icons.get_chart_no_axes_gantt(),
        "check" => icons.get_check(),
        "chevron-down" => icons.get_chevron_down(),
        "chevron-up" => icons.get_chevron_up(),
        "chevrons-right" => icons.get_chevrons_right(),
        "circle-alert" => icons.get_circle_alert(),
        "circle-check" => icons.get_circle_check(),
        "circle-gauge" => icons.get_circle_gauge(),
        "clock-3" => icons.get_clock_3(),
        "clock" => icons.get_clock(),
        "compass" => icons.get_compass(),
        "copy" => icons.get_copy(),
        "corner-up-left" => icons.get_corner_up_left(),
        "corner-up-right" => icons.get_corner_up_right(),
        "database" => icons.get_database(),
        "download" => icons.get_download(),
        "external-link" => icons.get_external_link(),
        "eye" => icons.get_eye(),
        "eye-off" => icons.get_eye_off(),
        "file-box" => icons.get_file_box(),
        "file-plus-2" => icons.get_file_plus_2(),
        "file-up" => icons.get_file_up(),
        "files" => icons.get_files(),
        "film" => icons.get_film(),
        "flag" => icons.get_flag(),
        "folder-kanban" => icons.get_folder_kanban(),
        "folder-open" => icons.get_folder_open(),
        "gauge" => icons.get_gauge(),
        "grid-3x3" => icons.get_grid_3x3(),
        "grip-vertical" => icons.get_grip_vertical(),
        "hash" => icons.get_hash(),
        "heart-pulse" => icons.get_heart_pulse(),
        "image-down" => icons.get_image_down(),
        "image" => icons.get_image(),
        "info" => icons.get_info(),
        "italic" => icons.get_italic(),
        "keyboard" => icons.get_keyboard(),
        "layout-grid" => icons.get_layout_grid(),
        "loader-circle" => icons.get_loader_circle(),
        "magnet" => icons.get_magnet(),
        "map-pin" => icons.get_map_pin(),
        "map" => icons.get_map(),
        "menu" => icons.get_menu(),
        "minus" => icons.get_minus(),
        "mountain" => icons.get_mountain(),
        "move" => icons.get_move(),
        "navigation" => icons.get_navigation(),
        "octagon-minus" => icons.get_octagon_minus(),
        "palette" => icons.get_palette(),
        "pause" => icons.get_pause(),
        "pin-off" => icons.get_pin_off(),
        "pin" => icons.get_pin(),
        "pipette" => icons.get_pipette(),
        "play" => icons.get_play(),
        "plus" => icons.get_plus(),
        "presentation" => icons.get_presentation(),
        "redo-2" => icons.get_redo_2(),
        "refresh-cw" => icons.get_refresh_cw(),
        "rewind" => icons.get_rewind(),
        "rotate-ccw" => icons.get_rotate_ccw(),
        "rotate-cw-clock" => icons.get_rotate_cw_clock(),
        "route" => icons.get_route(),
        "rows-3" => icons.get_rows_3(),
        "ruler" => icons.get_ruler(),
        "save" => icons.get_save(),
        "scissors" => icons.get_scissors(),
        "settings-2" => icons.get_settings_2(),
        "settings" => icons.get_settings(),
        "shapes" => icons.get_shapes(),
        "sliders-horizontal" => icons.get_sliders_horizontal(),
        "sparkles" => icons.get_sparkles(),
        "square-chevron-left" => icons.get_square_chevron_left(),
        "square" => icons.get_square(),
        "step-back" => icons.get_step_back(),
        "step-forward" => icons.get_step_forward(),
        "tags" => icons.get_tags(),
        "target" => icons.get_target(),
        "timer" => icons.get_timer(),
        "trash-2" => icons.get_trash_2(),
        "trending-up" => icons.get_trending_up(),
        "triangle-alert" => icons.get_triangle_alert(),
        "type" => icons.get_type(),
        "undo-2" => icons.get_undo_2(),
        "video" => icons.get_video(),
        "volume-2" => icons.get_volume_2(),
        "volume-x" => icons.get_volume_x(),
        "x" => icons.get_x(),
        "zoom-in" => icons.get_zoom_in(),
        "zoom-out" => icons.get_zoom_out(),
        other => panic!("the Icons global has no glyph {other}"),
    }
}

/// A manifest's PascalCase lucide name (`Navigation`, `ChartLine`) as the `Icons` name.
fn kebab(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (index, character) in name.chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            out.push('-');
        }
        out.push(character.to_ascii_lowercase());
    }
    out
}

/// Manifest icons as the `Icons` glyph or a bundled SVG image, each SVG decoded once.
#[derive(Default)]
pub struct IconCache {
    images: RefCell<HashMap<String, Image>>,
}

impl IconCache {
    /// A manifest icon: `(glyph, image)`, one of them empty.
    ///
    /// # Panics
    ///
    /// When a bundled SVG is invalid or the reference names nothing: both are catalog bugs.
    pub fn icon(
        &self,
        icons: &Icons<'_>,
        icon: &ovrley_widgets::model::catalog::IconRef,
    ) -> (SharedString, Image) {
        if let Some(asset_file) = &icon.asset_file {
            let image = self
                .images
                .borrow_mut()
                .entry(asset_file.clone())
                .or_insert_with(|| {
                    Image::load_from_svg_data(
                        ovrley_widgets::render::icons::icon_asset_markup(asset_file).as_bytes(),
                    )
                    .unwrap_or_else(|error| {
                        panic!("the bundled icon {asset_file} is not a valid SVG: {error:?}")
                    })
                })
                .clone();
            return (SharedString::new(), image);
        }
        let name = icon
            .name
            .as_deref()
            .unwrap_or_else(|| panic!("manifest icon {icon:?} has neither a name nor an asset"));
        (glyph(icons, &kebab(name)), Image::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sync(
        model: &VecModel<String>,
        previous: &[(&str, i32)],
        next: &[(&str, i32)],
    ) -> Vec<usize> {
        let mut updated = Vec::new();
        sync_keyed(
            model,
            previous,
            next,
            |row| row.0,
            |row| format!("{}={}", row.0, row.1),
            |index, _, row| {
                updated.push(index);
                model.set_row_data(index, format!("{}={}", row.0, row.1));
            },
        );
        updated
    }

    fn rows(model: &VecModel<String>) -> Vec<String> {
        model.iter().collect()
    }

    #[test]
    fn keyed_rows_update_in_place_and_insert_or_remove_at_their_position() {
        let model = VecModel::default();
        let first = [("a", 1), ("b", 1), ("c", 1)];
        assert!(
            sync(&model, &[], &first).is_empty(),
            "new rows are created, not updated"
        );
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
