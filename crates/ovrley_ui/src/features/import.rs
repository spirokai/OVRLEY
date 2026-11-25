//! The video import dialog (`VideoImport`), and the keyed seconds fields (`SecondsFields`) and sync
//! controls the dialog shares with the video drawer, the video panel and Sync Doctor.

use crate::features::transitions::transition_index;
use crate::platform::keys::{key_name, Modifiers};
use crate::projection::{convert_enum, replace_if_changed, set_if_changed, strings};
use crate::{AppWindow, ImportFailureView, ImportPhase, ImportRowView, ImportTone, SecondsFieldView, SecondsFields, VideoImport, VideoSyncView};
use ovrley_app::features::seconds_fields::{self, SecondsField};
use ovrley_app::features::video_preview::import::{self, ReviewPhase};
use ovrley_app::features::video_sync::VideoSyncControls;
use ovrley_app::store::{DispatchHandle, Dispatcher, Store};
use ovrley_core::media::video_edits::TimezoneMode;
use slint::{ComponentHandle, SharedString};

convert_enum!(ImportPhase <=> ReviewPhase { Probing, Reviewing, Confirming, Importing });

/// The timezone mode of a UTC switch.
pub fn timezone_mode(utc: bool) -> TimezoneMode {
    if utc {
        TimezoneMode::Utc
    } else {
        TimezoneMode::Local
    }
}

/// A keyed field's view.
pub fn field_view(store: &Store, field: &SecondsField) -> SecondsFieldView {
    let view = seconds_fields::field_view(store, field);
    SecondsFieldView { key: field.key().into(), text: view.text.into(), preview: view.preview.unwrap_or_default().into(), error: view.error.unwrap_or_default().into() }
}

/// The sync controls' view, its shared offset on `shared_field`.
pub fn sync_view(store: &Store, controls: &VideoSyncControls, shared_field: &SecondsField) -> VideoSyncView {
    VideoSyncView {
        can_reset_creation_time: controls.can_reset_creation_time,
        filename_creation_time_available: controls.filename_creation_time_available,
        timezone_shown: controls.timezone.is_some(),
        timezone_disabled: controls.timezone_disabled,
        utc: controls.utc,
        warning: controls.warning.map(|warning| warning.message(store.i18n.language)).unwrap_or_default().into(),
        shared_offset: field_view(store, shared_field),
    }
}

pub fn project(window: &AppWindow, store: &Store) {
    let global = window.global::<VideoImport>();
    let Some(view) = import::import_dialog_view(store) else {
        set_if_changed(global.get_open(), false, |value| global.set_open(value));
        return;
    };
    set_if_changed(global.get_phase(), view.phase.into(), |value| global.set_phase(value));
    set_if_changed(global.get_reading_label(), SharedString::from(view.reading_label), |value| global.set_reading_label(value));
    let rows = view
        .rows
        .iter()
        .map(|model| {
            let row = &model.row;
            let format_tone = if row.has_other_resolution {
                ImportTone::Red
            } else if row.is_incompatible {
                ImportTone::Amber
            } else {
                ImportTone::Muted
            };
            ImportRowView {
                id: row.id.as_str().into(),
                filename: row.filename.as_str().into(),
                path: row.path.as_str().into(),
                is_new: row.is_new,
                included: row.is_included,
                can_switch: model.can_switch,
                switch_label: model.switch_label.as_str().into(),
                format_label: model.format_label.as_str().into(),
                format_tone,
                status: model.status.clone().unwrap_or_default().into(),
                status_red: row.has_other_resolution,
                far: model.far_label.clone().unwrap_or_default().into(),
                positioned: row.is_positioned,
                overlaps: strings(&model.overlap_labels),
                can_move_up: model.can_move_up,
                can_move_down: model.can_move_down,
                move_up_label: model.move_up_label.as_str().into(),
                move_down_label: model.move_down_label.as_str().into(),
                offset: field_view(store, &SecondsField::ImportRowOffset(row.id.clone())),
                manual: row.is_manual,
                reset_label: model.reset_label.as_str().into(),
                sync_label: model.sync_label.as_str().into(),
            }
        })
        .collect();
    replace_if_changed(global.get_rows(), rows, |model| global.set_rows(model));
    let failures = view.failures.iter().map(|(name, path, message)| ImportFailureView { name: name.into(), path: path.into(), message: message.into() }).collect();
    replace_if_changed(global.get_failures(), failures, |model| global.set_failures(model));
    replace_if_changed(global.get_issues(), view.issues.iter().map(SharedString::from).collect(), |model| global.set_issues(model));
    set_if_changed(global.get_far_text(), SharedString::from(view.far_text.unwrap_or_default()), |value| global.set_far_text(value));
    set_if_changed(global.get_error(), SharedString::from(view.error.unwrap_or_default()), |value| global.set_error(value));
    set_if_changed(global.get_import_label(), SharedString::from(view.import_label), |value| global.set_import_label(value));
    let alternatives = view.import_label_alternatives.iter().map(SharedString::from).collect();
    replace_if_changed(global.get_import_label_alternatives(), alternatives, |model| global.set_import_label_alternatives(model));
    set_if_changed(global.get_can_import(), view.import_count > 0, |value| global.set_can_import(value));
    set_if_changed(global.get_sync(), sync_view(store, &view.sync, &SecondsField::ImportSharedOffset), |value| global.set_sync(value));
    set_if_changed(global.get_transition_label(), SharedString::from(view.transition_label), |value| global.set_transition_label(value));
    set_if_changed(global.get_transition_preview_index(), transition_index(view.transition_preview.as_deref()), |value| global.set_transition_preview_index(value));
    set_if_changed(global.get_transition_duration(), field_view(store, &SecondsField::ImportTransitionDuration), |value| global.set_transition_duration(value));
    // Opened last, so the dialog's first frame has its values.
    set_if_changed(global.get_open(), true, |value| global.set_open(value));
}

