//! Active visual synchronization matcher, currently using signed turning.
//!
//! Four-second observed-only bins tolerate at most two seconds of missing data.
//! No interpolation, raw-threshold events, speed evidence or confidence probability.
//! The first two thirds nominate offsets; the final third is held out. Search uses
//! one-second steps and requires the entire video timeline to fit in the activity.
//! Thresholds below are experimental engineering gates, not population calibration.

use std::ops::Range;

use serde::Serialize;

use super::fingerprint::{ChannelKind, FeatureInterval, Fingerprint, SupportReason};
use super::{AnalysisError, AnalysisResult, Cancellation};

const WIDTH: f64 = 4.0;
const MAX_GAP: f64 = 2.0;
const MIN_BIN_FRACTION: f64 = 0.5;
const MIN_VIDEO_DURATION_SECONDS: f64 = 120.0;
const MIN_ABS_OBSERVED_SECONDS: f64 = 30.0;
const MIN_OBSERVED_FRACTION: f64 = 0.35;
const SHORT_CLIP_SECONDS: f64 = 180.0;
const MIN_RETAINED_FRACTION: f64 = 0.8;
const MIN_CORRELATION: f64 = 0.45;
const MIN_SECTION_CORRELATION: f64 = 0.35;
const MIN_MARGIN: f64 = 0.08;
const REGION_SECONDS: f64 = 10.0;
const SECTION_TOLERANCE: f64 = 12.0;

