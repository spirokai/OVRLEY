//! Lenient readers for sensor values in external activity files.
//!
//! The rules follow ECMAScript `Number()`, `String(number)`, and `trim()` so GPX
//! imports keep the values established by the WebView parser they replaced:
//! whitespace-only text reads as zero and `0x`/`0o`/`0b` literals are numbers.

/// Whitespace as defined by ECMAScript. `str::trim` differs: it keeps U+FEFF and strips U+0085.
pub fn is_js_whitespace(character: char) -> bool {
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

/// Removes leading and trailing ECMAScript whitespace.
pub fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_whitespace)
}

/// Reads a finite number, or `None` for empty, malformed, and non-finite text.
/// Negative zero reads as zero because raw activity values are never signed zeros.
pub fn safe_number(text: &str) -> Option<f64> {
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
pub fn safe_gear_value(text: &str) -> Option<String> {
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
pub fn js_number_to_string(value: f64) -> String {
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
    let all_digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
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
