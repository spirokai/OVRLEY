//! Sync Doctor (Phase 8): its projection into the `VideoSync` global and its callbacks.

use crate::features::video_preview::sync_view;
use crate::hooks::app_store_selectors::{replace_if_changed, set_if_changed};
use crate::{
    AppWindow, CandidateCardView, CandidateListView, LandmarkKind, LandmarkRowView, VideoSync,
};
use ovrley_app::features::seconds_fields::SecondsField;
use ovrley_app::features::video_sync::matching::MatchScope;
use ovrley_app::features::video_sync::{self as sync, CandidateListView as AppCandidateList};
use ovrley_app::lib::color_picker::{hsl_to_rgb, Hsl};
use ovrley_app::store::slices::layout::VideoSyncDrawerTab;
use ovrley_app::store::slices::manual_video_sync::{
    self as slice, SPEED_THRESHOLD_RANGE, TURN_THRESHOLD_RANGE,
};
use ovrley_app::store::{services::Services, Dispatcher, Store};
use ovrley_core::media::manual_video_sync::LandmarkKind as CoreKind;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

fn to_slint(kind: CoreKind) -> LandmarkKind {
    match kind {
        CoreKind::Stop => LandmarkKind::Stop,
        CoreKind::LeftTurn => LandmarkKind::LeftTurn,
        CoreKind::RightTurn => LandmarkKind::RightTurn,
        CoreKind::Location => LandmarkKind::Location,
    }
}

fn from_slint(kind: LandmarkKind) -> CoreKind {
    match kind {
        LandmarkKind::Stop => CoreKind::Stop,
        LandmarkKind::LeftTurn => CoreKind::LeftTurn,
        LandmarkKind::RightTurn => CoreKind::RightTurn,
        LandmarkKind::Location => CoreKind::Location,
    }
}

fn scope_of(scope: &str) -> MatchScope {
    if scope == "location" {
        MatchScope::Location
    } else {
        MatchScope::All
    }
}

/// `hsl(score × 1.2, 72%, 48%)`.
fn score_color(hue: f64) -> slint::Color {
    let rgb = hsl_to_rgb(
        Hsl {
            h: hue,
            s: 72.0,
            l: 48.0,
        },
        1.0,
    );
    slint::Color::from_rgb_u8(rgb.r as u8, rgb.g as u8, rgb.b as u8)
}

fn candidate_list(list: &AppCandidateList) -> CandidateListView {
    let cards: Vec<CandidateCardView> = list
        .cards
        .iter()
        .map(|card| CandidateCardView {
            offset: card.offset_label.as_str().into(),
            subline: card.subline.as_str().into(),
            score: card
                .score
                .map(|score| score.to_string())
                .unwrap_or_default()
                .into(),
            score_color: score_color(card.score_hue),
            applied: card.applied,
        })
        .collect();
    CandidateListView {
        shown: list.shown,
        error: list.error.clone().unwrap_or_default().into(),
        stale: list.stale,
        empty: list.empty,
        disabled: list.disabled,
        cards: ModelRc::new(VecModel::from(cards)),
    }
}

/// Nested models compare by pointer; compare their rows before replacing the view.
fn same_list(left: &CandidateListView, right: &CandidateListView) -> bool {
    left.shown == right.shown
        && left.error == right.error
        && left.stale == right.stale
        && left.empty == right.empty
        && left.disabled == right.disabled
        && left.cards.row_count() == right.cards.row_count()
        && left
            .cards
            .iter()
            .zip(right.cards.iter())
            .all(|(left, right)| left == right)
}

