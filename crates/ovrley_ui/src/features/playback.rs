//! Video playback (`VideoPreview`): MDK's players follow the store, and inside the pieces their
//! frames are its clock. What to show is `ovrley_app::features::player::preview`'s answer;
//! how to get MDK to show it is decided here.
//!
//! Two players take turns: one shows the shot at the playhead, the other waits, paused, on the
//! first frame of the next one. A cut sets that one playing and pauses the first. MDK's own
//! playlist (`setNextMedia` on one player) flashes at transitions and jumps the playhead back
//! when it preloads.
//!
//! Everything here runs on the UI thread, which owns the GPU queue: MDK's callbacks only post to
//! it. Effects on the store are dispatched after the state's borrow ends, because a dispatch
//! projects, and the projection comes back here.

use crate::projection::set_if_changed;
use crate::platform::gpu::GpuDevice;
use crate::{AppWindow, VideoPlaceholder, VideoPreview};
use ovrley_app::features::player::playback;
use ovrley_app::features::player::preview::{self, Placeholder, Preview, Shot};
use ovrley_app::video::program::PlaybackSource;
use ovrley_app::store::editor::PreviewPlaybackState;
use ovrley_app::store::{DispatchHandle, Store};
use ovrley_playback::{Callbacks, Event, Renderer, State, Surface};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The textures' size until the surface's is known.
const INITIAL_SIZE: (u32, u32) = (1920, 1080);
/// A size counts as settled after this long without a change.
const RESIZE_SETTLE: Duration = Duration::from_millis(150);
/// How often the playing shot's end is checked.
const CUT_TIMER_INTERVAL: Duration = Duration::from_millis(8);
/// How long MDK may take to start playing before `Paused` means the end of the file.
const PLAY_REQUEST_GRACE: Duration = Duration::from_millis(500);

thread_local! {
    static VIDEO_PLAYBACK: RefCell<Option<VideoPlayback>> = const { RefCell::new(None) };
}

/// What playback asks of the store.
enum Effect {
    Frame { piece: String, second: f64 },
    Cut { piece: String, second: f64 },
    Resume,
    Failed { path: String },
    Reproject,
}

/// The file one player has open.
struct Media {
    path: String,
    /// Tells this player's posted callbacks from those of the player before it.
    generation: u64,
    /// Where it was loaded, until it plays or seeks: the cue it waits on.
    parked_at: Option<f64>,
    /// Where it was sent, until a playing frame takes over.
    target: Option<f64>,
    /// The last frame drawn, and when.
    frame: Option<(f64, Instant)>,
    seeking: bool,
    /// The target waits for the seek in flight or for the first frame: one seek at a time, the
    /// latest wins.
    seek_waiting: bool,
    playing: bool,
    play_requested: Instant,
    seen_playing: bool,
}

impl Media {
    fn position(&self) -> f64 {
        self.target
            .or(self.frame.map(|(second, _)| second))
            .expect("media has a target until it has a frame")
    }
}

struct Slot {
    surface: Surface,
    image: slint::Image,
    /// The texture holds a frame.
    drawn: bool,
    media: Option<Media>,
    muted: bool,
}

impl Slot {
    fn new(surface: Surface) -> Self {
        let image = image_of(&surface);
        Self {
            surface,
            image,
            drawn: false,
            media: None,
            muted: false,
        }
    }

    fn holds(&self, path: &str) -> bool {
        self.media.as_ref().is_some_and(|media| media.path == path)
    }

    fn is_near(&self, path: &str, second: f64, tolerance: f64) -> bool {
        self.media
            .as_ref()
            .is_some_and(|media| media.path == path && (media.position() - second).abs() < tolerance)
    }

    fn is_playing(&self) -> bool {
        self.media.as_ref().is_some_and(|media| media.playing)
    }

    fn unload(&mut self) {
        self.surface.unload();
        self.media = None;
    }

