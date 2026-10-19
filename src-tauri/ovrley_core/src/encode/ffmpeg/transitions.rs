//! The `xfade` transition catalogue shared by project validation and rendering.

/// The join type of a hard cut.
pub const NO_TRANSITION: &str = "none";

/// Built-in FFmpeg `xfade` transitions (FFmpeg 8.1, `transition=custom` excluded).
/// Mirrors `app/src/lib/video-program/transition-catalog.js`.
pub const XFADE_TRANSITIONS: [&str; 58] = [
    "fade",
    "wipeleft",
    "wiperight",
    "wipeup",
    "wipedown",
    "slideleft",
    "slideright",
    "slideup",
    "slidedown",
    "circlecrop",
    "rectcrop",
    "distance",
    "fadeblack",
    "fadewhite",
    "radial",
    "smoothleft",
    "smoothright",
    "smoothup",
    "smoothdown",
    "circleopen",
    "circleclose",
    "vertopen",
    "vertclose",
    "horzopen",
    "horzclose",
    "dissolve",
    "pixelize",
    "diagtl",
    "diagtr",
    "diagbl",
    "diagbr",
    "hlslice",
    "hrslice",
    "vuslice",
    "vdslice",
    "hblur",
    "fadegrays",
    "wipetl",
    "wipetr",
    "wipebl",
    "wipebr",
    "squeezeh",
    "squeezev",
    "zoomin",
    "fadefast",
    "fadeslow",
    "hlwind",
    "hrwind",
    "vuwind",
    "vdwind",
    "coverleft",
    "coverright",
    "coverup",
    "coverdown",
    "revealleft",
    "revealright",
    "revealup",
    "revealdown",
];

/// Whether a name is a built-in `xfade` transition.
pub fn is_xfade_transition(name: &str) -> bool {
    XFADE_TRANSITIONS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_catalogue_mirrors_the_frontend_catalogue() {
        let frontend =
            include_str!("../../../../../app/src/lib/video-program/transition-catalog.js");
        let quoted: Vec<&str> = frontend
            .split("XFADE_TRANSITIONS = Object.freeze([")
            .nth(1)
            .unwrap()
            .split("])")
            .next()
            .unwrap()
            .split('\'')
            .skip(1)
            .step_by(2)
            .collect();
        assert_eq!(quoted, XFADE_TRANSITIONS);
        assert!(is_xfade_transition("wipeleft") && !is_xfade_transition(NO_TRANSITION));
    }
}
