//! The number input's rules: the characters it accepts, parsing, stepping up and down,
//! constraint validation, and the inline number field's draft.

use crate::format::plain_number;

/// The input's `step` attribute.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    /// `step="any"`: no step grid, and stepping is not allowed.
    Any,
    Value(f64),
}

const DEFAULT_STEP: f64 = 1.0;

impl Step {
    /// Parses the attribute: `any` (any case), a positive number, or the default step of 1
    /// for an absent or unusable value, as the specification requires.
    pub fn parse(attribute: &str) -> Step {
        if attribute.eq_ignore_ascii_case("any") {
            return Step::Any;
        }
        match parse_number(attribute) {
            Some(step) if step > 0.0 => Step::Value(step),
            _ => Step::Value(DEFAULT_STEP),
        }
    }
}

/// `min`, `max` and `step` of a number input. `min` and `max` are optional attributes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumberConstraints {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Step,
}

impl NumberConstraints {
    /// The step base: `min` when present, otherwise 0.
    fn step_base(&self) -> f64 {
        self.min.unwrap_or(0.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepDirection {
    Up,
    Down,
}

/// Whether text contains only characters a number input lets the user type
/// (digits, `e`, `E`, `+`, `-` and `.`, as in Chromium).
pub fn is_number_input_text(text: &str) -> bool {
    text.chars()
        .all(|c| c.is_ascii_digit() || matches!(c, 'e' | 'E' | '+' | '-' | '.'))
}

/// Parses a "valid floating-point number": an optional `-`, digits and/or a fraction, and an
/// optional exponent. Anything else (including `+1`, `1.` and the empty string) is `None`,
/// which is what the input reports as `NaN`.
pub fn parse_number(text: &str) -> Option<f64> {
    let bytes = text.as_bytes();
    let mut i = 0;
    if bytes.first() == Some(&b'-') {
        i += 1;
    }
    let int_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    let mut has_digits = i > int_start;
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        let frac_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == frac_start {
            return None;
        }
        has_digits = true;
    }
    if !has_digits {
        return None;
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        i += 1;
        if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
            i += 1;
        }
        let exp_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == exp_start {
            return None;
        }
    }
    if i != bytes.len() {
        return None;
    }
    text.parse::<f64>().ok().filter(|value| value.is_finite())
}

/// Decimal places in a number's shortest representation; stepping works in decimal arithmetic,
/// so results are rounded to the precision of their operands.
fn decimals(value: f64) -> i32 {
    let text = format!("{value}");
    text.split_once('.')
        .map_or(0, |(_, fraction)| fraction.len() as i32)
}

fn round_to(value: f64, places: i32) -> f64 {
    let factor = 10f64.powi(places);
    (value * factor).round() / factor
}

/// Whether `value` lies on the step grid that starts at `base`.
fn on_grid(value: f64, base: f64, step: f64) -> bool {
    let steps = round_to((value - base) / step, 9);
    steps.fract() == 0.0
}

/// Steps the input's text up or down, returning the text afterwards. `None` for `step="any"`,
/// where the value cannot be stepped. A step that would move the wrong way leaves the text as it
/// is.
pub fn step_value(
    text: &str,
    constraints: &NumberConstraints,
    direction: StepDirection,
) -> Option<String> {
    let Step::Value(step) = constraints.step else {
        return None;
    };
    if let (Some(min), Some(max)) = (constraints.min, constraints.max) {
        if min > max {
            return Some(text.into());
        }
    }
    let before = parse_number(text).unwrap_or(0.0);
    let base = constraints.step_base();
    let precision = decimals(step).max(decimals(base)).max(decimals(before));
    let steps = round_to((before - base) / step, 9);
    let mut value = if steps.fract() == 0.0 {
        match direction {
            StepDirection::Up => before + step,
            StepDirection::Down => before - step,
        }
    } else {
        let snapped = match direction {
            StepDirection::Up => steps.ceil(),
            StepDirection::Down => steps.floor(),
        };
        base + snapped * step
    };
    if let Some(min) = constraints.min {
        if value < min {
            value = base + round_to((min - base) / step, 9).ceil() * step;
        }
    }
    if let Some(max) = constraints.max {
        if value > max {
            value = base + round_to((max - base) / step, 9).floor() * step;
        }
    }
    let value = round_to(value, precision);
    let moved_wrong_way = match direction {
        StepDirection::Up => value < before,
        StepDirection::Down => value > before,
    };
    if moved_wrong_way {
        return Some(text.into());
    }
    Some(plain_number(value))
}

/// Validity for an input that is not `required`: an empty value is valid; otherwise
/// the text must parse (no bad input), lie within `min`/`max`, and sit on the step grid.
pub fn check_validity(text: &str, constraints: &NumberConstraints) -> bool {
    if text.is_empty() {
        return true;
    }
    let Some(value) = parse_number(text) else {
        return false;
    };
    if constraints.min.is_some_and(|min| value < min)
        || constraints.max.is_some_and(|max| value > max)
    {
        return false;
    }
    match constraints.step {
        Step::Any => true,
        Step::Value(step) => on_grid(value, constraints.step_base(), step),
    }
}

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
/// Text that does not parse is `NaN`, which never equals `value`. An empty field is valid (the
/// input is not `required`) and therefore commits `NaN`.
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

/// The leading integer of `text` (whitespace, an optional sign, then ASCII digits), or `None`
/// when it has no digits.
pub fn leading_integer(text: &str) -> Option<f64> {
    let trimmed = text.trim_start();
    let (sign, rest) = match trimmed.as_bytes().first() {
        Some(b'-') => (-1.0, &trimmed[1..]),
        Some(b'+') => (1.0, &trimmed[1..]),
        _ => (1.0, trimmed),
    };
    let digits = &rest[..rest.bytes().take_while(u8::is_ascii_digit).count()];
    if digits.is_empty() {
        return None;
    }
    digits.parse::<f64>().ok().map(|value| sign * value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constraints(min: Option<f64>, max: Option<f64>, step: &str) -> NumberConstraints {
        NumberConstraints {
            min,
            max,
            step: Step::parse(step),
        }
    }

    #[test]
    fn parses_valid_floating_point_numbers_only() {
        assert_eq!(parse_number("12"), Some(12.0));
        assert_eq!(parse_number("-0.5"), Some(-0.5));
        assert_eq!(parse_number(".5"), Some(0.5));
        assert_eq!(parse_number("1e3"), Some(1000.0));
        assert_eq!(parse_number("1E-2"), Some(0.01));
        assert_eq!(parse_number(""), None);
        assert_eq!(parse_number("1."), None);
        assert_eq!(parse_number("+1"), None);
        assert_eq!(parse_number("1e"), None);
        assert_eq!(parse_number("-"), None);
        assert_eq!(parse_number(" 1"), None);
    }

    #[test]
    fn accepts_only_number_characters() {
        assert!(is_number_input_text("-1.5e+3"));
        assert!(is_number_input_text(""));
        assert!(!is_number_input_text("12a"));
        assert!(!is_number_input_text("1 2"));
    }

    #[test]
    fn parses_the_step_attribute() {
        assert_eq!(Step::parse("any"), Step::Any);
        assert_eq!(Step::parse("ANY"), Step::Any);
        assert_eq!(Step::parse("0.1"), Step::Value(0.1));
        assert_eq!(Step::parse(""), Step::Value(1.0));
        assert_eq!(Step::parse("0"), Step::Value(1.0));
        assert_eq!(Step::parse("-2"), Step::Value(1.0));
    }

    #[test]
    fn steps_by_the_step_from_an_aligned_value() {
        let c = constraints(None, None, "");
        assert_eq!(
            step_value("30", &c, StepDirection::Up).as_deref(),
            Some("31")
        );
        assert_eq!(
            step_value("30", &c, StepDirection::Down).as_deref(),
            Some("29")
        );
        assert_eq!(step_value("", &c, StepDirection::Up).as_deref(), Some("1"));
        let decimal = constraints(None, None, "0.1");
        assert_eq!(
            step_value("0.2", &decimal, StepDirection::Up).as_deref(),
            Some("0.3")
        );
    }

    #[test]
    fn snaps_an_unaligned_value_to_the_grid_in_the_step_direction() {
        let c = constraints(Some(1.0), None, "2");
        assert_eq!(step_value("4", &c, StepDirection::Up).as_deref(), Some("5"));
        assert_eq!(
            step_value("4", &c, StepDirection::Down).as_deref(),
            Some("3")
        );
    }

    #[test]
    fn clamps_to_the_range_on_the_grid() {
        let c = constraints(Some(1.0), Some(10.0), "1");
        assert_eq!(
            step_value("10", &c, StepDirection::Up).as_deref(),
            Some("10")
        );
        assert_eq!(
            step_value("1", &c, StepDirection::Down).as_deref(),
            Some("1")
        );
        assert_eq!(
            step_value("12", &c, StepDirection::Up).as_deref(),
            Some("12")
        );
        assert_eq!(
            step_value("-5", &c, StepDirection::Up).as_deref(),
            Some("1")
        );
        let grid = constraints(Some(0.0), Some(10.0), "3");
        assert_eq!(
            step_value("50", &grid, StepDirection::Down).as_deref(),
            Some("9")
        );
    }

    #[test]
    fn does_not_step_without_a_step_grid() {
        assert_eq!(
            step_value("1", &constraints(None, None, "any"), StepDirection::Up),
            None
        );
    }

    #[test]
    fn validates_range_step_and_bad_input() {
        let c = constraints(Some(0.0), Some(100.0), "0.1");
        assert!(check_validity("", &c));
        assert!(check_validity("0.3", &c));
        assert!(check_validity("100", &c));
        assert!(!check_validity("100.1", &c));
        assert!(!check_validity("-0.1", &c));
        assert!(!check_validity("0.35", &c));
        assert!(!check_validity("1e", &c));
        assert!(check_validity(
            "0.35",
            &constraints(Some(0.0), Some(100.0), "any")
        ));
        assert!(check_validity("7", &constraints(Some(1.0), None, "2")));
        assert!(!check_validity("6", &constraints(Some(1.0), None, "2")));
    }

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
    fn an_empty_field_commits_nan() {
        assert!(matches!(blur("", 12.0, &RANGE), InlineBlur::Commit(value) if value.is_nan()));
    }
}
