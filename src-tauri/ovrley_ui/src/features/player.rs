//! The player feature (phase 4): its projection into the `Player` global, its callbacks, and the
//! playback timer.
//!
//! The timeline clock is driven by a repeating 8 ms timer while it plays (Concat drives its clock
//! the same way; `concat-playback.md`). A tick dispatches only when the next preview frame is due,
//! so the projections run at the preview's frame rate, not at the timer's. Nothing runs while
//! paused.

use crate::hooks::app_store_selectors::{replace_if_changed, set_if_changed};
use crate::{AppWindow, Choice, ChoiceKind, MajorTickView, Player, PointerPhase, TimelineSurfaceKind};
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
    strip: RefCell<Option<TickStrip>>,
}

impl Projection {
    pub fn new(dispatch: DispatchHandle) -> Self {
        Self { dispatch, timer: Rc::new(slint::Timer::default()), strip: RefCell::new(None) }
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
        self.drive_playback(store);
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
