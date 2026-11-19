//! Sync Doctor (Phase 8): its projection into the `VideoSync` global and its callbacks.

use crate::features::video_preview::sync_view;
use crate::hooks::app_store_selectors::{replace_if_changed, set_if_changed};
use crate::{AppWindow, EventBandView, CandidateCardView, CandidateListView, LandmarkKind, LandmarkRowView, TimelineLandmarkView, VideoSync};
use ovrley_app::features::video_sync::timeline::{self, BandTone, GraphView};
use std::cell::{Cell, RefCell};
use ovrley_app::features::video_sync::maps::{self, WheelDevice};
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::time::Instant;
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
    if mode {
        project_timeline(&global, store);
    }
    project_maps(&global, store);
    project_speed(&global, store);
    if mode && store.layout.left_drawer_visible {
        project_drawer(&global, store);
    }
}

thread_local! {
    /// The device of the last raw wheel event, which the scroll event right after it reads.
    static WHEEL_DEVICE: Cell<Option<WheelDevice>> = const { Cell::new(None) };
    /// The maps' Slint images, built once per frame they show: `(revision, image)`.
    static SELECTION_IMAGE: RefCell<Option<(u64, Image)>> = const { RefCell::new(None) };
    static NAVIGATION_IMAGE: RefCell<Option<(u64, Image)>> = const { RefCell::new(None) };
    /// The speed readout's image, by its pixels' revision.
    static SPEED_IMAGE: RefCell<Option<(u32, Image)>> = const { RefCell::new(None) };
}

/// The raw wheel event's device, from the window's winit events.
pub fn record_wheel_device(device: WheelDevice) {
    WHEEL_DEVICE.with(|cell| cell.set(Some(device)));
}

fn wheel_device() -> Option<WheelDevice> {
    WHEEL_DEVICE.with(Cell::get)
}

/// The frame's pixels as a Slint image, built only when `revision` changes, so Slint uploads it
/// once and draws the same texture until the next frame.
fn cached_image(cache: &'static std::thread::LocalKey<RefCell<Option<(u64, Image)>>>, revision: u64, frame: &ovrley_widgets::widgets::map::preview::MapFramePixels) -> (Image, bool) {
    cache.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((cached, image)) = cache.as_ref() {
            if *cached == revision {
                return (image.clone(), false);
            }
        }
        let source = frame.image();
        let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(frame.pixels(), source.width() as u32, source.height() as u32);
        let image = Image::from_rgba8_premultiplied(buffer);
        *cache = Some((revision, image.clone()));
        (image, true)
    })
}

/// The speed readout: its image is built again only when its pixels' revision changes.
fn project_speed(global: &VideoSync<'_>, store: &Store) {
    let readout = store.video_sync.speed.as_ref();
    let frame = readout.and_then(|readout| readout.frame());
    set_if_changed(global.get_speed_shown(), frame.is_some(), |value| global.set_speed_shown(value));
    set_if_changed(global.get_speed_error(), SharedString::from(readout.and_then(|readout| readout.error.clone()).unwrap_or_default()), |value| global.set_speed_error(value));
    let Some(frame) = frame else {
        SPEED_IMAGE.with(|cache| *cache.borrow_mut() = None);
        return;
    };
    let geometry = &frame.geometry.widget;
    let fresh = SPEED_IMAGE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.as_ref().is_some_and(|(revision, _)| *revision == frame.revision) {
            return None;
        }
        let image = Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(frame.pixels, geometry.width, geometry.height));
        *cache = Some((frame.revision, image.clone()));
        Some(image)
    });
    if let Some(image) = fresh {
        global.set_speed_image(image);
    }
    // The scene scale already lays the widget out at its screen size.
    let scale = f64::from(geometry.preview_raster_scale);
    set_if_changed(global.get_speed_x(), geometry.left, |value| global.set_speed_x(value));
    set_if_changed(global.get_speed_y(), geometry.top, |value| global.set_speed_y(value));
    set_if_changed(global.get_speed_width(), (f64::from(geometry.width) / scale) as f32, |value| global.set_speed_width(value));
    set_if_changed(global.get_speed_height(), (f64::from(geometry.height) / scale) as f32, |value| global.set_speed_height(value));
}

