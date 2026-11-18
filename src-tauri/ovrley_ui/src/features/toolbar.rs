//! The toolbar feature: its projection into the `Toolbar` global and its callbacks.

use crate::hooks::app_store_selectors::{replace_if_changed, set_if_changed};
use crate::features::video_preview::{field_view, sync_view, transition_index};
use crate::{ActivityLapView, AppWindow, DrawerRowView, DrawerTool, JoinControlView, SelectedVideoView, Toolbar, VideoCardView, VideoExitView, VideoSegmentView};
use ovrley_app::features::seconds_fields::SecondsField;
use ovrley_app::features::toolbar::video_drawer::{self, MergeSide, SelectedVideoRow};
use ovrley_app::features::transitions::JoinModel;
use ovrley_app::lib::video_program::edits::CreationTimeSource;
use slint::{Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use ovrley_app::features::app_shell;
use ovrley_app::features::toolbar::activity_drawer_view;
use slint::SharedString;
use ovrley_app::store::slices::layout::LeftDrawerTool;
use ovrley_app::store::{Dispatcher, Store};
use slint::ComponentHandle;

fn to_slint(tool: LeftDrawerTool) -> DrawerTool {
    match tool {
        LeftDrawerTool::Projects => DrawerTool::Projects,
        LeftDrawerTool::Activity => DrawerTool::Activity,
        LeftDrawerTool::Video => DrawerTool::Video,
        LeftDrawerTool::VideoSync => DrawerTool::VideoSync,
        LeftDrawerTool::Widgets => DrawerTool::Widgets,
    }
}

fn from_slint(tool: DrawerTool) -> LeftDrawerTool {
    match tool {
        DrawerTool::Projects => LeftDrawerTool::Projects,
        DrawerTool::Activity => LeftDrawerTool::Activity,
        DrawerTool::Video => LeftDrawerTool::Video,
        DrawerTool::VideoSync => LeftDrawerTool::VideoSync,
        DrawerTool::Widgets => LeftDrawerTool::Widgets,
    }
}

pub fn project(window: &AppWindow, store: &Store) {
    let toolbar = window.global::<Toolbar>();
    let layout = &store.layout;
    set_if_changed(toolbar.get_active_tool(), to_slint(layout.active_left_drawer_tool), |value| toolbar.set_active_tool(value));
    set_if_changed(toolbar.get_visible(), layout.left_drawer_visible, |value| toolbar.set_visible(value));
    set_if_changed(toolbar.get_pinned(), layout.left_drawer_pinned, |value| toolbar.set_pinned(value));
    project_activity_drawer(&toolbar, store);
    project_video_drawer(&toolbar, store);
    project_video_panel(&toolbar, store);
}

fn join_control(store: &Store, model: &JoinModel) -> JoinControlView {
    JoinControlView {
        target: format!("join:{}", model.key).into(),
        type_label: model.type_label.as_str().into(),
        preview_index: transition_index(model.preview_type),
        has_duration: model.duration_seconds.is_some(),
        duration: field_view(store, &SecondsField::JoinDuration(model.key.clone())),
        error: model.error.clone().unwrap_or_default().into(),
    }
}

fn selected_view(store: &Store, row: &SelectedVideoRow) -> SelectedVideoView {
    let locale = store.i18n.language;
    let t = |key: &str, default: &'static str| ovrley_app::i18n::catalog().t_or(locale, key, default).to_string();
    let named = |key: &str, default: &'static str| ovrley_app::i18n::interpolate(&t(key, default), &[("name", &row.filename)]);
    let segments = row
        .segments
        .iter()
        .map(|segment| VideoSegmentView {
            key: segment.key.as_str().into(),
            range: format!("{} – {}", segment.start_label, segment.end_label).into(),
            included: segment.included,
            shows_overlay: segment.shows_overlay,
            overlay_label: format!("{}: {} {}", t("toolbar.showOverlay", "Show overlay"), row.filename, segment.start_label).into(),
            include_label: format!("{}: {} {}", t("toolbar.includeInExport", "Include in export"), row.filename, segment.start_label).into(),
            has_join: segment.join.is_some(),
            join: segment.join.as_ref().map(|join| join_control(store, join)).unwrap_or_default(),
            removable: segment.removal.is_some(),
            remove_label: match segment.removal.as_ref().map(|removal| removal.2) {
                Some(MergeSide::Next) => t("toolbar.removePartIntoNext", "Remove this part, joining it to the next part"),
                _ => t("toolbar.removePartIntoPrevious", "Remove this part, joining it to the previous part"),
            }
            .into(),
        })
        .collect::<Vec<_>>();
    let exits = row.exits.iter().map(|exit| VideoExitView { next_filename: exit.next_filename.as_str().into(), join: join_control(store, &exit.join) }).collect::<Vec<_>>();
    SelectedVideoView {
        id: row.id.as_str().into(),
        filename: row.filename.as_str().into(),
        path: row.path.as_str().into(),
        missing: row.missing,
        remove_label: named("toolbar.removeVideo", "Remove {{name}} from project").into(),
        warnings: ModelRc::new(VecModel::from(row.warnings.iter().map(SharedString::from).collect::<Vec<_>>())),
        metadata: ModelRc::new(VecModel::from(row.metadata.iter().map(|row| DrawerRowView { label: row.label.as_str().into(), value: row.value.as_str().into() }).collect::<Vec<_>>())),
        start: field_view(store, &SecondsField::DrawerVideoStart(row.id.clone())),
        creation_source: match row.creation_time_source {
            CreationTimeSource::Detected => "detected",
            CreationTimeSource::Filename => "filename",
        }
        .into(),
        filename_available: row.filename_creation_time_available,
        timezone_shown: row.timezone_shown,
        applies_timezone: row.applies_timezone,
        timezone_disabled: row.timezone_disabled,
        segments: ModelRc::new(VecModel::from(segments)),
        exits: ModelRc::new(VecModel::from(exits)),
    }
}

fn project_video_drawer(toolbar: &Toolbar<'_>, store: &Store) {
    set_if_changed(toolbar.get_has_videos(), store.video_import.has_videos(), |value| toolbar.set_has_videos(value));
    let row = video_drawer::selected_video_row(store);
    set_if_changed(toolbar.get_has_selected_video(), row.is_some(), |value| toolbar.set_has_selected_video(value));
    if let Some(row) = row {
        let next = selected_view(store, &row);
        let current = toolbar.get_selected_video();
        // Nested models compare by pointer; compare their rows before replacing the view.
        let same = current.id == next.id
            && current.start == next.start
            && current.filename == next.filename
            && current.missing == next.missing
            && current.creation_source == next.creation_source
            && current.applies_timezone == next.applies_timezone
            && current.timezone_shown == next.timezone_shown
            && current.timezone_disabled == next.timezone_disabled
            && current.filename_available == next.filename_available
            && models_equal(&current.warnings, &next.warnings)
            && models_equal(&current.metadata, &next.metadata)
            && models_equal(&current.segments, &next.segments)
            && models_equal(&current.exits, &next.exits);
        if !same {
            toolbar.set_selected_video(next);
        }
    }
}

fn models_equal<T: Clone + PartialEq + 'static>(left: &ModelRc<T>, right: &ModelRc<T>) -> bool {
    use slint::Model;
    left.row_count() == right.row_count() && left.iter().zip(right.iter()).all(|(left, right)| left == right)
}

