//! The activity's GPS track for one window as a smooth line in Mercator, with travelled distance.
//! Built once, so every frame is a pure function of the activity second. The route, marker and
//! camera all read this one line, so the marker stays on the stroke.
use crate::widgets::route::sampling::TrackSample;
use ovrley_map::mercator::lon_lat_to_world;

const EARTH_RADIUS_METERS: f64 = 6_371_000.0;
/// GPS wanders while stationary. Movement under this adds no distance.
const MIN_MOVE_METERS: f64 = 3.0;
/// How far behind and ahead of the rider the bearing's chord reaches along the route.
const BEARING_REACH_METERS: f64 = 10.0;
/// Spacing of the line before smoothing. Samples are too far apart to round a corner.
const RESAMPLE_METERS: f64 = 2.0;
/// Where the Gaussian kernel ends, in sigmas.
const KERNEL_SIGMAS: f64 = 3.0;

/// Where the activity is at one second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackPosition {
    pub world: (f64, f64),
    pub distance_meters: f64,
}

pub struct MapTrack {
    seconds: Vec<f64>,
    /// Travelled distance at each of `seconds`.
    sample_distance: Vec<f64>,
    line: EvenLine,
    world: Vec<(f64, f64)>,
}

impl MapTrack {
    /// `None` without two GPS samples: the widget then shows its placeholder. `smoothing_meters` is
    /// the Gaussian's sigma: a real-world length because GPS noise is one, so it doesn't follow zoom.
    /// A right-angle corner is cut by about `0.56` of it.
    pub fn new(samples: Vec<TrackSample>, smoothing_meters: f64) -> Option<Self> {
        if samples.len() < 2 {
            return None;
        }
        let mut sample_distance = Vec::with_capacity(samples.len());
        let mut anchors = vec![Anchor {
            world: lon_lat_to_world(samples[0].lon, samples[0].lat),
            distance: 0.0,
        }];
        let (mut total, mut anchor) = (0.0, 0);
        for index in 0..samples.len() {
            let moved = distance_meters(coordinate(&samples[anchor]), coordinate(&samples[index]));
            if moved >= MIN_MOVE_METERS {
                total += moved;
                anchor = index;
                anchors.push(Anchor {
                    world: lon_lat_to_world(samples[index].lon, samples[index].lat),
                    distance: total,
                });
            }
            sample_distance.push(total);
        }
        let line = EvenLine::new(&anchors);
        Some(Self {
            seconds: samples.iter().map(|sample| sample.second).collect(),
            sample_distance,
            world: gaussian_smooth(&line, smoothing_meters),
            line,
        })
    }

    /// This track at another smoothing, without walking the GPS samples again.
    pub fn smoothed(&self, smoothing_meters: f64) -> Self {
        Self {
            seconds: self.seconds.clone(),
            sample_distance: self.sample_distance.clone(),
            line: self.line.clone(),
            world: gaussian_smooth(&self.line, smoothing_meters),
        }
    }

    /// The smoothed line's vertices, evenly spaced by travelled distance.
    pub fn world(&self) -> &[(f64, f64)] {
        &self.world
    }

    /// Travelled distance at each vertex of [`Self::world`].
    pub fn distance(&self) -> &[f64] {
        &self.line.distance
    }

    /// The track's position at `second`, held at its ends outside the sampled range.
    pub fn position_at(&self, second: f64) -> TrackPosition {
        let last = self.seconds.len() - 1;
        let second = second.clamp(self.seconds[0], self.seconds[last]);
        let right = self
            .seconds
            .partition_point(|sample| *sample <= second)
            .clamp(1, last);
        let mix =
            (second - self.seconds[right - 1]) / (self.seconds[right] - self.seconds[right - 1]);
        let (a, b) = (self.sample_distance[right - 1], self.sample_distance[right]);
        let distance_meters = a + (b - a) * mix;
        TrackPosition {
            world: self.world_at(distance_meters),
            distance_meters,
        }
    }

    /// Clockwise from north, along the chord [`BEARING_REACH_METERS`] either side of the rider, so a
    /// turn is centred on its corner. The chord holds at the line's ends and a track that never moved
    /// faces north. The camera and the marker share it, so they turn together.
    pub fn bearing_at(&self, position: &TrackPosition) -> f64 {
        world_bearing(
            self.world_at(position.distance_meters - BEARING_REACH_METERS),
            self.world_at(position.distance_meters + BEARING_REACH_METERS),
        )
    }