/// Runs a review edit. A rejected one sets the dialog's error itself, so its result is not needed.
fn edit(dispatch: &DispatchHandle, run: impl FnOnce(&mut Store) -> Result<(), String> + 'static) {
    dispatch.dispatch(move |store, _| {
        let _ = run(store);
    });
}

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    let global = window.global::<VideoImport>();
    let d = dispatcher.handle();
    global.on_cancel(move || d.dispatch(|store, _| import::cancel_import_review(store)));
    let d = dispatcher.handle();
    global.on_confirm(move || d.dispatch(import::confirm_import_review));
    let d = dispatcher.handle();
    global.on_back(move || d.dispatch(|store, _| import::back_to_import_review(store)));
    let d = dispatcher.handle();
    global.on_row_included(move |id, included| edit(&d, move |store| import::set_review_row_included(store, &id, included)));
    let d = dispatcher.handle();
    global.on_row_move(move |id, up| edit(&d, move |store| import::move_review_row(store, &id, up)));
    let d = dispatcher.handle();
    global.on_row_reset(move |id| edit(&d, move |store| import::reset_review_row_offset(store, &id)));
    let d = dispatcher.handle();
    global.on_sync_compute(move || edit(&d, import::review_compute_video_sync));
    let d = dispatcher.handle();
    global.on_sync_reset_creation_times(move || edit(&d, import::review_reset_creation_times));
    let d = dispatcher.handle();
    global.on_sync_filename_creation_times(move || edit(&d, import::review_set_creation_times_from_filenames));
    let d = dispatcher.handle();
    global.on_sync_set_utc(move |utc| edit(&d, move |store| import::review_set_timezone_mode(store, timezone_mode(utc))));

    let fields = window.global::<SecondsFields>();
    let d = dispatcher.handle();
    fields.on_edited(move |key, text| d.dispatch_with_result(move |store, _| seconds_fields::edited(store, &key, &text)).unwrap_or(false));
    let d = dispatcher.handle();
    fields.on_key(move |key, text, alt, control, shift, meta| {
        let input = Modifiers { alt, control, shift, meta }.key_input(&text);
        d.dispatch_with_result(move |store, _| seconds_fields::key_down(store, &key, &input)).unwrap_or(false)
    });
    let d = dispatcher.handle();
    fields.on_key_up(move |key, text| {
        let released = key_name(&text);
        d.dispatch_with_result(move |store, _| seconds_fields::key_up(store, &key, &released)).unwrap_or(false)
    });
    let d = dispatcher.handle();
    fields.on_blurred(move |key| d.dispatch(move |store, _| seconds_fields::blurred(store, &key)));
    let d = dispatcher.handle();
    fields.on_step(move |key, direction| d.dispatch(move |store, _| seconds_fields::step(store, &key, f64::from(direction))));
    let d = dispatcher.handle();
    fields.on_clear(move |key| d.dispatch(move |store, _| seconds_fields::clear(store, &key)));
}
