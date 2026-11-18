//! The `xfade` transition catalogue shared by project validation and rendering.
//!
//! The catalogue and the transition defaults live in `assets/transitions.json`
//! so frontend and backend share one source of truth.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::composite_filters::format_seconds_arg;

/// The join type of a hard cut.
pub const NO_TRANSITION: &str = "none";

/// The project's default join transition and continuity threshold. The catalogue's are those of
/// new and migrated projects; the store holds the project's, and projects save them.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransitionDefaults {
    #[serde(rename = "type")]
    pub transition_type: String,
    pub duration_seconds: f64,
    pub split_gap_threshold_seconds: f64,
}

impl TransitionDefaults {
    /// `requireTransitionDefaults`: a catalogue type, a positive duration (kept for "no
    /// transition" so that choosing a type later keeps it) and a non-negative threshold.
    pub fn validate(&self) -> Result<(), String> {
        if self.transition_type != NO_TRANSITION && !is_xfade_transition(&self.transition_type) {
            return Err(format!("transitionDefaults unknown transition type: {}", self.transition_type));
        }
        if !self.duration_seconds.is_finite() || self.duration_seconds <= 0.0 {
            return Err("transitionDefaults.durationSeconds must be a positive finite number".into());
        }
        if !self.split_gap_threshold_seconds.is_finite() || self.split_gap_threshold_seconds < 0.0 {
            return Err("transitionDefaults.splitGapThresholdSeconds must be a finite non-negative number".into());
        }
        Ok(())
    }
}

/// The grid of `assets/transition-previews.webp`: a cell per `xfade` transition, in catalogue
/// order, row by row.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TransitionPreviewGrid {
    pub columns: u32,
    /// One cell's size in pixels.
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TransitionCatalogue {
    /// Built-in FFmpeg `xfade` transitions (FFmpeg 8.1, `transition=custom` excluded).
    xfade: Vec<String>,
    defaults: TransitionDefaults,
    preview: TransitionPreviewGrid,
}

static TRANSITION_CATALOGUE: OnceLock<TransitionCatalogue> = OnceLock::new();

fn catalogue() -> &'static TransitionCatalogue {
    TRANSITION_CATALOGUE.get_or_init(|| {
        serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/transitions.json"
        )))
        .expect("shared transition catalogue must be valid JSON")
    })
}

/// Whether a name is a built-in `xfade` transition.
pub fn is_xfade_transition(name: &str) -> bool {
    catalogue()
        .xfade
        .iter()
        .any(|transition| transition == name)
}

/// The built-in `xfade` transitions in catalogue order.
pub fn xfade_transitions() -> &'static [String] {
    &catalogue().xfade
}

/// The cell grid of the transition preview sprite sheet.
pub fn transition_preview_grid() -> TransitionPreviewGrid {
    catalogue().preview
}

/// Transition defaults of new and migrated projects.
pub fn transition_defaults() -> &'static TransitionDefaults {
    &catalogue().defaults
}

/// Sample rate of every blended or joined audio stream.
pub(crate) const BLENDED_AUDIO_SAMPLE_RATE: u32 = 48_000;

/// The `xfade` of two video streams over `duration`, from their first frames.
pub(crate) fn xfade_filter(transition: &str, duration: f64) -> String {
    format!(
        "xfade=transition={transition}:duration={}:offset=0",
        format_seconds_arg(duration)
    )
}

/// The crossfade of two audio streams over `duration`, matching `xfade_filter`.
pub(crate) fn acrossfade_filter(duration: f64) -> String {
    format!(
        "acrossfade=d={}:c1=tri:c2=tri",
        format_seconds_arg(duration)
    )
}

/// One audio stream as exactly `duration` seconds of stereo float audio at
/// `BLENDED_AUDIO_SAMPLE_RATE`, starting `start` seconds into the input, labelled
/// `[output]`. Without an input it is silence.
pub(crate) fn blended_audio_chain(
    input: Option<usize>,
    start: f64,
    duration: f64,
    output: &str,
) -> String {
    let duration = format_seconds_arg(duration);
    match input {
        Some(input) => format!(
            "[{input}:a]atrim=start={}:duration={duration},asetpts=N/SR/TB,\
             aresample={BLENDED_AUDIO_SAMPLE_RATE},\
             aformat=sample_fmts=fltp:channel_layouts=stereo,\
             apad=whole_dur={duration}[{output}]",
            format_seconds_arg(start)
        ),
        None => format!(
            "anullsrc=r={BLENDED_AUDIO_SAMPLE_RATE}:cl=stereo,atrim=duration={duration}[{output}]"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shared_catalogue_holds_unique_transitions_and_valid_defaults() {
        let catalogue = catalogue();
        let unique: std::collections::HashSet<_> = catalogue.xfade.iter().collect();
        assert_eq!(unique.len(), catalogue.xfade.len());
        assert!(is_xfade_transition("wipeleft") && !is_xfade_transition(NO_TRANSITION));
        let defaults = transition_defaults();
        assert!(
            defaults.transition_type == NO_TRANSITION
                || is_xfade_transition(&defaults.transition_type)
        );
        assert!(defaults.duration_seconds > 0.0 && defaults.split_gap_threshold_seconds >= 0.0);
    }
}
