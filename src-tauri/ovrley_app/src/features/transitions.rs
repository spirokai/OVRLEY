//! Port of `features/transitions` (plan D13): the model of one join's transition (an override or
//! the project default, the choices, the override's duration, validation), its labels, and the
//! picker's options. The animated previews' frames are `ovrley_ui`'s.

use crate::i18n::{catalog, interpolate, Locale};
use crate::js::{number_to_string, to_fixed};
use crate::lib::seconds_input::{SecondsDisplay, SecondsInputOptions, SecondsInputState, SecondsMode};
use crate::lib::video_program::collection::TimelineProgram;
use crate::lib::video_program::program::{find_invalid_transitions, JoinSide, SkippedDefault};
use crate::lib::video_program::{Transition, TransitionKind};
use crate::store::slices::video_import::set_join_transition;
use crate::store::Store;
use ovrley_core::encode::ffmpeg::transitions::xfade_transitions;
use ovrley_core::encode::fps::Fps;

/// `DEFAULT_TRANSITION_OPTION`: the picker's value of "follow the defaults".
pub const DEFAULT_TRANSITION_OPTION: &str = "default";

fn text(locale: Locale, key: &str, default: &'static str) -> String {
    catalog().t_or(locale, key, default).to_string()
}

/// `transitionLabel`.
pub fn transition_label(transition: Transition, locale: Locale) -> String {
    match transition.kind() {
        TransitionKind::HardCut => text(locale, "transitions.none", "No transition"),
        TransitionKind::Xfade(name) => interpolate(
            &text(locale, "transitions.withDuration", "{{type}}, {{seconds}} s"),
            &[("type", name), ("seconds", &number_to_string(transition.duration_seconds()))],
        ),
    }
}

/// `skippedDefaultLabel`: why a join that follows the defaults is a hard cut.
pub fn skipped_default_label(skipped: SkippedDefault, locale: Locale) -> String {
    let part = match skipped.side {
        JoinSide::Previous => text(locale, "transitions.partBefore", "the part before"),
        JoinSide::Next => text(locale, "transitions.partAfter", "the part after"),
    };
    let room = ((skipped.room_seconds * 10.0).floor() / 10.0).max(0.0);
    interpolate(
        &text(locale, "transitions.skippedDefault", "Default transition skipped: {{part}} this join has room for {{room}} s of transition, not {{seconds}} s."),
        &[("part", &part), ("room", &number_to_string(room)), ("seconds", &number_to_string(to_fixed(skipped.duration_seconds, 1)))],
    )
}

/// `buildJoinTransitionModel`'s presentational model.
#[derive(Clone, Debug, PartialEq)]
pub struct JoinModel {
    pub key: String,
    pub video_id: String,
    pub segment_key: String,
    /// The picker's value: an `xfade` name, `none`, or `default`.
    pub value: String,
    pub label: String,
    pub type_label: String,
    pub default_label: String,
    /// The transition the preview shows; `None` for a hard cut.
    pub preview_type: Option<&'static str>,
    /// The effective transition's duration; `None` for a hard cut.
    pub duration_seconds: Option<f64>,
    /// The frame rate of the joined pieces, which durations snap to.
    pub duration_fps: Fps,
    pub error: Option<String>,
    /// The second the join sits at (the end of its outgoing piece) and its transition window.
    pub end_second: f64,
    pub skipped_default: Option<SkippedDefault>,
    pub from_video_id: String,
    pub from_segment_key: String,
}

/// `useJoinTransitionModels`: the model of each join of a timeline.
pub fn join_models(timeline: &TimelineProgram, locale: Locale) -> Vec<JoinModel> {
    let program = &timeline.program;
    let invalid: Vec<String> = find_invalid_transitions(program).into_iter().flat_map(|issue| issue.join_keys).collect();
    program
        .joins
        .iter()
        .map(|join| {
            let from = program.piece(&join.from_piece_id);
            let override_ = timeline.clips.iter().find(|clip| clip.id() == join.video_id).and_then(|clip| clip.segment(&join.segment_key).transition_in);
            let effective = join.transition;
            let is_cut = effective.is_hard_cut();
            JoinModel {
                key: join.key.clone(),
                video_id: join.video_id.clone(),
                segment_key: join.segment_key.clone(),
                value: override_.map_or(DEFAULT_TRANSITION_OPTION.to_string(), |transition| transition.kind().name().to_string()),
                label: transition_label(effective, locale),
                type_label: if is_cut { text(locale, "transitions.none", "No transition") } else { effective.kind().name().to_string() },
                default_label: if join.is_default {
                    interpolate(&text(locale, "transitions.defaultIs", "Default ({{transition}})"), &[("transition", &transition_label(effective, locale))])
                } else {
                    text(locale, "transitions.default", "Default")
                },
                preview_type: match effective.kind() {
                    TransitionKind::Xfade(name) => Some(name),
                    TransitionKind::HardCut => None,
                },
                duration_seconds: (!is_cut).then_some(effective.duration_seconds()),
                duration_fps: from.fps,
                error: invalid.contains(&join.key).then(|| text(locale, "transitions.tooLong", "Too long for the video pieces it joins")),
                end_second: from.timeline_end_second,
                skipped_default: join.skipped_default,
                from_video_id: from.video_id.clone(),
                from_segment_key: from.segment_key.clone(),
            }
        })
        .collect()
}