fn project_maps(global: &VideoSync<'_>, store: &Store) {
    let Some(view) = maps::map_view(store) else {
        set_if_changed(global.get_has_map_image(), false, |value| global.set_has_map_image(value));
        set_if_changed(global.get_has_nav_image(), false, |value| global.set_has_nav_image(value));
        SELECTION_IMAGE.with(|cache| *cache.borrow_mut() = None);
        NAVIGATION_IMAGE.with(|cache| *cache.borrow_mut() = None);
        return;
    };
    set_if_changed(global.get_has_map_image(), view.image.is_some(), |value| global.set_has_map_image(value));
    if let Some((image, (origin, size))) = view.image {
        let (slint_image, fresh) = cached_image(&SELECTION_IMAGE, image.revision, &image.pixels);
        if fresh {
            global.set_map_image(slint_image);
        }
        set_if_changed(global.get_map_image_x(), origin.0 as f32, |value| global.set_map_image_x(value));
        set_if_changed(global.get_map_image_y(), origin.1 as f32, |value| global.set_map_image_y(value));
        set_if_changed(global.get_map_image_width(), size.0 as f32, |value| global.set_map_image_width(value));
        set_if_changed(global.get_map_image_height(), size.1 as f32, |value| global.set_map_image_height(value));
    }
    let [r, g, b] = view.background;
    set_if_changed(global.get_map_background(), slint::Color::from_rgb_u8(r, g, b), |value| global.set_map_background(value));
    macro_rules! marker {
        ($point:expr, $shown:ident, $set_shown:ident, $x:ident, $set_x:ident, $y:ident, $set_y:ident) => {
            let point: Option<(f64, f64)> = $point;
            set_if_changed(global.$shown(), point.is_some(), |value| global.$set_shown(value));
            if let Some((px, py)) = point {
                set_if_changed(global.$x(), px as f32, |value| global.$set_x(value));
                set_if_changed(global.$y(), py as f32, |value| global.$set_y(value));
            }
        };
    }
    marker!(view.playback, get_playback_shown, set_playback_shown, get_playback_x, set_playback_x, get_playback_y, set_playback_y);
    marker!(view.hover, get_hover_shown, set_hover_shown, get_hover_x, set_hover_x, get_hover_y, set_hover_y);
    marker!(view.action, get_action_shown, set_action_shown, get_action_x, set_action_x, get_action_y, set_action_y);
    marker!(view.pin, get_pin_shown, set_pin_shown, get_pin_x, set_pin_x, get_pin_y, set_pin_y);
    set_if_changed(global.get_popup_open(), view.popup_open, |value| global.set_popup_open(value));
    set_if_changed(global.get_map_dragging(), view.dragging, |value| global.set_map_dragging(value));
    set_if_changed(global.get_map_can_zoom_in(), view.can_zoom_in, |value| global.set_map_can_zoom_in(value));
    set_if_changed(global.get_map_can_zoom_out(), view.can_zoom_out, |value| global.set_map_can_zoom_out(value));
    set_if_changed(global.get_has_nav_image(), view.navigation_frame.is_some(), |value| global.set_has_nav_image(value));
    if let Some((frame, serial)) = view.navigation_frame {
        let (image, fresh) = cached_image(&NAVIGATION_IMAGE, serial, frame);
        if fresh {
            global.set_nav_image(image);
        }
    }
    set_if_changed(global.get_nav_marker_shown(), view.navigation_marker, |value| global.set_nav_marker_shown(value));
    set_if_changed(global.get_nav_pitch(), view.navigation_pitch as f32, |value| global.set_nav_pitch(value));
    set_if_changed(global.get_nav_max_pitch(), maps::NAVIGATION_MAX_PITCH as f32, |value| global.set_nav_max_pitch(value));
    set_if_changed(global.get_nav_can_zoom_in(), view.navigation_zoom < maps::NAVIGATION_MAX_ZOOM, |value| global.set_nav_can_zoom_in(value));
    set_if_changed(global.get_nav_can_zoom_out(), view.navigation_zoom > 0.0, |value| global.set_nav_can_zoom_out(value));
    let chosen = store.video_sync.maps.as_ref().and_then(|maps| maps.chosen_style.clone());
    let (options, value) = ovrley_app::features::maps::map_style_select(store, chosen.as_deref());
    let choices = options.into_iter().map(|option| crate::Choice { kind: crate::ChoiceKind::Item, value: option.value.into(), label: option.label.into(), ..Default::default() }).collect();
    replace_if_changed(global.get_map_styles(), choices, |model| global.set_map_styles(model));
    set_if_changed(global.get_map_style(), SharedString::from(value), |value| global.set_map_style(value));
    set_if_changed(global.get_attribution(), SharedString::from(ovrley_widgets::widgets::map::layout::ATTRIBUTION), |value| global.set_attribution(value));
}

/// The graph's inputs: the detection's pointer, the view, the width and the language.
type GraphKey = (usize, (u64, u64), u64, u64, ovrley_app::i18n::Locale);

thread_local! {
    /// The graph row, rebuilt only when its inputs change, never per playhead frame.
    static GRAPH: RefCell<Option<(GraphKey, GraphView)>> = const { RefCell::new(None) };
}

