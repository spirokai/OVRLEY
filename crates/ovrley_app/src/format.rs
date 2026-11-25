//! UI formatting, the crate's only user of ICU4X: durations and clocks, dates in the app
//! language, numbers and text order in the system locale.

use std::sync::OnceLock;
use icu::collator::{Collator, CollatorBorrowed};
use icu::collator::options::CollatorOptions;
use icu::datetime::DateTimeFormatter;
use icu::datetime::fieldsets::YMD;
use icu::datetime::input::{Date, DateTime, Time};
use icu::datetime::options::YearStyle;
use icu::decimal::DecimalFormatter;
use icu::decimal::input::Decimal;
use crate::i18n::Locale;

/// A cache size in binary units: whole megabytes below one gigabyte, otherwise
/// gigabytes with one decimal ("512 MB", "1.5 GB").
pub fn format_cache_bytes(bytes: u64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * MIB;
    let bytes = bytes as f64;
    if bytes < GIB {
        format!("{} MB", (bytes / MIB).round())
    } else {
        // Rounds a tie up, to the larger neighbour.
        format!("{:.1} GB", (bytes / GIB * 10.0).round() / 10.0)
    }
}

/// A download's percentage, whole.
pub fn format_update_progress(percent: f64) -> String {
    format!("{}%", percent.round())
}

/// `MM:SS` below an hour, `H:MM:SS` from one, with a sign for negative
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

/// `mm:ss`, or `hh:mm:ss` from one hour on, with up to three decimals
/// when the value has a fraction (`330.5` → `05:30.5`).
pub fn format_seconds_clock(seconds: f64) -> String {
    let rounded = round_decimals(seconds.abs(), 3);
    let sign = if seconds < 0.0 && rounded > 0.0 { "-" } else { "" };
    let whole = rounded.floor();
    let fraction = plain_number(round_decimals(rounded - whole, 3));
    let fraction = fraction.strip_prefix('0').unwrap_or("");
    let whole = whole as u64;
    let (hours, minutes, rest) = (whole / 3600, whole % 3600 / 60, whole % 60);
    if hours > 0 {
        format!("{sign}{hours:02}:{minutes:02}:{rest:02}{fraction}")
    } else {
        format!("{sign}{minutes:02}:{rest:02}{fraction}")
    }
}

/// The system locale (`sys-locale`) that numbers and text order follow; English when the system
/// names none ICU understands.
fn system_locale() -> &'static icu::locale::Locale {
    static LOCALE: OnceLock<icu::locale::Locale> = OnceLock::new();
    LOCALE.get_or_init(|| {
        sys_locale::get_locale()
            .and_then(|name| icu::locale::Locale::try_from_str(&name.replace('_', "-")).ok())
            .unwrap_or(icu::locale::locale!("en"))
    })
}

/// A timestamp in its recording time zone, as numeric date and time with
/// seconds on a 24-hour clock, in the app language. `None` for a timestamp or zone that cannot be
/// read (activity data is external).
///
/// ICU4X has no skeleton API, so the date follows the language's short pattern with a full year
/// (English shows `7/5/2026`).
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

/// GPS times, and camera or filename clocks read as UTC, in the
/// recording timezone; other clocks as they read. `Unknown` without a time. A time that cannot be
/// read shows as it is (external data).
pub fn format_video_creation_time(timestamp: Option<&str>, source: Option<&str>, timezone: Option<&str>, timezone_mode: Option<ovrley_core::media::video_edits::TimezoneMode>, language: Locale) -> String {
    use ovrley_core::media::video_edits::TimezoneMode;
    let Some(timestamp) = timestamp.filter(|timestamp| !timestamp.is_empty()) else {
        return "Unknown".into();
    };
    let zoned = source == Some("gps") || (matches!(source, Some("ffprobe" | "filename")) && timezone_mode == Some(TimezoneMode::Utc));
    // A clock without a zone is read as UTC, as placement reads it.
    let as_utc = format!("{}Z", crate::video::sync::strip_camera_clock(timestamp));
    let formatted = match timezone.filter(|_| zoned) {
        Some(timezone) => format_zoned_date_time(timestamp, timezone, language).or_else(|| format_zoned_date_time(&as_utc, timezone, language)),
        None => format_zoned_date_time(&as_utc, "UTC", language),
    };
    formatted.unwrap_or_else(|| timestamp.to_string())
}

/// `{width}×{height} · {fps} fps · {duration}`, the rate to two decimals.
pub fn format_video_details(resolution: ovrley_core::media::video_edits::VideoResolution, fps: ovrley_core::encode::fps::Fps, duration_seconds: f64, language: Locale) -> String {
    let template = crate::i18n::catalog().t_or(language, "video-preview.videoDetails", "{{width}}×{{height}} · {{fps}} fps · {{duration}}");
    let rate = plain_number((fps.as_f64() * 100.0).round() / 100.0);
    crate::i18n::interpolate(
        template,
        &[("width", &resolution.width.to_string()), ("height", &resolution.height.to_string()), ("fps", &rate), ("duration", &format_seconds_clock(duration_seconds))],
    )
}

