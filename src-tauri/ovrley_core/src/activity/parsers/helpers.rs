//! Helpers shared by the activity format parsers: lenient readers for sensor values in
//! external files, and element lookups for the XML formats.
//!
//! The `js_*` and `safe_*` rules follow ECMAScript `Number()`, `String(number)`, and
//! `trim()` so imports keep the values established by the WebView parsers they replaced:
//! whitespace-only text reads as zero and `0x`/`0o`/`0b` literals are numbers.

use crate::error::{CoreError, CoreResult};
use roxmltree::{Document, Node};

pub(super) const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

/// Whitespace as defined by ECMAScript. `str::trim` differs: it keeps U+FEFF and strips U+0085.
pub(super) fn is_js_whitespace(character: char) -> bool {
    matches!(
        character,
        '\t' | '\n' | '\u{B}' | '\u{C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// Returns whether every byte is an ASCII digit, which empty input satisfies.
pub(super) fn all_digits(text: impl AsRef<[u8]>) -> bool {
    text.as_ref().iter().all(u8::is_ascii_digit)
}

/// Reads a Rust float literal for formats without the WebView parsers' `Number()` rules.
pub(super) fn finite_number(text: &str) -> Option<f64> {
    text.parse().ok().filter(|value: &f64| value.is_finite())
}

/// Removes leading and trailing ECMAScript whitespace.
pub(super) fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_whitespace)
}

/// Reads a finite number, or `None` for empty, malformed, and non-finite text.
/// Negative zero reads as zero because raw activity values are never signed zeros.
pub(super) fn safe_number(text: &str) -> Option<f64> {
    if text.is_empty() {
        return None;
    }
    let literal = js_trim(text);
    if literal.is_empty() {
        return Some(0.0);
    }
    let value = match radix_literal(literal) {
        Some((digits, bits_per_digit)) => parse_radix_digits(digits, bits_per_digit)?,
        None if is_decimal_literal(literal) => literal.parse().ok()?,
        None => return None,
    };
    value
        .is_finite()
        .then_some(if value == 0.0 { 0.0 } else { value })
}

/// Reads a gear label: numeric text takes its canonical number form, other text is kept trimmed.
pub(super) fn safe_gear_value(text: &str) -> Option<String> {
    let label = js_trim(text);
    if label.is_empty() || matches!(label.to_lowercase().as_str(), "n/a" | "na" | "null") {
        return None;
    }
    if is_decimal_literal(label) {
        if let Some(value) = label.parse::<f64>().ok().filter(|value| value.is_finite()) {
            return Some(js_number_to_string(value));
        }
    }
    Some(label.to_string())
}

/// Formats a finite number as ECMAScript `String(number)` does.
pub(super) fn js_number_to_string(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    let sign = if value < 0.0 { "-" } else { "" };
    let scientific = format!("{:e}", value.abs());
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("LowerExp output contains an exponent");
    let digits = mantissa.replace('.', "");
    let exponent: i32 = exponent
        .parse()
        .expect("LowerExp output has an integer exponent");
    let digit_count = digits.len() as i32;
    let point = exponent + 1;
    let unsigned = if digit_count <= point && point <= 21 {
        format!("{digits}{}", "0".repeat((point - digit_count) as usize))
    } else if 0 < point && point <= 21 {
        let (whole, fraction) = digits.split_at(point as usize);
        format!("{whole}.{fraction}")
    } else if -6 < point && point <= 0 {
        format!("0.{}{digits}", "0".repeat(-point as usize))
    } else {
        let exponent_sign = if exponent < 0 { '-' } else { '+' };
        let magnitude = exponent.abs();
        let (first, rest) = digits.split_at(1);
        if rest.is_empty() {
            format!("{first}e{exponent_sign}{magnitude}")
        } else {
            format!("{first}.{rest}e{exponent_sign}{magnitude}")
        }
    };
    format!("{sign}{unsigned}")
}

