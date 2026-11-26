//! The seconds inputs: what they accept, how a draft parses, how a value shows, how it snaps to
//! a frame grid, each field's state, and the fields of the import dialog, the video drawer and
//! the video panel with the action each commit runs.
//!
//! A draft is a sum of terms (`1:39:25+3:45`, `5965-225`). Each term is a clock (`5:30`,
//! `1:01:40.5`) or plain seconds (`330`): digits, up to two `:` and one `.` in the last part.
//! Signed values can start with a `-`. Spaces are allowed around `+` and `-`, and a comma reads as
//! the decimal point.

use std::collections::HashMap;
use std::sync::LazyLock;
use ovrley_core::encode::timing::Fps;
use regex::Regex;
use crate::features::import::{review_commit_shared_offset, set_review_row_offset, set_review_transition_defaults};
use crate::format::{format_seconds_clock, plain_number, round_decimals};
use crate::i18n::{catalog, interpolate, Locale};
use crate::shortcuts::{match_keyboard_shortcut, Binding, KeyInput, Scope};
use crate::store::Store;

/// Decimals shown for a value; three resolve every frame up to 500 fps.
const SHOWN_DECIMALS: usize = 3;

const DRAFT_TERM: &str = r"(?:\d*:){0,2}\d*\.?\d*";
static DRAFT_SUM: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"^{DRAFT_TERM}(?:\s*[+-]\s*{DRAFT_TERM})*\s*$")).expect("valid pattern"));
static COMPLETE_SUM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(-?)([^+-]+)((?:[+-][^+-]+)*)$").expect("valid pattern"));
static SUM_TERM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([+-])([^+-]+)").expect("valid pattern"));
static COMPLETE_CLOCK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(?:(?:(\d+):)?(\d+):)?(\d+\.?\d*|\.\d+)$").expect("valid pattern"));

/// A time snaps to the nearest frame; a duration is above 0 and at least one frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecondsMode {
    Time,
    Duration,
}

/// How the field shows its value: a clock (`00:01.500`) or seconds with one decimal (`1.5`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecondsDisplay {
    Clock,
    Seconds,
}

/// Typed text as a draft, or `None` when it holds anything but a time
/// being typed. The first term starts with a digit, `:` or `.`, so only a sign comes before it.
pub fn normalize_seconds_draft(text: &str, signed: bool) -> Option<String> {
    let draft = text.replace(',', ".");
    let sum = if signed { draft.strip_prefix('-').unwrap_or(&draft) } else { draft.as_str() };
    let starts_a_term = sum.chars().next().is_some_and(|first| first.is_ascii_digit() || first == ':' || first == '.');
    let valid = sum.chars().all(char::is_whitespace) || starts_a_term && DRAFT_SUM.is_match(sum);
    valid.then_some(draft)
}

fn parse_clock_term(term: &str) -> Option<f64> {
    let captures = COMPLETE_CLOCK.captures(term)?;
    let number = |index: usize| captures.get(index).map(|part| part.as_str().parse::<f64>().expect("digits parse"));
    let (hours, minutes, seconds) = (number(1), number(2), number(3).expect("the seconds part is required"));
    if (hours.is_some() && minutes.is_some_and(|minutes| minutes >= 60.0)) || (minutes.is_some() && seconds >= 60.0) {
        return None;
    }
    Some(hours.unwrap_or(0.0) * 3600.0 + minutes.unwrap_or(0.0) * 60.0 + seconds)
}

/// A draft holding a complete sum of times. After a `:`, minutes and seconds
/// stay below 60. `None` for an incomplete or invalid draft (empty, `-`, `5:`, `5:75`, `5+`).
pub fn parse_seconds_draft(draft: &str) -> Option<f64> {
    let compact: String = draft.chars().filter(|character| !character.is_whitespace()).collect();
    let captures = COMPLETE_SUM.captures(&compact)?;
    let first = parse_clock_term(&captures[2])?;
    let mut total = if &captures[1] == "-" { -first } else { first };
    for term in SUM_TERM.captures_iter(&captures[3]) {
        let seconds = parse_clock_term(&term[2])?;
        total += if &term[1] == "+" { seconds } else { -seconds };
    }
    Some(total)
}

