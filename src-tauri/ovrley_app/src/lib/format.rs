//! The one home of UI formatting (D20), and the only user of ICU4X. Later phases add their
//! formatters here instead of writing their own.
//!
//! - Durations: `formatClockDuration` and `formatSecondsClock` (`lib/time-format.js`).
//!   `timeToSeconds` has no caller outside its tests and is not ported.
//! - Dates: `formatZonedDateTime`, in the app language.
//! - Numbers: every `toLocaleString()` call, in the system locale, as the webview formats them.
//! - Text order: `localeCompare`, in the system locale.

use crate::i18n::Locale;
use icu::collator::{options::CollatorOptions, Collator, CollatorBorrowed};
use icu::datetime::fieldsets::YMD;
use icu::datetime::input::{Date, DateTime, Time};
use icu::datetime::options::YearStyle;
use icu::datetime::DateTimeFormatter;
use icu::decimal::input::Decimal;
use icu::decimal::DecimalFormatter;
use std::sync::OnceLock;

/// `formatClockDuration`: `MM:SS` below an hour, `H:MM:SS` from one, with a sign for negative
/// durations. The time widget's `format_elapsed_time` differs (always `H:MM:SS`), so both exist.
pub fn format_clock_duration(seconds: f64) -> String {
    let sign = if seconds < 0.0 { "-" } else { "" };
    let whole = seconds.abs().floor() as u64;
    let (hours, minutes, rest) = (whole / 3600, whole % 3600 / 60, whole % 60);
    if hours > 0 {
        format!("{sign}{hours}:{minutes:02}:{rest:02}")
    } else {
        format!("{sign}{minutes:02}:{rest:02}")
    }
}

/// `formatSecondsClock`: `mm:ss`, or `hh:mm:ss` from one hour on, with up to three decimals
/// when the value has a fraction (`330.5` → `05:30.5`).
pub fn format_seconds_clock(seconds: f64) -> String {
    let rounded = crate::js::to_fixed(seconds.abs(), 3);
    let sign = if seconds < 0.0 && rounded > 0.0 { "-" } else { "" };
    let whole = rounded.floor();
    let fraction = crate::js::number_to_string(crate::js::to_fixed(rounded - whole, 3));
    let fraction = fraction.strip_prefix('0').unwrap_or("");
    let whole = whole as u64;
    let (hours, minutes, rest) = (whole / 3600, whole % 3600 / 60, whole % 60);
    if hours > 0 {
        format!("{sign}{hours:02}:{minutes:02}:{rest:02}{fraction}")
    } else {
        format!("{sign}{minutes:02}:{rest:02}{fraction}")
    }
}

/// The system locale (`sys-locale`), as the webview resolves `toLocaleString()`; English when the
/// system names none ICU understands.
fn system_locale() -> &'static icu::locale::Locale {
    static LOCALE: OnceLock<icu::locale::Locale> = OnceLock::new();
    LOCALE.get_or_init(|| {
        sys_locale::get_locale()
            .and_then(|name| icu::locale::Locale::try_from_str(&name.replace('_', "-")).ok())
            .unwrap_or(icu::locale::locale!("en"))
    })
}

/// `formatZonedDateTime`: a timestamp in its recording time zone, as numeric date and time with
/// seconds on a 24-hour clock, in the app language. `None` for a timestamp or zone that cannot be
/// read (activity data is external).
///
/// Known difference: ICU4X has no skeleton API, so the date follows the language's short pattern
/// with a full year; English shows `7/5/2026` where `Intl` shows `07/05/2026`.
pub fn format_zoned_date_time(timestamp: &str, timezone: &str, language: Locale) -> Option<String> {
    use chrono::{Datelike, Timelike};
    let zone: chrono_tz::Tz = timezone.parse().ok()?;
    let instant = chrono::DateTime::parse_from_rfc3339(timestamp).ok()?.with_timezone(&zone);
    let locale = icu::locale::Locale::try_from_str(&format!("{}-u-hc-h23", language.code())).ok()?;
    let formatter = DateTimeFormatter::try_new(locale.into(), YMD::short().with_year_style(YearStyle::Full).with_time_hms()).ok()?;
    let datetime = DateTime {
        date: Date::try_new_iso(instant.year(), instant.month() as u8, instant.day() as u8).ok()?,
        time: Time::try_new(instant.hour() as u8, instant.minute() as u8, instant.second() as u8, 0).ok()?,
    };
    Some(formatter.format(&datetime).to_string())
}

