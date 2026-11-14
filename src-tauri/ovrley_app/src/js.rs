//! JavaScript number semantics the ported helpers rely on.

/// `Math.round`: rounds half toward positive infinity (`-0.5` → `-0`, `2.5` → `3`).
pub fn round(value: f64) -> f64 {
    (value + 0.5).floor()
}

/// `Number.parseInt(text, 10)`: leading whitespace, an optional sign, then the longest run of
/// ASCII digits. Returns `None` where JavaScript returns `NaN`.
pub fn parse_int(text: &str) -> Option<f64> {
    let trimmed = text.trim_start();
    let (sign, rest) = match trimmed.as_bytes().first() {
        Some(b'-') => (-1.0, &trimmed[1..]),
        Some(b'+') => (1.0, &trimmed[1..]),
        _ => (1.0, trimmed),
    };
    let digits: &str = &rest[..rest.bytes().take_while(u8::is_ascii_digit).count()];
    if digits.is_empty() {
        return None;
    }
    digits.parse::<f64>().ok().map(|value| sign * value)
}

/// `Number.parseFloat(text)` restricted to what the colour regexes let through (`[0-9.]+`):
/// the longest prefix of digits with at most one decimal point.
pub fn parse_float_prefix(text: &str) -> Option<f64> {
    let mut seen_point = false;
    let end = text
        .char_indices()
        .find(|&(_, c)| {
            if c == '.' && !seen_point {
                seen_point = true;
                false
            } else {
                !c.is_ascii_digit()
            }
        })
        .map_or(text.len(), |(index, _)| index);
    let prefix = &text[..end];
    if !prefix.bytes().any(|b| b.is_ascii_digit()) {
        return None;
    }
    prefix
        .trim_end_matches('.')
        .parse::<f64>()
        .ok()
        .or_else(|| format!("0{prefix}").parse().ok())
}

/// `String(number)` for the finite values the components display.
pub fn number_to_string(value: f64) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value == 0.0 {
        return "0".into();
    }
    format!("{value}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_matches_math_round() {
        assert_eq!(round(2.5), 3.0);
        assert_eq!(round(-2.5), -2.0);
        assert_eq!(round(-0.4), 0.0);
        assert_eq!(round(1.49), 1.0);
    }

    #[test]
    fn parse_int_takes_the_leading_digits() {
        assert_eq!(parse_int("12abc"), Some(12.0));
        assert_eq!(parse_int("  -7"), Some(-7.0));
        assert_eq!(parse_int("abc"), None);
        assert_eq!(parse_int(""), None);
        assert_eq!(parse_int("-"), None);
    }

    #[test]
    fn parse_float_prefix_stops_at_the_second_point() {
        assert_eq!(parse_float_prefix("0.5"), Some(0.5));
        assert_eq!(parse_float_prefix("1.2.3"), Some(1.2));
        assert_eq!(parse_float_prefix(".5"), Some(0.5));
        assert_eq!(parse_float_prefix("1."), Some(1.0));
        assert_eq!(parse_float_prefix("."), None);
    }

    #[test]
    fn number_to_string_matches_string_conversion() {
        assert_eq!(number_to_string(30.0), "30");
        assert_eq!(number_to_string(-0.0), "0");
        assert_eq!(number_to_string(0.25), "0.25");
        assert_eq!(number_to_string(0.1 + 0.2), "0.30000000000000004");
    }
}