/// Plain seconds, up to three decimals, without trailing zeros.
pub fn format_seconds_text(seconds: f64) -> String {
    let value = round_decimals(seconds, SHOWN_DECIMALS);
    plain_number(if value == 0.0 { 0.0 } else { value })
}

/// Exactly one decimal (`1.0`), as transition durations show.
pub fn format_duration_seconds(seconds: f64) -> String {
    format!("{seconds:.1}")
}

/// The seconds a `secondsInput.step` binding moves a value; `None` for a
/// frame step on a value without a frame grid.
pub fn binding_step_seconds(binding: &Binding, fps: Option<Fps>) -> Option<f64> {
    match binding.frames {
        None => binding.seconds,
        Some(frames) => fps.map(|fps| fps.frames_to_seconds(frames.into())),
    }
}

/// A time to the nearest frame, a duration to at least one whole frame.
pub fn snap_seconds(seconds: f64, fps: Option<Fps>, mode: SecondsMode) -> f64 {
    match (fps, mode) {
        (None, _) => seconds,
        (Some(fps), SecondsMode::Duration) => fps.snap_duration_to_frames(seconds),
        (Some(fps), SecondsMode::Time) => fps.frames_to_seconds(fps.seconds_to_frames(seconds)),
    }
}

/// The fixed options of one field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SecondsInputOptions {
    pub fps: Option<Fps>,
    pub mode: SecondsMode,
    pub signed: bool,
    pub display: SecondsDisplay,
}

/// One field's state: the typed draft until it is committed (Enter or leaving the field),
/// its error, and the key of a running step.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SecondsInputState {
    pub draft: Option<String>,
    pub error: Option<String>,
    stepping_key: Option<String>,
}

/// What the field shows.
#[derive(Clone, Debug, PartialEq)]
pub struct SecondsInputView {
    pub text: String,
    /// `= 05:30 (330 s)` while a complete clock draft is typed.
    pub preview: Option<String>,
    pub error: Option<String>,
}

fn format_value(options: SecondsInputOptions, seconds: f64) -> String {
    match options.display {
        SecondsDisplay::Seconds => format_duration_seconds(seconds),
        SecondsDisplay::Clock => format_seconds_clock(seconds),
    }
}

impl SecondsInputState {
    fn draft_seconds(&self) -> Option<f64> {
        self.draft.as_deref().and_then(parse_seconds_draft)
    }

    pub fn view(&self, options: SecondsInputOptions, value: f64, locale: Locale) -> SecondsInputView {
        let draft_seconds = self.draft_seconds();
        let preview = match draft_seconds {
            Some(seconds) if self.stepping_key.is_none() && options.display == SecondsDisplay::Clock => Some(interpolate(
                catalog().t_or(locale, "components.secondsPreview", "= {{clock}} ({{seconds}} s)"),
                &[("clock", &format_seconds_clock(seconds)), ("seconds", &format_seconds_text(seconds))],
            )),
            _ => None,
        };
        SecondsInputView { text: self.draft.clone().unwrap_or_else(|| format_value(options, value)), preview, error: self.error.clone() }
    }

    fn rejection_of(options: SecondsInputOptions, seconds: f64, locale: Locale) -> Option<String> {
        if options.mode == SecondsMode::Duration && seconds <= 0.0 {
            return Some(catalog().t_or(locale, "components.secondsAboveZero", "Enter a duration above 0 seconds").into());
        }
        if !options.signed && seconds < 0.0 {
            return Some(catalog().t_or(locale, "components.secondsNotNegative", "Enter a time of 0 seconds or more").into());
        }
        None
    }

    /// Snaps and stores a value through `on_commit`, which returns a message when it rejects it.
    fn store(&mut self, options: SecondsInputOptions, value: f64, seconds: f64, locale: Locale, on_commit: impl FnOnce(f64) -> Option<String>) {
        if let Some(rejection) = Self::rejection_of(options, seconds, locale) {
            self.error = Some(rejection);
            return;
        }
        let snapped = snap_seconds(seconds, options.fps, options.mode);
        let failure = if snapped == value { None } else { on_commit(snapped) };
        if failure.is_none() {
            self.draft = None;
        }
        self.error = failure;
    }

