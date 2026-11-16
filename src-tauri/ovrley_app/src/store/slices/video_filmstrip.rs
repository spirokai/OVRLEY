//! Port of `store/slices/createVideoFilmstripSlice.js`: the timeline filmstrips of the source
//! videos. Session state: not undoable, not saved (strips live in the disk cache).

use crate::lib::video_program::SecondsRange;
use ovrley_services::video_filmstrip::FilmstripWindow;
use std::collections::BTreeMap;
use std::sync::Arc;

/// A loaded strip: its sprite decoded to RGBA (frames side by side), and its sampling: sample `i`
/// stands for the stretch from `start + i × spacing` to `start + (i + 1) × spacing`.
#[derive(Clone, Debug, PartialEq)]
pub struct ReadyStrip {
    pub pixels: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    pub start_seconds: f64,
    pub spacing_seconds: f64,
    pub sample_count: u32,
}

impl ReadyStrip {
    pub fn end_seconds(&self) -> f64 {
        self.start_seconds + f64::from(self.sample_count) * self.spacing_seconds
    }

    /// The source stretch the strip covers.
    pub fn stretch(&self) -> SecondsRange {
        SecondsRange { start_second: self.start_seconds, end_second: self.end_seconds() }
    }
}

/// One strip of a video.
#[derive(Clone, Debug, PartialEq)]
pub enum StripState {
    /// Loading by its job.
    Generating { job: u64 },
    Ready(Arc<ReadyStrip>),
    Failed,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct VideoFilmstripSlice {
    /// `videoFilmstrips[path][key]`: `base`, or `z<level>-<window>`.
    pub strips: BTreeMap<String, BTreeMap<String, StripState>>,
    /// `wantedFilmstripWindows`: the zoom windows the timeline shows, per video path.
    pub wanted: BTreeMap<String, Vec<FilmstripWindow>>,
}

impl VideoFilmstripSlice {
    pub fn set(&mut self, path: &str, key: &str, strip: StripState) {
        self.strips.entry(path.to_string()).or_default().insert(key.to_string(), strip);
    }

    pub fn remove(&mut self, path: &str, key: &str) {
        if let Some(strips) = self.strips.get_mut(path) {
            strips.remove(key);
            if strips.is_empty() {
                self.strips.remove(path);
            }
        }
    }

    pub fn get(&self, path: &str, key: &str) -> Option<&StripState> {
        self.strips.get(path).and_then(|strips| strips.get(key))
    }
}