#[derive(Clone, Debug, Serialize)]
pub struct MatchResult {
    pub accepted: bool,
    pub bin_seconds: f64,
    pub evaluated_offsets: usize,
    pub video_observed_seconds: f64,
    pub video_binned_observed_seconds: f64,
    pub candidates: Vec<Candidate>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Candidate {
    pub offset_seconds: f64,
    pub turn_polarity: i8,
    /// Signed Pearson correlation after applying the clip-wide polarity.
    pub correlation: Option<f64>,
    pub nomination_correlation: f64,
    pub nomination_margin: f64,
    pub observed_seconds: f64,
    pub observed_fraction: f64,
    pub retained_video_observation_fraction: f64,
    pub paired_bins: usize,
    pub sections: Vec<SectionEvidence>,
    pub accepted: bool,
    pub rejection_reasons: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SectionEvidence {
    pub video_start_seconds: f64,
    pub video_end_seconds: f64,
    pub held_out: bool,
    pub correlation: Option<f64>,
    pub observed_seconds: f64,
    /// Independent whole-activity section search with the same polarity.
    pub best_offset_seconds: Option<f64>,
    pub agrees: bool,
}

struct Bin {
    value: f64,
    quality: f64,
    /// Actual observed ranges relative to the bin start, never padded support.
    observed: Vec<Range<f64>>,
}

impl Bin {
    fn duration(&self) -> f64 {
        self.observed.iter().map(|r| r.end - r.start).sum()
    }
}

fn aggregate(samples: &[FeatureInterval], start: f64) -> Option<Bin> {
    let end = start + WIDTH;
    let first = samples.partition_point(|s| s.end_seconds <= start);
    let mut observed = Vec::<Range<f64>>::new();
    let mut value = 0.0;
    let mut quality = 0.0;
    let mut duration = 0.0;
    let mut previous: Option<&FeatureInterval> = None;
    for s in samples[first..]
        .iter()
        .take_while(|s| s.start_seconds < end)
    {
        if s.support_seconds > WIDTH {
            continue;
        }
        // A span boundary with no missing interval is a discontinuity, not a
        // small observation hole. It cannot be averaged across.
        if previous
            .is_some_and(|p| p.span != s.span && (s.start_seconds - p.end_seconds).abs() <= 1e-8)
        {
            return None;
        }
        let left = s.start_seconds.max(start) - start;
        let right = s.end_seconds.min(end) - start;
        let last_end = observed.last().map_or(0.0, |r| r.end);
        if left - last_end > MAX_GAP + 1e-8 {
            return None;
        }
        let weight = right - left;
        duration += weight;
        value += weight * s.value;
        quality += weight * s.quality;
        if let Some(last) = observed.last_mut().filter(|r| (r.end - left).abs() <= 1e-8) {
            last.end = right;
        } else {
            observed.push(left..right);
        }
        previous = Some(s);
    }
    if duration < WIDTH * MIN_BIN_FRACTION || WIDTH - observed.last()?.end > MAX_GAP + 1e-8 {
        return None;
    }
    Some(Bin {
        value: value / duration,
        quality: quality / duration,
        observed,
    })
}

fn intersection(a: &Bin, b: &Bin) -> f64 {
    let (mut i, mut j) = (0, 0);
    let mut duration = 0.0;
    while i < a.observed.len() && j < b.observed.len() {
        let x = &a.observed[i];
        let y = &b.observed[j];
        duration += (x.end.min(y.end) - x.start.max(y.start)).max(0.0);
        if x.end < y.end {
            i += 1
        } else {
            j += 1
        }
    }
    duration
}

#[derive(Clone, Default)]
struct Moments {
    weight: f64,
    x: f64,
    y: f64,
    xx: f64,
    yy: f64,
    xy: f64,
    duration: f64,
    bins: usize,
}

impl Moments {
    fn add(&mut self, a: &Bin, v: &Bin) {
        let duration = intersection(a, v);
        if duration <= 0.0 {
            return;
        }
        let weight = duration * a.quality.min(v.quality);
        self.weight += weight;
        self.x += weight * a.value;
        self.y += weight * v.value;
        self.xx += weight * a.value * a.value;
        self.yy += weight * v.value * v.value;
        self.xy += weight * a.value * v.value;
        self.duration += duration;
        self.bins += 1;
    }

    fn combine(&self, other: &Self) -> Self {
        Self {
            weight: self.weight + other.weight,
            x: self.x + other.x,
            y: self.y + other.y,
            xx: self.xx + other.xx,
            yy: self.yy + other.yy,
            xy: self.xy + other.xy,
            duration: self.duration + other.duration,
            bins: self.bins + other.bins,
        }
    }

    fn correlation(&self) -> Option<f64> {
        if self.weight <= 0.0 || self.bins < 8 {
            return None;
        }
        let vx = self.xx - self.x * self.x / self.weight;
        let vy = self.yy - self.y * self.y / self.weight;
        if vx <= 1e-10 * self.xx.max(self.weight) || vy <= 1e-10 * self.yy.max(self.weight) {
            return None;
        }
        Some(((self.xy - self.x * self.y / self.weight) / (vx * vy).sqrt()).clamp(-1.0, 1.0))
    }
}

struct Score {
    offset: f64,
    sections: [Moments; 3],
}

fn turns(input: &Fingerprint) -> &[FeatureInterval] {
    input
        .channels()
        .iter()
        .find(|c| c.kind == ChannelKind::SignedTurn)
        .expect("fingerprint must contain the canonical signed-turn channel")
        .intervals()
}

fn supported(m: &Moments, duration: f64, available: f64, minimum: f64) -> bool {
    m.duration >= minimum
        && m.duration >= duration * MIN_OBSERVED_FRACTION
        && m.duration >= available * MIN_RETAINED_FRACTION
}

fn discontinuities(input: &Fingerprint) -> Vec<f64> {
    let mut times: Vec<_> = input
        .issues
        .iter()
        .filter(|issue| {
            issue.channel == ChannelKind::SignedTurn
                && matches!(
                    issue.reason,
                    SupportReason::InvalidValue
                        | SupportReason::GpsJump
                        | SupportReason::VideoDiscontinuity
                )
                || issue.reason == SupportReason::TimestampDiscontinuity
        })
        .filter_map(|issue| issue.at_seconds)
        .collect();
    times.sort_by(f64::total_cmp);
    times
}

fn prepare_bin(samples: &[FeatureInterval], breaks: &[f64], start: f64) -> Option<Bin> {
    let index = breaks.partition_point(|&time| time <= start);
    if breaks.get(index).is_some_and(|&time| time < start + WIDTH) {
        return None;
    }
    aggregate(samples, start)
}

/// Evaluate signed turning alone with bounded missing observations and held-out
/// verification. Empty external timelines produce an inconclusive report.
pub fn match_fingerprints(
    activity: &Fingerprint,
    video: &Fingerprint,
    cancellation: &Cancellation,
) -> AnalysisResult<MatchResult> {
    if cancellation.is_cancelled() {
        return Err(AnalysisError::Cancelled);
    }
    let mut result = MatchResult {
        accepted: false,
        bin_seconds: WIDTH,
        evaluated_offsets: 0,
        video_observed_seconds: turns(video)
            .iter()
            .map(|s| s.end_seconds - s.start_seconds)
            .sum(),
        video_binned_observed_seconds: 0.0,
        candidates: Vec::new(),
    };
    let (Some(ab), Some(vb)) = (&activity.time_range, &video.time_range) else {
        return Ok(result);
    };
    let duration = vb.end - vb.start;
    if ab.end - ab.start < duration || duration < MIN_VIDEO_DURATION_SECONDS {
        return Ok(result);
    }
    let count = (duration / WIDTH).floor() as usize;
    let short_clip = duration < SHORT_CLIP_SECONDS;
    let video_breaks = discontinuities(video);
    let activity_breaks = discontinuities(activity);
    let video_bins: Vec<_> = (0..count)
        .map(|i| prepare_bin(turns(video), &video_breaks, vb.start + i as f64 * WIDTH))
        .collect();
    let section_for = |i: usize| if short_clip { 0 } else { (i * 3 / count).min(2) };
    let mut available = [0.0; 3];
    for (i, bin) in video_bins.iter().enumerate() {
        if let Some(bin) = bin {
            available[section_for(i)] += bin.duration();
        }
    }
    result.video_binned_observed_seconds = available.iter().sum();
    let mut activity_bins = Vec::new();
    for i in 0..=((ab.end - ab.start - WIDTH).floor() as usize) {
        if cancellation.is_cancelled() {
            return Err(AnalysisError::Cancelled);
        }
        activity_bins.push(prepare_bin(
            turns(activity),
            &activity_breaks,
            ab.start + i as f64,
        ));
    }
    let mut scores = Vec::new();
    for shift in 0..=((ab.end - ab.start - duration).floor() as usize) {
        if cancellation.is_cancelled() {
            return Err(AnalysisError::Cancelled);
        }
        let mut sections = std::array::from_fn(|_| Moments::default());
        for (i, v) in video_bins.iter().enumerate() {
            if let (Some(a), Some(v)) = (&activity_bins[shift + i * WIDTH as usize], v) {
                sections[section_for(i)].add(a, v);
            }
        }
        scores.push(Score {
            offset: ab.start - vb.start + shift as f64,
            sections,
        });
    }
    result.evaluated_offsets = scores.len();
    let training_available = available[0] + available[1];
    let training_duration = (0..count).filter(|&i| section_for(i) < 2).count() as f64 * WIDTH;

    // Temporary nomination diagnostic: print the best offsets by support and by correlation.
    let best_support = scores
        .iter()
        .max_by(|a, b| {
            let ta = a.sections[0].combine(&a.sections[1]);
            let tb = b.sections[0].combine(&b.sections[1]);
            ta.duration.partial_cmp(&tb.duration).unwrap()
        })
        .map(|s| {
            let t = s.sections[0].combine(&s.sections[1]);
            (s.offset, t.duration, t.bins, t.correlation())
        });
    let best_corr = scores
        .iter()
        .filter_map(|s| {
            let t = s.sections[0].combine(&s.sections[1]);
            t.correlation().map(|r| (s.offset, t.duration, t.bins, r))
        })
        .max_by(|a, b| a.3.abs().total_cmp(&b.3.abs()));
    let mode_label = if short_clip { "short-clip full" } else { "training" };
    eprintln!(
        "[matcher diagnostic] {mode_label} gates: duration_required={:.1} available={:.1} retained_required={:.1} absolute_min={:.1}",
        training_duration * MIN_OBSERVED_FRACTION,
        training_available,
        training_available * MIN_RETAINED_FRACTION,
        MIN_ABS_OBSERVED_SECONDS * 2.0 / 3.0
    );
    eprintln!("[matcher diagnostic] best by support: {best_support:?}");
    eprintln!("[matcher diagnostic] best by correlation: {best_corr:?}");

    let mut nominated: Vec<_> = scores
        .iter()
        .filter_map(|s| {
            let training = s.sections[0].combine(&s.sections[1]);
            supported(
                &training,
                training_duration,
                training_available,
                MIN_ABS_OBSERVED_SECONDS * 2.0 / 3.0,
            )
            .then(|| training.correlation().map(|r| (s, r)))
            .flatten()
        })
        .collect();
    nominated.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()));
    let mut peaks: Vec<(&Score, f64)> = Vec::new();
    for entry in nominated {
        if peaks
            .iter()
            .all(|p| (p.0.offset - entry.0.offset).abs() > REGION_SECONDS)
        {
            peaks.push(entry);
        }
        if peaks.len() == 20 {
            break;
        }
    }
    for (index, &(score, training)) in peaks.iter().enumerate() {
        let polarity = if training < 0.0 { -1 } else { 1 };
        let sign = polarity as f64;
        let rival = peaks
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != index)
            .map(|(_, p)| p.1.abs())
            .reduce(f64::max);
        let margin = training.abs() - rival.unwrap_or(0.0);
        let all = score.sections[0]
            .combine(&score.sections[1])
            .combine(&score.sections[2]);
        let correlation = all.correlation().map(|r| r * sign);
        let mut reasons = Vec::new();
        if !supported(
            &all,
            duration,
            result.video_binned_observed_seconds,
            MIN_ABS_OBSERVED_SECONDS,
        ) {
            reasons.push("insufficient_observed_duration");
        }
        if !correlation.is_some_and(|r| r >= MIN_CORRELATION) {
            reasons.push("weak_turning_agreement");
        }
        if margin < MIN_MARGIN {
            reasons.push("ambiguous_offset");
        }
        let mut sections = Vec::new();
        if short_clip {
            // Short clip: treat the whole video as one bootprint, no held-out verification.
            sections.push(SectionEvidence {
                video_start_seconds: vb.start,
                video_end_seconds: vb.start + duration,
                held_out: false,
                correlation,
                observed_seconds: all.duration,
                best_offset_seconds: Some(score.offset),
                agrees: true,
            });
        } else {
            for (section, &section_available) in available.iter().enumerate() {
                let first = (0..count).find(|&i| section_for(i) == section).unwrap();
                let last = (first..count)
                    .take_while(|&i| section_for(i) == section)
                    .last()
                    .unwrap()
                    + 1;
                let section_duration = (last - first) as f64 * WIDTH;
                let best = scores
                    .iter()
                    .filter_map(|s| {
                        let m = &s.sections[section];
                        supported(m, section_duration, section_available, 20.0)
                            .then(|| m.correlation().map(|r| (s.offset, r * sign)))
                            .flatten()
                    })
                    .max_by(|a, b| a.1.total_cmp(&b.1));
                let m = &score.sections[section];
                let r = m.correlation().map(|r| r * sign);
                let agrees = supported(m, section_duration, section_available, 20.0)
                    && r.is_some_and(|r| r >= MIN_SECTION_CORRELATION)
                    && best
                        .is_some_and(|(offset, _)| (offset - score.offset).abs() <= SECTION_TOLERANCE);
                sections.push(SectionEvidence {
                    video_start_seconds: vb.start + first as f64 * WIDTH,
                    video_end_seconds: vb.start + last as f64 * WIDTH,
                    held_out: section == 2,
                    correlation: r,
                    observed_seconds: m.duration,
                    best_offset_seconds: best.map(|b| b.0),
                    agrees,
                });
            }
            if !sections[0].agrees || !sections[1].agrees {
                reasons.push("inconsistent_nomination_sections");
            }
            if !sections[2].agrees {
                reasons.push("held_out_section_disagrees");
            }
        }
        result.candidates.push(Candidate {
            offset_seconds: score.offset,
            turn_polarity: polarity,
            correlation,
            nomination_correlation: training.abs(),
            nomination_margin: margin,
            observed_seconds: all.duration,
            observed_fraction: all.duration / duration,
            retained_video_observation_fraction: all.duration
                / result.video_binned_observed_seconds,
            paired_bins: all.bins,
            sections,
            accepted: reasons.is_empty(),
            rejection_reasons: reasons,
        });
    }
    result.accepted = result.candidates.iter().any(|c| c.accepted);
    Ok(result)
}

#[cfg(test)]
mod tests;
