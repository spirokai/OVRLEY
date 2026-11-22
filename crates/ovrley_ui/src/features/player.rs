//! The player feature (phase 4): its projection into the `Player` global, its callbacks, and the
//! playback timer.
//!
//! The timeline clock is driven by a repeating 8 ms timer while it plays (Concat drives its clock
//! the same way; `concat-playback.md`). A tick dispatches only when the next preview frame is due,
//! so the projections run at the preview's frame rate, not at the timer's. Nothing runs while
//! paused.

use crate::hooks::app_store_selectors::{replace_if_changed, set_if_changed};
use crate::{AppWindow, TileView, BandView, Choice, ChoiceKind, ClipTooltipView, ClipView, JoinMarkerView, MajorTickView, MarkerAlign, MarkerView, Player, PointerPhase, SegmentView, TimelineSurfaceKind, TransitionOption, TransitionOptionKind};
use crate::platform::key_input::{key_input, SlintModifiers};
use ovrley_app::features::transitions;
use ovrley_app::features::player::lanes::{self, MarkerTarget};
use ovrley_app::lib::activity::TrimEdge;
use ovrley_app::store::slices::video_import::ClipLane;
use ovrley_app::features::player::gestures::{surface_cancel, surface_down, surface_move, surface_up, SurfaceGesture};
use ovrley_app::features::player::playback::{self, player_timing};
use ovrley_app::features::player::viewport::{self, compute_tick_strip, fit_targets, matching_fit_target, tick_placement, zoom_limits, FitTargetId, TickPlacement};
use ovrley_app::lib::format::format_clock_duration;
use ovrley_app::lib::utils::clamp;
use ovrley_app::store::slices::editor::PreviewPlaybackState;
use ovrley_app::store::{DispatchHandle, Dispatcher, Store};
use slint::{ComponentHandle, SharedString};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// How often the playback timer checks whether a preview frame is due.
const PLAYBACK_TIMER_INTERVAL: Duration = Duration::from_millis(8);

/// The tick strip as last built: rebuilt only when its placement or the pixel grid changes.
type TickStrip = (TickPlacement, f64, f64, Vec<MajorTickView>, Vec<f32>);

pub struct Projection {
    dispatch: DispatchHandle,
    timer: Rc<slint::Timer>,
    /// Steps the auto-scroll while a clip drag sits in an edge zone (D9), standing in for React's
    /// animation-frame loop.
    scroll_timer: slint::Timer,
    strip: RefCell<Option<TickStrip>>,
    /// Each loaded filmstrip's sprite, uploaded once however many tiles show it (D12).
    sprites: RefCell<std::collections::HashMap<usize, (std::sync::Arc<ovrley_app::store::slices::video_filmstrip::ReadyStrip>, slint::Image)>>,
    /// Publishes the timeline's wanted filmstrip windows once the view settled.
    wanted_timer: slint::Timer,
    wanted_deadline: std::cell::Cell<Option<Instant>>,
}

impl Projection {
    pub fn new(dispatch: DispatchHandle) -> Self {
        Self { dispatch, timer: Rc::new(slint::Timer::default()), scroll_timer: slint::Timer::default(),
            strip: RefCell::new(None),
            sprites: RefCell::default(),
            wanted_timer: slint::Timer::default(),
            wanted_deadline: std::cell::Cell::new(None),
        }
    }

    /// Runs the playback timer while the timeline clock plays.
    fn drive_playback(&self, store: &Store) {
        let playing = store.editor.preview_playback_state == PreviewPlaybackState::Playing && playback::next_frame_at(store).is_some();
        if !playing {
            self.timer.stop();
            return;
        }
        if self.timer.running() {
            return;
        }
        let dispatch = self.dispatch.clone();
        self.timer.start(slint::TimerMode::Repeated, PLAYBACK_TIMER_INTERVAL, move || {
            let now = Instant::now();
            let due = dispatch.with_store(playback::next_frame_at).flatten();
            if due.is_some_and(|due| due <= now) {
                dispatch.dispatch(move |store, _| {
                    playback::playback_tick(store, now);
                });
            }
        });
    }