    /// The line's point at a travelled distance, held at its ends.
    fn world_at(&self, distance: f64) -> (f64, f64) {
        let along = self.distance();
        let right = along
            .partition_point(|along| *along <= distance)
            .clamp(1, along.len() - 1);
        let (a, b) = (along[right - 1], along[right]);
        // A track that never moved has one place, at distance zero.
        if b == a {
            return self.world[right - 1];
        }
        lerp(
            self.world[right - 1],
            self.world[right],
            ((distance - a) / (b - a)).clamp(0.0, 1.0),
        )
    }
}

/// A sample that moved the track, so wander while stopped never reaches the line.
struct Anchor {
    world: (f64, f64),
    distance: f64,
}

/// The unfiltered line, evenly spaced by distance so the smoothing is the same at any speed or sample
/// rate.
#[derive(Clone)]
struct EvenLine {
    points: Vec<(f64, f64)>,
    distance: Vec<f64>,
    spacing: f64,
}

impl EvenLine {
    /// A track that never moved stays a two-point line in one place.
    fn new(anchors: &[Anchor]) -> Self {
        let total = anchors[anchors.len() - 1].distance;
        if total == 0.0 {
            return Self {
                points: vec![anchors[0].world; 2],
                distance: vec![0.0; 2],
                spacing: RESAMPLE_METERS,
            };
        }
        let segments = (total / RESAMPLE_METERS).ceil() as usize;
        let spacing = total / segments as f64;
        let mut cursor = 0;
        let mut points = Vec::with_capacity(segments + 1);
        let mut distance = Vec::with_capacity(segments + 1);
        for index in 0..=segments {
            let along = index as f64 * spacing;
            while cursor + 2 < anchors.len() && anchors[cursor + 1].distance < along {
                cursor += 1;
            }
            let (a, b) = (&anchors[cursor], &anchors[cursor + 1]);
            let mix = ((along - a.distance) / (b.distance - a.distance)).clamp(0.0, 1.0);
            points.push(lerp(a.world, b.world, mix));
            distance.push(along);
        }
        Self {
            points,
            distance,
            spacing,
        }
    }
}

/// The kernel narrows symmetrically toward each end, so the line still starts and ends where the
/// activity does and its ends don't drift.
fn gaussian_smooth(line: &EvenLine, smoothing_meters: f64) -> Vec<(f64, f64)> {
    let points = &line.points;
    let sigma = smoothing_meters / line.spacing;
    let weights: Vec<f64> = (0..=(sigma * KERNEL_SIGMAS).ceil() as usize)
        .map(|offset| (-((offset * offset) as f64) / (2.0 * sigma * sigma)).exp())
        .collect();
    (0..points.len())
        .map(|index| {
            let reach = (weights.len() - 1).min(index).min(points.len() - 1 - index);
            let (mut x, mut y) = (points[index].0 * weights[0], points[index].1 * weights[0]);
            let mut sum = weights[0];
            for offset in 1..=reach {
                let (before, after) = (points[index - offset], points[index + offset]);
                x += (before.0 + after.0) * weights[offset];
                y += (before.1 + after.1) * weights[offset];
                sum += 2.0 * weights[offset];
            }
            (x / sum, y / sum)
        })
        .collect()
}

fn lerp(a: (f64, f64), b: (f64, f64), mix: f64) -> (f64, f64) {
    (a.0 + (b.0 - a.0) * mix, a.1 + (b.1 - a.1) * mix)
}

/// `(lon, lat)` in degrees.
fn coordinate(sample: &TrackSample) -> (f64, f64) {
    (sample.lon, sample.lat)
}

