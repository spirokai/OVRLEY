//! The activity's GPS track for one window: Mercator points, travelled distance, and prefix sums of
//! unit headings. Built once, so every frame is a pure function of the activity second.
use crate::widgets::route::sampling::TrackSample;
use ovrley_map::mercator::lon_lat_to_world;

const EARTH_RADIUS_METERS: f64 = 6_371_000.0;
/// GPS wanders while stationary. Movement under this adds neither distance nor heading.
const MIN_MOVE_METERS: f64 = 3.0;
/// How far ahead the bearing looks. More of the window is ahead than behind, so the camera is already
/// turning when the rider reaches a corner. The whole route is known, so looking ahead costs nothing.
const LOOKAHEAD_METERS: f64 = 30.0;
/// How far behind the bearing also looks. The window's length smooths GPS noise out of the rotating
/// map, and its centre sets how early the camera turns.
const SMOOTHING_METERS: f64 = 15.0;

/// Where the activity is at one second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackPosition {
    pub lon: f64,
    pub lat: f64,
    pub distance_meters: f64,
}

pub struct MapTrack {
    samples: Vec<TrackSample>,
    world: Vec<(f64, f64)>,
    distance: Vec<f64>,
    /// Distance-weighted sum of unit headings up to each sample, as (east, north).
    heading_sum: Vec<(f64, f64)>,
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
        let mut heading_sum = Vec::with_capacity(samples.len());
        let (mut total, mut sum, mut anchor) = (0.0, (0.0, 0.0), 0);
        for index in 0..samples.len() {
            let moved = distance_meters(&samples[anchor], &samples[index]);
            if moved >= MIN_MOVE_METERS {
                // Mercator is conformal, so its direction is the true bearing. World y runs south.
                let (east, south) = (
                    world[index].0 - world[anchor].0,
                    world[index].1 - world[anchor].1,
                );
                let length = east.hypot(south);
                total += moved;
                sum = (
                    sum.0 + east / length * moved,
                    sum.1 - south / length * moved,
                );
                anchor = index;
            }
            distance.push(total);
            heading_sum.push(sum);
        }
        Some(Self {
            samples,
            world,
            distance,
            heading_sum,
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
        }
    }

    /// Clockwise from north, as the mean heading of the road around `distance`. A pure function of
    /// distance, so a stationary rider keeps the last bearing without any state.
    pub fn bearing_at(&self, distance: f64) -> f64 {
        let ahead = self.heading_sum_at(distance + LOOKAHEAD_METERS);
        let behind = self.heading_sum_at(distance - SMOOTHING_METERS);
        let (east, north) = (ahead.0 - behind.0, ahead.1 - behind.1);
        if east.hypot(north) > f64::EPSILON {
            east.atan2(north).to_degrees()
        } else {
            self.nearest_heading(distance)
        }
    }

    fn heading_sum_at(&self, distance: f64) -> (f64, f64) {
        let total = self.distance[self.distance.len() - 1];
        let distance = distance.clamp(0.0, total);
        let right = self
            .distance
            .partition_point(|value| *value <= distance)
            .clamp(1, self.distance.len() - 1);
        let (a, b) = (self.distance[right - 1], self.distance[right]);
        let mix = if b > a { (distance - a) / (b - a) } else { 0.0 };
        let (from, to) = (self.heading_sum[right - 1], self.heading_sum[right]);
        (
            from.0 + (to.0 - from.0) * mix,
            from.1 + (to.1 - from.1) * mix,
        )
    }

    /// When the window nets out to nothing: the last heading before `distance`, or else the first
    /// after it. North when the rider never moved.
    fn nearest_heading(&self, distance: f64) -> f64 {
        let at = self.distance.partition_point(|value| *value <= distance);
        let heading_of = |index: usize| {
            let (a, b) = (self.heading_sum[index - 1], self.heading_sum[index]);
            ((b.0 - a.0) != 0.0 || (b.1 - a.1) != 0.0)
                .then(|| (b.0 - a.0).atan2(b.1 - a.1).to_degrees())
        };
        (1..at.min(self.distance.len()))
            .rev()
            .chain(at.max(1)..self.distance.len())
            .find_map(heading_of)
            .unwrap_or(0.0)
    }
}

/// Haversine, as `coordinateDistanceMeters` in `mapPreviewGeometry.js`.
fn distance_meters(a: &TrackSample, b: &TrackSample) -> f64 {
    let (a_lat, b_lat) = (a.lat.to_radians(), b.lat.to_radians());
    let half_lat = (b_lat - a_lat) / 2.0;
    let half_lon = (b.lon - a.lon).to_radians() / 2.0;
    let haversine = half_lat.sin().powi(2) + a_lat.cos() * b_lat.cos() * half_lon.sin().powi(2);
    2.0 * EARTH_RADIUS_METERS * haversine.sqrt().atan2((1.0 - haversine).sqrt())
}