    fn set_muted(&mut self, muted: bool) {
        if self.muted != muted {
            if let Some(player) = self.surface.player() {
                player.set_muted(muted);
            }
            self.muted = muted;
        }
    }

    fn seek(&mut self, second: f64) {
        let Some(media) = self.media.as_mut() else { return };
        media.parked_at = None;
        media.target = Some(second);
        if media.seeking || media.frame.is_none() {
            media.seek_waiting = true;
        } else {
            self.send_seek();
        }
    }

    fn send_seek(&mut self) {
        let (Some(media), Some(player)) = (self.media.as_mut(), self.surface.player()) else { return };
        let Some(second) = media.target else { return };
        media.seek_waiting = false;
        media.seeking = player.seek(second);
        if !media.seeking {
            log::warn!("playback: MDK refused a seek to {second:.3} s");
        }
        // A player that reached the end of its file paused itself.
        if media.playing {
            player.set_state(State::Playing);
            media.play_requested = Instant::now();
            media.seen_playing = false;
        }
    }

    /// Seeks unless the player is within `tolerance` of `second` already.
    fn hold(&mut self, second: f64, tolerance: f64) {
        let away = self
            .media
            .as_ref()
            .is_some_and(|media| (media.position() - second).abs() >= tolerance);
        if away {
            self.seek(second);
        }
    }

    fn play(&mut self) {
        let (Some(media), Some(player)) = (self.media.as_mut(), self.surface.player()) else { return };
        if media.playing {
            return;
        }
        media.playing = true;
        media.parked_at = None;
        media.play_requested = Instant::now();
        media.seen_playing = false;
        // Without a frame yet, playing starts with the first one (`rendered`).
        if let Some((_, presented)) = media.frame.as_mut() {
            *presented = media.play_requested;
            player.set_state(State::Playing);
        }
    }

    fn pause(&mut self) {
        let (Some(media), Some(player)) = (self.media.as_mut(), self.surface.player()) else { return };
        if media.playing {
            player.set_state(State::Paused);
            media.playing = false;
        }
    }

    /// A frame was drawn. Returns whether it is the media's first.
    fn rendered(&mut self, second: f64) -> bool {
        self.drawn = true;
        let Some(media) = self.media.as_mut() else { return false };
        let first = media.frame.is_none();
        // A redraw of the same frame keeps the time it was presented at.
        if media.frame.is_none_or(|(last, _)| last != second) {
            media.frame = Some((second, Instant::now()));
        }
        if media.playing && !media.seeking && !media.seek_waiting {
            media.target = None;
        }
        if first {
            if media.playing {
                media.play_requested = Instant::now();
                if let Some(player) = self.surface.player() {
                    player.set_state(State::Playing);
                }
            }
            if media.seek_waiting {
                self.send_seek();
            }
        }
        first
    }
}

/// The playing shot, which is the clock.
struct Clock {
    piece: String,
    end: f64,
    source_shift: f64,
    frame_seconds: f64,
}

/// The shot the active player was last lined up with: its piece, its place on the timeline and
/// whether it is a transition's blend.
#[derive(PartialEq)]
struct Aligned {
    piece: String,
    timeline_offset: f64,
    source_shift: f64,
}

impl Aligned {
    fn of(shot: &Shot<'_>) -> Self {
        Self {
            piece: shot.piece.id.clone(),
            timeline_offset: shot.piece.timeline_start_second - shot.piece.source_start_second,
            source_shift: shot.source_shift,
        }
    }
}

struct VideoPlayback {
    window: slint::Weak<AppWindow>,
    dispatch: DispatchHandle,
    slots: [Slot; 2],
    active: usize,
    /// The slot on screen: the active one, once it has a frame.
    displayed: usize,
    generation: u64,
    clock: Option<Clock>,
    aligned: Option<Aligned>,
    /// The playhead that pausing stopped at. The frame on screen stays while the playhead does.
    paused_at: Option<f64>,
    cut_timer: slint::Timer,
    resize_timer: slint::Timer,
    pending_size: Option<(u32, u32)>,
    /// The longest texture side the device allows.
    max_side: u32,
}

