//! Lap-boundary resolution from normalized activity extraction inputs.
//!
//! Explicit source lap labels are authoritative when present. Otherwise this
//! module resolves canonical boundaries from elapsed-time beacons or geographic
//! VBO timing-line crossings. Every path emits the same zero-based lap labels
//! and exact activity-relative boundary times for downstream derivation.

use crate::activity::schema::{LapMarkers, TimingMarker, TimingMarkerKind};

/// Sample-aligned canonical lap IDs and exact elapsed times of their boundaries.
pub(super) struct ResolvedLaps {
    /// Canonical zero-based lap ID at each sample, or `-1` before the first boundary.
    pub(super) lap_number: Vec<i64>,
    /// Exact activity-relative elapsed time of each observed lap boundary.
    pub(super) lap_start_elapsed: Vec<f64>,
}

/// Planar point used only for local line-segment intersection calculations.
#[derive(Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
}

/// Finite line segment shared by timing lines and consecutive route samples.
#[derive(Clone, Copy)]
struct Segment {
    start: Point,
    end: Point,
}

/// Resolves laps from the single authoritative boundary source available.
///
/// Explicit sample-aligned lap labels take precedence over marker metadata.
/// Without either source, every sample remains in the canonical out-lap.
pub(super) fn resolve_laps(
    elapsed: &[f64],
    course: &[(Option<f64>, Option<f64>)],
    lap_number_source: &[Option<i64>],
    lap_markers: &LapMarkers,
) -> ResolvedLaps {
    if lap_number_source.iter().any(Option::is_some) {
        return resolve_from_source(elapsed, lap_number_source);
    }

    match lap_markers {
        LapMarkers::BeaconMarkers(beacons) => resolve_from_beacons(elapsed, beacons),
        LapMarkers::TimingMarkers(markers) => resolve_from_timing_markers(elapsed, course, markers),
        LapMarkers::None => ResolvedLaps {
            lap_number: vec![-1; elapsed.len()],
            lap_start_elapsed: Vec::new(),
        },
    }
}

/// Canonicalizes changes in source lap labels into sequential zero-based laps.
///
/// Missing source samples retain the last resolved label. Negative labels denote
/// an out-lap while still marking a source transition boundary.
fn resolve_from_source(elapsed: &[f64], lap_number_source: &[Option<i64>]) -> ResolvedLaps {
    let mut lap_number = Vec::with_capacity(lap_number_source.len());
    let mut lap_start_elapsed = Vec::new();
    let mut current_source_lap = -1;
    let mut current_timing_lap = -1;

    for (index, source_lap) in lap_number_source.iter().copied().enumerate() {
        if let Some(source_lap) = source_lap {
            if source_lap != current_source_lap {
                lap_start_elapsed.push(elapsed[index]);
                current_timing_lap = if source_lap >= 0 {
                    lap_start_elapsed.len() as i64 - 1
                } else {
                    -1
                };
            }
            current_source_lap = source_lap;
        }
        lap_number.push(current_timing_lap);
    }

    ResolvedLaps {
        lap_number,
        lap_start_elapsed,
    }
}

/// Resolves canonical lap labels from exact elapsed-time beacon boundaries.
///
/// Beacons beyond the final activity sample are excluded because they do not
/// start an observed lap in this activity.
fn resolve_from_beacons(elapsed: &[f64], beacons: &[f64]) -> ResolvedLaps {
    if beacons.is_empty() {
        return ResolvedLaps {
            lap_number: vec![-1; elapsed.len()],
            lap_start_elapsed: Vec::new(),
        };
    }
    let mut started_lap_count = 0;
    let lap_number = elapsed
        .iter()
        .map(|sample_elapsed| {
            while beacons
                .get(started_lap_count)
                .is_some_and(|beacon| *beacon <= *sample_elapsed)
            {
                started_lap_count += 1;
            }
            started_lap_count as i64 - 1
        })
        .collect();
    let last_elapsed = elapsed.last().copied().unwrap_or(0.0);
    let lap_start_elapsed = beacons
        .iter()
        .copied()
        .take_while(|beacon| *beacon <= last_elapsed)
        .collect();
    ResolvedLaps {
        lap_number,
        lap_start_elapsed,
    }
}

