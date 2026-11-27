//! The left toolbar and its drawers (`Toolbar`): the drawer tool, the activity drawer, the video
//! drawer with the selected video's parts and joins, and the video panel's cards and defaults.

use crate::features::import::{field_view, sync_view, timezone_mode};
use crate::features::transitions::transition_index;
use crate::projection::{convert_enum, dispatch_logged, models_equal, replace_if_changed, set_if_changed, strings, PngImages};
use crate::{
    ActivityLapView, AppWindow, CreationTimeSource, DrawerRowView, DrawerTool, JoinControlView, SelectedVideoView, Toolbar, VideoCardView, VideoExitView, VideoSegmentView,
};
use ovrley_app::features::shell;
use ovrley_app::controls::seconds::SecondsField;
use ovrley_app::features::toolbar::activity_drawer_view;
use ovrley_app::features::toolbar::{self, MergeSide, SelectedVideoRow};
use ovrley_app::features::transitions::JoinModel;
use ovrley_app::features::import;
use ovrley_app::i18n::{catalog, interpolate};
use ovrley_app::video::edits::CreationTimeSource as AppCreationTimeSource;
use ovrley_app::features::toolbar::LeftDrawerTool;
use ovrley_app::store::videos as videos;
use ovrley_app::store::{Dispatcher, Store};
use ovrley_core::encode::ffmpeg::transitions::NO_TRANSITION;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

convert_enum!(DrawerTool <=> LeftDrawerTool { Projects, Activity, Video, VideoSync, Widgets });
convert_enum!(CreationTimeSource <=> AppCreationTimeSource { Detected, Filename });

/// The video cards' thumbnails, by video id.
#[derive(Default)]
pub struct Projection {
    thumbnails: PngImages,
}

impl Projection {
    pub fn project(&self, window: &AppWindow, store: &Store) {
        let toolbar = window.global::<Toolbar>();
        let layout = &store.layout;
        set_if_changed(toolbar.get_active_tool(), layout.active_left_drawer_tool.into(), |value| toolbar.set_active_tool(value));
        set_if_changed(toolbar.get_visible(), layout.left_drawer_visible, |value| toolbar.set_visible(value));
        set_if_changed(toolbar.get_pinned(), layout.left_drawer_pinned, |value| toolbar.set_pinned(value));
        project_activity_drawer(&toolbar, store);
        project_video_drawer(&toolbar, store);
        self.project_video_panel(&toolbar, store);
    }

