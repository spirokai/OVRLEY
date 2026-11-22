//! The seconds fields of the import dialog, the video drawer and the video panel: one
//! `useSecondsInput` state per field, keyed by the field, with the value it shows, its options and
//! the action a commit runs. `ovrley_ui` binds one generic field component to these functions by
//! the field's key.

use crate::features::video_preview::import::{
    review_commit_shared_offset, set_review_row_offset, set_review_transition_defaults,
};
use crate::i18n::Locale;
use crate::lib::keyboard_shortcuts::KeyInput;
use crate::lib::seconds_input::{
    SecondsDisplay, SecondsInputOptions, SecondsInputState, SecondsInputView, SecondsMode,
};
use crate::store::Store;
use ovrley_core::encode::fps::Fps;
use std::collections::HashMap;

/// A seconds field.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SecondsField {
    /// A row's sync in the import dialog.
    ImportRowOffset(String),
    /// The import dialog's shared offset.
    ImportSharedOffset,
    /// The import dialog's default transition duration.
    ImportTransitionDuration,
    /// The selected video's start on the timeline, in the video drawer.
    DrawerVideoStart(String),
    /// A join's duration, in the video drawer.
    JoinDuration(String),
    /// The panel's default transition duration.
    PanelTransitionDuration,
    /// The panel's continuity threshold.
    PanelContinuityThreshold,
    /// Sync Doctor's shared offset: where the earliest clip starts (`commitSharedOffset`).
    SyncSharedOffset,
}

impl SecondsField {
    /// The key the UI names the field by.
    pub fn key(&self) -> String {
        match self {
            Self::ImportRowOffset(video_id) => format!("import-row:{video_id}"),
            Self::ImportSharedOffset => "import-shared".into(),
            Self::ImportTransitionDuration => "import-transition".into(),
            Self::DrawerVideoStart(video_id) => format!("drawer-start:{video_id}"),
            Self::JoinDuration(join_key) => format!("join:{join_key}"),
            Self::PanelTransitionDuration => "panel-transition".into(),
            Self::PanelContinuityThreshold => "panel-threshold".into(),
            Self::SyncSharedOffset => "sync-shared".into(),
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "import-shared" => Some(Self::ImportSharedOffset),
            "import-transition" => Some(Self::ImportTransitionDuration),
            "panel-transition" => Some(Self::PanelTransitionDuration),
            "panel-threshold" => Some(Self::PanelContinuityThreshold),
            "sync-shared" => Some(Self::SyncSharedOffset),
            _ => key
                .strip_prefix("import-row:")
                .map(|video_id| Self::ImportRowOffset(video_id.to_string()))
                .or_else(|| key.strip_prefix("drawer-start:").map(|video_id| Self::DrawerVideoStart(video_id.to_string())))
                .or_else(|| key.strip_prefix("join:").map(|join_key| Self::JoinDuration(join_key.to_string()))),
        }
    }

    fn is_import(&self) -> bool {
        matches!(
            self,
            Self::ImportRowOffset(_) | Self::ImportSharedOffset | Self::ImportTransitionDuration
        )
    }
}

/// What a field shows and how it steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldTarget {
    pub options: SecondsInputOptions,
    pub value: f64,
    /// Seconds the step buttons move, without them `None`.
    pub step: Option<f64>,
}

/// The field states, keyed by field.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SecondsFieldsState {
    fields: HashMap<SecondsField, SecondsInputState>,
}

impl SecondsFieldsState {
    /// Forgets the import dialog's fields, when a review starts or ends.
    pub fn clear_import(&mut self) {
        self.fields.retain(|field, _| !field.is_import());
    }
}

/// A frame-snapped clock of a signed time, as video starts are edited.
fn start_options(fps: Option<Fps>) -> SecondsInputOptions {
    SecondsInputOptions {
        fps,
        mode: SecondsMode::Time,
        signed: true,
        display: SecondsDisplay::Clock,
    }
}

/// The render frame rate as a whole-frame rate, which durations snap to.
pub fn render_frame_rate(fps: f64) -> Option<Fps> {
    Fps::new(crate::js::round(fps) as u32, 1).ok()
}

