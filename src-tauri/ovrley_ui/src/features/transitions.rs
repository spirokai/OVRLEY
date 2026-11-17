//! The transitions feature's animated previews (plan D13).
//!
//! `assets/transition-previews.webp` is one animated sprite sheet with a cell per `xfade`
//! transition. A `slint::Image` shows only the first frame of an animated WebP, so this does what
//! the webview does: while a transition panel is open, a timer takes the next frame from a lazy
//! decoder, copies it into the one image every preview tile clips its cell from, and waits that
//! frame's delay. At the last frame a new decoder starts over the same bytes, so the animation
//! loops. One decoded frame (960 × 544 RGBA) is held at a time, not all of them. When the last
//! panel closes, the timer stops and the decoder is dropped.

use crate::{AppWindow, TransitionPreviews};
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
    /// Runs the animation while a transition panel is open (the join panel now; the video drawer's
    /// defaults picker joins it in Phase 6).
    pub fn project(&self, window: &AppWindow, store: &Store) {
        let wanted = store.player.join_panel.is_some();
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
