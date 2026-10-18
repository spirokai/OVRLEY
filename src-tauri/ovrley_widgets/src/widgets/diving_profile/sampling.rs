//! Original timestamps drive both hosts. Missing external telemetry separates profile runs.
use crate::{interpolation::interpolate_points, widgets::route::sampling::RouteWindow};

pub type DepthSample = (f32, f64, f32);

#[derive(Debug)]
pub struct DivingSamples {
    pub depth: Vec<Vec<DepthSample>>,
    pub ceiling: Vec<Vec<(f32, f64)>>,
}

pub struct DivingActivity {
    depth: Vec<Vec<(f64, f64)>>,
    ceiling: Vec<CeilingCurve>,
    pub end: f64,
    duration: f64,
    range: Option<(f64, f64)>,
}

/// Monotone cubic Hermite interpolation. Reversals have zero tangent; gaps are separate curves.
pub struct CeilingCurve {
    pub knots: Vec<(f64, f64)>,
    tangents: Vec<f64>,
}

impl CeilingCurve {
    pub fn new(points: &[(f64, f64)]) -> Self {
        let mut knots = vec![points[0]];
        for (index, pair) in points.windows(2).enumerate() {
            if pair[0].1 == pair[1].1 {
                continue;
            }
            // Ease into the first positive step over its duration; keep its recorded value/time exact.
            if pair[0].1 == 0.0 && knots.last().unwrap().0 < pair[0].0 {
                let first = pair[1];
                let end = points[index + 2..]
                    .iter()
                    .find(|p| p.1 != first.1)
                    .unwrap_or(points.last().unwrap())
                    .0;
                let zero_start = knots.last().unwrap().0;
                let onset = (first.0 - (end - first.0)).clamp(zero_start, pair[0].0);
                if onset > zero_start {
                    knots.push((onset, 0.0));
                }
            }
            knots.push(pair[1]);
        }
        if knots.last().unwrap().0 < points.last().unwrap().0 {
            knots.push(*points.last().unwrap());
        }
        let h: Vec<_> = knots.windows(2).map(|p| p[1].0 - p[0].0).collect();
        let slopes: Vec<_> = knots
            .windows(2)
            .zip(&h)
            .map(|(p, h)| (p[1].1 - p[0].1) / h)
            .collect();
        let mut tangents = vec![0.0; knots.len()];
        if slopes.len() == 1 {
            tangents.fill(slopes[0]);
        } else if slopes.len() > 1 {
            let endpoint = |h0: f64, h1: f64, d0: f64, d1: f64| {
                let m = ((2.0 * h0 + h1) * d0 - h0 * d1) / (h0 + h1);
                if m.signum() != d0.signum() {
                    0.0
                } else if d0.signum() != d1.signum() && m.abs() > 3.0 * d0.abs() {
                    3.0 * d0
                } else {
                    m
                }
            };
            tangents[0] = endpoint(h[0], h[1], slopes[0], slopes[1]);
            let last = slopes.len() - 1;
            tangents[last + 1] = endpoint(h[last], h[last - 1], slopes[last], slopes[last - 1]);
            for i in 1..knots.len() - 1 {
                let (a, b) = (slopes[i - 1], slopes[i]);
                if a * b > 0.0 {
                    let (w1, w2) = (2.0 * h[i] + h[i - 1], h[i] + 2.0 * h[i - 1]);
                    tangents[i] = (w1 + w2) / (w1 / a + w2 / b);
                }
            }
        }
        Self { knots, tangents }
    }

    pub fn at(&self, second: f64) -> Option<f64> {
        if second < self.knots[0].0 || second > self.knots.last().unwrap().0 {
            return None;
        }
        if self.knots.len() == 1 {
            return Some(self.knots[0].1);
        }
        let i = self
            .knots
            .partition_point(|p| p.0 <= second)
            .clamp(1, self.knots.len() - 1)
            - 1;
        let (a, b) = (self.knots[i], self.knots[i + 1]);
        let h = b.0 - a.0;
        let u = (second - a.0) / h;
        let value = (2.0 * u.powi(3) - 3.0 * u.powi(2) + 1.0) * a.1
            + (u.powi(3) - 2.0 * u.powi(2) + u) * h * self.tangents[i]
            + (-2.0 * u.powi(3) + 3.0 * u.powi(2)) * b.1
            + (u.powi(3) - u.powi(2)) * h * self.tangents[i + 1];
        Some(value)
    }
}