fn project_timeline(global: &VideoSync<'_>, store: &Store) {
    let detection = store.manual_video_sync_detection.as_deref();
    let view = store.editor.timeline_viewport;
    let width = store.player.viewport.metrics.width_px;
    let height = store.player.viewport.metrics.rem * timeline::GRAPH_HEIGHT_REM;
    let language = store.i18n.language;
    let key = (detection.map_or(0, |detection| detection as *const _ as usize), (view.view_start.to_bits(), view.view_end.to_bits()), width.to_bits(), height.to_bits(), language);
    let fresh = GRAPH.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.as_ref().is_some_and(|(cached, _)| *cached == key) {
            return None;
        }
        let graph = timeline::graph_view(detection, view, width, height, language);
        *cache = Some((key, graph.clone()));
        Some(graph)
    });
    if let Some(graph) = fresh {
        let t = |key: &str| ovrley_app::i18n::catalog().t(language, key).to_string();
        global.set_graph_width(graph.width_px as f32);
        global.set_graph_height(graph.height_px as f32);
        global.set_speed_path(graph.speed_path.into());
        global.set_turning_path(graph.turning_path.into());
        let bands: Vec<EventBandView> = graph
            .bands
            .iter()
            .map(|band| {
                let label = t(band.label_key);
                let (start, end) = (format!("{:.1}", band.start_second), format!("{:.1}", band.end_second));
                let values = [("label", label.as_str()), ("start", start.as_str()), ("end", end.as_str())];
                EventBandView {
                    left: band.left as f32,
                    width: band.width as f32,
                    tone: match band.tone {
                        BandTone::Stop => LandmarkKind::Stop,
                        BandTone::Turn => LandmarkKind::LeftTurn,
                        BandTone::Location => LandmarkKind::Location,
                    },
                    tooltip: ovrley_app::i18n::catalog().t_with(language, "videoSync.detectedEvent", &values).into(),
                    accessible_label: ovrley_app::i18n::catalog().t_with(language, "videoSync.detectedEventRange", &values).into(),
                }
            })
            .collect();
        global.set_bands(ModelRc::new(VecModel::from(bands)));
    }
    let landmarks = timeline::landmark_models(store)
        .into_iter()
        .map(|model| TimelineLandmarkView {
            id: model.id.into(),
            kind: to_slint(model.kind),
            left: model.left as f32,
            interactive: model.interactive,
            seconds: format!("{:.1}", model.video_second).into(),
        })
        .collect();
    replace_if_changed(global.get_timeline_landmarks(), landmarks, |model| global.set_timeline_landmarks(model));
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
    global.on_landmark_pointer(move |id, phase, x| {
        let x = f64::from(x);
        match phase.as_str() {
            "down" => d.dispatch(move |store, _| timeline::landmark_down(store, &id)),
            "move" => d.dispatch(move |store, _| timeline::landmark_move(store, x)),
            "up" => command(&d, |store, _| timeline::landmark_up(store, true)),
            _ => command(&d, |store, _| timeline::landmark_up(store, false)),
        }
    });
    let d = dispatcher.clone();
    global.on_map_pointer(move |phase, x, y| {
        let point = (f64::from(x), f64::from(y));
        d.dispatch(move |store, _| maps::map_pointer(store, &phase, point, Instant::now()));
    });
    // Slint reports a wheel turned towards the user as negative; the browser's `deltaY` is
    // positive then.
    let d = dispatcher.clone();
    global.on_map_wheel(move |delta_y, x, y, shift| {
        let (value, point, device) = (-f64::from(delta_y), (f64::from(x), f64::from(y)), wheel_device());
        d.dispatch(move |store, _| maps::map_wheel(store, value, device, shift, point, Instant::now()));
    });
    let d = dispatcher.clone();
    global.on_map_double_click(move |x, y, shift| {
        let point = (f64::from(x), f64::from(y));
        d.dispatch(move |store, _| maps::map_double_click(store, point, shift, Instant::now()));
    });
    let d = dispatcher.clone();
    global.on_map_zoom(move |zoom_in| d.dispatch(move |store, _| maps::map_zoom(store, zoom_in, Instant::now())));
    let d = dispatcher.clone();
    global.on_set_location(move || command(&d, |store, _| maps::set_location_from_action(store)));
    let d = dispatcher.clone();
    global.on_pin_dropped(move |x, y| {
        let point = (f64::from(x), f64::from(y));
        command(&d, move |store, _| maps::pin_dropped(store, point));
    });
    let d = dispatcher.clone();
    global.on_pin_clicked(move || d.dispatch(|store, _| maps::toggle_popup(store)));
    let d = dispatcher.clone();
    global.on_delete_location(move || d.dispatch(|store, _| maps::delete_location(store)));
    let d = dispatcher.clone();
    global.on_set_map_style(move |id| {
        let id = id.to_string();
        d.dispatch(move |store, services| maps::set_map_style(store, services, id));
    });
    let d = dispatcher.clone();
    global.on_nav_wheel(move |delta_y, shift| {
        let (value, device) = (-f64::from(delta_y), wheel_device());
        d.dispatch(move |store, _| maps::nav_wheel(store, value, device, shift, Instant::now()));
    });
    let d = dispatcher.clone();
    global.on_nav_zoom(move |zoom_in| d.dispatch(move |store, _| maps::nav_zoom(store, zoom_in, Instant::now())));
    let d = dispatcher.clone();
    global.on_nav_pitch_changed(move |pitch| d.dispatch(move |store, _| maps::nav_pitch(store, f64::from(pitch))));
    let d = dispatcher.clone();
    global.on_mark(move |kind| {
        command(&d, move |store, _| {
            sync::add_landmark_at_playhead(store, from_slint(kind)).map(drop)
        })
    });
}