    fn drive_auto_scroll(&self, store: &Store) {
        if !ovrley_app::features::player::gestures::clip_drag_in_edge_zone(store) {
            self.scroll_timer.stop();
            return;
        }
        if self.scroll_timer.running() {
            return;
        }
        let dispatch = self.dispatch.clone();
        self.scroll_timer.start(slint::TimerMode::Repeated, Duration::from_millis(16), move || {
            dispatch.dispatch(|store, _| {
                ovrley_app::features::player::gestures::clip_drag_auto_scroll(store, Instant::now());
            });
        });
    }

    fn drive_wanted_windows(&self, store: &Store) {
        let deadline = store.filmstrip_queue.wanted_deadline();
        if deadline == self.wanted_deadline.get() {
            return;
        }
        self.wanted_deadline.set(deadline);
        let Some(deadline) = deadline else {
            self.wanted_timer.stop();
            return;
        };
        let dispatch = self.dispatch.clone();
        self.wanted_timer.start(slint::TimerMode::SingleShot, deadline.saturating_duration_since(Instant::now()), move || {
            dispatch.dispatch(|store, _| ovrley_app::features::video_preview::filmstrips::publish_wanted_windows(store, Instant::now()));
        });
    }

    /// The sprite of a strip as one Slint image, cached by the strip.
    fn sprite(&self, strip: &std::sync::Arc<ovrley_app::store::slices::video_filmstrip::ReadyStrip>) -> slint::Image {
        let key = std::sync::Arc::as_ptr(strip) as usize;
        let mut sprites = self.sprites.borrow_mut();
        sprites
            .entry(key)
            .or_insert_with(|| {
                let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&strip.pixels, strip.width, strip.height);
                (std::sync::Arc::clone(strip), slint::Image::from_rgba8(buffer))
            })
            .1
            .clone()
    }

    fn project_lanes(&self, player: &Player<'_>, store: &Store) {
        let (video, activity) = ovrley_app::features::player::lanes_view(store);
        let tiles: Vec<TileView> = project_lanes(player, video, activity)
            .iter()
            .map(|tile| {
                let frame_width = tile.strip.width / tile.strip.sample_count.max(1);
                TileView {
                    x: tile.x as f32,
                    width: tile.width as f32,
                    clip_x: tile.clip_x as f32,
                    clip_width: tile.clip_width as f32,
                    image: self.sprite(&tile.strip),
                    frame_x: (tile.sample * frame_width) as i32,
                    frame_width: frame_width as i32,
                }
            })
            .collect();
        // Sprites no strip holds any more are released.
        let live: std::collections::HashSet<usize> = store
            .video_filmstrip
            .strips
            .values()
            .flat_map(|strips| strips.values())
            .filter_map(|strip| if let ovrley_app::store::slices::video_filmstrip::StripState::Ready(strip) = strip { Some(std::sync::Arc::as_ptr(strip) as usize) } else { None })
            .collect();
        self.sprites.borrow_mut().retain(|key, _| live.contains(key));
        replace_if_changed(player.get_tiles(), tiles, |model| player.set_tiles(model));
    }

    pub fn project(&self, window: &AppWindow, store: &Store) {
        let player = window.global::<Player>();
        let timing = player_timing(store);
        let has_video = store.video_import.has_videos();
        set_if_changed(player.get_visible(), timing.has_activity || has_video, |value| player.set_visible(value));
        set_if_changed(player.get_transport_disabled(), !timing.has_activity, |value| player.set_transport_disabled(value));
        let playing = store.editor.preview_playback_state == PreviewPlaybackState::Playing;
        set_if_changed(player.get_playing(), playing, |value| player.set_playing(value));
        let current = format_clock_duration(clamp(store.editor.selected_second, timing.timeline_minimum, timing.total_duration));
        let label = format!("{current} / {}", format_clock_duration(timing.total_duration));
        set_if_changed(player.get_time_label(), SharedString::from(label), |value| player.set_time_label(value));
        set_if_changed(player.get_has_video(), has_video, |value| player.set_has_video(value));
        set_if_changed(player.get_muted(), store.editor.is_video_muted, |value| player.set_muted(value));

        let view = store.editor.timeline_viewport;
        let metrics = store.player.viewport.metrics;
        let minimum = ovrley_app::features::player::playback::timeline_minimum(store.player.timeline.timeline().displayed.bounds);
        let (can_zoom_in, can_zoom_out) = zoom_limits(view, timing.total_duration, minimum, metrics.width_px);
        set_if_changed(player.get_can_zoom_in(), can_zoom_in, |value| player.set_can_zoom_in(value));
        set_if_changed(player.get_can_zoom_out(), can_zoom_out, |value| player.set_can_zoom_out(value));
        let targets = fit_targets(store);
        let matched = matching_fit_target(view, &targets);
        set_if_changed(player.get_reset_view_disabled(), matched == Some(FitTargetId::All), |value| player.set_reset_view_disabled(value));
        let locale = store.i18n.language;
        let choices = targets
            .iter()
            .map(|target| Choice { kind: ChoiceKind::Item, value: target.id.id().into(), label: target.id.label(locale).into(), ..Choice::default() })
            .collect();
        replace_if_changed(player.get_fit_targets(), choices, |model| player.set_fit_targets(model));
        set_if_changed(player.get_fit_target(), SharedString::from(matched.map_or("", FitTargetId::id)), |value| player.set_fit_target(value));

        self.project_ticks(&player, store);
        let px = |second: f64| if view.view_end > view.view_start && metrics.width_px > 0.0 { (second - view.view_start) / (view.view_end - view.view_start) * metrics.width_px } else { 0.0 };
        // The playhead's line centre (6.5 px into its 13 px) sits on the playhead.
        let playhead = metrics.snap_to_screen_pixel(px(timing.clamped_playhead) - 6.5);
        set_if_changed(player.get_playhead_x(), playhead as f32, |value| player.set_playhead_x(value));
        self.project_lanes(&player, store);
        project_edits(&player, store);
        project_join_panel(&player, store);
        // One drag runs at a time: the guideline of a dragged cut, or of a dragged clip.
        let guideline = store.player.cut_drag.as_ref().and_then(|drag| drag.guideline_second).or_else(|| store.player.clip_drag.as_ref().and_then(|drag| drag.guideline_second));
        set_if_changed(player.get_guideline_visible(), guideline.is_some(), |value| player.set_guideline_visible(value));
        if let Some(second) = guideline {
            set_if_changed(player.get_guideline_x(), metrics.round_to_device_pixel(px(second)) as f32, |value| player.set_guideline_x(value));
        }
        self.drive_playback(store);
        self.drive_auto_scroll(store);
        self.drive_wanted_windows(store);
    }

    fn project_ticks(&self, player: &Player<'_>, store: &Store) {
        let view = store.editor.timeline_viewport;
        let metrics = store.player.viewport.metrics;
        let Some(placement) = tick_placement(view, metrics.width_px) else {
            replace_if_changed(player.get_major_ticks(), Vec::new(), |model| player.set_major_ticks(model));
            replace_if_changed(player.get_minor_ticks(), Vec::new(), |model| player.set_minor_ticks(model));
            return;
        };
        let mut strip = self.strip.borrow_mut();
        let stale = strip.as_ref().is_none_or(|(cached, width, scale, _, _)| *cached != placement || *width != metrics.width_px || *scale != metrics.scale_factor);
        if stale {
            let (major, minor) = compute_tick_strip(placement.anchor_second, placement.step, placement.px_per_second, metrics.width_px);
            let major = major
                .into_iter()
                .map(|tick| MajorTickView { line_x: metrics.round_to_device_pixel(tick.x) as f32, label_x: tick.x as f32, label: tick.label.unwrap_or_default().into() })
                .collect();
            let minor = minor.into_iter().map(|tick| metrics.round_to_device_pixel(tick.x) as f32).collect();
            *strip = Some((placement, metrics.width_px, metrics.scale_factor, major, minor));
        }
        let (_, _, _, major, minor) = strip.as_ref().expect("built above");
        replace_if_changed(player.get_major_ticks(), major.clone(), |model| player.set_major_ticks(model));
        replace_if_changed(player.get_minor_ticks(), minor.clone(), |model| player.set_minor_ticks(model));
        let offset = metrics.round_to_device_pixel((placement.anchor_second - view.view_start) * placement.px_per_second);
        set_if_changed(player.get_strip_offset(), offset as f32, |value| player.set_strip_offset(value));
    }
}