thread_local! {
    /// Each video's decoded thumbnail, by the bytes it was decoded from.
    static THUMBNAILS: RefCell<HashMap<String, (Arc<[u8]>, Image)>> = RefCell::new(HashMap::new());
}

/// The card's image: the PNG decoded once per video.
fn thumbnail_image(id: &str, bytes: &Arc<[u8]>) -> Option<Image> {
    THUMBNAILS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((cached, image)) = cache.get(id) {
            if Arc::ptr_eq(cached, bytes) {
                return Some(image.clone());
            }
        }
        let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).ok()?.into_rgba8();
        let pixels = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(decoded.as_raw(), decoded.width(), decoded.height());
        let image = Image::from_rgba8(pixels);
        cache.insert(id.to_string(), (Arc::clone(bytes), image.clone()));
        Some(image)
    })
}

fn project_video_panel(toolbar: &Toolbar<'_>, store: &Store) {
    let shown = video_drawer::video_panel_shown(store);
    set_if_changed(toolbar.get_video_panel_shown(), shown, |value| toolbar.set_video_panel_shown(value));
    let locale = store.i18n.language;
    let cards = video_drawer::video_cards(store)
        .into_iter()
        .map(|card| {
            let image = card.thumbnail.as_ref().and_then(|bytes| thumbnail_image(&card.id, bytes));
            VideoCardView {
                id: card.id.as_str().into(),
                remove_label: ovrley_app::i18n::interpolate(ovrley_app::i18n::catalog().t_or(locale, "toolbar.removeVideo", "Remove {{name}} from project"), &[("name", &card.filename)]).into(),
                filename: card.filename.into(),
                duration: card.duration_label.into(),
                has_thumbnail: image.is_some(),
                thumbnail: image.unwrap_or_default(),
                missing: card.missing,
                selected: card.selected,
            }
        })
        .collect();
    replace_if_changed(toolbar.get_video_cards(), cards, |model| toolbar.set_video_cards(model));
    if !shown {
        return;
    }
    let timezone = store.media.parsed_activity.as_ref().and_then(|_| store.media.activity_summary.as_ref()).and_then(|summary| summary.timezone.as_deref());
    let controls = ovrley_app::features::video_sync::derive_video_sync_controls(&store.video_import.imported_videos, &store.video_import.video_edits, timezone);
    set_if_changed(toolbar.get_panel_sync(), sync_view(store, &controls, &SecondsField::PanelTransitionDuration), |value| toolbar.set_panel_sync(value));
    let defaults = &store.video_import.transition_defaults;
    let label = ovrley_app::features::transitions::default_transition_label(&defaults.transition_type, locale);
    set_if_changed(toolbar.get_panel_transition_label(), SharedString::from(label), |value| toolbar.set_panel_transition_label(value));
    let preview = (defaults.transition_type != ovrley_core::encode::ffmpeg::transitions::NO_TRANSITION).then_some(defaults.transition_type.as_str());
    set_if_changed(toolbar.get_panel_transition_preview_index(), transition_index(preview), |value| toolbar.set_panel_transition_preview_index(value));
    set_if_changed(toolbar.get_panel_transition_duration(), field_view(store, &SecondsField::PanelTransitionDuration), |value| toolbar.set_panel_transition_duration(value));
    set_if_changed(toolbar.get_panel_continuity_threshold(), field_view(store, &SecondsField::PanelContinuityThreshold), |value| toolbar.set_panel_continuity_threshold(value));
}

