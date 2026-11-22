//! Port of `lib/utils.js`: `clamp` and `isMacOS`. `cn` has no equivalent (Phase 1); `pickDefined`
//! arrives with its first user in Phase 3.

/// `clamp(value, min, max)`: `Math.min(Math.max(value, min), max)`. Unlike `f64::clamp` it does
/// not panic when `min > max`; `max` wins, as in JavaScript.
pub fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.max(min).min(max)
}

/// The version label `AppHeader` shows: `VITE_OVRLEY_VERSION`, which CI sets, read at compile time,
/// with React's fallback, `0.00.0`, and a `v` prefix.
pub fn app_version() -> String {
    let raw = option_env!("VITE_OVRLEY_VERSION").map(str::trim).filter(|version| !version.is_empty()).unwrap_or("0.00.0");
    if raw.starts_with('v') {
        raw.to_string()
    } else {
        format!("v{raw}")
    }
}

/// `isMacOS()`, decided at compile time.
pub const IS_MACOS: bool = cfg!(target_os = "macos");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_matches_javascript() {
        assert_eq!(clamp(5.0, 0.0, 3.0), 3.0);
        assert_eq!(clamp(-1.0, 0.0, 3.0), 0.0);
        assert_eq!(clamp(2.0, 3.0, 1.0), 1.0);
    }
}