fn clip_view(clip: &lanes::ClipView) -> ClipView {
    ClipView {
        lane_id: clip.lane.id().into(),
        x: clip.x as f32,
        width: clip.width as f32,
        label: clip.label.as_str().into(),
        duration_label: clip.duration_label.as_str().into(),
        format_label: clip.format_label.as_str().into(),
        missing: clip.missing,
        show_text: clip.show_text,
        text_x: clip.text_x as f32,
        text_width: clip.text_width as f32,
        badge_x: clip.badge_x as f32,
        badge_max_width: clip.badge_max_width as f32,
        selected: clip.selected,
        has_selected_segment: clip.has_selected_segment,
        has_warnings: clip.has_warnings,
    }
}

fn band_view(band: &lanes::BandView) -> BandView {
    BandView { x: band.x as f32, width: band.width as f32, stripes: band.stripes.as_str().into() }
}

fn marker_view(marker: &lanes::MarkerView) -> MarkerView {
    let (lane_id, cut_id) = match &marker.target {
        MarkerTarget::Cut { video_id, cut_id } => (ClipLane::Video(video_id.clone()).id(), cut_id.clone()),
        MarkerTarget::ActivityTrim(edge) => (ClipLane::Activity.id(), if *edge == TrimEdge::Start { "start".into() } else { "end".into() }),
    };
    MarkerView {
        lane_id: lane_id.into(),
        cut_id: cut_id.into(),
        x: marker.x as f32,
        align: match marker.align {
            lanes::MarkerAlign::Start => MarkerAlign::Start,
            lanes::MarkerAlign::Center => MarkerAlign::Center,
            lanes::MarkerAlign::End => MarkerAlign::End,
        },
        line_inside: marker.line_inside,
        label: marker.label.as_str().into(),
        dragging: marker.dragging,
        removable: marker.removable,
    }
}