fn is_decimal_literal(text: &str) -> bool {
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    let (mantissa, exponent) = match unsigned.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, Some(exponent)),
        None => (unsigned, None),
    };
    let exponent_valid = exponent.is_none_or(|exponent| {
        let digits = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        !digits.is_empty() && all_digits(digits)
    });
    let mantissa_valid = match mantissa.split_once('.') {
        Some((whole, fraction)) => {
            !(whole.is_empty() && fraction.is_empty()) && all_digits(whole) && all_digits(fraction)
        }
        None => !mantissa.is_empty() && all_digits(mantissa),
    };
    mantissa_valid && exponent_valid
}

fn radix_literal(text: &str) -> Option<(&str, u32)> {
    let prefix = text.get(..2)?;
    let bits_per_digit = match prefix {
        "0x" | "0X" => 4,
        "0o" | "0O" => 3,
        "0b" | "0B" => 1,
        _ => return None,
    };
    Some((&text[2..], bits_per_digit))
}

/// Rounds an arbitrarily long power-of-two literal to the nearest double, ties to even.
fn parse_radix_digits(digits: &str, bits_per_digit: u32) -> Option<f64> {
    const ROUNDING_BIT: u64 = 1 << 53;
    if digits.is_empty() {
        return None;
    }
    let mut significand = 0_u64;
    let mut exponent = 0_i32;
    let mut discarded_nonzero = false;
    for character in digits.chars() {
        let digit = character.to_digit(1 << bits_per_digit)?;
        for shift in (0..bits_per_digit).rev() {
            let bit = u64::from((digit >> shift) & 1);
            if significand < ROUNDING_BIT {
                significand = (significand << 1) | bit;
            } else {
                exponent += 1;
                discarded_nonzero |= bit == 1;
            }
        }
    }
    if significand >= ROUNDING_BIT {
        let round_up = significand & 1 == 1;
        significand >>= 1;
        exponent += 1;
        if round_up && (discarded_nonzero || significand & 1 == 1) {
            significand += 1;
        }
    }
    Some(significand as f64 * 2_f64.powi(exponent))
}

/// Slices by UTF-16 column, with negative indices counting from the end, as ECMAScript does.
pub(super) fn js_slice(text: &str, start: i64, end: i64) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    let resolve = |index: i64| {
        let length = units.len() as i64;
        let column = if index < 0 { length + index } else { index };
        column.clamp(0, length) as usize
    };
    let (start, end) = (resolve(start), resolve(end));
    if start < end {
        String::from_utf16_lossy(&units[start..end])
    } else {
        String::new()
    }
}

/// Reads the leading integer of a short column field, as ECMAScript `parseInt` does.
pub(super) fn js_parse_int(text: &str) -> Option<i64> {
    let text = text.trim_start_matches(is_js_whitespace);
    let (negative, unsigned) = match text.strip_prefix('-') {
        Some(unsigned) => (true, unsigned),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let end = unsigned
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(unsigned.len());
    let value: i64 = unsigned[..end].parse().ok()?;
    Some(if negative { -value } else { value })
}

/// Parses a document, naming the format in the error.
pub(super) fn parse_document<'input>(
    text: &'input str,
    format: &str,
) -> CoreResult<Document<'input>> {
    Document::parse(text).map_err(|error| {
        CoreError::Activity(format!("The {format} file could not be parsed: {error}"))
    })
}

pub(super) fn children<'a, 'input>(
    node: Node<'a, 'input>,
    name: &'a str,
    namespace: Option<&'a str>,
) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(move |child| {
        child.is_element()
            && child.tag_name().name() == name
            && child.tag_name().namespace() == namespace
    })
}

/// Returns the first matching child's trimmed text, when it has any.
pub(super) fn child_text<'a>(
    node: Node<'a, '_>,
    name: &'a str,
    namespace: Option<&'a str>,
) -> Option<&'a str> {
    children(node, name, namespace)
        .next()?
        .text()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}