/// The join model of a key in the displayed timeline.
pub fn join_model(store: &Store, key: &str) -> Option<JoinModel> {
    join_models(&store.player.timeline.timeline().displayed, store.i18n.language).into_iter().find(|model| model.key == key)
}

/// `onSelect`: the default (no override), a hard cut, or an `xfade`, which keeps an override's
/// duration or starts from the default snapped to the joined frames. One undo step.
pub fn select_join_transition(store: &mut Store, key: &str, value: &str) -> Result<(), String> {
    set_join(store, key, value, None)
}

/// A join's transition by picker value, as `select_join_transition` chooses it, or with an
/// explicit `xfade` duration snapped to the joined frames as the duration field snaps it. One undo
/// step; the agent's `video_join_transition_set`.
pub fn set_join(store: &mut Store, key: &str, value: &str, duration_seconds: Option<f64>) -> Result<(), String> {
    let model = join_model(store, key).ok_or_else(|| format!("Unknown join: {key}"))?;
    let override_ = store.player.timeline.timeline().displayed.clips.iter().find(|clip| clip.id() == model.video_id).and_then(|clip| clip.segment(&model.segment_key).transition_in);
    let transition = match value {
        DEFAULT_TRANSITION_OPTION if duration_seconds.is_some() => return Err("The default transition takes its duration from the defaults".into()),
        DEFAULT_TRANSITION_OPTION => None,
        _ => match TransitionKind::from_name(value)? {
            TransitionKind::HardCut if duration_seconds.is_some() => return Err("A hard cut (`none`) has no duration".into()),
            TransitionKind::HardCut => Some(Transition::HARD_CUT),
            TransitionKind::Xfade(name) => {
                let duration = match (duration_seconds, override_) {
                    (Some(seconds), _) => model.duration_fps.snap_duration_to_frames(seconds),
                    (None, Some(transition)) if !transition.is_hard_cut() => transition.duration_seconds(),
                    (None, _) => model.duration_fps.snap_duration_to_frames(store.video_import.transition_defaults.duration_seconds),
                };
                Some(Transition::new(TransitionKind::Xfade(name), duration)?)
            }
        },
    };
    set_join_transition(store, &model.video_id, &model.segment_key, transition)
}

/// The key of the join into a clip's segment.
pub fn join_key_of_segment(store: &Store, video_id: &str, segment_key: &str) -> Option<String> {
    let timeline = store.player.timeline.timeline();
    timeline.displayed.program.joins.iter().find(|join| join.video_id == video_id && join.segment_key == segment_key).map(|join| join.key.clone())
}

/// `commitDuration`: a typed duration (already snapped to the joined frames) as an override of the
/// effective transition. One undo step.
pub fn commit_join_duration(store: &mut Store, key: &str, duration_seconds: f64) -> Result<(), String> {
    let model = join_model(store, key).ok_or_else(|| format!("Unknown join: {key}"))?;
    let Some(name) = model.preview_type else { return Err("A hard cut has no duration".into()) };
    let transition = Transition::new(TransitionKind::Xfade(name), duration_seconds)?;
    set_join_transition(store, &model.video_id, &model.segment_key, Some(transition))
}

/// The picker's options after the filter: the `xfade` names containing it.
pub fn filtered_transitions(filter: &str) -> Vec<&'static str> {
    let needle = filter.trim().to_lowercase();
    xfade_transitions().iter().map(String::as_str).filter(|name| name.contains(&needle)).collect()
}

/// The open join panel: the join, its duration field and the picker's filter.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JoinPanelState {
    pub key: String,
    pub duration: SecondsInputState,
    pub filter: String,
}

/// Opens a join's panel.
pub fn open_join_panel(store: &mut Store, key: &str) {
    store.player.join_panel = Some(JoinPanelState { key: key.to_string(), ..JoinPanelState::default() });
}