/// The field's value and options, or `None` while it shows nothing.
pub fn field_target(store: &Store, field: &SecondsField) -> Option<FieldTarget> {
    match field {
        SecondsField::ImportRowOffset(video_id) => {
            let review = store.video_import_review.review.as_ref()?;
            let derived = review.derived();
            let row = derived.model.rows.iter().find(|row| &row.id == video_id)?;
            Some(FieldTarget {
                options: start_options(Some(row.fps)),
                value: row.offset_seconds,
                step: None,
            })
        }
        SecondsField::ImportSharedOffset => {
            let review = store.video_import_review.review.as_ref()?;
            let included = review.derived().model.included;
            let clips = included.clips();
            let first = crate::lib::video_program::collection::sort_clips_by_timeline(&clips)
                .first()
                .map(|clip| clip.fps());
            Some(FieldTarget {
                options: start_options(first),
                value: included.shared_offset_seconds(),
                step: Some(0.1),
            })
        }
        SecondsField::ImportTransitionDuration => {
            let review = store.video_import_review.review.as_ref()?;
            Some(FieldTarget {
                options: SecondsInputOptions {
                    fps: render_frame_rate(store.render_settings.fps),
                    mode: SecondsMode::Duration,
                    signed: false,
                    display: SecondsDisplay::Seconds,
                },
                value: review.transition_defaults.duration_seconds,
                step: None,
            })
        }
        SecondsField::DrawerVideoStart(video_id) => {
            let timeline = store.player.timeline.timeline();
            let clip = timeline.committed.clips.iter().find(|clip| clip.id() == video_id)?;
            Some(FieldTarget { options: start_options(Some(clip.fps())), value: clip.offset_seconds(), step: None })
        }
        SecondsField::JoinDuration(join_key) => {
            let model = crate::features::transitions::join_models(&store.player.timeline.timeline().committed, store.i18n.language).into_iter().find(|model| &model.key == join_key)?;
            Some(FieldTarget { options: crate::features::transitions::duration_options(model.duration_fps), value: model.duration_seconds?, step: None })
        }
        SecondsField::PanelTransitionDuration => Some(FieldTarget {
            options: SecondsInputOptions { fps: render_frame_rate(store.render_settings.fps), mode: SecondsMode::Duration, signed: false, display: SecondsDisplay::Seconds },
            value: store.video_import.transition_defaults.duration_seconds,
            step: None,
        }),
        SecondsField::PanelContinuityThreshold => Some(FieldTarget {
            options: SecondsInputOptions { fps: None, mode: SecondsMode::Time, signed: false, display: SecondsDisplay::Clock },
            value: store.video_import.transition_defaults.split_gap_threshold_seconds,
            step: None,
        }),
        SecondsField::SyncSharedOffset => {
            let controls = crate::features::video_sync::store_video_sync_controls(store);
            Some(FieldTarget { options: start_options(controls.shared_offset_fps), value: controls.shared_offset_seconds, step: Some(0.1) })
        }
    }
}

/// Runs the field's commit action; its error shows under the field.
fn commit(store: &mut Store, field: &SecondsField, seconds: f64) -> Result<(), String> {
    match field {
        SecondsField::ImportRowOffset(video_id) => set_review_row_offset(store, video_id, seconds),
        SecondsField::ImportSharedOffset => review_commit_shared_offset(store, seconds),
        SecondsField::ImportTransitionDuration => {
            set_review_transition_defaults(store, |defaults| defaults.duration_seconds = seconds)
        }
        SecondsField::DrawerVideoStart(video_id) => crate::store::slices::video_import::set_video_offset(store, video_id, seconds, false),
        SecondsField::JoinDuration(join_key) => crate::features::transitions::commit_join_duration(store, join_key, seconds),
        SecondsField::PanelTransitionDuration => set_panel_defaults(store, |defaults| defaults.duration_seconds = seconds),
        SecondsField::PanelContinuityThreshold => set_panel_defaults(store, |defaults| defaults.split_gap_threshold_seconds = seconds),
        SecondsField::SyncSharedOffset => {
            let shared = crate::features::video_sync::store_video_sync_controls(store).shared_offset_seconds;
            crate::store::slices::video_import::shift_video_offsets(store, seconds - shared, false)
        }
    }
}

/// `useTransitionDefaultsSection`'s commit: the store's defaults with one change.
pub fn set_panel_defaults(store: &mut Store, change: impl FnOnce(&mut ovrley_core::encode::ffmpeg::transitions::TransitionDefaults)) -> Result<(), String> {
    let mut defaults = store.video_import.transition_defaults.clone();
    change(&mut defaults);
    crate::store::slices::video_import::set_transition_defaults(store, defaults)
}