    fn project_video_panel(&self, toolbar: &Toolbar<'_>, store: &Store) {
        let shown = toolbar::video_panel_shown(store);
        set_if_changed(toolbar.get_video_panel_shown(), shown, |value| toolbar.set_video_panel_shown(value));
        let in_sync = ovrley_app::features::player::video_sync_mode(store);
        set_if_changed(toolbar.get_video_panel_in_sync(), in_sync, |value| toolbar.set_video_panel_in_sync(value));
        let locale = store.i18n.language;
        let cards = toolbar::video_cards(store)
            .into_iter()
            .map(|card| {
                let image = card.thumbnail.as_ref().and_then(|bytes| self.thumbnails.image(&card.id, bytes));
                VideoCardView {
                    id: card.id.as_str().into(),
                    remove_label: interpolate(catalog().t_or(locale, "toolbar.removeVideo", "Remove {{name}} from project"), &[("name", &card.filename)]).into(),
                    filename: card.filename.into(),
                    duration: card.duration_label.into(),
                    has_thumbnail: image.is_some(),
                    thumbnail: image.unwrap_or_default(),
                    missing: card.missing,
                    selected: card.selected,
                    holds_sync: card.holds_sync,
                }
            })
            .collect();
        replace_if_changed(toolbar.get_video_cards(), cards, |model| toolbar.set_video_cards(model));
        if !shown {
            return;
        }
        let summary = store.media.parsed_activity.as_ref().and(store.media.activity_summary.as_ref());
        let timezone = summary.and_then(|summary| summary.timezone.as_deref());
        let controls = ovrley_app::features::sync::derive_video_sync_controls(&store.video_import.imported_videos, &store.video_import.video_edits, timezone);
        set_if_changed(toolbar.get_panel_sync(), sync_view(store, &controls, &SecondsField::PanelTransitionDuration), |value| toolbar.set_panel_sync(value));
        let defaults = &store.video_import.transition_defaults;
        let label = ovrley_app::features::transitions::default_transition_label(&defaults.transition_type, locale);
        set_if_changed(toolbar.get_panel_transition_label(), SharedString::from(label), |value| toolbar.set_panel_transition_label(value));
        let preview = (defaults.transition_type != NO_TRANSITION).then_some(defaults.transition_type.as_str());
        set_if_changed(toolbar.get_panel_transition_preview_index(), transition_index(preview), |value| toolbar.set_panel_transition_preview_index(value));
        set_if_changed(toolbar.get_panel_transition_duration(), field_view(store, &SecondsField::PanelTransitionDuration), |value| {
            toolbar.set_panel_transition_duration(value);
        });
        set_if_changed(toolbar.get_panel_continuity_threshold(), field_view(store, &SecondsField::PanelContinuityThreshold), |value| {
            toolbar.set_panel_continuity_threshold(value);
        });
    }
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
    let t = |key: &str, default: &'static str| catalog().t_or(locale, key, default).to_string();
    let segments: Vec<VideoSegmentView> = row
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
        .collect();
    let exits: Vec<VideoExitView> =
        row.exits.iter().map(|exit| VideoExitView { next_filename: exit.next_filename.as_str().into(), join: join_control(store, &exit.join) }).collect();
    let metadata: Vec<DrawerRowView> = row.metadata.iter().map(|row| DrawerRowView { label: row.label.as_str().into(), value: row.value.as_str().into() }).collect();
    SelectedVideoView {
        id: row.id.as_str().into(),
        filename: row.filename.as_str().into(),
        path: row.path.as_str().into(),
        missing: row.missing,
        remove_label: interpolate(&t("toolbar.removeVideo", "Remove {{name}} from project"), &[("name", &row.filename)]).into(),
        warnings: strings(&row.warnings),
        metadata: ModelRc::new(VecModel::from(metadata)),
        start: field_view(store, &SecondsField::DrawerVideoStart(row.id.clone())),
        creation_source: row.creation_time_source.into(),
        filename_available: row.filename_creation_time_available,
        timezone_shown: row.timezone_shown,
        applies_timezone: row.applies_timezone,
        timezone_disabled: row.timezone_disabled,
        segments: ModelRc::new(VecModel::from(segments)),
        exits: ModelRc::new(VecModel::from(exits)),
    }
}

/// Whether the shown view equals `next`, its models compared by rows.
fn same_selected(current: &SelectedVideoView, next: &SelectedVideoView) -> bool {
    let scalars = SelectedVideoView {
        warnings: current.warnings.clone(),
        metadata: current.metadata.clone(),
        segments: current.segments.clone(),
        exits: current.exits.clone(),
        ..next.clone()
    };
    scalars == *current
        && models_equal(&current.warnings, &next.warnings)
        && models_equal(&current.metadata, &next.metadata)
        && models_equal(&current.segments, &next.segments)
        && models_equal(&current.exits, &next.exits)
}

fn project_video_drawer(toolbar: &Toolbar<'_>, store: &Store) {
    set_if_changed(toolbar.get_has_videos(), store.video_import.has_videos(), |value| toolbar.set_has_videos(value));
    let row = toolbar::selected_video_row(store);
    set_if_changed(toolbar.get_has_selected_video(), row.is_some(), |value| toolbar.set_has_selected_video(value));
    if let Some(row) = row {
        let next = selected_view(store, &row);
        if !same_selected(&toolbar.get_selected_video(), &next) {
            toolbar.set_selected_video(next);
        }
    }
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
            id: i32::try_from(lap.id).expect("a lap id fits an i32"),
            number: lap.number.into(),
            duration: lap.duration.into(),
            start: lap.start.into(),
            enabled: lap.enabled,
        })
        .collect();
    replace_if_changed(toolbar.get_activity_laps(), laps, |model| toolbar.set_activity_laps(model));
}

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    let toolbar = window.global::<Toolbar>();
    let d = dispatcher.handle();
    toolbar.on_select_tool(move |tool| d.dispatch(move |store, _| store.layout.select_left_drawer_tool(tool.into())));
    let d = dispatcher.handle();
    toolbar.on_set_pinned(move |pinned| d.dispatch(move |store, _| store.layout.set_left_drawer_pinned(pinned)));
    let d = dispatcher.handle();
    toolbar.on_dismiss_overlay(move || d.dispatch(|store, _| store.layout.dismiss_left_drawer_overlay()));
    let d = dispatcher.handle();
    toolbar.on_load_activity(move || d.dispatch(shell::open_activity_dialog));
    let d = dispatcher.handle();
    toolbar.on_delete_activity(move || dispatch_logged(&d, "activity drawer", |store, _| shell::delete_activity(store)));
    let d = dispatcher.handle();
    toolbar.on_set_lap_enabled(move |id, enabled| {
        // The table lists the activity's lap ids, which are never negative.
        let id = usize::try_from(id).expect("lap ids are not negative");
        d.dispatch(move |store, _| store.media.set_lap_enabled(id, enabled));
    });
    bind_videos(&toolbar, dispatcher);
}

