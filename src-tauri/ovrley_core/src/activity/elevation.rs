//! Canonical elevation-source selection shared by derivation and rendering.

/// Selects barometric altitude when the source provides it, otherwise generic elevation.
pub(crate) fn preferred_elevation_series<'a>(
    barometric_altitude: &'a [Option<f64>],
    elevation: &'a [Option<f64>],
) -> &'a [Option<f64>] {
    if barometric_altitude.iter().any(Option::is_some) {
        barometric_altitude
    } else {
        elevation
    }
}

/// Selects the finalized profile samples when available, otherwise raw elevation.
pub(crate) fn elevation_profile_series<'a>(
    sample_elevations: &'a [Option<f64>],
    elevation: &'a [Option<f64>],
) -> &'a [Option<f64>] {
    if sample_elevations.is_empty() {
        elevation
    } else {
        sample_elevations
    }
}