    fn commit(&mut self, options: SecondsInputOptions, value: f64, locale: Locale, on_commit: impl FnOnce(f64) -> Option<String>) {
        if self.draft.is_none() {
            return;
        }
        match self.draft_seconds() {
            None => self.error = Some(catalog().t_or(locale, "components.secondsInvalid", "Enter a time like 05:30, a number of seconds, or a sum like 05:30+10").into()),
            Some(seconds) => self.store(options, value, seconds, locale, on_commit),
        }
    }

    /// The text changed; text that is no time being typed is refused (the field keeps its text).
    /// Returns whether it was accepted.
    pub fn change(&mut self, options: SecondsInputOptions, text: &str) -> bool {
        let Some(draft) = normalize_seconds_draft(text, options.signed) else { return false };
        self.draft = Some(draft);
        self.error = None;
        true
    }

    /// The field lost focus: commits.
    pub fn blur(&mut self, options: SecondsInputOptions, value: f64, locale: Locale, on_commit: impl FnOnce(f64) -> Option<String>) {
        self.stepping_key = None;
        self.commit(options, value, locale, on_commit);
    }

    /// A key press. Enter commits, Escape discards, and `secondsInput.step` moves the draft.
    /// Returns whether the key was handled.
    pub fn key_down(&mut self, options: SecondsInputOptions, value: f64, input: &KeyInput, locale: Locale, on_commit: impl FnOnce(f64) -> Option<String>) -> bool {
        match input.key.as_str() {
            "enter" => {
                self.commit(options, value, locale, on_commit);
                return true;
            }
            "escape" => {
                self.draft = None;
                self.error = None;
                return true;
            }
            _ => {}
        }
        let Some(shortcut) = match_keyboard_shortcut(input, Scope::SecondsInput).filter(|shortcut| shortcut.command.id == "secondsInput.step") else {
            return false;
        };
        let Some(step) = binding_step_seconds(shortcut.binding, options.fps) else { return false };
        self.stepping_key = Some(input.key.clone());
        let next = self.draft_seconds().unwrap_or(value) + step;
        self.error = Self::rejection_of(options, next, locale);
        if self.error.is_none() {
            self.draft = Some(format_value(options, snap_seconds(next, options.fps, options.mode)));
        }
        true
    }

    /// A step button: the draft or the value moved by `direction × step`, committed.
    pub fn step(&mut self, options: SecondsInputOptions, value: f64, direction: f64, step: f64, locale: Locale, on_commit: impl FnOnce(f64) -> Option<String>) {
        let seconds = self.draft_seconds().unwrap_or(value) + direction * step;
        self.store(options, value, seconds, locale, on_commit);
    }

    /// The placement slot's reset: the draft and its error go before the reset runs.
    pub fn clear(&mut self) {
        self.draft = None;
        self.error = None;
    }

    /// A key release: the end of a step commits once. Matched by key alone, as a modifier may be
    /// released before the key it stepped with.
    pub fn key_up(&mut self, options: SecondsInputOptions, value: f64, key: &str, locale: Locale, on_commit: impl FnOnce(f64) -> Option<String>) -> bool {
        if self.stepping_key.as_deref() != Some(key) {
            return false;
        }
        self.stepping_key = None;
        self.commit(options, value, locale, on_commit);
        true
    }
}

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
    /// Sync Doctor's shared offset: where the earliest clip starts.
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
    Fps::new(fps.round() as u32, 1).ok()
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
            let first = crate::video::collection::sort_clips_by_timeline(&clips)
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
            let controls = crate::features::sync::store_video_sync_controls(store);
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
        SecondsField::DrawerVideoStart(video_id) => crate::store::videos::set_video_offset(store, video_id, seconds, false),
        SecondsField::JoinDuration(join_key) => crate::features::transitions::commit_join_duration(store, join_key, seconds),
        SecondsField::PanelTransitionDuration => set_panel_defaults(store, |defaults| defaults.duration_seconds = seconds),
        SecondsField::PanelContinuityThreshold => set_panel_defaults(store, |defaults| defaults.split_gap_threshold_seconds = seconds),
        SecondsField::SyncSharedOffset => {
            let shared = crate::features::sync::store_video_sync_controls(store).shared_offset_seconds;
            crate::store::videos::shift_video_offsets(store, seconds - shared, false)
        }
    }
}