/// The video drawer's and the video panel's commands. Their errors are bugs in what the drawer
/// offered.
fn bind_videos(toolbar: &Toolbar<'_>, dispatcher: &Dispatcher) {
    const CONTEXT: &str = "video drawer";
    let d = dispatcher.handle();
    toolbar.on_load_videos(move || d.dispatch(import::open_video_import_dialog));
    let d = dispatcher.handle();
    toolbar.on_remove_video(move |id| dispatch_logged(&d, CONTEXT, move |store, _| ovrley_app::features::player::remove_video(store, &id)));
    let d = dispatcher.handle();
    toolbar.on_locate_video(move |id| d.dispatch(move |store, services| toolbar::open_locate_dialog(store, services, id.to_string())));
    let d = dispatcher.handle();
    toolbar.on_sync_video(move |id| dispatch_logged(&d, CONTEXT, move |store, _| videos::reset_video_offset(store, &id)));
    let d = dispatcher.handle();
    toolbar.on_set_creation_source(move |id, source| dispatch_logged(&d, CONTEXT, move |store, _| videos::set_video_creation_time_source(store, &id, source.into())));
    let d = dispatcher.handle();
    toolbar.on_set_applies_timezone(move |id, applies| dispatch_logged(&d, CONTEXT, move |store, _| videos::set_video_timezone_mode(store, &id, timezone_mode(applies))));
    let d = dispatcher.handle();
    toolbar.on_set_segment_included(move |id, key, included| dispatch_logged(&d, CONTEXT, move |store, _| videos::set_video_segment_excluded(store, &id, &key, !included)));
    let d = dispatcher.handle();
    toolbar.on_set_segment_overlay(move |id, key, shows| dispatch_logged(&d, CONTEXT, move |store, _| videos::set_video_segment_skip_overlay(store, &id, &key, !shows)));
    let d = dispatcher.handle();
    toolbar.on_remove_segment(move |id, key| dispatch_logged(&d, CONTEXT, move |store, _| toolbar::remove_segment(store, &id, &key)));
    let d = dispatcher.handle();
    toolbar.on_seek_segment(move |id, key| dispatch_logged(&d, CONTEXT, move |store, _| toolbar::seek_segment(store, &id, &key)));
    let d = dispatcher.handle();
    toolbar.on_sync_all_videos(move || dispatch_logged(&d, CONTEXT, |store, _| videos::sync_all_videos(store)));
    let d = dispatcher.handle();
    toolbar.on_panel_reset_creation_times(move || dispatch_logged(&d, CONTEXT, |store, _| videos::reset_video_creation_times(store)));
    let d = dispatcher.handle();
    toolbar.on_panel_filename_creation_times(move || dispatch_logged(&d, CONTEXT, |store, _| videos::set_video_creation_times_from_filenames(store)));
    let d = dispatcher.handle();
    toolbar.on_panel_set_utc(move |utc| dispatch_logged(&d, CONTEXT, move |store, _| videos::set_video_sync_timezone_mode(store, timezone_mode(utc))));
    let d = dispatcher.handle();
    toolbar.on_remove_all_videos(move || d.dispatch(|store, _| import::clear_videos(store)));
    let d = dispatcher.handle();
    toolbar.on_select_card(move |id| {
        dispatch_logged(&d, CONTEXT, move |store, _| toolbar::select_card(store, &id));
    });
}