/// Resolves lap boundaries where route segments cross the first start marker.
///
/// Crossing time is interpolated within the route segment to preserve sub-sample
/// precision. Missing coordinates retain the current lap without creating a
/// boundary.
fn resolve_from_timing_markers(
    elapsed: &[f64],
    course: &[(Option<f64>, Option<f64>)],
    markers: &[TimingMarker],
) -> ResolvedLaps {
    let start_markers: Vec<&TimingMarker> = markers
        .iter()
        .filter(|marker| marker.kind == TimingMarkerKind::Start)
        .collect();

    if start_markers.is_empty() || course.len() < 2 {
        return ResolvedLaps {
            lap_number: vec![-1; course.len()],
            lap_start_elapsed: Vec::new(),
        };
    }

    let marker = &start_markers[0];
    let marker_segment = Segment {
        start: Point {
            x: marker.longitude_a,
            y: marker.latitude_a,
        },
        end: Point {
            x: marker.longitude_b,
            y: marker.latitude_b,
        },
    };

    let mut lap_number = vec![-1i64; course.len()];
    let mut lap_start_elapsed = Vec::new();

    for index in 1..course.len() {
        lap_number[index] = lap_start_elapsed.len() as i64 - 1;
        let (previous_latitude, previous_longitude) = course[index - 1];
        let (current_latitude, current_longitude) = course[index];

        let Some(previous_latitude) = previous_latitude else {
            continue;
        };
        let Some(previous_longitude) = previous_longitude else {
            continue;
        };
        let Some(current_latitude) = current_latitude else {
            continue;
        };
        let Some(current_longitude) = current_longitude else {
            continue;
        };

        let route_segment = Segment {
            start: Point {
                x: previous_longitude,
                y: previous_latitude,
            },
            end: Point {
                x: current_longitude,
                y: current_latitude,
            },
        };
        if let Some(fraction) = intersection_fraction(route_segment, marker_segment) {
            let crossing_elapsed =
                elapsed[index - 1] + fraction * (elapsed[index] - elapsed[index - 1]);
            lap_start_elapsed.push(crossing_elapsed);
            lap_number[index] = lap_start_elapsed.len() as i64 - 1;
        }
    }

    ResolvedLaps {
        lap_number,
        lap_start_elapsed,
    }
}

/// Returns the route-segment fraction where two finite segments intersect.
///
/// The route segment's start is excluded so a crossing at a shared route vertex
/// is counted by only the preceding segment. Parallel and non-intersecting
/// segments return `None`.
fn intersection_fraction(route: Segment, marker: Segment) -> Option<f64> {
    /// Computes the two-dimensional scalar cross product.
    fn cross(left: Point, right: Point) -> f64 {
        left.x * right.y - left.y * right.x
    }

    let route_vector = Point {
        x: route.end.x - route.start.x,
        y: route.end.y - route.start.y,
    };
    let marker_vector = Point {
        x: marker.end.x - marker.start.x,
        y: marker.end.y - marker.start.y,
    };
    let offset = Point {
        x: marker.start.x - route.start.x,
        y: marker.start.y - route.start.y,
    };
    let denominator = cross(route_vector, marker_vector);
    if denominator.abs() <= f64::EPSILON {
        return None;
    }
    let route_fraction = cross(offset, marker_vector) / denominator;
    let marker_fraction = cross(offset, route_vector) / denominator;
    ((0.0..=1.0).contains(&route_fraction)
        && route_fraction > f64::EPSILON
        && (0.0..=1.0).contains(&marker_fraction))
    .then_some(route_fraction)
}

#[cfg(test)]
mod tests {
    use super::{intersection_fraction, Point, Segment};

    /// Verifies crossing detection accepts an intersection and rejects disjoint segments.
    #[test]
    fn line_crossing_detection() {
        let diagonal = Segment {
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 1.0, y: 1.0 },
        };
        assert!(intersection_fraction(
            diagonal,
            Segment {
                start: Point { x: 1.0, y: 0.0 },
                end: Point { x: 0.0, y: 1.0 },
            }
        )
        .is_some());
        assert!(intersection_fraction(
            diagonal,
            Segment {
                start: Point { x: 2.0, y: 2.0 },
                end: Point { x: 3.0, y: 3.0 },
            }
        )
        .is_none());
    }
}