/// The transition panel's commit: the store's defaults with one change.
pub fn set_panel_defaults(store: &mut Store, change: impl FnOnce(&mut ovrley_core::encode::ffmpeg::transitions::TransitionDefaults)) -> Result<(), String> {
    let mut defaults = store.video_import.transition_defaults.clone();
    change(&mut defaults);
    crate::store::videos::set_transition_defaults(store, defaults)
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
        state.blur(target.options, target.value, locale, commit);
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
    use crate::i18n::DEFAULT_LOCALE;
    use crate::video::fixtures::*;
    use crate::store::videos::tests::store_with;

    fn pal() -> Option<Fps> {
        Some(Fps::new(25, 1).unwrap())
    }

    #[test]
    fn accepts_only_a_clock_or_seconds_being_typed() {
        for draft in ["", "3", "330", "330.5", ".5", "5:", "5:30", "1:01:40.5", "01:01:"] {
            assert_eq!(normalize_seconds_draft(draft, false).as_deref(), Some(draft), "{draft}");
        }
        assert_eq!(normalize_seconds_draft("5:30,5", false).as_deref(), Some("5:30.5"));
        for text in ["1:2:3:4", "5.5:30", "5garbage", "abc", "1.2.3", "-5", "1e3", " 5"] {
            assert_eq!(normalize_seconds_draft(text, false), None, "{text}");
        }
        assert_eq!(normalize_seconds_draft("-", true).as_deref(), Some("-"));
        assert_eq!(normalize_seconds_draft("-05:30", true).as_deref(), Some("-05:30"));
    }

    #[test]
    fn accepts_sums_and_differences_being_typed() {
        for draft in ["1:39:25+", "1:39:25+3:45", "1:39:25 + 3:45", "5965-225", "5965 -", "1:00 ", "1-2"] {
            assert_eq!(normalize_seconds_draft(draft, false).as_deref(), Some(draft), "{draft}");
        }
        assert_eq!(normalize_seconds_draft("1,5+2,5", false).as_deref(), Some("1.5+2.5"));
        assert_eq!(normalize_seconds_draft("-5+3", true).as_deref(), Some("-5+3"));
        for text in ["-5+3", "+5", "5*2", "5 5", "5+3:1:2:4"] {
            assert_eq!(normalize_seconds_draft(text, false), None, "{text}");
        }
        for text in ["+5", "- 5"] {
            assert_eq!(normalize_seconds_draft(text, true), None, "{text}");
        }
    }

    #[test]
    fn parses_a_complete_clock_or_plain_seconds() {
        assert_eq!(parse_seconds_draft("05:30"), Some(330.0));
        assert_eq!(parse_seconds_draft("01:01:40"), Some(3700.0));
        assert_eq!(parse_seconds_draft("5:00.04"), Some(300.04));
        assert_eq!(parse_seconds_draft("-05:30"), Some(-330.0));
        assert_eq!(parse_seconds_draft("330"), Some(330.0));
        assert_eq!(parse_seconds_draft("330."), Some(330.0));
        assert_eq!(parse_seconds_draft(".5"), Some(0.5));
        for draft in ["", "-", ".", "-.", "5:", ":30", "5:60", "1:60:00", "1::30"] {
            assert_eq!(parse_seconds_draft(draft), None, "{draft}");
        }
    }

    #[test]
    fn parses_a_sum_or_difference() {
        assert_eq!(parse_seconds_draft("1:39:25+3:45"), Some(5965.0 + 225.0));
        assert_eq!(parse_seconds_draft("1:39:25 - 3:45"), Some(5965.0 - 225.0));
        assert_eq!(parse_seconds_draft("5965-225+0.5"), Some(5740.5));
        assert_eq!(parse_seconds_draft("-1:00+30"), Some(-30.0));
        assert_eq!(parse_seconds_draft("10-30"), Some(-20.0));
        assert_eq!(parse_seconds_draft("1:00 "), Some(60.0));
        for draft in ["5+", "5-", "5+-3", "5--3", "5+5:75"] {
            assert_eq!(parse_seconds_draft(draft), None, "{draft}");
        }
    }

    #[test]
    fn formats_a_duration_with_one_decimal() {
        assert_eq!([2.0, 1.5, 1.0333, 0.04, 12.96].map(format_duration_seconds), ["2.0", "1.5", "1.0", "0.0", "13.0"]);
        assert_eq!(format_seconds_text(330.5), "330.5");
        assert_eq!(format_seconds_text(-0.0001), "0");
    }

    #[test]
    fn snaps_times_to_frames_and_durations_to_at_least_one() {
        let ntsc = Some(Fps::new(30000, 1001).unwrap());
        assert_eq!(snap_seconds(300.03, pal(), SecondsMode::Time), 300.04);
        assert_eq!(snap_seconds(0.01, pal(), SecondsMode::Time), 0.0);
        assert_eq!(snap_seconds(0.01, pal(), SecondsMode::Duration), 0.04);
        assert_eq!(snap_seconds(2.0, ntsc, SecondsMode::Duration), 60.0 * 1001.0 / 30000.0);
        assert_eq!(snap_seconds(-1.01, pal(), SecondsMode::Time), -1.0);
        assert_eq!(snap_seconds(2.345, None, SecondsMode::Time), 2.345);
    }

    #[test]
    fn the_field_commits_a_snapped_draft_once_and_shows_the_value_again() {
        let options = SecondsInputOptions { fps: pal(), mode: SecondsMode::Duration, signed: false, display: SecondsDisplay::Seconds };
        let mut state = SecondsInputState::default();
        assert_eq!(state.view(options, 2.0, DEFAULT_LOCALE).text, "2.0");
        assert!(state.change(options, "1,01"));
        assert!(!state.change(options, "abc"), "refused text keeps the draft");
        let mut committed = Vec::new();
        state.blur(options, 2.0, DEFAULT_LOCALE, |seconds| {
            committed.push(seconds);
            None
        });
        assert_eq!(committed, [1.0]);
        assert_eq!(state.draft, None);
        state.change(options, "0");
        state.key_down(options, 2.0, &KeyInput::new("enter"), DEFAULT_LOCALE, |_| panic!("a rejected value is not committed"));
        assert_eq!(state.error.as_deref(), Some("Enter a duration above 0 seconds"));
        state.key_down(options, 2.0, &KeyInput::new("escape"), DEFAULT_LOCALE, |_| None);
        assert_eq!((state.draft.clone(), state.error.clone()), (None, None));
    }

    #[test]
    fn steps_move_the_draft_and_commit_on_release() {
        let options = SecondsInputOptions { fps: pal(), mode: SecondsMode::Time, signed: false, display: SecondsDisplay::Clock };
        let mut state = SecondsInputState::default();
        assert!(state.key_down(options, 10.0, &KeyInput::new("arrowup"), DEFAULT_LOCALE, |_| None));
        assert!(state.key_down(options, 10.0, &KeyInput { key: "arrowup".into(), alt: true, ..KeyInput::default() }, DEFAULT_LOCALE, |_| None));
        assert_eq!(state.draft.as_deref(), Some("00:11.04"));
        assert_eq!(state.view(options, 10.0, DEFAULT_LOCALE).preview, None, "no preview while stepping");
        let mut committed = None;
        assert!(state.key_up(options, 10.0, "arrowup", DEFAULT_LOCALE, |seconds| {
            committed = Some(seconds);
            None
        }));
        assert_eq!(committed, Some(11.04));
        state.change(options, "5:30");
        assert_eq!(state.view(options, 10.0, DEFAULT_LOCALE).preview.as_deref(), Some("= 05:30 (330 s)"));
    }

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
