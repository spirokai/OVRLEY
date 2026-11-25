//! Transitions: the animated previews (`TransitionPreviews`), the transition select popover
//! (`TransitionSelect`), and the options list it shares with the player's join panel.
//!
//! `assets/transition-previews.webp` is one animated sprite sheet with a cell per `xfade`
//! transition. A `slint::Image` shows only the first frame of an animated WebP, so while a
//! transition panel is open, a timer takes the next frame from a lazy decoder, copies it into the
//! one image every preview tile clips its cell from, and waits that frame's delay. At the last
//! frame a new decoder starts over the same bytes, so the animation loops. One decoded frame
//! (960 × 544 RGBA) is held at a time, not all of them. When the last panel closes, the timer
//! stops and the decoder is dropped.

use crate::projection::{replace_if_changed, rgba_image, set_if_changed};
use crate::{AppWindow, TransitionOption, TransitionOptionKind, TransitionPreviews, TransitionSelect};
use image::codecs::webp::WebPDecoder;
use image::{AnimationDecoder, Frames};
use ovrley_app::features::transitions::{self, JoinModel, TransitionSelectTarget};
use ovrley_app::i18n::{catalog, Locale};
use ovrley_app::store::{Dispatcher, Store};
use ovrley_core::encode::ffmpeg::transitions::{transition_preview_grid, xfade_transitions, NO_TRANSITION};
use slint::{ComponentHandle, SharedString};
use std::cell::RefCell;
use std::io::Cursor;
use std::rc::Rc;
use std::time::Duration;

static PREVIEW_SHEET: &[u8] = include_bytes!("../../../../assets/transition-previews.webp");

/// The delay of a frame that has none.
const FALLBACK_FRAME_DELAY: Duration = Duration::from_millis(100);

/// The catalogue index of a transition, -1 for none: the preview tile's cell.
pub fn transition_index(name: Option<&str>) -> i32 {
    let index = name.and_then(|name| xfade_transitions().iter().position(|candidate| candidate == name));
    index.map_or(-1, |index| i32::try_from(index).expect("the catalogue fits an i32"))
}

/// A select's options: the join's default (when it has one), no transition, then the catalogue's
/// transitions that match `filter`.
pub fn transition_options(default: Option<&JoinModel>, filter: &str, locale: Locale) -> Vec<TransitionOption> {
    let default = default.map(|model| TransitionOption {
        kind: TransitionOptionKind::Default,
        value: transitions::DEFAULT_TRANSITION_OPTION.into(),
        label: model.default_label.as_str().into(),
        index: -1,
    });
    let none = TransitionOption {
        kind: TransitionOptionKind::None,
        value: NO_TRANSITION.into(),
        label: catalog().t_or(locale, "transitions.none", "No transition").into(),
        index: -1,
    };
    let xfades = transitions::filtered_transitions(filter)
        .into_iter()
        .map(|name| TransitionOption { kind: TransitionOptionKind::Xfade, value: name.into(), label: name.into(), index: transition_index(Some(name)) });
    default.into_iter().chain([none]).chain(xfades).collect()
}

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
            match frames.as_mut().and_then(Iterator::next) {
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
        if let Some(window) = self.window.upgrade() {
            window.global::<TransitionPreviews>().set_frame(rgba_image(buffer.as_raw(), buffer.width(), buffer.height()));
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
    let grid = transition_preview_grid();
    let previews = window.global::<TransitionPreviews>();
    let int = |value: u32| i32::try_from(value).expect("the preview grid fits an i32");
    previews.set_columns(int(grid.columns));
    previews.set_cell_width(int(grid.width));
    previews.set_cell_height(int(grid.height));
}

#[derive(Default)]
pub struct Projection {
    animation: RefCell<Option<Rc<Animation>>>,
}

impl Projection {
    /// Runs the animation while a join panel or a transition select is open, and projects the
    /// open select.
    pub fn project(&self, window: &AppWindow, store: &Store) {
        project_select(window, store);
        let wanted = store.player.join_panel.is_some() || store.transition_select.is_some();
        let mut animation = self.animation.borrow_mut();
        if wanted && animation.is_none() {
            let started = Rc::new(Animation { window: window.as_weak(), timer: slint::Timer::default(), frames: RefCell::new(frames()) });
            started.step();
            *animation = Some(started);
        } else if !wanted {
            if let Some(stopped) = animation.take() {
                stopped.stop();
            }
        }
    }
}

/// The key a `TransitionSelectTrigger` names its target by.
fn target_key(target: &TransitionSelectTarget) -> String {
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
fn join_of(store: &Store, key: &str) -> Option<JoinModel> {
    transitions::join_models(&store.player.timeline.timeline().committed, store.i18n.language).into_iter().find(|model| model.key == key)
}

/// The selected value of a target: its default type, or the join's option.
fn selected_value(store: &Store, target: &TransitionSelectTarget) -> String {
    match target {
        TransitionSelectTarget::ImportDefaults => {
            store.video_import_review.review.as_ref().map(|review| review.transition_defaults.transition_type.clone()).unwrap_or_default()
        }
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
    let join = match &select.target {
        TransitionSelectTarget::Join(key) => join_of(store, key),
        _ => None,
    };
    let options = transition_options(join.as_ref(), &select.filter, store.i18n.language);
    replace_if_changed(global.get_options(), options, |model| global.set_options(model));
    set_if_changed(global.get_value(), SharedString::from(selected_value(store, &select.target)), |value| global.set_value(value));
    set_if_changed(global.get_filter(), SharedString::from(select.filter.as_str()), |value| global.set_filter(value));
    set_if_changed(global.get_open_target(), SharedString::from(target_key(&select.target)), |value| global.set_open_target(value));
}

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    let global = window.global::<TransitionSelect>();
    let d = dispatcher.handle();
    global.on_open(move |key| {
        if let Some(target) = target_of(&key) {
            d.dispatch(move |store, _| transitions::open_transition_select(store, target));
        }
    });
    let d = dispatcher.handle();
    global.on_close(move || d.dispatch(|store, _| transitions::close_transition_select(store)));
    let d = dispatcher.handle();
    global.on_filter_edited(move |text| d.dispatch(move |store, _| transitions::set_transition_select_filter(store, text.to_string())));
    let d = dispatcher.handle();
    global.on_chosen(move |value| {
        d.dispatch(move |store, _| {
            if let Err(error) = transitions::choose_transition(store, &value) {
                log::error!("transition select: {error}");
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_frame_covers_the_whole_sheet_and_has_a_delay() {
        let grid = transition_preview_grid();
        let rows = xfade_transitions().len().div_ceil(grid.columns as usize) as u32;
        let frames: Vec<_> = frames().expect("the sheet decodes").collect::<Result<_, _>>().expect("every frame decodes");
        assert!(frames.len() > 1, "the sheet is animated");
        for frame in &frames {
            assert_eq!(frame.buffer().dimensions(), (grid.columns * grid.width, rows * grid.height));
            assert!(!Duration::from(frame.delay()).is_zero());
        }
    }
}