/// What the field shows.
pub fn field_view(store: &Store, field: &SecondsField) -> SecondsInputView {
    let locale = store.i18n.language;
    let state = store
        .seconds_fields
        .fields
        .get(field)
        .cloned()
        .unwrap_or_default();
    match field_target(store, field) {
        Some(target) => state.view(target.options, target.value, locale),
        None => SecondsInputView {
            text: String::new(),
            preview: None,
            error: None,
        },
    }
}

/// Runs `run` with the field's state and target, then the commit it asked for.
fn with_field<R>(
    store: &mut Store,
    key: &str,
    run: impl FnOnce(
        &mut SecondsInputState,
        FieldTarget,
        Locale,
        &mut dyn FnMut(f64) -> Option<String>,
    ) -> R,
) -> Option<R> {
    let field = SecondsField::from_key(key)?;
    let target = field_target(store, &field)?;
    let locale = store.i18n.language;
    let mut state = store
        .seconds_fields
        .fields
        .remove(&field)
        .unwrap_or_default();
    let mut pending = None;
    let result = run(&mut state, target, locale, &mut |seconds| {
        pending = Some(seconds);
        None
    });
    if let Some(seconds) = pending {
        if let Err(error) = commit(store, &field, seconds) {
            state.error = Some(error);
        }
    }
    store.seconds_fields.fields.insert(field, state);
    Some(result)
}

/// The text changed; returns whether it was accepted.
pub fn edited(store: &mut Store, key: &str, text: &str) -> bool {
    with_field(store, key, |state, target, _, _| {
        state.change(target.options, text)
    })
    .unwrap_or(false)
}

/// A key press; returns whether it was used.
pub fn key_down(store: &mut Store, key: &str, input: &KeyInput) -> bool {
    with_field(store, key, |state, target, locale, commit| {
        state.key_down(target.options, target.value, input, locale, commit)
    })
    .unwrap_or(false)
}

/// A key release; returns whether it ended a step.
pub fn key_up(store: &mut Store, key: &str, released: &str) -> bool {
    with_field(store, key, |state, target, locale, commit| {
        state.key_up(target.options, target.value, released, locale, commit)
    })
    .unwrap_or(false)
}

/// The field lost focus.
pub fn blurred(store: &mut Store, key: &str) {
    with_field(store, key, |state, target, locale, commit| {
        state.blur(target.options, target.value, locale, commit)
    });
}

/// A step button (`direction` 1 or -1).
pub fn step(store: &mut Store, key: &str, direction: f64) {
    with_field(store, key, |state, target, locale, commit| {
        if let Some(step) = target.step {
            state.step(
                target.options,
                target.value,
                direction,
                step,
                locale,
                commit,
            );
        }
    });
}

/// The placement slot's reset: the draft goes, then the field's reset action runs.
pub fn clear(store: &mut Store, key: &str) {
    if let Some(field) = SecondsField::from_key(key) {
        if let Some(state) = store.seconds_fields.fields.get_mut(&field) {
            state.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_name_their_fields() {
        for field in [
            SecondsField::ImportRowOffset("a:b".into()),
            SecondsField::ImportSharedOffset,
            SecondsField::ImportTransitionDuration,
            SecondsField::DrawerVideoStart("v".into()),
            SecondsField::JoinDuration("v:start".into()),
            SecondsField::PanelTransitionDuration,
            SecondsField::PanelContinuityThreshold,
        ] {
            assert_eq!(SecondsField::from_key(&field.key()), Some(field));
        }
        assert_eq!(SecondsField::from_key("nothing"), None);
    }
}

#[cfg(test)]
mod drawer_tests {
    use super::*;
    use crate::lib::video_program::fixtures::*;
    use crate::store::slices::video_import::tests::store_with;

    #[test]
    fn the_drawer_start_keeps_a_typed_draft_and_commits_it() {
        let mut store = store_with(vec![simple("ride", 10.0, 0)], &[]);
        crate::features::player::reconcile_player(&mut store);
        let key = SecondsField::DrawerVideoStart("ride".into()).key();
        assert!(edited(&mut store, &key, "12"));
        assert_eq!(field_view(&store, &SecondsField::DrawerVideoStart("ride".into())).text, "12");
        assert!(key_down(&mut store, &key, &KeyInput::new("enter")));
        crate::features::player::reconcile_player(&mut store);
        assert_eq!(store.video_import.video_edits["ride"].offset_seconds, 12.0);
    }
}
