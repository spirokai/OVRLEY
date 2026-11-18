//! The transitions feature's animated previews (plan D13).
//!
//! `assets/transition-previews.webp` is one animated sprite sheet with a cell per `xfade`
//! transition. A `slint::Image` shows only the first frame of an animated WebP, so this does what
//! the webview does: while a transition panel is open, a timer takes the next frame from a lazy
//! decoder, copies it into the one image every preview tile clips its cell from, and waits that
//! frame's delay. At the last frame a new decoder starts over the same bytes, so the animation
//! loops. One decoded frame (960 × 544 RGBA) is held at a time, not all of them. When the last
//! panel closes, the timer stops and the decoder is dropped.

use crate::hooks::app_store_selectors::{replace_if_changed, set_if_changed};
use crate::{AppWindow, TransitionOption, TransitionOptionKind, TransitionPreviews, TransitionSelect};
use ovrley_app::features::transitions::{self as transitions, TransitionSelectTarget};
use ovrley_app::store::Dispatcher;
use slint::SharedString;
use image::codecs::webp::WebPDecoder;
use image::{AnimationDecoder, Frames};
use ovrley_app::store::Store;
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};
use std::cell::RefCell;
use std::io::Cursor;
use std::rc::Rc;
use std::time::Duration;

static PREVIEW_SHEET: &[u8] = include_bytes!("../../../../assets/transition-previews.webp");

/// Used when a frame has no delay, as browsers clamp a zero delay.
const FALLBACK_FRAME_DELAY: Duration = Duration::from_millis(100);

fn frames() -> Option<Frames<'static>> {
    match WebPDecoder::new(Cursor::new(PREVIEW_SHEET)) {
        Ok(decoder) => Some(decoder.into_frames()),
        Err(error) => {
            log::error!("transition previews: {error}");
            None
        }
    }
}

struct Animation {
    window: slint::Weak<AppWindow>,
    timer: slint::Timer,
    frames: RefCell<Option<Frames<'static>>>,
}

impl Animation {
    /// Shows the next frame and schedules the one after it.
    fn step(self: &Rc<Self>) {
        let frame = {
            let mut frames = self.frames.borrow_mut();
            let next = frames.as_mut().and_then(Iterator::next);
            match next {
                Some(frame) => Some(frame),
                // The end of the frames: start over.
                None => {
                    *frames = self::frames();
                    frames.as_mut().and_then(Iterator::next)
                }
            }
        };
        let frame = match frame {
            Some(Ok(frame)) => frame,
            Some(Err(error)) => {
                log::error!("transition previews: {error}");
                return self.stop();
            }
            None => return self.stop(),
        };
        let delay = Duration::from(frame.delay());
        let buffer = frame.into_buffer();
        let pixels = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(buffer.as_raw(), buffer.width(), buffer.height());
        if let Some(window) = self.window.upgrade() {
            window.global::<TransitionPreviews>().set_frame(Image::from_rgba8(pixels));
        }
        let next = Rc::downgrade(self);
        self.timer.start(slint::TimerMode::SingleShot, if delay.is_zero() { FALLBACK_FRAME_DELAY } else { delay }, move || {
            if let Some(animation) = next.upgrade() {
                animation.step();
            }
        });
    }

    fn stop(&self) {
        self.timer.stop();
        self.frames.borrow_mut().take();
    }
}

/// Gives the previews the sheet's grid from the catalogue (`assets/transitions.json`).
pub fn initialize(window: &AppWindow) {
    let grid = ovrley_core::encode::ffmpeg::transitions::transition_preview_grid();
    let previews = window.global::<TransitionPreviews>();
    previews.set_columns(grid.columns as i32);
    previews.set_cell_width(grid.width as i32);
    previews.set_cell_height(grid.height as i32);
}

#[derive(Default)]
pub struct Projection {
    animation: RefCell<Option<Rc<Animation>>>,
}

impl Projection {
    /// Runs the animation while a transition panel or a `TransitionSelect` popover is open, and
    /// projects the open popover.
    pub fn project(&self, window: &AppWindow, store: &Store) {
        project_select(window, store);
        let wanted = store.player.join_panel.is_some() || store.transition_select.is_some();
        let mut animation = self.animation.borrow_mut();
        match (wanted, animation.is_some()) {
            (true, false) => {
                let started = Rc::new(Animation { window: window.as_weak(), timer: slint::Timer::default(), frames: RefCell::new(frames()) });
                started.step();
                *animation = Some(started);
            }
            (false, true) => {
                if let Some(stopped) = animation.take() {
                    stopped.stop();
                }
            }
            _ => {}
        }
    }
}