fn join_view(join: &lanes::JoinMarkerView) -> JoinMarkerView {
    let (band_x, band_width) = join.band.unwrap_or_default();
    JoinMarkerView {
        key: join.key.as_str().into(),
        x: join.x as f32,
        visible: join.visible,
        has_band: join.band.is_some(),
        band_x: band_x as f32,
        band_width: band_width as f32,
        invalid: join.invalid,
        has_transition: join.has_transition,
        disabled_reason: join.disabled_reason.as_deref().unwrap_or_default().into(),
        label: join.label.as_str().into(),
    }
}

/// The open join's panel (D13): its transition, duration field and picker.
fn project_join_panel(player: &Player<'_>, store: &Store) {
    let open = store.player.join_panel.as_ref().and_then(|panel| transitions::join_model(store, &panel.key).map(|model| (panel, model)));
    let Some((panel, model)) = open else {
        set_if_changed(player.get_join_panel_key(), SharedString::new(), |value| player.set_join_panel_key(value));
        return;
    };
    let locale = store.i18n.language;
    set_if_changed(player.get_join_panel_key(), SharedString::from(model.key.as_str()), |value| player.set_join_panel_key(value));
    set_if_changed(player.get_join_label(), SharedString::from(model.label.as_str()), |value| player.set_join_label(value));
    let catalogue = ovrley_core::encode::ffmpeg::transitions::xfade_transitions();
    let index_of = |name: &str| catalogue.iter().position(|candidate| candidate == name).map_or(-1, |index| index as i32);
    set_if_changed(player.get_join_preview_index(), model.preview_type.map_or(-1, index_of), |value| player.set_join_preview_index(value));
    set_if_changed(player.get_join_duration_visible(), model.duration_seconds.is_some(), |value| player.set_join_duration_visible(value));
    if let Some(seconds) = model.duration_seconds {
        let field = panel.duration.view(transitions::duration_options(model.duration_fps), seconds, locale);
        set_if_changed(player.get_join_duration_text(), SharedString::from(field.text), |value| player.set_join_duration_text(value));
        set_if_changed(player.get_join_duration_preview(), SharedString::from(field.preview.unwrap_or_default()), |value| player.set_join_duration_preview(value));
        set_if_changed(player.get_join_duration_error(), SharedString::from(field.error.unwrap_or_default()), |value| player.set_join_duration_error(value));
    }
    set_if_changed(player.get_join_error(), SharedString::from(model.error.unwrap_or_default()), |value| player.set_join_error(value));
    let none_label = ovrley_app::i18n::catalog().t_or(locale, "transitions.none", "No transition").to_string();
    let options = [
        TransitionOption { kind: TransitionOptionKind::Default, value: transitions::DEFAULT_TRANSITION_OPTION.into(), label: model.default_label.as_str().into(), index: -1 },
        TransitionOption { kind: TransitionOptionKind::None, value: ovrley_core::encode::ffmpeg::transitions::NO_TRANSITION.into(), label: none_label.into(), index: -1 },
    ]
    .into_iter()
    .chain(transitions::filtered_transitions(&panel.filter).into_iter().map(|name| TransitionOption { kind: TransitionOptionKind::Xfade, value: name.into(), label: name.into(), index: index_of(name) }))
    .collect();
    replace_if_changed(player.get_join_options(), options, |model| player.set_join_options(model));
    set_if_changed(player.get_join_value(), SharedString::from(model.value.as_str()), |value| player.set_join_value(value));
    set_if_changed(player.get_join_filter(), SharedString::from(panel.filter.as_str()), |value| player.set_join_filter(value));
}