/// `value.toLocaleString(undefined, {maximumFractionDigits})` in the system locale; the default
/// maximum is 3, as `toLocaleString()`'s.
pub fn format_locale_number(value: f64, maximum_fraction_digits: usize) -> String {
    format_number_in(system_locale(), value, maximum_fraction_digits)
}

fn format_number_in(locale: &icu::locale::Locale, value: f64, maximum_fraction_digits: usize) -> String {
    if !value.is_finite() {
        return value.to_string();
    }
    let text = format!("{value:.maximum_fraction_digits$}");
    let text = if text.contains('.') { text.trim_end_matches('0').trim_end_matches('.').to_string() } else { text };
    let text = if text == "-0" { "0".to_string() } else { text };
    let Ok(decimal) = Decimal::try_from_str(&text) else { return text };
    match DecimalFormatter::try_new(locale.clone().into(), Default::default()) {
        Ok(formatter) => formatter.format(&decimal).to_string(),
        Err(_) => text,
    }
}

/// `left.localeCompare(right)` in the system locale.
pub fn compare_text(left: &str, right: &str, _language: Locale) -> std::cmp::Ordering {
    static COLLATOR: OnceLock<Option<CollatorBorrowed<'static>>> = OnceLock::new();
    let collator = COLLATOR.get_or_init(|| Collator::try_new(system_locale().clone().into(), CollatorOptions::default()).ok());
    match collator {
        Some(collator) => collator.compare(left, right),
        None => left.cmp(right),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_clocks_show_up_to_three_decimals() {
        assert_eq!(format_seconds_clock(330.5), "05:30.5");
        assert_eq!(format_seconds_clock(3700.0), "01:01:40");
        assert_eq!(format_seconds_clock(1.5), "00:01.5");
        assert_eq!(format_seconds_clock(-330.0), "-05:30");
        assert_eq!(format_seconds_clock(-0.0001), "00:00", "a value that rounds to zero has no sign");
        assert_eq!(format_seconds_clock(59.9996), "01:00");
        assert_eq!(format_seconds_clock(1.0333333), "00:01.033");
    }

    #[test]
    fn clock_durations_switch_format_at_one_hour() {
        assert_eq!(format_clock_duration(65.0), "01:05");
        assert_eq!(format_clock_duration(-65.0), "-01:05");
        assert_eq!(format_clock_duration(3599.9), "59:59");
        assert_eq!(format_clock_duration(3600.0), "1:00:00");
        assert_eq!(format_clock_duration(3661.0), "1:01:01");
    }

    #[test]
    fn dates_follow_the_recording_zone_and_the_app_language() {
        let german = format_zoned_date_time("2026-07-15T12:03:05Z", "Europe/Berlin", Locale::De).unwrap();
        assert_eq!(german, "15.07.2026, 14:03:05");
        let english = format_zoned_date_time("2026-07-15T12:03:05.000Z", "America/New_York", Locale::En).unwrap();
        assert!(english.contains("2026") && english.contains("08:03:05"), "{english}");
        assert!(format_zoned_date_time("nope", "Europe/Berlin", Locale::En).is_none());
        assert!(format_zoned_date_time("2026-07-15T12:03:05Z", "Mars/Base", Locale::En).is_none());
    }

    #[test]
    fn numbers_group_and_round_in_a_locale() {
        let english = icu::locale::locale!("en-US");
        let german = icu::locale::locale!("de-DE");
        assert_eq!(format_number_in(&english, 1234567.0, 3), "1,234,567");
        assert_eq!(format_number_in(&english, 12.3456, 2), "12.35");
        assert_eq!(format_number_in(&german, 1234.5, 2), "1.234,5");
        assert_eq!(format_number_in(&english, -0.0001, 2), "0");
    }

    #[test]
    fn text_compares_by_collation() {
        assert_eq!(compare_text("apple", "Banana", Locale::En), std::cmp::Ordering::Less);
    }
}
