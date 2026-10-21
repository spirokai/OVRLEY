//! The `xfade` transition catalogue shared by project validation and rendering.
//!
//! The catalogue and the transition defaults live in `assets/transitions.json`
//! so frontend and backend share one source of truth.

use std::sync::OnceLock;

use serde::Deserialize;

/// The join type of a hard cut.
pub const NO_TRANSITION: &str = "none";

/// Transition defaults of new and migrated projects.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransitionDefaults {
    #[serde(rename = "type")]
    pub transition_type: String,
    pub duration_seconds: f64,
    pub split_gap_threshold_seconds: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransitionCatalogue {
    /// Built-in FFmpeg `xfade` transitions (FFmpeg 8.1, `transition=custom` excluded).
    xfade: Vec<String>,
    defaults: TransitionDefaults,
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

/// Transition defaults of new and migrated projects.
pub fn transition_defaults() -> &'static TransitionDefaults {
    &catalogue().defaults
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