fn project_lanes(player: &Player<'_>, video: Option<lanes::VideoLaneView>, activity: Option<lanes::ActivityLaneView>) -> Vec<lanes::TileView> {
    set_if_changed(player.get_video_lane_visible(), video.is_some(), |value| player.set_video_lane_visible(value));
    set_if_changed(player.get_activity_lane_visible(), activity.is_some(), |value| player.set_activity_lane_visible(value));
    let video = video.unwrap_or_default();
    replace_if_changed(player.get_video_clips(), video.clips.iter().map(clip_view).collect(), |model| player.set_video_clips(model));
    let segments = video
        .segments
        .iter()
        .map(|segment| SegmentView {
            lane_id: ClipLane::Video(segment.video_id.clone()).id().into(),
            key: segment.key.as_str().into(),
            x: segment.x as f32,
            width: segment.width as f32,
            excluded: segment.excluded,
            missing: segment.missing,
            seam_before: segment.seam_before,
            seam_after: segment.seam_after,
            selected: segment.selected,
            stripes: segment.stripes.as_str().into(),
            has_tiles: segment.has_tiles,
        })
        .collect();
    replace_if_changed(player.get_segments(), segments, |model| player.set_segments(model));
    replace_if_changed(player.get_overlap_bands(), video.bands.iter().map(band_view).collect(), |model| player.set_overlap_bands(model));
    replace_if_changed(player.get_cut_markers(), video.cut_markers.iter().map(marker_view).collect(), |model| player.set_cut_markers(model));
    replace_if_changed(player.get_joins(), video.joins.iter().map(join_view).collect(), |model| player.set_joins(model));

    let activity_clip = activity.as_ref().and_then(|lane| lane.clip.as_ref());
    set_if_changed(player.get_activity_clip_visible(), activity_clip.is_some(), |value| player.set_activity_clip_visible(value));
    if let Some(clip) = activity_clip {
        set_if_changed(player.get_activity_clip(), clip_view(clip), |value| player.set_activity_clip(value));
    }
    let (masks, trim_markers) = activity.as_ref().map_or((Vec::new(), Vec::new()), |lane| (lane.masks.iter().map(band_view).collect(), lane.trim_markers.iter().map(marker_view).collect()));
    replace_if_changed(player.get_trim_masks(), masks, |model| player.set_trim_masks(model));
    replace_if_changed(player.get_trim_markers(), trim_markers, |model| player.set_trim_markers(model));

    let tooltip = video.clips.iter().chain(activity_clip).find_map(|clip| clip.tooltip.as_ref().map(|tooltip| (clip.lane.id(), tooltip)));
    let warning_text = tooltip.as_ref().map(|(_, tooltip)| tooltip.warnings.join("\n")).unwrap_or_default();
    set_if_changed(player.get_tooltip_warning_text(), SharedString::from(warning_text), |value| player.set_tooltip_warning_text(value));
    let view = match tooltip {
        Some((lane_id, tooltip)) => ClipTooltipView {
            visible: true,
            lane_id: lane_id.into(),
            anchor_x: tooltip.anchor_x as f32,
            title: tooltip.title.as_str().into(),
            duration_label: tooltip.duration_label.as_str().into(),
            metrics: tooltip.metrics.as_str().into(),
            warnings: slint::ModelRc::new(slint::VecModel::from(tooltip.warnings.iter().map(|warning| SharedString::from(warning.as_str())).collect::<Vec<_>>())),
        },
        None => ClipTooltipView::default(),
    };
    let current = player.get_tooltip();
    let same = current.visible == view.visible
        && current.lane_id == view.lane_id
        && current.anchor_x == view.anchor_x
        && current.title == view.title
        && current.metrics == view.metrics
        && slint::Model::iter(&current.warnings).eq(slint::Model::iter(&view.warnings));
    if !same {
        player.set_tooltip(view);
    }
    video.tiles
}