/// Makes the players on the shared device. Without one there is no video: the timeline clock plays
/// over the videos and the surface says why.
pub fn start(window: &AppWindow, gpu: Option<&GpuDevice>, dispatch: &DispatchHandle) {
    let Some(gpu) = gpu else {
        log::warn!("playback: no video, the window has no hardware GPU device");
        return;
    };
    let slots = Renderer::new(&gpu.device, &gpu.queue).and_then(|renderer| {
        let slots = [
            Slot::new(Surface::new(&renderer, INITIAL_SIZE)?),
            Slot::new(Surface::new(&renderer, INITIAL_SIZE)?),
        ];
        Ok((slots, renderer.max_side()))
    });
    let (slots, max_side) = match slots {
        Ok(slots) => slots,
        Err(error) => {
            log::error!("playback: no video on this GPU: {error}");
            return;
        }
    };
    let state = VideoPlayback {
        window: window.as_weak(),
        dispatch: dispatch.clone(),
        slots,
        active: 0,
        displayed: 0,
        generation: 0,
        clock: None,
        aligned: None,
        paused_at: None,
        cut_timer: slint::Timer::default(),
        resize_timer: slint::Timer::default(),
        pending_size: None,
        max_side,
    };
    VIDEO_PLAYBACK.with(|cell| *cell.borrow_mut() = Some(state));
    dispatch.dispatch(|store, _| store.player.video_clock_available = true);
}

/// Runs after each dispatch (from the projections).
pub fn project(window: &AppWindow, store: &Store) {
    let visible = preview::surface_visible(store);
    let preview = preview::resolve_preview(store);
    project_surface(window, &preview, visible);
    with_video_playback(|state| {
        if visible {
            state.follow_size(window, store);
        }
        state.follow(store, &preview, visible)
    });
}

fn project_surface(window: &AppWindow, preview: &Preview<'_>, visible: bool) {
    let global = window.global::<VideoPreview>();
    let placeholder = match preview.placeholder.filter(|_| visible) {
        None => VideoPlaceholder::None,
        Some(Placeholder::Missing) => VideoPlaceholder::Missing,
        Some(Placeholder::Failed) => VideoPlaceholder::Failed,
        Some(Placeholder::NoGpu) => VideoPlaceholder::NoGpu,
    };
    set_if_changed(global.get_placeholder(), placeholder, |value| global.set_placeholder(value));
    set_if_changed(global.get_frame_shown(), preview.shot.is_some(), |value| global.set_frame_shown(value));
    set_if_changed(global.get_dimmed(), preview.dimmed, |value| global.set_dimmed(value));
    set_if_changed(global.get_excluded(), preview.excluded, |value| global.set_excluded(value));
    set_if_changed(global.get_overlap(), preview.overlap, |value| global.set_overlap(value));
    set_if_changed(global.get_rendering_transition(), preview.rendering_transition, |value| {
        global.set_rendering_transition(value)
    });
}

/// Drops the players (on the UI thread, before the device) when the window closed.
pub fn shutdown() {
    let state = VIDEO_PLAYBACK.with(|cell| cell.borrow_mut().take());
    if let Some(state) = state {
        let started = Instant::now();
        drop(state);
        log::debug!("playback: torn down in {:?}", started.elapsed());
    }
}

/// Runs on the playback state, then dispatches what it asks for.
fn with_video_playback(run: impl FnOnce(&mut VideoPlayback) -> Option<Effect>) {
    let asked = VIDEO_PLAYBACK.with(|cell| {
        let mut cell = cell.borrow_mut();
        let state = cell.as_mut()?;
        let effect = run(state)?;
        Some((state.dispatch.clone(), effect))
    });
    let Some((dispatch, effect)) = asked else { return };
    let now = Instant::now();
    match effect {
        Effect::Frame { piece, second } => playback::video_frame_presented(&dispatch, piece, second),
        Effect::Cut { piece, second } => {
            dispatch.dispatch(move |store, _| playback::video_clock_cut(store, &piece, second, now))
        }
        Effect::Resume => dispatch.dispatch(move |store, _| playback::resume_from_playhead(store, now)),
        Effect::Failed { path } => dispatch.dispatch(move |store, _| preview::playback_failed(store, path, now)),
        Effect::Reproject => dispatch.dispatch(|_, _| {}),
    }
}

