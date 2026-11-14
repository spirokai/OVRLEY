//! Draft rules of `InlineNumberInput` (port of `app/src/hooks/useInlineNumberInput.js`).

use crate::lib::number_input::{check_validity, parse_number, NumberConstraints};

/// What leaving the field does with the draft.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InlineBlur {
    /// The number equals the displayed value: drop the draft without committing. An untouched
    /// value may sit between steps after a canvas resize, so it is not validated.
    Unchanged,
    /// A changed, valid number: drop the draft and commit it.
    Commit(f64),
    /// A changed, invalid number: keep the draft and show the invalid state.
    Invalid,
}

/// The blur decision for the field's text against the displayed `value`.
///
/// The number is the input's `valueAsNumber`, so text that does not parse is `NaN`, which never
/// equals `value`. Like the React hook, an empty field is valid (the input is not `required`)
/// and therefore commits `NaN`.
pub fn blur(text: &str, value: f64, constraints: &NumberConstraints) -> InlineBlur {
    let next = parse_number(text).unwrap_or(f64::NAN);
    if next == value {
        InlineBlur::Unchanged
    } else if check_validity(text, constraints) {
        InlineBlur::Commit(next)
    } else {
        InlineBlur::Invalid
    }
}

/// What a key press does while the field is focused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineKeyAction {
    /// Escape: put the displayed value back in the field, then blur.
    RestoreAndBlur,
    /// Enter: blur, which runs the blur decision.
    Blur,
    /// Every other key edits the draft.
    Edit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineKey {
    Escape,
    Enter,
    Other,
}

pub fn key_action(key: InlineKey) -> InlineKeyAction {
    match key {
        InlineKey::Escape => InlineKeyAction::RestoreAndBlur,
        InlineKey::Enter => InlineKeyAction::Blur,
        InlineKey::Other => InlineKeyAction::Edit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lib::number_input::Step;

    const RANGE: NumberConstraints = NumberConstraints {
        min: Some(0.0),
        max: Some(100.0),
        step: Step::Value(1.0),
    };

    #[test]
    fn an_unchanged_value_does_nothing_even_off_the_step_grid() {
        assert_eq!(blur("12.5", 12.5, &RANGE), InlineBlur::Unchanged);
        assert_eq!(blur("150", 150.0, &RANGE), InlineBlur::Unchanged);
        assert_eq!(blur("1e1", 10.0, &RANGE), InlineBlur::Unchanged);
    }

    #[test]
    fn a_changed_valid_value_commits() {
        assert_eq!(blur("40", 12.0, &RANGE), InlineBlur::Commit(40.0));
    }

    #[test]
    fn a_changed_invalid_value_keeps_the_draft() {
        assert_eq!(blur("101", 12.0, &RANGE), InlineBlur::Invalid);
        assert_eq!(blur("12.5", 12.0, &RANGE), InlineBlur::Invalid);
        assert_eq!(blur("1e", 12.0, &RANGE), InlineBlur::Invalid);
    }

    #[test]
    fn an_empty_field_commits_nan_like_the_react_hook() {
        assert!(matches!(blur("", 12.0, &RANGE), InlineBlur::Commit(value) if value.is_nan()));
    }

    #[test]
    fn escape_restores_and_enter_blurs() {
        assert_eq!(
            key_action(InlineKey::Escape),
            InlineKeyAction::RestoreAndBlur
        );
        assert_eq!(key_action(InlineKey::Enter), InlineKeyAction::Blur);
        assert_eq!(key_action(InlineKey::Other), InlineKeyAction::Edit);
    }
}