/// Mercator keeps angles and its world's y grows south.
fn world_bearing(start: (f64, f64), end: (f64, f64)) -> f64 {
    (end.0 - start.0).atan2(start.1 - end.1).to_degrees()
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
    const SMOOTHING_METERS: f64 = 8.0;

    fn degrees(metres: f64) -> f64 {
        (metres / EARTH_RADIUS_METERS).to_degrees()
    }

    /// North along a meridian on the equator, then a right angle east along it.
    fn corner() -> MapTrack {
        let step = degrees(SAMPLE_METERS);
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
        MapTrack::new(samples, SMOOTHING_METERS).unwrap()
    }

    /// How far the line stays from the sharp corner, in metres.
    fn corner_cut(track: &MapTrack) -> f64 {
        let sharp = lon_lat_to_world(0.0, degrees(SAMPLE_METERS * LEG_SAMPLES as f64));
        track
            .world()
            .iter()
            .map(|point| (point.0 - sharp.0).hypot(point.1 - sharp.1))
            .fold(f64::MAX, f64::min)
            * 2.0
            * std::f64::consts::PI
            * EARTH_RADIUS_METERS
    }

    /// The bearing with the rider `metres` past the corner, negative before it.
    fn bearing_around_corner(track: &MapTrack, metres: f64) -> f64 {
        let second = LEG_SAMPLES as f64 + metres / SAMPLE_METERS;
        track.bearing_at(&track.position_at(second))
    }

    #[test]
    fn the_bearing_follows_the_road_away_from_a_corner() {
        let track = corner();
        assert!(bearing_around_corner(&track, -45.0).abs() < 0.1);
        assert!((bearing_around_corner(&track, 45.0) - 90.0).abs() < 0.1);
    }

    #[test]
    fn the_turn_is_centred_on_the_corner() {
        let track = corner();
        assert!((bearing_around_corner(&track, 0.0) - 45.0).abs() < 1.0);
        assert!(bearing_around_corner(&track, -8.0) < 30.0);
        assert!(bearing_around_corner(&track, 8.0) > 60.0);
    }

    #[test]
    fn the_bearing_turns_through_a_corner_without_a_jump() {
        let track = corner();
        let mut previous = bearing_around_corner(&track, -45.0);
        for metres in -44..=45 {
            let bearing = bearing_around_corner(&track, f64::from(metres));
            let turned = bearing - previous;
            assert!((-1e-6..6.0).contains(&turned), "{metres} m: {turned}");
            previous = bearing;
        }
    }

    #[test]
    fn the_end_of_the_track_keeps_the_last_direction() {
        let track = corner();
        let end = track.position_at(f64::MAX);
        assert!((track.bearing_at(&end) - 90.0).abs() < 0.1);
    }

    #[test]
    fn the_line_starts_and_ends_on_the_activity_and_rounds_the_corner() {
        let track = corner();
        let leg = degrees(SAMPLE_METERS * LEG_SAMPLES as f64);
        let (start, end) = (track.world()[0], track.world()[track.world().len() - 1]);
        let close = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() + (a.1 - b.1).abs() < 1e-12;
        assert!(close(start, lon_lat_to_world(0.0, 0.0)));
        assert!(close(end, lon_lat_to_world(leg, leg)));

        let cut = corner_cut(&track);
        assert!((3.5..5.5).contains(&cut), "{cut}");
    }

    #[test]
    fn more_smoothing_cuts_the_corner_further_and_keeps_distances() {
        let track = corner();
        let stronger = track.smoothed(SMOOTHING_METERS * 2.0);
        assert!(corner_cut(&stronger) > corner_cut(&track) * 1.5);
        assert_eq!(stronger.distance(), track.distance());
        assert_eq!(track.smoothed(SMOOTHING_METERS).world(), track.world());
    }

    #[test]
    fn jitter_on_a_straight_road_is_removed() {
        let (along, off) = (degrees(8.0), degrees(2.0));
        let samples = (0..200)
            .map(|index| TrackSample {
                second: index as f64,
                lat: index as f64 * along,
                lon: if index % 2 == 0 { off } else { -off },
            })
            .collect();
        let track = MapTrack::new(samples, SMOOTHING_METERS).unwrap();
        let points = track.world();
        let centre = lon_lat_to_world(0.0, 0.0).0;
        let widest = points[50..points.len() - 50]
            .iter()
            .map(|point| {
                (point.0 - centre).abs() * 2.0 * std::f64::consts::PI * EARTH_RADIUS_METERS
            })
            .fold(0.0, f64::max);
        assert!(widest < 0.1, "{widest}");
    }

    #[test]
    fn the_marker_stays_on_the_line_and_distance_keeps_growing() {
        let track = corner();
        let mut previous = 0.0;
        for tenth in 0..=800 {
            let position = track.position_at(f64::from(tenth) / 10.0);
            assert!(position.distance_meters >= previous);
            previous = position.distance_meters;
            let right = track
                .distance()
                .partition_point(|along| *along <= position.distance_meters)
                .clamp(1, track.world().len() - 1);
            let (a, b) = (track.world()[right - 1], track.world()[right]);
            let span = (b.0 - a.0).hypot(b.1 - a.1);
            let away = (position.world.0 - a.0).hypot(position.world.1 - a.1)
                + (position.world.0 - b.0).hypot(position.world.1 - b.1);
            assert!(away - span < 1e-10);
        }
    }

    #[test]
    fn a_track_that_never_moved_is_one_place_facing_north() {
        let samples = (0..5)
            .map(|index| TrackSample {
                second: index as f64,
                lat: 50.0,
                lon: 14.0 + degrees(index as f64 * 0.2),
            })
            .collect();
        let track = MapTrack::new(samples, SMOOTHING_METERS).unwrap();
        assert_eq!(track.world().len(), 2);
        assert_eq!(track.world()[0], track.world()[1]);
        let position = track.position_at(2.5);
        assert_eq!(position.world, track.world()[0]);
        assert_eq!(track.bearing_at(&position), 0.0);
    }
}