/// The key a `TransitionSelectTrigger` names its target by.
pub fn target_key(target: &TransitionSelectTarget) -> String {
    match target {
        TransitionSelectTarget::ImportDefaults => "import-defaults".into(),
        TransitionSelectTarget::PanelDefaults => "panel-defaults".into(),
        TransitionSelectTarget::Join(key) => format!("join:{key}"),
    }
}

fn target_of(key: &str) -> Option<TransitionSelectTarget> {
    match key {
        "import-defaults" => Some(TransitionSelectTarget::ImportDefaults),
        "panel-defaults" => Some(TransitionSelectTarget::PanelDefaults),
        _ => key.strip_prefix("join:").map(|join| TransitionSelectTarget::Join(join.to_string())),
    }
}

/// The join model of a join target.
fn join_of(store: &Store, key: &str) -> Option<transitions::JoinModel> {
    transitions::join_models(&store.player.timeline.timeline().committed, store.i18n.language).into_iter().find(|model| model.key == key)
}

/// The selected value of a target: its default type, or the join's option.
fn selected_value(store: &Store, target: &TransitionSelectTarget) -> String {
    match target {
        TransitionSelectTarget::ImportDefaults => store.video_import_review.review.as_ref().map(|review| review.transition_defaults.transition_type.clone()).unwrap_or_default(),
        TransitionSelectTarget::PanelDefaults => store.video_import.transition_defaults.transition_type.clone(),
        TransitionSelectTarget::Join(key) => join_of(store, key).map(|model| model.value).unwrap_or_default(),
    }
}

fn project_select(window: &AppWindow, store: &Store) {
    let global = window.global::<TransitionSelect>();
    let Some(select) = &store.transition_select else {
        set_if_changed(global.get_open_target(), SharedString::new(), |value| global.set_open_target(value));
        return;
    };
    let locale = store.i18n.language;
    let none_label = ovrley_app::i18n::catalog().t_or(locale, "transitions.none", "No transition").to_string();
    let default_option = match &select.target {
        TransitionSelectTarget::Join(key) => join_of(store, key).map(|model| TransitionOption {
            kind: TransitionOptionKind::Default,
            value: transitions::DEFAULT_TRANSITION_OPTION.into(),
            label: model.default_label.as_str().into(),
            index: -1,
        }),
        _ => None,
    };
    let options = default_option.into_iter().chain(std::iter::once(TransitionOption { kind: TransitionOptionKind::None, value: ovrley_core::encode::ffmpeg::transitions::NO_TRANSITION.into(), label: none_label.into(), index: -1 })
        .chain(transitions::filtered_transitions(&select.filter).into_iter().map(|name| TransitionOption {
            kind: TransitionOptionKind::Xfade,
            value: name.into(),
            label: name.into(),
            index: crate::features::video_preview::transition_index(Some(name)),
        })))
        .collect();
    replace_if_changed(global.get_options(), options, |model| global.set_options(model));
    set_if_changed(global.get_value(), SharedString::from(selected_value(store, &select.target)), |value| global.set_value(value));
    set_if_changed(global.get_filter(), SharedString::from(select.filter.as_str()), |value| global.set_filter(value));
    set_if_changed(global.get_open_target(), SharedString::from(target_key(&select.target).as_str()), |value| global.set_open_target(value));
}

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    let global = window.global::<TransitionSelect>();
    let handle = dispatcher.handle();
    global.on_open(move |key| {
        if let Some(target) = target_of(&key) {
            handle.dispatch(move |store, _| transitions::open_transition_select(store, target));
        }
    });
    let handle = dispatcher.handle();
    global.on_close(move || handle.dispatch(|store, _| transitions::close_transition_select(store)));
    let handle = dispatcher.handle();
    global.on_filter_edited(move |text| handle.dispatch(move |store, _| transitions::set_transition_select_filter(store, text.to_string())));
    let handle = dispatcher.handle();
    global.on_chosen(move |value| {
        handle.dispatch(move |store, _| {
            if let Err(error) = transitions::choose_transition(store, &value) {
                log::error!("transition select: {error}");
            }
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_frame_covers_the_whole_sheet_and_has_a_delay() {
        let grid = ovrley_core::encode::ffmpeg::transitions::transition_preview_grid();
        let rows = ovrley_core::encode::ffmpeg::transitions::xfade_transitions().len().div_ceil(grid.columns as usize) as u32;
        let frames: Vec<_> = frames().unwrap().collect::<Result<_, _>>().unwrap();
        assert!(frames.len() > 1, "the sheet is animated");
        for frame in &frames {
            assert_eq!(frame.buffer().dimensions(), (grid.columns * grid.width, rows * grid.height));
            assert!(!Duration::from(frame.delay()).is_zero());
        }
    }
}