fn post(run: impl FnOnce() + Send + 'static) {
    if let Err(error) = slint::invoke_from_event_loop(run) {
        log::debug!("playback: could not post to the UI thread: {error}");
    }
}

fn callbacks(generation: u64) -> Callbacks {
    let pending = Arc::new(AtomicBool::new(false));
    Callbacks {
        render: Box::new(move || {
            // Posts coalesce while one is pending.
            if !pending.swap(true, Ordering::AcqRel) {
                let pending = Arc::clone(&pending);
                post(move || {
                    pending.store(false, Ordering::Release);
                    with_video_playback(|state| state.render(state.slot_of(generation)?));
                });
            }
        }),
        event: Box::new(move |event| match event {
            Event::SeekFinished => post(move || with_video_playback(|state| state.seek_finished(generation))),
            Event::Failed => post(move || with_video_playback(|state| state.failed(generation))),
        }),
    }
}

fn image_of(surface: &Surface) -> slint::Image {
    slint::Image::try_from(surface.texture().clone())
        .expect("an Rgba8Unorm texture with TEXTURE_BINDING | RENDER_ATTACHMENT")
}

impl VideoPlayback {
    fn slot_of(&self, generation: u64) -> Option<usize> {
        self.slots
            .iter()
            .position(|slot| slot.media.as_ref().is_some_and(|media| media.generation == generation))
    }

    fn load(&mut self, index: usize, path: &str, second: f64) {
        log::debug!("playback: player {index} opens {path} at {second:.3} s");
        self.generation += 1;
        let slot = &mut self.slots[index];
        slot.surface.load(path, second, callbacks(self.generation));
        // A new player has its sound on. Only the user's mute changes that: the waiting player is
        // paused, so it is silent without one.
        slot.muted = false;
        slot.media = Some(Media {
            path: path.to_string(),
            generation: self.generation,
            parked_at: Some(second),
            target: Some(second),
            frame: None,
            seeking: false,
            seek_waiting: false,
            playing: false,
            play_requested: Instant::now(),
            seen_playing: false,
        });
    }

    /// Shows the active slot once it has a frame; until then the one before stays on screen.
    fn show(&mut self) {
        let active = &self.slots[self.active];
        if active.media.as_ref().is_some_and(|media| media.frame.is_some()) {
            self.displayed = self.active;
        }
        let shown = &self.slots[self.displayed];
        if let (true, Some(window)) = (shown.drawn, self.window.upgrade()) {
            // MDK drew into the same texture. Setting its image again marks the video's item dirty
            // (Slint never counts two wgpu-texture images as equal), so partial rendering repaints
            // the video's area; a redraw request alone repaints nothing.
            window.global::<VideoPreview>().set_frame(shown.image.clone());
        }
    }

    fn render(&mut self, index: usize) -> Option<Effect> {
        let second = self.slots[index].surface.render()?;
        let first = self.slots[index].rendered(second);
        if index == self.active || index == self.displayed {
            self.show();
        }
        if index != self.active {
            return None;
        }
        if first {
            // The other player may load now that this one is on screen.
            return Some(Effect::Reproject);
        }
        let clock = self.clock.as_ref()?;
        // While a seek is under way, the frames still come from where it started.
        self.slots[index].media.as_ref()?.target.is_none().then(|| Effect::Frame {
            piece: clock.piece.clone(),
            second: second + clock.source_shift,
        })
    }

    fn seek_finished(&mut self, generation: u64) -> Option<Effect> {
        let index = self.slot_of(generation)?;
        let slot = &mut self.slots[index];
        let media = slot.media.as_mut()?;
        media.seeking = false;
        if media.seek_waiting {
            slot.send_seek();
        }
        None
    }