fn continuous_runs(times: &[f64], values: &[Option<f64>]) -> Vec<Vec<(f64, f64)>> {
    let mut runs = Vec::new();
    let mut run = Vec::new();
    for (&time, value) in times.iter().zip(values) {
        if let Some(value) = value.filter(|v| v.is_finite() && *v >= 0.0) {
            run.push((time, value));
        } else if !run.is_empty() {
            runs.push(std::mem::take(&mut run));
        }
    }
    if !run.is_empty() {
        runs.push(run);
    }
    runs
}

impl DivingActivity {
    pub fn new(times: &[f64], depth: &[Option<f64>], ceiling: &[Option<f64>], end: f64) -> Self {
        let depth = continuous_runs(times, depth);
        let range = depth.iter().flatten().fold(None, |range, &(_, value)| {
            Some(range.map_or((value, value), |(min, max): (f64, f64)| {
                (min.min(value), max.max(value))
            }))
        });
        Self {
            depth,
            ceiling: continuous_runs(times, ceiling)
                .iter()
                .map(|run| CeilingCurve::new(run))
                .collect(),
            end,
            duration: times.last().copied().unwrap_or(0.0).max(1e-9),
            range,
        }
    }

    pub fn depth_range(&self) -> Option<(f64, f64)> {
        self.range
    }

    pub fn frame_state(
        &self,
        geometry: &super::geometry::DivingGeometry,
        second: f64,
        window: Option<RouteWindow>,
    ) -> super::geometry::DivingFrameState {
        let mut state = geometry.state(self.progress_at(second, window), self.depth_at(second));
        if state.depth_m.is_none() {
            state.marker = None;
        }
        state
    }

    pub fn depth_at(&self, second: f64) -> Option<f64> {
        if second < 0.0 || second > self.end {
            return None;
        }
        self.depth
            .iter()
            .find(|run| second >= run[0].0 && second <= run.last().unwrap().0)
            .and_then(|run| interpolate_points(run, second))
    }

    pub fn progress_at(&self, second: f64, window: Option<RouteWindow>) -> f32 {
        let (start, end) = self.bounds(window);
        ((second - start) / (end - start)).clamp(0.0, 1.0) as f32
    }

    fn bounds(&self, window: Option<RouteWindow>) -> (f64, f64) {
        window.map_or((0.0, self.duration), |w| (w.start, w.end))
    }

    /// Interpolate crop boundaries within observed runs; never bridge a telemetry gap.
    pub fn samples(&self, window: Option<RouteWindow>, width: f32) -> DivingSamples {
        let (start, end) = self.bounds(window);
        let fraction = |t: f64| ((t - start) / (end - start)) as f32;
        let depth = self
            .depth
            .iter()
            .filter_map(|run| {
                let (a, b) = (start.max(run[0].0), end.min(run.last().unwrap().0));
                if a > b {
                    return None;
                }
                let mut samples = vec![(
                    fraction(a),
                    interpolate_points(run, a).unwrap(),
                    fraction(a),
                )];
                samples.extend(
                    run.iter()
                        .filter(|p| p.0 > a && p.0 < b)
                        .map(|&(t, d)| (fraction(t), d, fraction(t))),
                );
                if b > a {
                    samples.push((
                        fraction(b),
                        interpolate_points(run, b).unwrap(),
                        fraction(b),
                    ));
                }
                Some(samples)
            })
            .collect();
        let ceiling = self
            .ceiling
            .iter()
            .filter_map(|curve| {
                let (a, b) = (
                    start.max(curve.knots[0].0),
                    end.min(curve.knots.last().unwrap().0),
                );
                if a > b {
                    return None;
                }
                let count = ((fraction(b) - fraction(a)) * width).ceil().max(1.0) as usize;
                let mut times: Vec<_> = (0..=count)
                    .map(|i| a + (b - a) * i as f64 / count as f64)
                    .collect();
                times.extend(
                    curve
                        .knots
                        .iter()
                        .filter(|p| p.0 > a && p.0 < b)
                        .map(|p| p.0),
                );
                times.sort_by(f64::total_cmp);
                times.dedup();
                Some(
                    times
                        .into_iter()
                        .map(|t| (fraction(t), curve.at(t).unwrap()))
                        .collect(),
                )
            })
            .collect();
        DivingSamples { depth, ceiling }
    }
}
