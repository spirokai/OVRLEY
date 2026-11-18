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
}

impl SecondsField {
    /// The key the UI names the field by.
    pub fn key(&self) -> String {
        match self {
            Self::ImportRowOffset(video_id) => format!("import-row:{video_id}"),
            Self::ImportSharedOffset => "import-shared".into(),
            Self::ImportTransitionDuration => "import-transition".into(),
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "import-shared" => Some(Self::ImportSharedOffset),
            "import-transition" => Some(Self::ImportTransitionDuration),
            _ => key
                .strip_prefix("import-row:")
                .map(|video_id| Self::ImportRowOffset(video_id.to_string())),
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
    }
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
        ] {
            assert_eq!(SecondsField::from_key(&field.key()), Some(field));
        }
        assert_eq!(SecondsField::from_key("nothing"), None);
    }
}
