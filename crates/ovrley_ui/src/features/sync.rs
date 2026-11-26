//! Sync Doctor (`VideoSync`): the preview screens, the selection and navigation maps, the speed
//! readout, the timeline's detection graph and landmarks, and the drawer's Autodetect and Manual
//! Sync tabs.

use crate::features::import::{sync_view, timezone_mode};
use crate::projection::{convert_enum, dispatch_logged, models_equal, replace_if_changed, rgba_image, set_if_changed};
use crate::{
    AppWindow, CandidateCardView, CandidateListView, Choice, ChoiceKind, EventBandView, LandmarkKind, LandmarkRowView, MatchScope, PointerPhase, TimelineLandmarkView,
    VideoSync, VideoSyncTab,
};
use ovrley_app::controls::seconds::SecondsField;
use ovrley_app::features::sync::maps::{self, WheelDevice};
use ovrley_app::features::sync::matching::MatchScope as AppMatchScope;
use ovrley_app::features::sync::timeline::{self, BandTone, GraphView};
use ovrley_app::features::sync::{self as sync, CandidateListView as AppCandidateList};
use ovrley_app::i18n::{catalog, Locale};
use ovrley_app::controls::color::{hsl_to_rgb, Hsl};
use ovrley_app::features::toolbar::VideoSyncDrawerTab;
use ovrley_app::features::sync::manual::{self as manual, SPEED_THRESHOLD_RANGE, TURN_THRESHOLD_RANGE};
use ovrley_app::store::videos as videos;
use ovrley_app::store::{Dispatcher, Store};
use ovrley_core::media::sync::LandmarkKind as AppLandmarkKind;
use ovrley_widgets::widgets::map::preview::MapFramePixels;
use slint::{ComponentHandle, Image, ModelRc, SharedPixelBuffer, SharedString, VecModel};
use std::cell::{Cell, RefCell};
use std::time::Instant;

convert_enum!(LandmarkKind <=> AppLandmarkKind { Stop, LeftTurn, RightTurn, Location });
convert_enum!(MatchScope <=> AppMatchScope { All, Location });
convert_enum!(VideoSyncTab <=> VideoSyncDrawerTab { Autodetect, Manual });

thread_local! {
    /// The device of the last raw wheel event, which the scroll event right after it reads: Slint's
    /// scroll event does not say whether a notched wheel or a trackpad sent it.
    static WHEEL_DEVICE: Cell<Option<WheelDevice>> = const { Cell::new(None) };
}

/// Records the raw wheel event's device, from the window's winit events.
pub fn record_wheel_device(device: WheelDevice) {
    WHEEL_DEVICE.with(|cell| cell.set(Some(device)));
}

fn wheel_device() -> Option<WheelDevice> {
    WHEEL_DEVICE.with(Cell::get)
}

/// An image built from a frame's pixels, rebuilt only when the frame's revision changes, so Slint
/// uploads it once and draws the same texture until the next frame.
#[derive(Default)]
struct FrameImage {
    cached: RefCell<Option<(u64, Image)>>,
}

impl FrameImage {
    /// The image of revision `revision`, built with `build` unless cached; `None` when it is the
    /// image already shown.
    fn fresh(&self, revision: u64, build: impl FnOnce() -> Image) -> Option<Image> {
        let mut cached = self.cached.borrow_mut();
        if cached.as_ref().is_some_and(|(shown, _)| *shown == revision) {
            return None;
        }
        let image = build();
        *cached = Some((revision, image.clone()));
        Some(image)
    }

    fn clear(&self) {
        self.cached.borrow_mut().take();
    }
}

fn map_image(frame: &MapFramePixels) -> Image {
    let source = frame.image();
    let width = u32::try_from(source.width()).expect("a map frame's width fits a u32");
    let height = u32::try_from(source.height()).expect("a map frame's height fits a u32");
    Image::from_rgba8_premultiplied(SharedPixelBuffer::clone_from_slice(frame.pixels(), width, height))
}

/// The graph's inputs: the detection's address, the view, the width, the height and the language.
type GraphKey = (usize, (u64, u64), u64, u64, Locale);

/// The projection's memory: the maps' and the speed readout's images, and the graph row, rebuilt
/// only when its inputs change, never per playhead frame.
#[derive(Default)]
pub struct Projection {
    selection_image: FrameImage,
    navigation_image: FrameImage,
    speed_image: FrameImage,
    graph: RefCell<Option<GraphKey>>,
}