    fn failed(&mut self, generation: u64) -> Option<Effect> {
        let index = self.slot_of(generation)?;
        let path = self.slots[index].media.as_ref()?.path.clone();
        log::error!("playback: MDK could not play {path}");
        self.slots[index].unload();
        Some(Effect::Failed { path })
    }

    /// The end of the playing shot: its last frame has had its time, or the file ended (with
    /// `continue_at_end` MDK pauses there, and may skip the last frames).
    fn check_cut(&mut self) -> Option<Effect> {
        let clock = self.clock.as_ref()?;
        let slot = &mut self.slots[self.active];
        let media = slot.media.as_mut()?;
        let (second, presented) = media.frame?;
        if media.seeking || media.seek_waiting {
            return None;
        }
        let state = slot.surface.player()?.state();
        media.seen_playing |= state == State::Playing;
        let file_ended =
            state == State::Paused && (media.seen_playing || media.play_requested.elapsed() >= PLAY_REQUEST_GRACE);
        let reached_end = media.target.is_none()
            && second + presented.elapsed().as_secs_f64().min(clock.frame_seconds) >= clock.end - 0.002;
        if !file_ended && !reached_end {
            return None;
        }
        let effect = Effect::Cut {
            piece: clock.piece.clone(),
            second: clock.end + clock.source_shift,
        };
        self.clock = None;
        Some(effect)
    }

    /// A player lets go of a file that is no longer in the collection, is missing or failed, and of
    /// a transition's blend no join needs.
    fn release_unused(&mut self, store: &Store) {
        for slot in &mut self.slots {
            let Some(media) = &slot.media else { continue };
            let used = !store.player.unplayable_paths.contains(&media.path)
                && (store.transition_proxies.is_ready(&media.path)
                    || store
                        .video_import
                        .imported_videos
                        .iter()
                        .any(|video| !video.missing && video.path == media.path));
            if !used {
                slot.unload();
            }
        }
    }

