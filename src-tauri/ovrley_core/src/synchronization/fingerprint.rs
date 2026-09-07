//! Temporary, format-independent matching features. No parsing or activity mutation.
//!
//! All support is an estimate from processed signals, never sensor provenance.
//! Intervals retain source time and are not independent merely because rows are dense.
//! Thresholds here are conservative starting values for integrated manual tuning.

use crate::activity::schema::ParsedActivity;
use crate::media::telemetry_math::haversine_distance;

use super::{AnalysisError, AnalysisResult, MotionInterval, MotionOutcome};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureFamily {
    Turning,
    Speed,
    Stops,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelKind {
    SignedTurn,
    TurnMagnitude,
    SpeedTrend,
    SpeedChange,
    LowMotion,
}

impl ChannelKind {
    pub fn family(self) -> FeatureFamily {
        match self {
            Self::SignedTurn | Self::TurnMagnitude => FeatureFamily::Turning,
            Self::SpeedTrend | Self::SpeedChange => FeatureFamily::Speed,
            Self::LowMotion => FeatureFamily::Stops,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupportReason {
    UnavailableChannel,
    InvalidValue,
    TimestampDiscontinuity,
    Gap,
    GpsJump,
    InsufficientDisplacement,
    UntrackableVideo,
    VideoDiscontinuity,
    UnknownMetricSupport,
    ProcessedMetricSupport,
}

#[derive(Debug)]
pub struct SupportIssue {
    pub channel: ChannelKind,
    /// None for an entirely unavailable channel or an invalid timestamp.
    pub at_seconds: Option<f64>,
    pub reason: SupportReason,
}

#[derive(Clone, Copy, Debug)]
pub struct FeatureInterval {
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub value: f64,
    pub quality: f64,
    /// Minimum useful sampling period, including visible cadence/bandwidth limits.
    pub support_seconds: f64,
    /// Continuous filters cannot cross spans. Observed-only aggregation may
    /// combine spans separated by a bounded hole, but cannot fill that hole.
    pub span: usize,
}

#[derive(Debug)]
pub struct Channel {
    pub kind: ChannelKind,
    intervals: Vec<FeatureInterval>,
}

impl Channel {
    pub fn intervals(&self) -> &[FeatureInterval] {
        &self.intervals
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    TurnLeft,
    TurnRight,
    Stop,
}

#[derive(Debug)]
pub struct MotionEvent {
    pub kind: EventKind,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub strength: f64,
    pub quality: f64,
    pub uncertainty_seconds: f64,
}

#[derive(Debug)]
pub struct Fingerprint {
    channels: Vec<Channel>,
    /// Observed source extent, including unsupported leading/trailing intervals.
    /// None means the source provided no usable timeline.
    pub time_range: Option<std::ops::Range<f64>>,
    pub events: Vec<MotionEvent>,
    pub issues: Vec<SupportIssue>,
    /// Activity speed/stops can include smoothing and synthetic idle insertion.
    pub processed_activity: bool,
    /// Repeated coordinates cannot identify the hidden receiver update rate.
    pub repeated_gps: bool,
}

impl Fingerprint {
    pub fn channels(&self) -> &[Channel] {
        &self.channels
    }

    /// Retained heap allocation, for the managed job's in-memory cache budget.
    pub(super) fn heap_bytes(&self) -> usize {
        self.channels.capacity() * std::mem::size_of::<Channel>()
            + self
                .channels
                .iter()
                .map(|channel| {
                    channel.intervals.capacity() * std::mem::size_of::<FeatureInterval>()
                })
                .sum::<usize>()
            + self.events.capacity() * std::mem::size_of::<MotionEvent>()
            + self.issues.capacity() * std::mem::size_of::<SupportIssue>()
    }
}

#[derive(Clone, Copy)]
struct Observation {
    time: f64,
    value: f64,
    span: usize,
}

fn issue(
    issues: &mut Vec<SupportIssue>,
    channel: ChannelKind,
    time: Option<f64>,
    reason: SupportReason,
) {
    issues.push(SupportIssue {
        channel,
        at_seconds: time,
        reason,
    });
}

// Keep a monotonic high-water mark after a clock reset. Overlapping elapsed times
// cannot safely participate in a single-offset match. Do not sort them into a fake path.
fn timeline(times: &[f64], issues: &mut Vec<SupportIssue>) -> Vec<Option<usize>> {
    let mut high_water = -1.0;
    let mut span = 0;
    times
        .iter()
        .map(|&time| {
            if !time.is_finite() || time < 0.0 || time <= high_water {
                span += 1;
                issue(
                    issues,
                    ChannelKind::SpeedTrend,
                    time.is_finite().then_some(time),
                    SupportReason::TimestampDiscontinuity,
                );
                None
            } else {
                high_water = time;
                Some(span)
            }
        })
        .collect()
}

fn observations(
    times: &[f64],
    spans: &[Option<usize>],
    values: &[Option<f64>],
    kind: ChannelKind,
    issues: &mut Vec<SupportIssue>,
) -> Vec<Observation> {
    if values.is_empty() {
        issue(issues, kind, None, SupportReason::UnavailableChannel);
    }
    let mut result = Vec::new();
    let mut span = 0;
    let mut previous_source_span = None;
    for (i, &time) in times.iter().enumerate() {
        let value = values.get(i).copied().flatten();
        let valid = value.filter(|v| {
            v.is_finite()
                && match kind {
                    ChannelKind::SignedTurn => (0.0..=360.0).contains(v),
                    _ => *v >= 0.0,
                }
        });
        match (spans[i], valid) {
            (Some(source_span), Some(value)) => {
                if previous_source_span != Some(source_span) {
                    span += 1;
                }
                result.push(Observation { time, value, span });
                previous_source_span = Some(source_span);
            }
            // Channels can update more slowly than the activity rows. Select
            // available observations; signal_intervals checks their actual gaps.
            (Some(_), None) if value.is_none() => {}
            _ => {
                span += 1;
                if value.is_some() {
                    issue(issues, kind, Some(time), SupportReason::InvalidValue);
                }
            }
        }
    }
    result
}

fn circular_delta(after: f64, before: f64) -> f64 {
    (after - before + 180.0).rem_euclid(360.0) - 180.0
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

// A symmetric four-second neighborhood keeps cadence estimation time-based.
// An isolated interval supplies its own observed period, not an invented rate.
fn local_period(points: &[Observation], index: usize) -> f64 {
    let point = points[index];
    let start = points.partition_point(|p| p.time < point.time - 2.0);
    let end = points.partition_point(|p| p.time <= point.time + 2.0);
    let deltas = points[start..end]
        .windows(2)
        .filter(|p| p[0].span == point.span && p[1].span == point.span)
        .map(|p| p[1].time - p[0].time)
        .collect::<Vec<_>>();
    if deltas.is_empty() {
        return point.time - points[index - 1].time;
    }
    median(deltas)
}

fn signal_intervals(
    points: &[Observation],
    kind: ChannelKind,
    issues: &mut Vec<SupportIssue>,
) -> Vec<FeatureInterval> {
    let mut result = Vec::new();
    let mut span = 0;
    let mut repeated_support = vec![0.0; points.len()];
    let mut run_start = 0;
    while run_start < points.len() {
        let mut end = run_start + 1;
        while end < points.len()
            && points[end].span == points[run_start].span
            && points[end].value == points[run_start].value
        {
            end += 1;
        }
        let duration = points[end - 1].time - points[run_start].time;
        repeated_support[run_start..end].fill(duration.min(1.0));
        run_start = end;
    }
    for i in 1..points.len() {
        let before = points[i - 1];
        let after = points[i];
        let dt = after.time - before.time;
        let cadence = local_period(points, i);
        if before.span != after.span || dt > (3.0 * cadence).min(5.0) {
            span += 1;
            issue(issues, kind, Some(after.time), SupportReason::Gap);
            continue;
        }
        // Constant/quantized runs are not asserted to be held samples, but their
        // bandwidth cannot justify independent high-rate evidence. Limit to 1 Hz.
        let support = dt
            .max(cadence)
            .max(repeated_support[i - 1])
            .max(repeated_support[i]);
        let value = if kind == ChannelKind::SignedTurn {
            circular_delta(after.value, before.value).to_radians() / dt
        } else {
            (before.value + after.value) * 0.5
        };
        result.push(FeatureInterval {
            start_seconds: before.time,
            end_seconds: after.time,
            value,
            quality: 0.6,
            support_seconds: support,
            span,
        });
    }
    result
}

fn gps_support(
    activity: &ParsedActivity,
    spans: &[Option<usize>],
    issues: &mut Vec<SupportIssue>,
) -> (Vec<FeatureInterval>, bool) {
    let mut support = Vec::new();
    let mut anchor: Option<(f64, f64, f64, usize)> = None;
    let mut previous: Option<(f64, f64, f64, usize)> = None;
    let mut repeated = false;
    for (i, &time) in activity.sample_elapsed_seconds.iter().enumerate() {
        let Some(source_span) = spans[i] else {
            anchor = None;
            previous = None;
            continue;
        };
        let Some(&(Some(lat), Some(lon))) = activity.course.get(i) else {
            continue;
        };
        if !lat.is_finite() || !lon.is_finite() || lat.abs() > 90.0 || lon.abs() > 180.0 {
            issue(
                issues,
                ChannelKind::SignedTurn,
                Some(time),
                SupportReason::InvalidValue,
            );
            anchor = None;
            previous = None;
            continue;
        }
        let current = (time, lat, lon, source_span);
        if let Some((pt, plat, plon, ps)) = previous {
            let distance = haversine_distance(plat, plon, lat, lon);
            repeated |= distance == 0.0;
            // Duplicate rows do not move the last distinct observation's clock.
            // Otherwise a 1 Hz coordinate update on 40 Hz rows looks like a jump.
            if distance == 0.0 && ps == source_span && time - pt <= 5.0 {
                continue;
            }
            if ps != source_span || time - pt > 5.0 || distance / (time - pt) > 150.0 {
                let reason = if distance / (time - pt) > 150.0 {
                    SupportReason::GpsJump
                } else {
                    SupportReason::Gap
                };
                issue(issues, ChannelKind::SignedTurn, Some(time), reason);
                anchor = Some(current);
                previous = Some(current);
                continue;
            }
        }
        previous = Some(current);
        if let Some((at, alat, alon, _)) = anchor {
            let dt = time - at;
            if dt > 5.0 {
                issue(
                    issues,
                    ChannelKind::SignedTurn,
                    Some(time),
                    SupportReason::InsufficientDisplacement,
                );
                anchor = Some(current);
            } else if haversine_distance(alat, alon, lat, lon) >= 3.0 {
                support.push(FeatureInterval {
                    start_seconds: at,
                    end_seconds: time,
                    value: 1.0,
                    quality: 0.6,
                    support_seconds: dt,
                    span: source_span,
                });
                anchor = Some(current);
            }
        } else {
            anchor = Some(current);
        }
    }
    if support.is_empty() {
        issue(
            issues,
            ChannelKind::SignedTurn,
            None,
            SupportReason::InsufficientDisplacement,
        );
    }
    (support, repeated)
}

fn mask_heading(turns: Vec<FeatureInterval>, gps: &[FeatureInterval]) -> Vec<FeatureInterval> {
    let mut result = Vec::new();
    let mut cursor = 0;
    let mut span = 0;
    let mut previous_turn_span = None;
    for turn in turns {
        while cursor < gps.len() && gps[cursor].end_seconds <= turn.start_seconds {
            cursor += 1;
        }
        for support in gps[cursor..]
            .iter()
            .take_while(|s| s.start_seconds < turn.end_seconds)
        {
            let start = turn.start_seconds.max(support.start_seconds);
            let end = turn.end_seconds.min(support.end_seconds);
            if result
                .last()
                .is_some_and(|last: &FeatureInterval| last.end_seconds != start)
                || previous_turn_span != Some(turn.span)
            {
                span += 1;
            }
            result.push(FeatureInterval {
                start_seconds: start,
                end_seconds: end,
                support_seconds: turn.support_seconds.max(support.support_seconds),
                span,
                ..turn
            });
            previous_turn_span = Some(turn.span);
        }
    }
    result
}

/// Prepare existing imported or saved activity data on its original time origin.
/// Missing channels/rows and external anomalies are masked here exactly once.
pub fn prepare_activity(activity: &ParsedActivity) -> Fingerprint {
    let mut issues = Vec::new();
    let spans = timeline(&activity.sample_elapsed_seconds, &mut issues);
    let speed = observations(
        &activity.sample_elapsed_seconds,
        &spans,
        &activity.speed,
        ChannelKind::SpeedTrend,
        &mut issues,
    );
    let heading = observations(
        &activity.sample_elapsed_seconds,
        &spans,
        &activity.heading,
        ChannelKind::SignedTurn,
        &mut issues,
    );
    let mut speed = signal_intervals(&speed, ChannelKind::SpeedTrend, &mut issues);
    let turns = signal_intervals(&heading, ChannelKind::SignedTurn, &mut issues);
    let (gps, repeated_gps) = gps_support(activity, &spans, &mut issues);
    let source = activity
        .extra
        .get("coverage")
        .and_then(|coverage| coverage.get("speed"))
        .and_then(|coverage| coverage.get("source"))
        .and_then(serde_json::Value::as_str);
    if source != Some("direct") {
        // Coverage has only whole-series provenance/counts, not per-sample
        // sensor timestamps. Derived/mixed or legacy unknown speed must not
        // inherit the row rate as independent evidence. Use visible GPS timing
        // where supported, otherwise a conservative one-second lower bound.
        let reason = if matches!(source, Some("derived" | "mixed")) {
            SupportReason::ProcessedMetricSupport
        } else {
            SupportReason::UnknownMetricSupport
        };
        issue(&mut issues, ChannelKind::SpeedTrend, None, reason);
        for sample in &mut speed {
            let index = gps.partition_point(|s| s.end_seconds <= sample.start_seconds);
            let period = gps
                .get(index)
                .filter(|s| {
                    s.start_seconds <= sample.start_seconds && s.end_seconds >= sample.end_seconds
                })
                .map_or(1.0, |s| s.support_seconds.max(1.0));
            sample.support_seconds = sample.support_seconds.max(period);
        }
    }
    let turns = mask_heading(turns, &gps);
    let mut fingerprint = finish(turns, speed, issues, true, repeated_gps);
    let times: Vec<_> = activity
        .sample_elapsed_seconds
        .iter()
        .zip(&spans)
        .filter_map(|(&time, span)| span.map(|_| time))
        .collect();
    fingerprint.time_range = times
        .first()
        .zip(times.last())
        .and_then(|(&start, &end)| (end > start).then_some(start..end));
    fingerprint
}

/// Consume compact Phase 1 results without retaining decoded images. Original PTS
/// survive window/chunk boundaries; unavailable tracking is never a stationary sample.
pub fn prepare_video(intervals: impl IntoIterator<Item = MotionInterval>) -> Fingerprint {
    let mut builder = VideoFingerprintBuilder::default();
    for interval in intervals {
        builder.push(interval);
    }
    builder.finish()
}

/// Streaming sink retaining only compact native features, never MotionIntervals
/// or frames. Temporal computations run in bounded chunks with source overlap.
#[derive(Default)]
pub struct VideoFingerprintBuilder {
    turns: Vec<FeatureInterval>,
    speed: Vec<FeatureInterval>,
    issues: Vec<SupportIssue>,
    previous_end: Option<f64>,
    start_seconds: Option<f64>,
    span: usize,
}

impl VideoFingerprintBuilder {
    pub fn push(&mut self, interval: MotionInterval) {
        self.start_seconds.get_or_insert(interval.start_seconds);
        if self.previous_end != Some(interval.start_seconds) {
            self.span += 1;
            if self.previous_end.is_some() {
                issue(
                    &mut self.issues,
                    ChannelKind::SignedTurn,
                    Some(interval.start_seconds),
                    SupportReason::VideoDiscontinuity,
                );
            }
        }
        self.previous_end = Some(interval.end_seconds);
        match interval.outcome {
            MotionOutcome::Estimated(motion) => {
                let sample = FeatureInterval {
                    start_seconds: interval.start_seconds,
                    end_seconds: interval.end_seconds,
                    value: motion.horizontal_per_second,
                    quality: motion.inlier_fraction * motion.inlier_coverage,
                    support_seconds: interval.end_seconds - interval.start_seconds,
                    span: self.span,
                };
                self.turns.push(sample);
                // Image expansion/residual is a weak translation proxy, never m/s.
                self.speed.push(FeatureInterval {
                    value: motion
                        .expansion_per_second
                        .abs()
                        .hypot(motion.residual_per_second),
                    ..sample
                });
            }
            outcome => {
                self.span += 1;
                if matches!(
                    outcome,
                    MotionOutcome::Unavailable(
                        super::motion_estimation::QualityReason::TimestampGap
                            | super::motion_estimation::QualityReason::GeometryChange
                    )
                ) {
                    issue(
                        &mut self.issues,
                        ChannelKind::SignedTurn,
                        Some(interval.start_seconds),
                        SupportReason::VideoDiscontinuity,
                    );
                }
                issue(
                    &mut self.issues,
                    ChannelKind::SpeedTrend,
                    Some(interval.start_seconds),
                    SupportReason::UntrackableVideo,
                );
            }
        }
    }
    pub fn finish(self) -> Fingerprint {
        let mut fingerprint = finish(self.turns, self.speed, self.issues, false, false);
        fingerprint.time_range = self
            .start_seconds
            .zip(self.previous_end)
            .map(|(start, end)| start..end);
        fingerprint
    }
}

fn finish(
    turns: Vec<FeatureInterval>,
    speed: Vec<FeatureInterval>,
    issues: Vec<SupportIssue>,
    activity: bool,
    repeated_gps: bool,
) -> Fingerprint {
    let magnitude = turns
        .iter()
        .map(|s| FeatureInterval {
            value: s.value.abs(),
            ..*s
        })
        .collect();
    let changes = chunked_slopes(&speed);
    let stops = speed
        .iter()
        .map(|s| FeatureInterval {
            value: if s.value <= if activity { 0.5 } else { 0.002 } {
                1.0
            } else {
                0.0
            },
            ..*s
        })
        .collect::<Vec<_>>();
    let mut events = turn_events(&turns);
    events.extend(stop_events(&stops));
    events.sort_by(|a, b| a.start_seconds.total_cmp(&b.start_seconds));
    Fingerprint {
        time_range: None,
        channels: vec![
            Channel {
                kind: ChannelKind::SignedTurn,
                intervals: turns,
            },
            Channel {
                kind: ChannelKind::TurnMagnitude,
                intervals: magnitude,
            },
            Channel {
                kind: ChannelKind::SpeedTrend,
                intervals: speed,
            },
            Channel {
                kind: ChannelKind::SpeedChange,
                intervals: changes,
            },
            Channel {
                kind: ChannelKind::LowMotion,
                intervals: stops,
            },
        ],
        events,
        issues,
        processed_activity: activity,
        repeated_gps,
    }
}

// Time-weighted local linear regression over a symmetric one-second window.
// Fits interval integrals rather than row differences, avoiding dense quantization spikes.
fn chunked_slopes(samples: &[FeatureInterval]) -> Vec<FeatureInterval> {
    let mut result = Vec::new();
    let mut first = 0;
    while first < samples.len() {
        let end_time = samples[first].start_seconds + 120.0;
        let last = samples.partition_point(|s| s.start_seconds < end_time);
        let radius = samples[first..last]
            .iter()
            .map(|s| s.support_seconds)
            .fold(0.5, f64::max);
        let left =
            samples.partition_point(|s| s.end_seconds <= samples[first].start_seconds - radius);
        let right =
            samples.partition_point(|s| s.start_seconds < samples[last - 1].end_seconds + radius);
        result.extend(local_slopes(&samples[left..right]).into_iter().filter(|s| {
            s.start_seconds >= samples[first].start_seconds && s.start_seconds < end_time
        }));
        first = last;
    }
    result
}

fn local_slopes(samples: &[FeatureInterval]) -> Vec<FeatureInterval> {
    samples
        .iter()
        .filter_map(|sample| {
            let center = (sample.start_seconds + sample.end_seconds) * 0.5;
            let radius = sample.support_seconds.max(0.5);
            let left = center - radius;
            let right = center + radius;
            let first = samples.partition_point(|s| s.end_seconds <= left);
            let mut sums = [0.0; 5];
            let mut support: f64 = 0.0;
            for s in samples[first..]
                .iter()
                .take_while(|s| s.start_seconds < right)
            {
                if s.span != sample.span {
                    return None;
                }
                let a = s.start_seconds.max(left) - center;
                let b = s.end_seconds.min(right) - center;
                let w = b - a;
                sums[0] += w;
                sums[1] += (b * b - a * a) / 2.0;
                sums[2] += (b.powi(3) - a.powi(3)) / 3.0;
                sums[3] += w * s.value;
                sums[4] += (b * b - a * a) / 2.0 * s.value;
                support = support.max(s.support_seconds);
            }
            if sums[0] < 2.0 * radius - 1e-8 {
                return None;
            }
            let slope =
                (sums[4] - sums[1] * sums[3] / sums[0]) / (sums[2] - sums[1].powi(2) / sums[0]);
            Some(FeatureInterval {
                value: slope,
                support_seconds: support.max(radius),
                ..*sample
            })
        })
        .collect()
}

fn turn_events(samples: &[FeatureInterval]) -> Vec<MotionEvent> {
    collect_events(
        samples,
        |s| {
            if s.value.abs() < 0.03 {
                None
            } else if s.value < 0.0 {
                Some(EventKind::TurnLeft)
            } else {
                Some(EventKind::TurnRight)
            }
        },
        0.0,
    )
}

fn stop_events(samples: &[FeatureInterval]) -> Vec<MotionEvent> {
    collect_events(
        samples,
        |s| (s.value == 1.0).then_some(EventKind::Stop),
        2.0,
    )
}

fn collect_events(
    samples: &[FeatureInterval],
    classify: impl Fn(&FeatureInterval) -> Option<EventKind>,
    minimum_duration: f64,
) -> Vec<MotionEvent> {
    let mut events: Vec<MotionEvent> = Vec::new();
    let mut previous_span = None;
    for s in samples {
        let Some(kind) = classify(s) else {
            previous_span = None;
            continue;
        };
        if let Some(last) = events.last_mut().filter(|e| {
            e.kind == kind && e.end_seconds == s.start_seconds && previous_span == Some(s.span)
        }) {
            last.end_seconds = s.end_seconds;
            last.strength += s.value.abs() * (s.end_seconds - s.start_seconds);
            last.quality = last.quality.min(s.quality);
            last.uncertainty_seconds = last.uncertainty_seconds.max(s.support_seconds);
        } else {
            events.push(MotionEvent {
                kind,
                start_seconds: s.start_seconds,
                end_seconds: s.end_seconds,
                strength: s.value.abs() * (s.end_seconds - s.start_seconds),
                quality: s.quality,
                uncertainty_seconds: s.support_seconds,
            });
        }
        previous_span = Some(s.span);
    }
    events.retain(|e| e.end_seconds - e.start_seconds >= minimum_duration);
    events
}

#[derive(Debug)]
pub struct FeatureBin {
    pub value: f64,
    pub valid_duration_seconds: f64,
    pub quality: f64,
    pub effective_sample_count: f64,
    pub support_seconds: f64,
}

#[derive(Debug)]
pub struct ChannelLevel {
    pub kind: ChannelKind,
    /// None masks unsupported bins, filter edges, gaps and excessive requested rate.
    pub bins: Vec<Option<FeatureBin>>,
}

#[derive(Debug)]
pub struct FingerprintLevel {
    pub start_seconds: f64,
    pub bin_seconds: f64,
    pub channels: Vec<ChannelLevel>,
}

#[derive(Debug)]
pub struct SharedFingerprintLevel {
    pub activity: FingerprintLevel,
    pub video: FingerprintLevel,
}

/// Select supported refinement levels for candidate windows. Both sources use
/// identical bandwidth; masks retain slower local sections at slower levels.
/// A level is retained only when at least one channel has support in both inputs.
pub fn shared_levels(
    activity: &Fingerprint,
    video: &Fingerprint,
    activity_window: std::ops::Range<f64>,
    video_window: std::ops::Range<f64>,
) -> AnalysisResult<Vec<SharedFingerprintLevel>> {
    let mut levels = Vec::new();
    for width in [0.025, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 4.0, 8.0] {
        let activity = prepare_level(activity, activity_window.start, activity_window.end, width)?;
        let video = prepare_level(video, video_window.start, video_window.end, width)?;
        if activity
            .channels
            .iter()
            .zip(&video.channels)
            .any(|(a, v)| a.bins.iter().any(Option::is_some) && v.bins.iter().any(Option::is_some))
        {
            levels.push(SharedFingerprintLevel { activity, video });
        }
    }
    Ok(levels)
}

/// Shared settings for either source; a finer grid never promotes low-rate support.
/// Start/end remain in that source's time origin, including nonzero video windows.
pub fn prepare_level(
    fingerprint: &Fingerprint,
    start_seconds: f64,
    end_seconds: f64,
    bin_seconds: f64,
) -> AnalysisResult<FingerprintLevel> {
    if !start_seconds.is_finite()
        || !end_seconds.is_finite()
        || !bin_seconds.is_finite()
        || start_seconds < 0.0
        || end_seconds <= start_seconds
        || bin_seconds < 0.025
        || (end_seconds - start_seconds) / bin_seconds > 1_000_000.0
    {
        return Err(AnalysisError::Settings("fingerprint window must be finite and increasing, bin >= 0.025 s, at most one million bins".into()));
    }
    let count = ((end_seconds - start_seconds) / bin_seconds).ceil() as usize;
    let channels = fingerprint
        .channels
        .iter()
        .map(|channel| ChannelLevel {
            kind: channel.kind,
            bins: (0..count)
                .map(|i| {
                    let start = start_seconds + i as f64 * bin_seconds;
                    aggregate(
                        &channel.intervals,
                        start,
                        (start + bin_seconds).min(end_seconds),
                        bin_seconds,
                    )
                })
                .collect(),
        })
        .collect();
    Ok(FingerprintLevel {
        start_seconds,
        bin_seconds,
        channels,
    })
}

/// Coarse search and broad-pattern summaries use the same filter as refinement.
pub fn coarse_levels(
    fingerprint: &Fingerprint,
    start_seconds: f64,
    end_seconds: f64,
) -> AnalysisResult<Vec<FingerprintLevel>> {
    let mut widths = vec![1.0, 2.0, 4.0];
    if fingerprint
        .channels
        .iter()
        .flat_map(|c| &c.intervals)
        .any(|s| {
            s.end_seconds > start_seconds
                && s.start_seconds < end_seconds
                && s.support_seconds > 4.0
        })
    {
        widths.push(8.0);
    }
    widths
        .into_iter()
        .map(|width| prepare_level(fingerprint, start_seconds, end_seconds, width))
        .collect()
}

// Integral of a unit-area triangular kernel of radius r. Its -3 dB cutoff is
// below destination Nyquist when r = bin width. Symmetric offline filtering has
// no time shift. Full kernel support is mandatory: no padding or gap bridging.
#[cfg(test)]
fn triangle_integral(x: f64, radius: f64) -> f64 {
    let x = (x / radius).clamp(-1.0, 1.0);
    if x < 0.0 {
        0.5 * (x + 1.0).powi(2)
    } else {
        1.0 - 0.5 * (1.0 - x).powi(2)
    }
}

#[cfg(test)]
fn filtered(samples: &[FeatureInterval], time: f64, radius: f64) -> Option<(f64, f64, f64)> {
    let left = time - radius;
    let right = time + radius;
    let first = samples.partition_point(|s| s.end_seconds <= left);
    let mut end = left;
    let mut span = None;
    let mut value = 0.0;
    let mut quality = 0.0;
    let mut support: f64 = 0.0;
    for s in samples[first..]
        .iter()
        .take_while(|s| s.start_seconds < right)
    {
        if s.start_seconds > end + 1e-8
            || span.is_some_and(|span| span != s.span)
            || s.support_seconds > radius + 1e-8
        {
            return None;
        }
        let a = s.start_seconds.max(left);
        let b = s.end_seconds.min(right);
        let weight = triangle_integral(b - time, radius) - triangle_integral(a - time, radius);
        value += weight * s.value;
        quality += weight * s.quality;
        support = support.max(s.support_seconds);
        end = b;
        span = Some(s.span);
    }
    (end >= right - 1e-8).then_some((value, quality, support))
}

fn aggregate(samples: &[FeatureInterval], start: f64, end: f64, width: f64) -> Option<FeatureBin> {
    // Integrate the symmetric filter analytically over each supported center span.
    // Partial-bin support is exact elapsed duration, never a count of test points.
    let first = samples.partition_point(|s| s.end_seconds <= start - width);
    let last = samples.partition_point(|s| s.start_seconds < end + width);
    let samples = &samples[first..last];
    let mut valid = 0.0;
    let mut value = 0.0;
    let mut quality = 0.0;
    let mut cursor = 0;
    while cursor < samples.len() {
        if samples[cursor].support_seconds > width + 1e-8 {
            cursor += 1;
            continue;
        }
        let first = cursor;
        cursor += 1;
        while cursor < samples.len()
            && samples[cursor].span == samples[first].span
            && (samples[cursor].start_seconds - samples[cursor - 1].end_seconds).abs() <= 1e-8
            && samples[cursor].support_seconds <= width + 1e-8
        {
            cursor += 1;
        }
        let left = start.max(samples[first].start_seconds + width);
        let right = end.min(samples[cursor - 1].end_seconds - width);
        if right <= left {
            continue;
        }
        valid += right - left;
        for s in &samples[first..cursor] {
            let weight = triangle_second_integral(s.end_seconds - left, width)
                - triangle_second_integral(s.end_seconds - right, width)
                - triangle_second_integral(s.start_seconds - left, width)
                + triangle_second_integral(s.start_seconds - right, width);
            value += weight * s.value;
            quality += weight * s.quality;
        }
    }
    if valid < (end - start) * 0.75 {
        return None;
    }
    Some(FeatureBin {
        value: value / valid,
        quality: quality / valid,
        valid_duration_seconds: valid,
        effective_sample_count: valid / width,
        support_seconds: width,
    })
}

fn triangle_second_integral(x: f64, radius: f64) -> f64 {
    if x <= -radius {
        0.0
    } else if x >= radius {
        x
    } else if x < 0.0 {
        (x + radius).powi(3) / (6.0 * radius.powi(2))
    } else {
        x + (radius - x).powi(3) / (6.0 * radius.powi(2))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synchronization::motion_estimation::{MotionEstimate, QualityReason};
    use crate::synchronization::optical_flow::TrackingQuality;

    fn activity(times: Vec<f64>) -> ParsedActivity {
        let mut activity: ParsedActivity = serde_json::from_str("{}").unwrap();
        activity.extra.insert(
            "coverage".into(),
            serde_json::json!({ "speed": { "source": "direct" } }),
        );
        activity.speed = times.iter().map(|t| Some(5.0 + t * 0.2)).collect();
        activity.heading = times
            .iter()
            .map(|t| Some((359.0 + t * 2.0) % 360.0))
            .collect();
        activity.course = times
            .iter()
            .map(|t| (Some(52.0 + t * 0.0001), Some(13.0)))
            .collect();
        activity.sample_elapsed_seconds = times;
        activity
    }

    fn channel(fingerprint: &Fingerprint, kind: ChannelKind) -> &[FeatureInterval] {
        fingerprint
            .channels
            .iter()
            .find(|c| c.kind == kind)
            .unwrap()
            .intervals()
    }

    fn samples(rate: usize, seconds: usize, value: impl Fn(f64) -> f64) -> Vec<FeatureInterval> {
        (0..rate * seconds)
            .map(|i| {
                let start = i as f64 / rate as f64;
                let end = (i + 1) as f64 / rate as f64;
                FeatureInterval {
                    start_seconds: start,
                    end_seconds: end,
                    value: value((start + end) * 0.5),
                    quality: 1.0,
                    support_seconds: 1.0 / rate as f64,
                    span: 0,
                }
            })
            .collect()
    }

    #[test]
    fn saved_activity_needs_no_new_fields_and_keeps_origin() {
        let input = activity((10..30).map(f64::from).collect());
        let saved = serde_json::to_string(&input).unwrap();
        let restored: ParsedActivity = serde_json::from_str(&saved).unwrap();
        let before = serde_json::to_string(&restored).unwrap();
        let fingerprint = prepare_activity(&restored);
        assert_eq!(
            channel(&fingerprint, ChannelKind::SpeedTrend)[0].start_seconds,
            10.0
        );
        assert_eq!(serde_json::to_string(&restored).unwrap(), before);
        assert!(fingerprint.processed_activity);
        assert_eq!(coarse_levels(&fingerprint, 10.0, 29.0).unwrap().len(), 3);
    }

    #[test]
    fn circular_heading_uses_actual_elapsed_interval() {
        let fingerprint = prepare_activity(&activity(vec![0.0, 0.5, 1.5, 2.0]));
        let turns = channel(&fingerprint, ChannelKind::SignedTurn);
        assert!(!turns.is_empty());
        for turn in turns {
            assert!((turn.value - 2.0_f64.to_radians()).abs() < 1e-10);
        }
    }

    #[test]
    fn sparse_and_repeated_gps_do_not_become_forty_hz_heading_evidence() {
        for repeated in [false, true] {
            let mut input = activity((0..401).map(|i| i as f64 / 40.0).collect());
            for (i, point) in input.course.iter_mut().enumerate() {
                *point = if repeated || i % 40 == 0 {
                    (Some(52.0 + (i / 40) as f64 * 0.0001), Some(13.0))
                } else {
                    (None, None)
                };
            }
            let fingerprint = prepare_activity(&input);
            let turns = channel(&fingerprint, ChannelKind::SignedTurn);
            assert!(!turns.is_empty());
            assert!(turns.iter().all(|s| s.support_seconds >= 1.0));
            assert_eq!(fingerprint.repeated_gps, repeated);
            let fine = prepare_level(&fingerprint, 0.0, 10.0, 0.1).unwrap();
            assert!(fine.channels[0].bins.iter().all(Option::is_none));
            let coarse = prepare_level(&fingerprint, 0.0, 10.0, 1.0).unwrap();
            assert!(coarse.channels[0].bins[3].is_some());
        }
    }

    #[test]
    fn unavailable_gps_and_stationary_positions_mask_heading() {
        for course in [vec![], vec![(Some(52.0), Some(13.0)); 20]] {
            let mut input = activity((0..20).map(f64::from).collect());
            input.course = course;
            let fingerprint = prepare_activity(&input);
            assert!(channel(&fingerprint, ChannelKind::SignedTurn).is_empty());
            assert!(!channel(&fingerprint, ChannelKind::SpeedTrend).is_empty());
        }
    }

    #[test]
    fn external_anomalies_do_not_bridge_gaps_or_duplicate_time() {
        let mut input = activity(vec![0.0, 1.0, 2.0, 2.0, 1.0, 3.0, 4.0, 30.0, 31.0]);
        input.speed[6] = Some(f64::NAN);
        input.course[7] = (Some(-52.0), Some(13.0));
        let fingerprint = prepare_activity(&input);
        let speed = channel(&fingerprint, ChannelKind::SpeedTrend);
        assert!(speed.iter().all(|s| s.end_seconds - s.start_seconds <= 1.0));
        assert!(speed.iter().all(|s| s.start_seconds != 2.0));
        assert!(fingerprint
            .issues
            .iter()
            .any(|i| i.reason == SupportReason::TimestampDiscontinuity));
        assert!(fingerprint
            .issues
            .iter()
            .any(|i| i.reason == SupportReason::InvalidValue));
        assert!(fingerprint
            .issues
            .iter()
            .any(|i| i.reason == SupportReason::GpsJump));
    }

    #[test]
    fn local_speed_fit_handles_dense_quantization_without_row_derivatives() {
        let quantized = samples(40, 10, |t| t.floor());
        let slopes = local_slopes(&quantized);
        assert!(!slopes.is_empty());
        assert!(slopes.iter().all(|s| s.value.abs() < 1.6));
        assert!(slopes.iter().all(|s| s.support_seconds >= 0.5));
        assert!(local_slopes(&samples(40, 10, |_| 5.0))
            .iter()
            .all(|s| s.value.abs() < 1e-10));
    }

    #[test]
    fn filtering_is_duration_weighted_and_suppresses_above_nyquist() {
        let dense = samples(40, 10, |t| {
            if (t * 4.0).floor() as usize % 2 == 0 {
                1.0
            } else {
                -1.0
            }
        });
        let bin = aggregate(&dense, 4.0, 5.0, 1.0).unwrap();
        assert!(bin.value.abs() < 1e-10);
        let irregular = vec![
            FeatureInterval {
                start_seconds: 0.0,
                end_seconds: 3.0,
                value: 2.0,
                quality: 1.0,
                support_seconds: 1.0,
                span: 0,
            },
            FeatureInterval {
                start_seconds: 3.0,
                end_seconds: 3.1,
                value: 4.0,
                quality: 1.0,
                support_seconds: 1.0,
                span: 0,
            },
            FeatureInterval {
                start_seconds: 3.1,
                end_seconds: 6.0,
                value: 4.0,
                quality: 1.0,
                support_seconds: 1.0,
                span: 0,
            },
        ];
        assert!((filtered(&irregular, 3.0, 1.0).unwrap().0 - 3.0).abs() < 1e-10);
        assert!(bin.effective_sample_count <= 1.0);
    }

    #[test]
    fn symmetric_filter_masks_edges_gaps_and_span_boundaries() {
        let mut signal = samples(10, 10, |_| 2.0);
        assert!(filtered(&signal, 0.0, 1.0).is_none());
        assert!(filtered(&signal, 5.0, 1.0).is_some());
        signal[50].span = 1;
        assert!(filtered(&signal, 5.0, 1.0).is_none());
        signal.remove(50);
        assert!(filtered(&signal, 5.0, 1.0).is_none());
    }

    #[test]
    fn native_events_survive_coarse_averaging_and_gaps_break_stops() {
        let mut turns = samples(40, 4, |t| if (1.0..1.1).contains(&t) { 0.8 } else { 0.0 });
        let events = turn_events(&turns);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].start_seconds, 1.0);
        assert_eq!(events[0].end_seconds, 1.1);
        for s in &mut turns {
            s.value = 1.0;
        }
        assert_eq!(stop_events(&turns).len(), 1);
        turns.retain(|s| s.start_seconds < 1.5 || s.start_seconds >= 2.5);
        assert!(stop_events(&turns).is_empty());
    }

    #[test]
    fn local_cadence_changes_do_not_force_a_global_rate() {
        let mut times = (0..10).map(f64::from).collect::<Vec<_>>();
        times.extend((0..401).map(|i| 10.0 + i as f64 / 40.0));
        let fingerprint = prepare_activity(&activity(times));
        let fine = prepare_level(&fingerprint, 0.0, 20.0, 0.1).unwrap();
        let speed = &fine.channels[2].bins;
        assert!(speed[20].is_none());
        assert!(speed[150].is_some());
    }

    #[test]
    fn malformed_level_settings_fail_loudly() {
        let fingerprint = prepare_activity(&activity(vec![]));
        for (start, end, width) in [
            (0.0, 10.0, 0.0),
            (0.0, 10.0, f64::NAN),
            (2.0, 1.0, 1.0),
            (-1.0, 1.0, 1.0),
            (0.0, f64::INFINITY, 1.0),
        ] {
            assert!(matches!(
                prepare_level(&fingerprint, start, end, width),
                Err(AnalysisError::Settings(_))
            ));
        }
    }

    fn video_interval(start: f64, outcome: MotionOutcome) -> MotionInterval {
        MotionInterval {
            start_seconds: start,
            end_seconds: start + 0.1,
            outcome,
        }
    }

    fn estimate() -> MotionOutcome {
        MotionOutcome::Estimated(MotionEstimate {
            horizontal_per_second: 0.2,
            vertical_per_second: 0.0,
            roll_per_second: 0.0,
            expansion_per_second: 0.0,
            residual_per_second: 0.0,
            inliers: 20,
            inlier_fraction: 0.8,
            inlier_coverage: 0.75,
            rms_residual: 0.0,
            tracking: TrackingQuality {
                detected: 25,
                retained: 20,
                coverage: 0.75,
                mean_forward_backward_error_pixels: 0.0,
            },
        })
    }

    #[test]
    fn video_quality_failures_are_unknown_and_window_pts_are_preserved() {
        let fingerprint = prepare_video([
            video_interval(20.0, estimate()),
            video_interval(
                20.1,
                MotionOutcome::Unavailable(QualityReason::DuplicateFrame),
            ),
            video_interval(20.2, estimate()),
        ]);
        let speed = channel(&fingerprint, ChannelKind::SpeedTrend);
        assert_eq!(speed.len(), 2);
        assert_eq!(speed[0].start_seconds, 20.0);
        assert_ne!(speed[0].span, speed[1].span);
        assert!((speed[0].quality - 0.6).abs() < 1e-10);
        assert!(fingerprint.events.iter().all(|e| e.kind != EventKind::Stop));
    }

    #[test]
    fn sparse_metrics_use_their_own_timestamps_and_mask_long_null_gaps() {
        let mut input = activity((0..801).map(|i| i as f64 / 40.0).collect());
        for i in 0..input.speed.len() {
            if i % 40 != 0 || (240..560).contains(&i) {
                input.speed[i] = None;
                input.heading[i] = None;
            }
        }
        let fingerprint = prepare_activity(&input);
        for kind in [ChannelKind::SpeedTrend, ChannelKind::SignedTurn] {
            let intervals = channel(&fingerprint, kind);
            assert!(!intervals.is_empty());
            assert!(intervals.iter().all(|s| s.support_seconds >= 1.0));
            assert!(intervals
                .iter()
                .all(|s| s.end_seconds <= 5.0 || s.start_seconds >= 14.0));
        }
    }

    #[test]
    fn repeated_value_transitions_keep_the_run_support() {
        let mut input = activity((0..401).map(|i| i as f64 / 40.0).collect());
        input.speed = input
            .sample_elapsed_seconds
            .iter()
            .map(|t| Some(t.floor()))
            .collect();
        let fingerprint = prepare_activity(&input);
        assert!(channel(&fingerprint, ChannelKind::SpeedTrend)
            .iter()
            .all(|s| s.support_seconds >= 0.97));
    }

    #[test]
    fn bin_support_uses_exact_overlap_at_filter_edges() {
        let signal = samples(40, 10, |_| 2.0);
        let bin = aggregate(&signal, 0.81, 1.81, 1.0).unwrap();
        assert!((bin.valid_duration_seconds - 0.81).abs() < 1e-10);
        assert!((bin.value - 2.0).abs() < 1e-10);
        assert!(aggregate(&signal, 0.74, 1.74, 1.0).is_none());
    }

    #[test]
    fn coverage_limits_derived_and_legacy_speed_support() {
        for source in [Some("derived"), Some("mixed"), None] {
            let mut input = activity((0..801).map(|i| i as f64 / 40.0).collect());
            if let Some(source) = source {
                input.extra.insert(
                    "coverage".into(),
                    serde_json::json!({ "speed": { "source": source } }),
                );
            } else {
                input.extra.remove("coverage");
            }
            let fingerprint = prepare_activity(&input);
            assert!(channel(&fingerprint, ChannelKind::SpeedTrend)
                .iter()
                .all(|s| s.support_seconds >= 1.0));
            assert!(fingerprint.issues.iter().any(|i| matches!(
                i.reason,
                SupportReason::UnknownMetricSupport | SupportReason::ProcessedMetricSupport
            )));
        }
    }

    #[test]
    fn shared_refinement_rate_is_capped_by_both_sources() {
        let fast = finish(vec![], samples(40, 20, |t| t), vec![], true, false);
        let video = finish(vec![], samples(10, 20, |t| t), vec![], false, false);
        let levels = shared_levels(&fast, &video, 0.0..20.0, 0.0..20.0).unwrap();
        assert_eq!(levels[0].activity.bin_seconds, 0.1);
        assert_eq!(levels[0].video.bin_seconds, 0.1);
        let slow = finish(vec![], samples(1, 20, |t| t), vec![], true, false);
        let levels = shared_levels(&slow, &video, 0.0..20.0, 0.0..20.0).unwrap();
        assert_eq!(levels[0].activity.bin_seconds, 1.0);
    }

    #[test]
    fn bounded_slope_chunks_preserve_seams_and_source_times() {
        let input = samples(10, 250, |t| (t * 0.13).sin());
        let expected = local_slopes(&input);
        let actual = chunked_slopes(&input);
        assert_eq!(actual.len(), expected.len());
        for (a, b) in actual.iter().zip(expected) {
            assert_eq!(a.start_seconds, b.start_seconds);
            assert!((a.value - b.value).abs() < 1e-12);
        }
    }

    #[test]
    fn matcher_searches_negative_offsets_and_nonzero_video_origins() {
        use super::super::{matcher::match_fingerprints, Cancellation};
        // An exact time translation checks offset arithmetic and the complete
        // search wiring, not real-world matching quality or calibrated confidence.
        let turns = samples(1, 480, |t| {
            0.02 * ((t * 0.07 + t * t * 0.00013).sin() + (t * 0.13).cos())
        });
        let speed = samples(1, 480, |t| 5.0 + (t * 0.37).sin() + (t * 0.09).cos());
        let mut activity = finish(turns.clone(), speed.clone(), vec![], true, false);
        activity.time_range = Some(0.0..480.0);
        let shifted = |data: Vec<FeatureInterval>| {
            data.into_iter()
                .filter(|s| s.start_seconds < 360.0)
                .map(|s| FeatureInterval {
                    start_seconds: s.start_seconds + 20.0,
                    end_seconds: s.end_seconds + 20.0,
                    ..s
                })
                .collect()
        };
        let mut video = finish(shifted(turns), shifted(speed), vec![], false, false);
        video.time_range = Some(20.0..380.0);
        let result = match_fingerprints(&activity, &video, &Cancellation::default()).unwrap();
        let candidates = result.candidates;
        assert!(!candidates.is_empty());
        assert!(
            (candidates[0].offset_seconds + 20.0).abs() <= 1.0,
            "{candidates:?}"
        );
        assert!(candidates[0].accepted, "{:?}", candidates[0]);
    }
}
