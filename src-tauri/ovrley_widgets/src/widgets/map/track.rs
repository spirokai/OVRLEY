//! The activity's GPS track for one window: Mercator points and travelled distance. Built once, so
//! every frame is a pure function of the activity second.
use crate::widgets::route::sampling::TrackSample;
use ovrley_map::mercator::lon_lat_to_world;

const EARTH_RADIUS_METERS: f64 = 6_371_000.0;
/// GPS wanders while stationary. Movement under this adds no distance.
const MIN_MOVE_METERS: f64 = 3.0;
/// How far along the route the bearing aims, as the Video Sync navigation map does.
const BEARING_LOOKAHEAD_METERS: f64 = 15.0;

/// Where the activity is at one second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackPosition {
    pub lon: f64,
    pub lat: f64,
    pub distance_meters: f64,
    /// The first sample after this position.
    next_sample: usize,
}

pub struct MapTrack {
    samples: Vec<TrackSample>,
    world: Vec<(f64, f64)>,
    distance: Vec<f64>,
}

impl MapTrack {
    /// `None` without two GPS samples: the widget then shows its placeholder.
    pub fn new(samples: Vec<TrackSample>) -> Option<Self> {
        if samples.len() < 2 {
            return None;
        }
        let world: Vec<(f64, f64)> = samples
            .iter()
            .map(|sample| lon_lat_to_world(sample.lon, sample.lat))
            .collect();
        let mut distance = Vec::with_capacity(samples.len());
        let (mut total, mut anchor) = (0.0, 0);
        for index in 0..samples.len() {
            let moved = distance_meters(coordinate(&samples[anchor]), coordinate(&samples[index]));
            if moved >= MIN_MOVE_METERS {
                total += moved;
                anchor = index;
            }
            distance.push(total);
        }
        Some(Self {
            samples,
            world,
            distance,
        })
    }

    pub fn world(&self) -> &[(f64, f64)] {
        &self.world
    }

    pub fn distance(&self) -> &[f64] {
        &self.distance
    }

    /// The track's position at `second`, held at its ends outside the sampled range.
    pub fn position_at(&self, second: f64) -> TrackPosition {
        let first = self.samples[0].second;
        let last = self.samples[self.samples.len() - 1].second;
        let second = second.clamp(first, last);
        let right = self
            .samples
            .partition_point(|sample| sample.second <= second)
            .clamp(1, self.samples.len() - 1);
        let (a, b) = (&self.samples[right - 1], &self.samples[right]);
        let mix = (second - a.second) / (b.second - a.second);
        TrackPosition {
            lon: a.lon + (b.lon - a.lon) * mix,
            lat: a.lat + (b.lat - a.lat) * mix,
            distance_meters: self.distance[right - 1]
                + (self.distance[right] - self.distance[right - 1]) * mix,
            next_sample: right,
        }
    }

    /// Clockwise from north, toward the point [`BEARING_LOOKAHEAD_METERS`] further along the route, as
    /// `getCourseNavigationCamera` in `mapPreviewGeometry.js`. The camera and the marker share it, so
    /// they turn together.
    pub fn bearing_at(&self, position: &TrackPosition) -> f64 {
        let center = (position.lon, position.lat);
        let mut remaining = BEARING_LOOKAHEAD_METERS;
        let mut cursor = center;
        for sample in &self.samples[position.next_sample..] {
            let target = coordinate(sample);
            let distance = distance_meters(cursor, target);
            if distance >= remaining && distance > 0.0 {
                let progress = remaining / distance;
                return coordinate_bearing(
                    center,
                    (
                        cursor.0 + progress * (target.0 - cursor.0),
                        cursor.1 + progress * (target.1 - cursor.1),
                    ),
                );
            }
            remaining -= distance;
            cursor = target;
        }
        if cursor != center {
            return coordinate_bearing(center, cursor);
        }
        self.bearing_into(position.next_sample)
    }

    /// At the very end there is nothing ahead: the last direction travelled. North if none.
    fn bearing_into(&self, end: usize) -> f64 {
        let last = coordinate(&self.samples[end]);
        self.samples[..end]
            .iter()
            .rev()
            .map(coordinate)
            .find(|sample| *sample != last)
            .map_or(0.0, |sample| coordinate_bearing(sample, last))
    }
}

/// `(lon, lat)` in degrees.
fn coordinate(sample: &TrackSample) -> (f64, f64) {
    (sample.lon, sample.lat)
}

/// As `coordinateBearing` in `mapPreviewGeometry.js`.
fn coordinate_bearing(start: (f64, f64), end: (f64, f64)) -> f64 {
    let (start_lat, end_lat) = (start.1.to_radians(), end.1.to_radians());
    let delta_lon = (end.0 - start.0).to_radians();
    let y = delta_lon.sin() * end_lat.cos();
    let x = start_lat.cos() * end_lat.sin() - start_lat.sin() * end_lat.cos() * delta_lon.cos();
    y.atan2(x).to_degrees()
}

/// Haversine, as `coordinateDistanceMeters` in `mapPreviewGeometry.js`.
fn distance_meters(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (a_lat, b_lat) = (a.1.to_radians(), b.1.to_radians());
    let half_lat = (b_lat - a_lat) / 2.0;
    let half_lon = (b.0 - a.0).to_radians() / 2.0;
    let haversine = half_lat.sin().powi(2) + a_lat.cos() * b_lat.cos() * half_lon.sin().powi(2);
    2.0 * EARTH_RADIUS_METERS * haversine.sqrt().atan2((1.0 - haversine).sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_METERS: f64 = 5.0;
    const LEG_SAMPLES: usize = 40;

    /// North along a meridian on the equator, then a right angle east along it.
    fn corner() -> (MapTrack, f64) {
        let step = (SAMPLE_METERS / EARTH_RADIUS_METERS).to_degrees();
        let samples = (0..=LEG_SAMPLES * 2)
            .map(|index| {
                let (north, east) = (index.min(LEG_SAMPLES), index.saturating_sub(LEG_SAMPLES));
                TrackSample {
                    second: index as f64,
                    lat: north as f64 * step,
                    lon: east as f64 * step,
                }
            })
            .collect();
        let track = MapTrack::new(samples).unwrap();
        let at_corner = track.distance()[LEG_SAMPLES];
        (track, at_corner)
    }

    /// The bearing with the rider `metres` past the corner, negative before it.
    fn bearing_around_corner(track: &MapTrack, metres: f64) -> f64 {
        let second = LEG_SAMPLES as f64 + metres / SAMPLE_METERS;
        track.bearing_at(&track.position_at(second))
    }

    #[test]
    fn the_bearing_starts_turning_when_the_lookahead_reaches_the_corner() {
        let (track, _) = corner();
        assert!(bearing_around_corner(&track, -16.0).abs() < 0.1);
        assert!(bearing_around_corner(&track, -14.0) > 1.0);
    }

    #[test]
    fn the_bearing_has_aimed_down_the_new_road_by_the_corner() {
        let (track, _) = corner();
        let halfway = bearing_around_corner(&track, -7.5);
        assert!((halfway - 45.0).abs() < 1.0, "{halfway}");
        assert!((bearing_around_corner(&track, 0.0) - 90.0).abs() < 0.1);
    }

    #[test]
    fn the_end_of_the_track_keeps_the_last_direction() {
        let (track, _) = corner();
        let end = track.position_at(f64::MAX);
        assert!((track.bearing_at(&end) - 90.0).abs() < 0.1);
    }
}