fn project_activity_drawer(toolbar: &Toolbar<'_>, store: &Store) {
    let view = activity_drawer_view(store);
    set_if_changed(toolbar.get_has_activity(), view.is_some(), |value| toolbar.set_has_activity(value));
    let Some(view) = view else { return };
    set_if_changed(toolbar.get_activity_file_name(), SharedString::from(view.file_name), |value| toolbar.set_activity_file_name(value));
    set_if_changed(toolbar.get_can_delete_activity(), view.can_delete, |value| toolbar.set_can_delete_activity(value));
    set_if_changed(toolbar.get_activity_format(), SharedString::from(view.format_label), |value| toolbar.set_activity_format(value));
    let rows = view.metadata.into_iter().map(|row| DrawerRowView { label: row.label.into(), value: row.value.into() }).collect();
    replace_if_changed(toolbar.get_activity_metadata(), rows, |model| toolbar.set_activity_metadata(model));
    replace_if_changed(toolbar.get_activity_extracted(), view.extracted.into_iter().map(SharedString::from).collect(), |model| toolbar.set_activity_extracted(model));
    replace_if_changed(toolbar.get_activity_derived(), view.derived.into_iter().map(SharedString::from).collect(), |model| toolbar.set_activity_derived(model));
    let laps = view
        .laps
        .into_iter()
        .map(|lap| ActivityLapView {
            id: lap.id as i32,
            number: lap.number.into(),
            duration: lap.duration.into(),
            start: lap.start.into(),
            enabled: lap.enabled,
        })
        .collect();
    replace_if_changed(toolbar.get_activity_laps(), laps, |model| toolbar.set_activity_laps(model));
}

/// Runs a drawer or panel command; a rejected one is logged, as React's throw lands in the console.
fn command(dispatcher: &Dispatcher, run: impl FnOnce(&mut Store, &mut ovrley_app::store::services::Services) -> Result<(), String> + 'static) {
    dispatcher.dispatch(move |store, services| {
        if let Err(error) = run(store, services) {
            log::error!("video drawer: {error}");
        }
    });
}

