//! The `Icons` global by lucide name, for projections whose view names a glyph.

use crate::Icons;
use slint::SharedString;

/// The `Icons` global's glyph for a lucide name a description uses.
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
pub fn kebab(name: &str) -> String {
    let mut out = String::new();
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
    images: std::cell::RefCell<std::collections::HashMap<String, slint::Image>>,
}

impl IconCache {
    /// A manifest icon: `(glyph, image)`, one of them empty.
    pub fn icon(&self, icons: &Icons<'_>, icon: &ovrley_widgets::model::catalog::IconRef) -> (SharedString, slint::Image) {
        if let Some(asset_file) = &icon.asset_file {
            let image = self
                .images
                .borrow_mut()
                .entry(asset_file.clone())
                .or_insert_with(|| {
                    slint::Image::load_from_svg_data(ovrley_widgets::render::icons::icon_asset_markup(asset_file).as_bytes())
                        .unwrap_or_else(|error| panic!("the bundled icon {asset_file} is not a valid SVG: {error:?}"))
                })
                .clone();
            return (SharedString::new(), image);
        }
        let name = icon.name.as_deref().unwrap_or_else(|| panic!("manifest icon {icon:?} has neither a name nor an asset"));
        (glyph(icons, &kebab(name)), slint::Image::default())
    }
}