fn project_edits(player: &Player<'_>, store: &Store) {
    use ovrley_app::features::player::{edits::video_edit_state, video_sync_mode};
    let sync = video_sync_mode(store);
    let has_video = store.video_import.has_videos();
    let has_activity_data = store.media.activity_summary.is_some();
    let state = video_edit_state(store);
    set_if_changed(player.get_trim_visible(), !sync && (has_video || has_activity_data), |value| player.set_trim_visible(value));
    set_if_changed(player.get_trim_targets_video(), has_video, |value| player.set_trim_targets_video(value));
    let (can_start, can_end) = if has_video { (state.can_trim_start, state.can_trim_end) } else { (has_activity_data, has_activity_data) };
    set_if_changed(player.get_can_trim_start(), !sync && can_start, |value| player.set_can_trim_start(value));
    set_if_changed(player.get_can_trim_end(), !sync && can_end, |value| player.set_can_trim_end(value));
    set_if_changed(player.get_video_edit_visible(), !sync && has_video, |value| player.set_video_edit_visible(value));
    set_if_changed(player.get_can_cut(), state.can_cut, |value| player.set_can_cut(value));
    set_if_changed(player.get_exclude_disabled(), state.excluded.is_none(), |value| player.set_exclude_disabled(value));
    set_if_changed(player.get_excluded(), state.excluded == Some(true), |value| player.set_excluded(value));
    let (lane, cut) = store.player.cut_popover.as_ref().map_or((String::new(), String::new()), |(video_id, cut_id)| (ClipLane::Video(video_id.clone()).id(), cut_id.clone()));
    set_if_changed(player.get_cut_popover_lane(), SharedString::from(lane), |value| player.set_cut_popover_lane(value));
    set_if_changed(player.get_cut_popover_cut(), SharedString::from(cut), |value| player.set_cut_popover_cut(value));
    let asking = store.video_import.cut_removal_request.is_some();
    set_if_changed(player.get_cut_removal_open(), asking, |value| player.set_cut_removal_open(value));
    let popover = store.player.cut_popover.is_some() || store.player.join_panel.is_some();
    set_if_changed(player.get_keeps_clip_selection(), asking || popover, |value| player.set_keeps_clip_selection(value));
}