impl Projection {
    pub fn project(&self, window: &AppWindow, store: &Store) {
        let global = window.global::<VideoSync>();
        let mode = ovrley_app::features::player::video_sync_mode(store);
        set_if_changed(global.get_mode(), mode, |value| global.set_mode(value));
        if mode {
            project_screens(&global, store);
            self.project_timeline(&global, store);
        }
        self.project_maps(&global, store);
        self.project_speed(&global, store);
        if mode && store.layout.left_drawer_visible {
            project_drawer(&global, store);
        }
    }

    fn project_speed(&self, global: &VideoSync<'_>, store: &Store) {
        let readout = store.video_sync.speed.as_ref();
        let frame = readout.and_then(ovrley_app::features::sync::readout::SpeedReadout::frame);
        set_if_changed(global.get_speed_shown(), frame.is_some(), |value| global.set_speed_shown(value));
        let error = readout.and_then(|readout| readout.error.clone()).unwrap_or_default();
        set_if_changed(global.get_speed_error(), SharedString::from(error), |value| global.set_speed_error(value));
        let Some(frame) = frame else {
            self.speed_image.clear();
            return;
        };
        let geometry = &frame.geometry;
        if let Some(image) = self.speed_image.fresh(frame.revision.into(), || rgba_image(frame.pixels, geometry.width, geometry.height)) {
            global.set_speed_image(image);
        }
        // The scene scale already lays the widget out at its screen size.
        let scale = f64::from(geometry.preview_raster_scale);
        set_if_changed(global.get_speed_x(), geometry.left, |value| global.set_speed_x(value));
        set_if_changed(global.get_speed_y(), geometry.top, |value| global.set_speed_y(value));
        set_if_changed(global.get_speed_width(), (f64::from(geometry.width) / scale) as f32, |value| global.set_speed_width(value));
        set_if_changed(global.get_speed_height(), (f64::from(geometry.height) / scale) as f32, |value| global.set_speed_height(value));
    }