fn bind_video(toolbar: &Toolbar<'_>, dispatcher: &Dispatcher) {
    use ovrley_app::features::video_preview::import;
    use ovrley_app::store::slices::video_import as slice;
    use ovrley_core::media::video_edits::TimezoneMode;
    let mode = |utc: bool| if utc { TimezoneMode::Utc } else { TimezoneMode::Local };
    let d = dispatcher.clone();
    toolbar.on_load_videos(move || d.dispatch(import::open_video_import_dialog));
    let d = dispatcher.clone();
    toolbar.on_remove_video(move |id| command(&d, move |store, _| ovrley_app::features::player::remove_video(store, &id)));
    let d = dispatcher.clone();
    toolbar.on_locate_video(move |id| d.dispatch(move |store, services| video_drawer::open_locate_dialog(store, services, id.to_string())));
    let d = dispatcher.clone();
    toolbar.on_sync_video(move |id| command(&d, move |store, _| slice::reset_video_offset(store, &id)));
    let d = dispatcher.clone();
    toolbar.on_set_creation_source(move |id, source| {
        let source = if source == "filename" { CreationTimeSource::Filename } else { CreationTimeSource::Detected };
        command(&d, move |store, _| slice::set_video_creation_time_source(store, &id, source))
    });
    let d = dispatcher.clone();
    toolbar.on_set_applies_timezone(move |id, applies| command(&d, move |store, _| slice::set_video_timezone_mode(store, &id, mode(applies))));
    let d = dispatcher.clone();
    toolbar.on_set_segment_included(move |id, key, included| command(&d, move |store, _| slice::set_video_segment_excluded(store, &id, &key, !included)));
    let d = dispatcher.clone();
    toolbar.on_set_segment_overlay(move |id, key, shows| command(&d, move |store, _| slice::set_video_segment_skip_overlay(store, &id, &key, !shows)));
    let d = dispatcher.clone();
    toolbar.on_remove_segment(move |id, key| command(&d, move |store, _| video_drawer::remove_segment(store, &id, &key)));
    let d = dispatcher.clone();
    toolbar.on_seek_segment(move |id, key| command(&d, move |store, _| video_drawer::seek_segment(store, &id, &key)));
    let d = dispatcher.clone();
    toolbar.on_sync_all_videos(move || command(&d, |store, _| slice::sync_all_videos(store)));
    let d = dispatcher.clone();
    toolbar.on_panel_reset_creation_times(move || command(&d, |store, _| slice::reset_video_creation_times(store)));
    let d = dispatcher.clone();
    toolbar.on_panel_filename_creation_times(move || command(&d, |store, _| slice::set_video_creation_times_from_filenames(store)));
    let d = dispatcher.clone();
    toolbar.on_panel_set_utc(move |utc| command(&d, move |store, _| slice::set_video_sync_timezone_mode(store, mode(utc))));
    let d = dispatcher.clone();
    toolbar.on_remove_all_videos(move || d.dispatch(|store, _| import::clear_videos(store)));
    let d = dispatcher.clone();
    toolbar.on_select_card(move |id| {
        command(&d, move |store, _| slice::select_clip(store, Some(slice::SelectedClip { lane: slice::ClipLane::Video(id.to_string()), segment_key: None })))
    });
}

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    let toolbar = window.global::<Toolbar>();
    bind_video(&toolbar, dispatcher);
    let dispatch = dispatcher.handle();
    toolbar.on_select_tool(move |tool| {
        let tool = from_slint(tool);
        dispatch.dispatch(move |store, _| store.layout.select_left_drawer_tool(tool));
    });
    let dispatch = dispatcher.handle();
    toolbar.on_set_pinned(move |pinned| dispatch.dispatch(move |store, _| store.layout.set_left_drawer_pinned(pinned)));
    let dispatch = dispatcher.handle();
    toolbar.on_dismiss_overlay(move || dispatch.dispatch(|store, _| store.layout.dismiss_left_drawer_overlay()));
    let dispatch = dispatcher.handle();
    toolbar.on_load_activity(move || dispatch.dispatch(app_shell::open_activity_dialog));
    let dispatch = dispatcher.handle();
    toolbar.on_delete_activity(move || {
        dispatch.dispatch(|store, _| {
            if let Err(error) = app_shell::delete_activity(store) {
                log::error!("{error}");
            }
        })
    });
    let dispatch = dispatcher.handle();
    toolbar.on_set_lap_enabled(move |id, enabled| {
        // The table lists the activity's lap ids, which are never negative.
        let id = usize::try_from(id).expect("lap ids are not negative");
        dispatch.dispatch(move |store, _| store.media.set_lap_enabled(id, enabled));
    });
}