    fn follow(&mut self, store: &Store, preview: &Preview<'_>, visible: bool) -> Option<Effect> {
        self.release_unused(store);
        let editor = &store.editor;
        let dragging = store.video_import.video_drag_preview.is_some();
        let video_playing = editor.preview_playback_state == PreviewPlaybackState::Playing
            && editor.preview_playback_source == PlaybackSource::Video
            && !dragging;
        self.clock = None;
        // An edit moved the playing piece away, or its file is gone.
        let lost = video_playing && !preview.shot.is_some_and(|shot| shot.inside);
        let Some(shot) = preview.shot.filter(|_| visible) else {
            self.slots.iter_mut().for_each(Slot::pause);
            self.cut_timer.stop();
            self.aligned = None;
            self.paused_at = None;
            return lost.then_some(Effect::Resume);
        };

        let was_playing = self.slots[self.active].is_playing();
        let frame = shot.piece.fps.frames_to_seconds(1);
        let near = frame * 1.5;
        let aligned = Aligned::of(&shot);
        let changed = self.aligned.as_ref() != Some(&aligned);
        self.aligned = Some(aligned);

        let (active, other) = (self.active, 1 - self.active);
        let reached = !changed && self.slots[active].holds(shot.path);
        if !reached && !self.slots[active].is_near(shot.path, shot.second, near) {
            let swap = self.slots[other].is_near(shot.path, shot.second, near)
                || (!self.slots[active].holds(shot.path) && self.slots[other].holds(shot.path));
            if swap {
                self.slots[active].pause();
                self.active = other;
            } else if !self.slots[active].holds(shot.path) {
                self.load(active, shot.path, shot.second);
            }
        }

        let selected = editor.selected_second;
        let slot = &mut self.slots[self.active];
        if video_playing && shot.inside {
            // A new shot and Play line up with the playhead, which then follows the player. Play
            // from where pausing stopped needs no seek, and a seek would delay the start.
            let resumes = !changed && self.paused_at == Some(selected);
            if (changed || !slot.is_playing()) && !resumes {
                slot.hold(shot.second, near);
            }
            self.paused_at = None;
            slot.set_muted(editor.is_video_muted);
            slot.play();
            self.clock = Some(Clock {
                piece: shot.piece.id.clone(),
                end: shot.end,
                source_shift: shot.source_shift,
                frame_seconds: frame,
            });
        } else {
            slot.pause();
            match editor.preview_playback_state {
                state if dragging || state == PreviewPlaybackState::Scrubbing => {
                    self.paused_at = None;
                    slot.hold(shot.second, frame / 2.0);
                }
                PreviewPlaybackState::Playing => {
                    // The timeline clock leaves the frame the video clock left.
                    self.paused_at = None;
                    if changed {
                        slot.hold(shot.second, near);
                    }
                }
                _ => {
                    // Pausing keeps the frame on screen: the player is up to a preview frame ahead
                    // of the playhead's last published second.
                    let published_within = 1.0 / playback::player_timing(store).effective_fps + 2.0 * frame;
                    let just_paused = was_playing && !changed && slot.is_near(shot.path, shot.second, published_within);
                    if just_paused {
                        self.paused_at = Some(selected);
                    } else if changed || self.paused_at != Some(selected) {
                        self.paused_at = None;
                        slot.hold(shot.second, frame / 2.0);
                    }
                }
            }
        }
        self.show();

        let settled = !dragging && editor.preview_playback_state != PreviewPlaybackState::Scrubbing;
        // The other player's texture is still on screen until this one has a frame.
        if settled && self.displayed == self.active {
            if let Some(cue) = preview.next {
                let other = 1 - self.active;
                let waits = self.slots[other].media.as_ref().is_some_and(|media| {
                    media.path == cue.path
                        && media.parked_at.is_some_and(|second| (second - cue.second).abs() < frame / 2.0)
                });
                if !waits {
                    self.load(other, cue.path, cue.second);
                }
            }
        }

        if self.clock.is_none() {
            self.cut_timer.stop();
        } else if !self.cut_timer.running() {
            self.cut_timer.start(slint::TimerMode::Repeated, CUT_TIMER_INTERVAL, || {
                with_video_playback(VideoPlayback::check_cut);
            });
        }
        lost.then_some(Effect::Resume)
    }

    /// The textures follow the visible surface's size in physical pixels, once it settled, up to
    /// the device's longest side. Sync Doctor's screen is also the scene at its mode's display scale.
    fn follow_size(&mut self, window: &AppWindow, store: &Store) {
        let (width, height) = ovrley_app::features::canvas::scene_size(store);
        let scale = (ovrley_app::features::canvas::display_scale(store) * f64::from(window.window().scale_factor()))
            .min(f64::from(self.max_side) / width.max(height));
        let size = ((width * scale).round().max(2.0) as u32, (height * scale).round().max(2.0) as u32);
        if self.slots.iter().all(|slot| slot.surface.size() == size) {
            self.pending_size = None;
            self.resize_timer.stop();
        } else if self.pending_size != Some(size) {
            self.pending_size = Some(size);
            self.resize_timer.start(slint::TimerMode::SingleShot, RESIZE_SETTLE, || {
                with_video_playback(VideoPlayback::resize);
            });
        }
    }

    fn resize(&mut self) -> Option<Effect> {
        let size = self.pending_size.take()?;
        for slot in &mut self.slots {
            if let Err(error) = slot.surface.resize(size) {
                log::error!("playback: no {}×{} texture: {error}", size.0, size.1);
                continue;
            }
            slot.image = image_of(&slot.surface);
            // Paused, nothing else redraws the current frame into the new texture.
            let second = slot.surface.render();
            slot.drawn = second.is_some();
            if let Some(second) = second {
                slot.rendered(second);
            }
        }
        self.show();
        None
    }
}