fn surface(kind: TimelineSurfaceKind) -> SurfaceGesture {
    match kind {
        TimelineSurfaceKind::Axis => SurfaceGesture::Scrub,
        TimelineSurfaceKind::Playhead => SurfaceGesture::Playhead,
        TimelineSurfaceKind::Lanes => SurfaceGesture::Pan,
    }
}

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    let player = window.global::<Player>();
    let on = |dispatcher: &Dispatcher, action: fn(&mut Store)| {
        let dispatcher = dispatcher.clone();
        move || dispatcher.dispatch(move |store, _| action(store))
    };
    let play = dispatcher.clone();
    player.on_play(move || play.dispatch(|store, _| playback::play(store, Instant::now())));
    player.on_pause(on(dispatcher, playback::pause));
    player.on_reset_to_start(on(dispatcher, playback::reset_to_start));
    player.on_jump_to_end(on(dispatcher, playback::jump_to_end));
    player.on_toggle_mute(on(dispatcher, |store| store.editor.toggle_video_mute()));
    player.on_zoom_in(on(dispatcher, |store| viewport::zoom(store, true)));
    player.on_zoom_out(on(dispatcher, |store| viewport::zoom(store, false)));
    player.on_reset_view(on(dispatcher, viewport::reset_view));
    let step = dispatcher.clone();
    player.on_step(move |seconds| step.dispatch(move |store, _| playback::step_by_seconds(store, f64::from(seconds))));
    let fit = dispatcher.clone();
    player.on_fit_target_selected(move |id| {
        if let Some(target) = FitTargetId::from_id(&id) {
            fit.dispatch(move |store, _| {
                viewport::fit_target(store, target);
            });
        }
    });
    let pointer = dispatcher.clone();
    player.on_surface_pointer(move |kind, phase, x| {
        let x = f64::from(x);
        pointer.dispatch(move |store, _| match phase {
            PointerPhase::Down => surface_down(store, surface(kind), x),
            PointerPhase::Move => surface_move(store, x),
            PointerPhase::Up => surface_up(store, x),
            PointerPhase::Cancel => surface_cancel(store),
        });
    });
    let clip = dispatcher.clone();
    player.on_clip_pointer(move |lane_id, phase, x| {
        let lane = ClipLane::from_id(&lane_id);
        let x = f64::from(x);
        clip.dispatch(move |store, _| match phase {
            PointerPhase::Down => ovrley_app::features::player::gestures::clip_down(store, lane, x),
            PointerPhase::Move => ovrley_app::features::player::gestures::clip_move(store, x),
            PointerPhase::Up => ovrley_app::features::player::gestures::clip_up(store, x),
            PointerPhase::Cancel => surface_cancel(store),
        });
    });
    let hover = dispatcher.clone();
    player.on_clip_hover(move |lane_id, hovered| {
        let lane = ClipLane::from_id(&lane_id);
        hover.dispatch(move |store, _| {
            if hovered {
                store.player.hovered_lane = Some(lane);
            } else if store.player.hovered_lane.as_ref() == Some(&lane) {
                store.player.hovered_lane = None;
            }
        });
    });
    let marker = dispatcher.clone();
    player.on_marker_pointer(move |lane_id, cut_id, phase, x| {
        use ovrley_app::features::player::gestures::{trim_down, trim_move, trim_up};
        let lane = ClipLane::from_id(&lane_id);
        let x = f64::from(x);
        let cut_id = cut_id.to_string();
        marker.dispatch(move |store, _| {
            use ovrley_app::features::player::gestures::{cut_down, cut_move, cut_up};
            if let Some(video_id) = lane.video_id() {
                match phase {
                    PointerPhase::Down => cut_down(store, video_id, &cut_id, x),
                    PointerPhase::Move => cut_move(store, x),
                    PointerPhase::Up => {
                        let removable = cut_id != ovrley_app::features::player::lanes::EDGE_START && cut_id != ovrley_app::features::player::lanes::EDGE_END;
                        if cut_up(store) && removable {
                            store.player.cut_popover = Some((video_id.to_string(), cut_id.clone()));
                        }
                    }
                    PointerPhase::Cancel => surface_cancel(store),
                }
                return;
            }
            let edge = if cut_id == "start" { TrimEdge::Start } else { TrimEdge::End };
            match phase {
                PointerPhase::Down => trim_down(store, edge, x),
                PointerPhase::Move => trim_move(store, x),
                PointerPhase::Up => trim_up(store),
                PointerPhase::Cancel => surface_cancel(store),
            }
        });
    });
    let trim = dispatcher.clone();
    player.on_trim(move |start| {
        trim.dispatch(move |store, _| {
            use ovrley_app::lib::video_program::segments::TrimSide;
            let side = if start { TrimSide::Start } else { TrimSide::End };
            if store.video_import.has_videos() {
                ovrley_app::features::player::edits::trim_at_playhead(store, side);
            } else {
                ovrley_app::features::player::trim_activity_at_playhead(store, side);
            }
        });
    });
    player.on_cut(on(dispatcher, |store| {
        ovrley_app::features::player::edits::cut_at_playhead(store);
    }));
    player.on_toggle_excluded(on(dispatcher, |store| {
        ovrley_app::features::player::edits::toggle_excluded_at_playhead(store);
    }));
    player.on_close_cut_popover(on(dispatcher, |store| store.player.cut_popover = None));
    let remove = dispatcher.clone();
    player.on_remove_cut(move |lane_id, cut_id| {
        let lane = ClipLane::from_id(&lane_id);
        let cut_id = cut_id.to_string();
        remove.dispatch(move |store, _| {
            store.player.cut_popover = None;
            if let Some(video_id) = lane.video_id() {
                let _ = ovrley_app::store::slices::video_import::request_cut_removal(store, video_id, &cut_id, None);
            }
        });
    });
    let resolve = dispatcher.clone();
    player.on_resolve_cut_removal(move |excluded| {
        resolve.dispatch(move |store, _| {
            ovrley_app::features::player::resolve_cut_removal(store, excluded);
        });
    });
    player.on_keep_cut(on(dispatcher, ovrley_app::features::player::dismiss_cut_removal));
    bind_join_panel(&player, dispatcher);
    let wheel = dispatcher.clone();
    player.on_wheel(move |delta_y, x, ctrl| {
        wheel.dispatch(move |store, _| {
            viewport::wheel(store, f64::from(delta_y), f64::from(x), ctrl);
        });
    });
    let metrics = dispatcher.clone();
    let weak = window.as_weak();
    player.on_timeline_metrics(move |width, x, rem| {
        let scale_factor = weak.upgrade().map_or(1.0, |window| f64::from(window.window().scale_factor()));
        let next = viewport::TimelineMetrics { width_px: f64::from(width), x: f64::from(x), scale_factor, rem: f64::from(rem) };
        metrics.dispatch(move |store, _| {
            if store.player.viewport.metrics != next {
                store.player.viewport.metrics = next;
            }
        });
    });
}