pub fn project(window: &AppWindow, store: &Store) {
    let global = window.global::<VideoSync>();
    let mode = ovrley_app::features::player::video_sync_mode(store);
    set_if_changed(global.get_mode(), mode, |value| global.set_mode(value));
    if mode {
        let (width, height) = ovrley_app::features::overlay_editor::scene_size(store);
        let scale = ovrley_app::features::overlay_editor::display_scale(store);
        set_if_changed(global.get_screen_width(), (width * scale) as f32, |value| {
            global.set_screen_width(value)
        });
        set_if_changed(
            global.get_screen_height(),
            (height * scale) as f32,
            |value| global.set_screen_height(value),
        );
        set_if_changed(
            global.get_screen_gap(),
            (sync::PREVIEW_SCREEN_GAP * scale) as f32,
            |value| global.set_screen_gap(value),
        );
        let marks = sync::mark_controls(store);
        set_if_changed(global.get_can_mark(), marks.can_mark, |value| {
            global.set_can_mark(value)
        });
        set_if_changed(
            global.get_can_mark_location(),
            marks.can_mark_location,
            |value| global.set_can_mark_location(value),
        );
        set_if_changed(
            global.get_mark_reason(),
            SharedString::from(marks.mark_disabled_reason.unwrap_or_default()),
            |value| global.set_mark_reason(value),
        );
        set_if_changed(
            global.get_location_reason(),
            SharedString::from(marks.location_disabled_reason.unwrap_or_default()),
            |value| global.set_location_reason(value),
        );
    }
    if mode && store.layout.left_drawer_visible {
        project_drawer(&global, store);
    }
}

fn project_drawer(global: &VideoSync<'_>, store: &Store) {
    let tab = match store.layout.video_sync_drawer_tab {
        VideoSyncDrawerTab::Autodetect => "autodetect",
        VideoSyncDrawerTab::Manual => "manual",
    };
    set_if_changed(global.get_tab(), SharedString::from(tab), |value| {
        global.set_tab(value)
    });
    set_if_changed(
        global.get_has_videos(),
        store.video_import.has_videos(),
        |value| global.set_has_videos(value),
    );
    if store.layout.video_sync_drawer_tab == VideoSyncDrawerTab::Autodetect {
        let controls = sync::store_video_sync_controls(store);
        set_if_changed(
            global.get_sync(),
            sync_view(store, &controls, &SecondsField::SyncSharedOffset),
            |value| global.set_sync(value),
        );
        return;
    }
    let view = sync::drawer_view(store);
    let rows = view
        .landmarks
        .iter()
        .map(|card| LandmarkRowView {
            id: card.id.as_str().into(),
            kind: to_slint(card.kind),
            label: card.label.as_str().into(),
            time: card.video_time_label.as_str().into(),
        })
        .collect();
    replace_if_changed(global.get_landmarks(), rows, |model| {
        global.set_landmarks(model)
    });
    set_if_changed(
        global.get_has_location_landmark(),
        view.has_location_landmark,
        |value| global.set_has_location_landmark(value),
    );
    set_if_changed(
        global.get_speed_draft(),
        view.speed_draft_kmh as f32,
        |value| global.set_speed_draft(value),
    );
    set_if_changed(
        global.get_turn_draft(),
        view.turn_draft_degrees as f32,
        |value| global.set_turn_draft(value),
    );
    set_if_changed(
        global.get_speed_label(),
        SharedString::from(view.speed_label),
        |value| global.set_speed_label(value),
    );
    set_if_changed(
        global.get_turn_label(),
        SharedString::from(view.turn_label),
        |value| global.set_turn_label(value),
    );
    set_if_changed(
        global.get_speed_minimum(),
        SPEED_THRESHOLD_RANGE.0 as f32,
        |value| global.set_speed_minimum(value),
    );
    set_if_changed(
        global.get_speed_maximum(),
        SPEED_THRESHOLD_RANGE.1 as f32,
        |value| global.set_speed_maximum(value),
    );
    set_if_changed(
        global.get_turn_minimum(),
        TURN_THRESHOLD_RANGE.0 as f32,
        |value| global.set_turn_minimum(value),
    );
    set_if_changed(
        global.get_turn_maximum(),
        TURN_THRESHOLD_RANGE.1 as f32,
        |value| global.set_turn_maximum(value),
    );
    set_if_changed(
        global.get_left_turns(),
        view.counts.left_turns as i32,
        |value| global.set_left_turns(value),
    );
    set_if_changed(global.get_stops(), view.counts.stops as i32, |value| {
        global.set_stops(value)
    });
    set_if_changed(
        global.get_right_turns(),
        view.counts.right_turns as i32,
        |value| global.set_right_turns(value),
    );
    set_if_changed(
        global.get_locations(),
        view.counts.locations as i32,
        |value| global.set_locations(value),
    );
    set_if_changed(
        global.get_can_calculate_all(),
        view.eligibility.can_calculate_all,
        |value| global.set_can_calculate_all(value),
    );
    set_if_changed(
        global.get_can_calculate_location(),
        view.eligibility.can_calculate_location,
        |value| global.set_can_calculate_location(value),
    );
    set_if_changed(global.get_calculating(), view.calculating, |value| {
        global.set_calculating(value)
    });
    set_if_changed(
        global.get_all_explanation(),
        SharedString::from(view.eligibility.all_explanation.unwrap_or_default()),
        |value| global.set_all_explanation(value),
    );
    let next = candidate_list(&view.location);
    if !same_list(&global.get_location_result(), &next) {
        global.set_location_result(next);
    }
    let next = candidate_list(&view.all);
    if !same_list(&global.get_all_result(), &next) {
        global.set_all_result(next);
    }
}