/// `value` in the system locale's digits and separators, with at most `maximum_fraction_digits`
/// decimals.
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

/// Orders text by the system locale's collation.
pub fn compare_text(left: &str, right: &str) -> std::cmp::Ordering {
    static COLLATOR: OnceLock<Option<CollatorBorrowed<'static>>> = OnceLock::new();
    let collator = COLLATOR.get_or_init(|| Collator::try_new(system_locale().clone().into(), CollatorOptions::default()).ok());
    match collator {
        Some(collator) => collator.compare(left, right),
        None => left.cmp(right),
    }
}

/// `m:ss` with unbounded minutes (`75:03`), `--:--` without a value. The progress
/// panel's time remaining; `format_clock_duration` shows hours, so both exist.
pub fn format_minutes_seconds(seconds: Option<f64>) -> String {
    let Some(seconds) = seconds else {
        return "--:--".into();
    };
    let rounded = seconds.round() as i64;
    format!("{}:{:02}", rounded.div_euclid(60), rounded.rem_euclid(60))
}

/// The render rate to one decimal, `--` without one.
pub fn format_render_fps(fps: Option<f64>) -> String {
    match fps.filter(|fps| fps.is_finite()) {
        Some(fps) => format!("{fps:.1}"),
        None => "--".into(),
    }
}

/// `{m} min {s} sec`, or `{s} sec` under a minute.
pub fn format_duration_summary(seconds: f64, language: Locale) -> String {
    let rounded = seconds.round() as i64;
    let (minutes, seconds) = (rounded.div_euclid(60), rounded.rem_euclid(60));
    let catalog = crate::i18n::catalog();
    if minutes > 0 {
        catalog.t_with(language, "render-video.minutesMinSecondsSec", &[("minutes", &minutes.to_string()), ("seconds", &seconds.to_string())])
    } else {
        catalog.t_with(language, "render-video.secondsSec", &[("seconds", &seconds.to_string())])
    }
}

/// `value` rounded to `digits` decimals, by its decimal text, so `1.005` keeps the `1.00` its
/// binary value is closest to.
pub fn round_decimals(value: f64, digits: usize) -> f64 {
    format!("{value:.digits$}").parse().expect("a formatted number parses")
}

/// The shortest text that reads back as `value`, with negative zero shown as `0`.
pub fn plain_number(value: f64) -> String {
    if value == 0.0 {
        return "0".into();
    }
    format!("{value}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_clocks_show_up_to_three_decimals() {
        assert_eq!(format_seconds_clock(330.5), "05:30.5");
        assert_eq!(format_cache_bytes(512 << 20), "512 MB");
        assert_eq!(format_cache_bytes(1 << 30), "1.0 GB");
        assert_eq!(format_cache_bytes(5 * (1 << 28)), "1.3 GB", "a tie rounds up");
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
        assert_eq!(compare_text("apple", "Banana"), std::cmp::Ordering::Less);
    }

    #[test]
    fn video_creation_times_follow_their_source() {
        use ovrley_core::media::video_edits::TimezoneMode;
        let en = Locale::En;
        assert_eq!(format_video_creation_time(None, None, None, None, en), "Unknown");
        // A camera clock shows as it reads, whatever its zone suffix.
        let clock = format_video_creation_time(Some("2024-06-01T10:30:00.5Z"), Some("ffprobe"), Some("Europe/Berlin"), None, en);
        assert_eq!(clock, format_zoned_date_time("2024-06-01T10:30:00Z", "UTC", en).unwrap());
        // GPS, and a clock read as UTC, in the recording zone.
        let gps = format_video_creation_time(Some("2024-06-01T08:30:00Z"), Some("gps"), Some("Europe/Berlin"), None, en);
        assert_eq!(gps, clock);
        assert_eq!(format_video_creation_time(Some("2024-06-01T08:30:00"), Some("filename"), Some("Europe/Berlin"), Some(TimezoneMode::Utc), en), clock);
    }

    #[test]
    fn video_details_round_the_rate() {
        let fps = ovrley_core::encode::fps::Fps::new(30000, 1001).unwrap();
        let resolution = ovrley_core::media::video_edits::VideoResolution { width: 1920, height: 1080 };
        assert_eq!(format_video_details(resolution, fps, 330.5, Locale::En), "1920×1080 · 29.97 fps · 05:30.5");
    }

    #[test]
    fn progress_panel_numbers() {
        assert_eq!(format_minutes_seconds(None), "--:--");
        assert_eq!(format_minutes_seconds(Some(330.0)), "5:30");
        assert_eq!(format_minutes_seconds(Some(4503.4)), "75:03");
        assert_eq!(format_minutes_seconds(Some(0.0)), "0:00");
        assert_eq!(format_render_fps(None), "--");
        assert_eq!(format_render_fps(Some(f64::NAN)), "--");
        assert_eq!(format_render_fps(Some(23.456)), "23.5");
        assert_eq!(format_duration_summary(125.4, Locale::En), "2 min 5 sec");
        assert_eq!(format_duration_summary(42.0, Locale::En), "42 sec");
    }
}