/// The join panel's callbacks (D13). A choice and a committed duration are one undo step each.
fn bind_join_panel(player: &Player<'_>, dispatcher: &Dispatcher) {
    let open = dispatcher.clone();
    player.on_open_join(move |key| open.dispatch(move |store, _| transitions::open_join_panel(store, &key)));
    let close = dispatcher.clone();
    player.on_close_join(move || close.dispatch(|store, _| store.player.join_panel = None));
    let chosen = dispatcher.clone();
    player.on_join_chosen(move |value| {
        chosen.dispatch(move |store, _| {
            let Some(key) = store.player.join_panel.as_ref().map(|panel| panel.key.clone()) else { return };
            if let Err(error) = transitions::select_join_transition(store, &key, &value) {
                log::error!("join transition: {error}");
            }
        });
    });
    let filter = dispatcher.clone();
    player.on_join_filter_edited(move |text| {
        filter.dispatch(move |store, _| {
            if let Some(panel) = store.player.join_panel.as_mut() {
                panel.filter = text.to_string();
            }
        });
    });
    let edited = dispatcher.handle();
    player.on_join_duration_edited(move |text| {
        edited
            .dispatch_with_result(move |store, _| transitions::with_duration_field(store, |field, options, _, _, _| field.change(options, &text)).unwrap_or(false))
            .unwrap_or(false)
    });
    let key = dispatcher.handle();
    player.on_join_duration_key(move |text, alt, control, shift, meta| {
        let input = key_input(&text, SlintModifiers { alt, control, shift, meta }, false, cfg!(target_os = "macos"));
        key.dispatch_with_result(move |store, _| {
            transitions::with_duration_field(store, |field, options, value, locale, commit| field.key_down(options, value, &input, locale, commit)).unwrap_or(false)
        })
        .unwrap_or(false)
    });
    let key_up = dispatcher.handle();
    player.on_join_duration_key_up(move |text| {
        let released = key_input(&text, SlintModifiers::default(), false, cfg!(target_os = "macos")).key;
        key_up
            .dispatch_with_result(move |store, _| {
                transitions::with_duration_field(store, |field, options, value, locale, commit| field.key_up(options, value, &released, locale, commit)).unwrap_or(false)
            })
            .unwrap_or(false)
    });
    let blurred = dispatcher.clone();
    player.on_join_duration_blurred(move || {
        blurred.dispatch(|store, _| {
            transitions::with_duration_field(store, |field, options, value, locale, commit| field.blur(options, value, locale, commit));
        });
    });
}
