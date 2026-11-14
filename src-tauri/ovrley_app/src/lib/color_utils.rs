//! Colour normalization (port of part of `app/src/lib/color-utils.js`).

fn is_hex(text: &str, len: usize) -> bool {
    text.len() == len && text.bytes().all(|b| b.is_ascii_hexdigit())
}

fn expand_short(digits: &str) -> String {
    digits
        .chars()
        .flat_map(|c| [c, c])
        .collect::<String>()
        .to_lowercase()
}

/// Normalizes a hex colour to `#rrggbb`: 3, 4, 6 and 8 digits (with or without `#`) are
/// accepted, any alpha is dropped, and anything else yields `fallback`.
pub fn normalize_hex_color(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    let digits = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if is_hex(digits, 3) {
        format!("#{}", expand_short(digits))
    } else if is_hex(digits, 4) {
        format!("#{}", expand_short(&digits[..3]))
    } else if is_hex(digits, 6) {
        format!("#{}", digits.to_lowercase())
    } else if is_hex(digits, 8) {
        format!("#{}", digits[..6].to_lowercase())
    } else {
        fallback.into()
    }
}

/// Whether a template field key holds a colour.
pub fn is_color_field_key(key: &str) -> bool {
    key == "color" || key.ends_with("_color") || key.starts_with("color_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_every_hex_length() {
        assert_eq!(normalize_hex_color("#ABC", "#000000"), "#aabbcc");
        assert_eq!(normalize_hex_color("abcd", "#000000"), "#aabbcc");
        assert_eq!(normalize_hex_color(" #A0B1C2 ", "#000000"), "#a0b1c2");
        assert_eq!(normalize_hex_color("#A0B1C2FF", "#000000"), "#a0b1c2");
    }

    #[test]
    fn falls_back_on_anything_else() {
        assert_eq!(normalize_hex_color("", "#000000"), "#000000");
        assert_eq!(normalize_hex_color("#12345", "#ffffff"), "#ffffff");
        assert_eq!(normalize_hex_color("red", "#123456"), "#123456");
        assert_eq!(normalize_hex_color("##abc", "#000000"), "#000000");
    }

    #[test]
    fn recognizes_color_field_keys() {
        assert!(is_color_field_key("color"));
        assert!(is_color_field_key("text_color"));
        assert!(is_color_field_key("color_primary"));
        assert!(!is_color_field_key("colorful"));
        assert!(!is_color_field_key("background"));
    }
}
