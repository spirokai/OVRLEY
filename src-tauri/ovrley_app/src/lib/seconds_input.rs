//! Port of `lib/seconds-input.js` and `hooks/useSecondsInput.js`: what a seconds input accepts,
//! how a draft parses, how a value shows, how it snaps to a frame grid, and the field's state.
//!
//! A draft is a sum of terms (`1:39:25+3:45`, `5965-225`). Each term is a clock (`5:30`,
//! `1:01:40.5`) or plain seconds (`330`): digits, up to two `:` and one `.` in the last part.
//! Signed values can start with a `-`. Spaces are allowed around `+` and `-`, and a comma reads as
//! the decimal point.

use crate::i18n::{catalog, interpolate, Locale};
use crate::js::{number_to_string, to_fixed};
use crate::lib::format::format_seconds_clock;
use crate::lib::keyboard_shortcuts::{match_keyboard_shortcut, Binding, KeyInput, Scope};
use ovrley_core::encode::fps::Fps;
use regex::Regex;
use std::sync::LazyLock;

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

/// `normalizeSecondsDraft`: typed text as a draft, or `None` when it holds anything but a time
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

/// `parseSecondsDraft`: a draft holding a complete sum of times. After a `:`, minutes and seconds
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

/// `formatSecondsText`: plain seconds, up to three decimals, without trailing zeros.
pub fn format_seconds_text(seconds: f64) -> String {
    let value = to_fixed(seconds, SHOWN_DECIMALS);
    number_to_string(if value == 0.0 { 0.0 } else { value })
}

/// `formatDurationSeconds`: exactly one decimal (`1.0`), as transition durations show.
pub fn format_duration_seconds(seconds: f64) -> String {
    format!("{seconds:.1}")
}

/// `bindingStepSeconds`: the seconds a `secondsInput.step` binding moves a value; `None` for a
/// frame step on a value without a frame grid.
pub fn binding_step_seconds(binding: &Binding, fps: Option<Fps>) -> Option<f64> {
    match binding.frames {
        None => binding.seconds,
        Some(frames) => fps.map(|fps| fps.frames_to_seconds(frames.into())),
    }
}

/// `snapSeconds`: a time to the nearest frame, a duration to at least one whole frame.
pub fn snap_seconds(seconds: f64, fps: Option<Fps>, mode: SecondsMode) -> f64 {
    match (fps, mode) {
        (None, _) => seconds,
        (Some(fps), SecondsMode::Duration) => fps.snap_duration_to_frames(seconds),
        (Some(fps), SecondsMode::Time) => fps.frames_to_seconds(fps.seconds_to_frames(seconds)),
    }
}

/// The fixed options of one field (`useSecondsInput`'s options without the callbacks).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SecondsInputOptions {
    pub fps: Option<Fps>,
    pub mode: SecondsMode,
    pub signed: bool,
    pub display: SecondsDisplay,
}

/// `useSecondsInput`'s state: the typed draft until it is committed (Enter or leaving the field),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::DEFAULT_LOCALE;

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
}