    fn project_maps(&self, global: &VideoSync<'_>, store: &Store) {
        let Some(view) = maps::map_view(store) else {
            set_if_changed(global.get_has_map_image(), false, |value| global.set_has_map_image(value));
            set_if_changed(global.get_has_nav_image(), false, |value| global.set_has_nav_image(value));
            self.selection_image.clear();
            self.navigation_image.clear();
            return;
        };
        set_if_changed(global.get_has_map_image(), view.image.is_some(), |value| global.set_has_map_image(value));
        if let Some((image, (origin, size))) = view.image {
            if let Some(fresh) = self.selection_image.fresh(image.revision, || map_image(&image.pixels)) {
                global.set_map_image(fresh);
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
                if let Some((x, y)) = point {
                    set_if_changed(global.$x(), x as f32, |value| global.$set_x(value));
                    set_if_changed(global.$y(), y as f32, |value| global.$set_y(value));
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
            if let Some(fresh) = self.navigation_image.fresh(serial, || map_image(frame)) {
                global.set_nav_image(fresh);
            }
        }
        set_if_changed(global.get_nav_marker_shown(), view.navigation_marker, |value| global.set_nav_marker_shown(value));
        set_if_changed(global.get_nav_pitch(), view.navigation_pitch as f32, |value| global.set_nav_pitch(value));
        set_if_changed(global.get_nav_max_pitch(), maps::NAVIGATION_MAX_PITCH as f32, |value| global.set_nav_max_pitch(value));
        set_if_changed(global.get_nav_can_zoom_in(), view.navigation_zoom < maps::NAVIGATION_MAX_ZOOM, |value| global.set_nav_can_zoom_in(value));
        set_if_changed(global.get_nav_can_zoom_out(), view.navigation_zoom > 0.0, |value| global.set_nav_can_zoom_out(value));
        let chosen = store.video_sync.maps.as_ref().and_then(|maps| maps.chosen_style.clone());
        let (options, value) = ovrley_app::features::maps::map_style_select(store, chosen.as_deref());
        let choices = options.into_iter().map(|option| Choice { kind: ChoiceKind::Item, value: option.value.into(), label: option.label.into(), ..Default::default() }).collect();
        replace_if_changed(global.get_map_styles(), choices, |model| global.set_map_styles(model));
        set_if_changed(global.get_map_style(), SharedString::from(value), |value| global.set_map_style(value));
        set_if_changed(global.get_attribution(), SharedString::from(ovrley_widgets::widgets::map::layout::ATTRIBUTION), |value| global.set_attribution(value));
    }

    fn project_timeline(&self, global: &VideoSync<'_>, store: &Store) {
        let detection = store.manual_video_sync_detection.as_deref();
        let view = store.editor.timeline_viewport;
        let width = store.player.viewport.metrics.width_px;
        let height = store.player.viewport.metrics.rem * timeline::GRAPH_HEIGHT_REM;
        let language = store.i18n.language;
        let address = detection.map_or(0, |detection| std::ptr::from_ref(detection) as usize);
        let key = (address, (view.view_start.to_bits(), view.view_end.to_bits()), width.to_bits(), height.to_bits(), language);
        if self.graph.borrow().as_ref() != Some(&key) {
            *self.graph.borrow_mut() = Some(key);
            project_graph(global, &timeline::graph_view(detection, view, width, height), language);
        }
        let landmarks = timeline::landmark_models(store)
            .into_iter()
            .map(|model| TimelineLandmarkView {
                id: model.id.into(),
                kind: model.kind.into(),
                left: model.left as f32,
                interactive: model.interactive,
                seconds: format!("{:.1}", model.video_second).into(),
            })
            .collect();
        replace_if_changed(global.get_timeline_landmarks(), landmarks, |model| global.set_timeline_landmarks(model));
    }
}

fn project_screens(global: &VideoSync<'_>, store: &Store) {
    let (width, height) = ovrley_app::features::canvas::scene_size(store);
    let scale = ovrley_app::features::canvas::display_scale(store);
    set_if_changed(global.get_screen_width(), (width * scale) as f32, |value| global.set_screen_width(value));
    set_if_changed(global.get_screen_height(), (height * scale) as f32, |value| global.set_screen_height(value));
    set_if_changed(global.get_screen_gap(), (sync::PREVIEW_SCREEN_GAP * scale) as f32, |value| global.set_screen_gap(value));
    let marks = sync::mark_controls(store);
    set_if_changed(global.get_can_mark(), marks.can_mark, |value| global.set_can_mark(value));
    set_if_changed(global.get_can_mark_location(), marks.can_mark_location, |value| global.set_can_mark_location(value));
    set_if_changed(global.get_mark_reason(), SharedString::from(marks.mark_disabled_reason.unwrap_or_default()), |value| global.set_mark_reason(value));
    set_if_changed(global.get_location_reason(), SharedString::from(marks.location_disabled_reason.unwrap_or_default()), |value| global.set_location_reason(value));
}

fn project_graph(global: &VideoSync<'_>, graph: &GraphView, language: Locale) {
    global.set_graph_width(graph.width_px as f32);
    global.set_graph_height(graph.height_px as f32);
    global.set_speed_path(graph.speed_path.as_str().into());
    global.set_turning_path(graph.turning_path.as_str().into());
    let bands: Vec<EventBandView> = graph
        .bands
        .iter()
        .map(|band| {
            let label = catalog().t(language, band.label_key).to_string();
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
                tooltip: catalog().t_with(language, "videoSync.detectedEvent", &values).into(),
                accessible_label: catalog().t_with(language, "videoSync.detectedEventRange", &values).into(),
            }
        })
        .collect();
    global.set_bands(ModelRc::new(VecModel::from(bands)));
}

/// A candidate's score colour, `hsl(score × 1.2, 72%, 48%)`.
fn score_color(hue: f64) -> slint::Color {
    let rgb = hsl_to_rgb(Hsl { h: hue, s: 72.0, l: 48.0 }, 1.0);
    let channel = |value: f64| value.round().clamp(0.0, 255.0) as u8;
    slint::Color::from_rgb_u8(channel(rgb.r), channel(rgb.g), channel(rgb.b))
}

fn candidate_list(list: &AppCandidateList) -> CandidateListView {
    let cards: Vec<CandidateCardView> = list
        .cards
        .iter()
        .map(|card| CandidateCardView {
            offset: card.offset_label.as_str().into(),
            subline: card.subline.as_str().into(),
            score: card.score.map(|score| score.to_string()).unwrap_or_default().into(),
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

/// Whether the shown list equals `next`, its cards compared by rows.
fn same_list(current: &CandidateListView, next: &CandidateListView) -> bool {
    *current == CandidateListView { cards: current.cards.clone(), ..next.clone() } && models_equal(&current.cards, &next.cards)
}

fn project_drawer(global: &VideoSync<'_>, store: &Store) {
    let tab = store.layout.video_sync_drawer_tab;
    set_if_changed(global.get_tab(), tab.into(), |value| global.set_tab(value));
    set_if_changed(global.get_has_videos(), store.video_import.has_videos(), |value| global.set_has_videos(value));
    if tab == VideoSyncDrawerTab::Autodetect {
        let controls = sync::store_video_sync_controls(store);
        set_if_changed(global.get_sync(), sync_view(store, &controls, &SecondsField::SyncSharedOffset), |value| global.set_sync(value));
        return;
    }
    let view = sync::drawer_view(store);
    let rows = view
        .landmarks
        .iter()
        .map(|card| LandmarkRowView { id: card.id.as_str().into(), kind: card.kind.into(), label: card.label.as_str().into(), time: card.video_time_label.as_str().into() })
        .collect();
    replace_if_changed(global.get_landmarks(), rows, |model| global.set_landmarks(model));
    set_if_changed(global.get_has_location_landmark(), view.has_location_landmark, |value| global.set_has_location_landmark(value));
    set_if_changed(global.get_speed_draft(), view.speed_draft_kmh as f32, |value| global.set_speed_draft(value));
    set_if_changed(global.get_turn_draft(), view.turn_draft_degrees as f32, |value| global.set_turn_draft(value));
    set_if_changed(global.get_speed_label(), SharedString::from(view.speed_label), |value| global.set_speed_label(value));
    set_if_changed(global.get_turn_label(), SharedString::from(view.turn_label), |value| global.set_turn_label(value));
    set_if_changed(global.get_speed_minimum(), SPEED_THRESHOLD_RANGE.0 as f32, |value| global.set_speed_minimum(value));
    set_if_changed(global.get_speed_maximum(), SPEED_THRESHOLD_RANGE.1 as f32, |value| global.set_speed_maximum(value));
    set_if_changed(global.get_turn_minimum(), TURN_THRESHOLD_RANGE.0 as f32, |value| global.set_turn_minimum(value));
    set_if_changed(global.get_turn_maximum(), TURN_THRESHOLD_RANGE.1 as f32, |value| global.set_turn_maximum(value));
    let count = |value: usize| i32::try_from(value).expect("a landmark count fits an i32");
    set_if_changed(global.get_left_turns(), count(view.counts.left_turns), |value| global.set_left_turns(value));
    set_if_changed(global.get_stops(), count(view.counts.stops), |value| global.set_stops(value));
    set_if_changed(global.get_right_turns(), count(view.counts.right_turns), |value| global.set_right_turns(value));
    set_if_changed(global.get_locations(), count(view.counts.locations), |value| global.set_locations(value));
    set_if_changed(global.get_can_calculate_all(), view.eligibility.can_calculate_all, |value| global.set_can_calculate_all(value));
    set_if_changed(global.get_can_calculate_location(), view.eligibility.can_calculate_location, |value| global.set_can_calculate_location(value));
    set_if_changed(global.get_calculating(), view.calculating, |value| global.set_calculating(value));
    set_if_changed(global.get_all_explanation(), SharedString::from(view.eligibility.all_explanation.unwrap_or_default()), |value| global.set_all_explanation(value));
    let next = candidate_list(&view.location);
    if !same_list(&global.get_location_result(), &next) {
        global.set_location_result(next);
    }
    let next = candidate_list(&view.all);
    if !same_list(&global.get_all_result(), &next) {
        global.set_all_result(next);
    }
}

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    const CONTEXT: &str = "video sync";
    let global = window.global::<VideoSync>();
    let d = dispatcher.handle();
    global.on_set_tab(move |tab| d.dispatch(move |store, _| store.layout.set_video_sync_drawer_tab(tab.into())));
    let d = dispatcher.handle();
    global.on_compute(move || dispatch_logged(&d, CONTEXT, |store, _| videos::compute_video_sync(store)));
    let d = dispatcher.handle();
    global.on_reset_creation_times(move || dispatch_logged(&d, CONTEXT, |store, _| videos::reset_video_creation_times(store)));
    let d = dispatcher.handle();
    global.on_filename_creation_times(move || dispatch_logged(&d, CONTEXT, |store, _| videos::set_video_creation_times_from_filenames(store)));
    let d = dispatcher.handle();
    global.on_set_utc(move |utc| dispatch_logged(&d, CONTEXT, move |store, _| videos::set_video_sync_timezone_mode(store, timezone_mode(utc))));
    let d = dispatcher.handle();
    global.on_open_manual_sync(move || d.dispatch(|store, _| store.layout.open_manual_video_sync()));
    let d = dispatcher.handle();
    global.on_clear_landmarks(move || d.dispatch(|store, _| manual::clear_landmarks(store)));
    let d = dispatcher.handle();
    global.on_set_landmark_kind(move |id, kind| dispatch_logged(&d, CONTEXT, move |store, _| manual::set_landmark_kind(store, &id, kind.into())));
    let d = dispatcher.handle();
    global.on_remove_landmark(move |id| dispatch_logged(&d, CONTEXT, move |store, _| manual::remove_landmark(store, &id)));
    let d = dispatcher.handle();
    global.on_speed_changed(move |value| d.dispatch(move |store, _| sync::set_speed_draft(store, f64::from(value))));
    let d = dispatcher.handle();
    global.on_speed_committed(move |value| dispatch_logged(&d, CONTEXT, move |store, _| manual::set_speed_threshold(store, f64::from(value))));
    let d = dispatcher.handle();
    global.on_turn_changed(move |value| d.dispatch(move |store, _| sync::set_turn_draft(store, f64::from(value))));
    let d = dispatcher.handle();
    global.on_turn_committed(move |value| dispatch_logged(&d, CONTEXT, move |store, _| manual::set_turn_threshold(store, f64::from(value))));
    let d = dispatcher.handle();
    global.on_calculate(move |scope| dispatch_logged(&d, CONTEXT, move |store, services| sync::calculate(store, services, scope.into(), None)));
    let d = dispatcher.handle();
    global.on_apply_candidate(move |scope, index| {
        let index = usize::try_from(index).expect("card indices are not negative");
        dispatch_logged(&d, CONTEXT, move |store, _| sync::apply_candidate_at(store, scope.into(), index));
    });
    let d = dispatcher.handle();
    global.on_landmark_pointer(move |id, phase, x| {
        let x = f64::from(x);
        match phase {
            PointerPhase::Down => d.dispatch(move |store, _| timeline::landmark_down(store, &id)),
            PointerPhase::Move => d.dispatch(move |store, _| timeline::landmark_move(store, x)),
            PointerPhase::Up => dispatch_logged(&d, CONTEXT, |store, _| timeline::landmark_up(store, true)),
            PointerPhase::Cancel => dispatch_logged(&d, CONTEXT, |store, _| timeline::landmark_up(store, false)),
        }
    });
    let d = dispatcher.handle();
    global.on_map_pointer(move |phase, x, y| {
        let point = (f64::from(x), f64::from(y));
        d.dispatch(move |store, _| maps::map_pointer(store, &phase, point, Instant::now()));
    });
    // Slint reports a wheel turned towards the user as negative; the maps take it as positive.
    let d = dispatcher.handle();
    global.on_map_wheel(move |delta_y, x, y, shift| {
        let (value, point, device) = (-f64::from(delta_y), (f64::from(x), f64::from(y)), wheel_device());
        d.dispatch(move |store, _| maps::map_wheel(store, value, device, shift, point, Instant::now()));
    });
    let d = dispatcher.handle();
    global.on_map_double_click(move |x, y, shift| {
        let point = (f64::from(x), f64::from(y));
        d.dispatch(move |store, _| maps::map_double_click(store, point, shift, Instant::now()));
    });
    let d = dispatcher.handle();
    global.on_map_zoom(move |zoom_in| d.dispatch(move |store, _| maps::map_zoom(store, zoom_in, Instant::now())));
    let d = dispatcher.handle();
    global.on_set_location(move || dispatch_logged(&d, CONTEXT, |store, _| maps::set_location_from_action(store)));
    let d = dispatcher.handle();
    global.on_pin_dropped(move |x, y| {
        let point = (f64::from(x), f64::from(y));
        dispatch_logged(&d, CONTEXT, move |store, _| maps::pin_dropped(store, point));
    });
    let d = dispatcher.handle();
    global.on_pin_clicked(move || d.dispatch(|store, _| maps::toggle_popup(store)));
    let d = dispatcher.handle();
    global.on_delete_location(move || d.dispatch(|store, _| maps::delete_location(store)));
    let d = dispatcher.handle();
    global.on_set_map_style(move |id| {
        let id = id.to_string();
        d.dispatch(move |store, services| maps::set_map_style(store, services, id));
    });
    let d = dispatcher.handle();
    global.on_nav_wheel(move |delta_y, shift| {
        let (value, device) = (-f64::from(delta_y), wheel_device());
        d.dispatch(move |store, _| maps::nav_wheel(store, value, device, shift, Instant::now()));
    });
    let d = dispatcher.handle();
    global.on_nav_zoom(move |zoom_in| d.dispatch(move |store, _| maps::nav_zoom(store, zoom_in, Instant::now())));
    let d = dispatcher.handle();
    global.on_nav_pitch_changed(move |pitch| d.dispatch(move |store, _| maps::nav_pitch(store, f64::from(pitch))));
    let d = dispatcher.handle();
    global.on_mark(move |kind| dispatch_logged(&d, CONTEXT, move |store, _| sync::add_landmark_at_playhead(store, kind.into()).map(drop)));
}
