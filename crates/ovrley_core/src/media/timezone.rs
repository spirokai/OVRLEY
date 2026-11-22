//! Timezone lookup for GPS coordinates.
//!
//! The finder is initialized once because loading the bundled timezone
//! boundaries is relatively expensive. Coordinates are passed to `tzf-rs` in
//! longitude/latitude order.

use std::sync::OnceLock;

use tzf_rs::DefaultFinder;

static FINDER: OnceLock<DefaultFinder> = OnceLock::new();

/// Finds the timezone containing one validated GPS coordinate.
pub fn timezone_for_coordinates(longitude: f64, latitude: f64) -> Option<String> {
    if !longitude.is_finite()
        || !latitude.is_finite()
        || !(-180.0..=180.0).contains(&longitude)
        || !(-90.0..=90.0).contains(&latitude)
        || (longitude == 0.0 && latitude == 0.0)
    {
        return None;
    }
    let timezone = FINDER
        .get_or_init(DefaultFinder::new)
        .get_tz_name(longitude, latitude);

    (!timezone.is_empty()).then(|| timezone.to_string())
}

#[cfg(test)]
mod tests {
    use super::timezone_for_coordinates;

    #[test]
    fn finds_timezone_from_coordinates() {
        assert_eq!(
            timezone_for_coordinates(14.421_254, 50.087_465).as_deref(),
            Some("Europe/Prague")
        );
    }

    #[test]
    fn returns_no_timezone_for_unmapped_coordinates() {
        assert_eq!(timezone_for_coordinates(0.0, 0.0), None);
    }
}