/// Runs the duration field with the open join, committing through `commit_join_duration`.
pub fn with_duration_field<R>(store: &mut Store, run: impl FnOnce(&mut SecondsInputState, SecondsInputOptions, f64, Locale, &mut dyn FnMut(f64) -> Option<String>) -> R) -> Option<R> {
    let mut panel = store.player.join_panel.take()?;
    let Some(model) = join_model(store, &panel.key) else {
        return None;
    };
    let Some(value) = model.duration_seconds else {
        store.player.join_panel = Some(panel);
        return None;
    };
    let locale = store.i18n.language;
    let mut pending = None;
    let result = run(&mut panel.duration, duration_options(model.duration_fps), value, locale, &mut |seconds| {
        pending = Some(seconds);
        None
    });
    if let Some(seconds) = pending {
        if let Err(error) = commit_join_duration(store, &panel.key, seconds) {
            panel.duration.error = Some(error);
        }
    }
    store.player.join_panel = Some(panel);
    Some(result)
}

/// The duration field's options: a frame-snapped duration shown as seconds.
pub fn duration_options(fps: Fps) -> SecondsInputOptions {
    SecondsInputOptions { fps: Some(fps), mode: SecondsMode::Duration, signed: false, display: SecondsDisplay::Seconds }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::player::reconcile_player;
    use crate::lib::video_program::fixtures::*;
    use crate::lib::video_program::SegmentEdits;
    use crate::store::slices::video_import::tests::store_with;
    use ovrley_core::encode::ffmpeg::transitions::TransitionDefaults;
    use std::rc::Rc;

    fn store() -> Store {
        let mut store = store_with(vec![simple("ride", 120.0, 0), simple("later", 60.0, 1)], &[("later", 125.0)]);
        let wipe = SegmentEdits { transition_in: Some(Transition::xfade("wipeleft", 1.5)), ..SegmentEdits::INCLUDED };
        let cuts = cut_edits(&[cut("short", 300, excluded()), cut("afterShort", 330, SegmentEdits::INCLUDED), cut("long", 900, excluded()), cut("afterLong", 1800, wipe)], SegmentEdits::INCLUDED);
        let edits = Rc::make_mut(&mut store.video_import.video_edits);
        let ride = edits.get_mut("ride").unwrap();
        ride.cuts = cuts.cuts;
        ride.segments = cuts.segments;
        store.video_import.transition_defaults = TransitionDefaults { transition_type: "fade".into(), duration_seconds: 1.0, split_gap_threshold_seconds: 2.0 };
        reconcile_player(&mut store);
        store
    }

    fn transition_of(store: &Store, segment: &str) -> Option<Transition> {
        store.video_import.video_edits["ride"].segments[segment].transition_in
    }

    #[test]
    fn selects_overrides_keeps_an_override_duration_and_stores_typed_durations() {
        let mut store = store();
        let keys: Vec<String> = join_models(&store.player.timeline.timeline().displayed, crate::i18n::DEFAULT_LOCALE).into_iter().map(|model| model.key).collect();
        assert_eq!(keys[..2], ["ride:afterShort".to_string(), "ride:afterLong".to_string()]);
        select_join_transition(&mut store, "ride:afterShort", "dissolve").unwrap();
        assert_eq!(transition_of(&store, "afterShort"), Some(Transition::xfade("dissolve", 1.0)));
        reconcile_player(&mut store);
        select_join_transition(&mut store, "ride:afterLong", "fade").unwrap();
        assert_eq!(transition_of(&store, "afterLong"), Some(Transition::xfade("fade", 1.5)));
        reconcile_player(&mut store);
        select_join_transition(&mut store, "ride:afterLong", "none").unwrap();
        assert_eq!(transition_of(&store, "afterLong"), Some(Transition::HARD_CUT));
        reconcile_player(&mut store);
        select_join_transition(&mut store, "ride:afterLong", "default").unwrap();
        assert_eq!(transition_of(&store, "afterLong"), None);
        reconcile_player(&mut store);
        commit_join_duration(&mut store, "ride:afterLong", 2.5).unwrap();
        assert_eq!(transition_of(&store, "afterLong"), Some(Transition::xfade("fade", 2.5)));
    }

    #[test]
    fn labels_read_like_react() {
        let locale = crate::i18n::DEFAULT_LOCALE;
        assert_eq!(transition_label(Transition::xfade("fade", 1.5), locale), "fade, 1.5 s");
        assert_eq!(transition_label(Transition::HARD_CUT, locale), "No transition");
        let skipped = SkippedDefault { side: JoinSide::Next, duration_seconds: 2.0, room_seconds: 0.96 };
        assert_eq!(skipped_default_label(skipped, locale), "Default transition skipped: the part after this join has room for 0.9 s of transition, not 2 s.");
        assert_eq!(filtered_transitions(" WIPE")[..2], ["wipeleft", "wiperight"]);
    }
}