/// Runs a drawer or screen command; a rejected one is logged, as React's throw lands in the console.
fn command(
    dispatcher: &Dispatcher,
    run: impl FnOnce(&mut Store, &mut Services) -> Result<(), String> + 'static,
) {
    dispatcher.dispatch(move |store, services| {
        if let Err(error) = run(store, services) {
            log::error!("video sync: {error}");
        }
    });
}

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    use ovrley_app::store::slices::video_import as videos;
    use ovrley_core::media::video_edits::TimezoneMode;
    let global = window.global::<VideoSync>();
    let d = dispatcher.clone();
    global.on_set_tab(move |tab| {
        let tab = if tab == "manual" {
            VideoSyncDrawerTab::Manual
        } else {
            VideoSyncDrawerTab::Autodetect
        };
        d.dispatch(move |store, _| store.layout.set_video_sync_drawer_tab(tab));
    });
    let d = dispatcher.clone();
    global.on_compute(move || command(&d, |store, _| videos::compute_video_sync(store)));
    let d = dispatcher.clone();
    global.on_reset_creation_times(move || {
        command(&d, |store, _| videos::reset_video_creation_times(store))
    });
    let d = dispatcher.clone();
    global.on_filename_creation_times(move || {
        command(&d, |store, _| {
            videos::set_video_creation_times_from_filenames(store)
        })
    });
    let d = dispatcher.clone();
    global.on_set_utc(move |utc| {
        command(&d, move |store, _| {
            videos::set_video_sync_timezone_mode(
                store,
                if utc {
                    TimezoneMode::Utc
                } else {
                    TimezoneMode::Local
                },
            )
        })
    });
    let d = dispatcher.clone();
    global
        .on_open_manual_sync(move || d.dispatch(|store, _| store.layout.open_manual_video_sync()));
    let d = dispatcher.clone();
    global.on_clear_landmarks(move || d.dispatch(|store, _| slice::clear_landmarks(store)));
    let d = dispatcher.clone();
    global.on_set_landmark_kind(move |id, kind| {
        command(&d, move |store, _| {
            slice::set_landmark_kind(store, &id, from_slint(kind))
        })
    });
    let d = dispatcher.clone();
    global.on_remove_landmark(move |id| {
        command(&d, move |store, _| slice::remove_landmark(store, &id))
    });
    let d = dispatcher.clone();
    global.on_speed_changed(move |value| {
        d.dispatch(move |store, _| sync::set_speed_draft(store, f64::from(value)))
    });
    let d = dispatcher.clone();
    global.on_speed_committed(move |value| {
        command(&d, move |store, _| {
            slice::set_speed_threshold(store, f64::from(value))
        })
    });
    let d = dispatcher.clone();
    global.on_turn_changed(move |value| {
        d.dispatch(move |store, _| sync::set_turn_draft(store, f64::from(value)))
    });
    let d = dispatcher.clone();
    global.on_turn_committed(move |value| {
        command(&d, move |store, _| {
            slice::set_turn_threshold(store, f64::from(value))
        })
    });
    let d = dispatcher.clone();
    global.on_calculate(move |scope| {
        let scope = scope_of(&scope);
        command(&d, move |store, services| {
            sync::calculate(store, services, scope, None)
        })
    });
    let d = dispatcher.clone();
    global.on_apply_candidate(move |scope, index| {
        let (scope, index) = (
            scope_of(&scope),
            usize::try_from(index).expect("card indices are not negative"),
        );
        command(&d, move |store, _| {
            sync::apply_candidate_at(store, scope, index)
        })
    });
    let d = dispatcher.clone();
    global.on_mark(move |kind| {
        command(&d, move |store, _| {
            sync::add_landmark_at_playhead(store, from_slint(kind)).map(drop)
        })
    });
}
