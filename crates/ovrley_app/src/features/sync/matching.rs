//! Matching the user's landmarks against the activity's events: eligibility and offset supports,
//! the score and interval consensus. Pure.

use std::cmp::Ordering;
use std::collections::HashMap;
use ovrley_core::media::manual_video_sync::{Landmark, LandmarkKind};
use crate::features::sync::detection::{DETECTED_LOCATION_ID, Detection, TurnDirection};
use crate::format::compare_text;

const USER_TIMING_TOLERANCE_SECONDS: f64 = 2.0;
/// `VIDEO_SYNC_LOCATION_TIMING_TOLERANCE_SECONDS`, also the graph's location band.
pub const LOCATION_TIMING_TOLERANCE_SECONDS: f64 = 10.0;
const CANDIDATE_MERGE_TOLERANCE_SECONDS: f64 = 10.0;
const MAX_CANDIDATES: usize = 5;
const MIN_MEANINGFUL_QUALITY: f64 = 0.2;

/// The two searches (`VIDEO_SYNC_MATCH_SCOPES`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum MatchScope {
    All,
    Location,
}

/// The landmarks that can take part in matching, sorted by video second then id.
#[derive(Clone, Debug, PartialEq)]
pub struct Eligibility<'a> {
    pub eligible: Vec<&'a Landmark>,
    pub can_match_all: bool,
    pub can_match_location: bool,
}

fn text_order(left: &str, right: &str) -> Ordering {
    compare_text(left, right)
}

fn by_time<T>(left: &T, right: &T, time: impl Fn(&T) -> f64, id: impl Fn(&T) -> &str) -> Ordering {
    time(left)
        .partial_cmp(&time(right))
        .unwrap_or(Ordering::Equal)
        .then_with(|| text_order(id(left), id(right)))
}

pub fn eligibility<'a>(
    landmarks: &'a [Landmark],
    detection: Option<&Detection>,
) -> Eligibility<'a> {
    let availability = detection
        .map(|detection| detection.availability)
        .unwrap_or_default();
    let has_location = detection.is_some_and(|detection| detection.location.is_some());
    let mut eligible = Vec::new();
    let mut can_match_location = false;
    for landmark in landmarks {
        match landmark.kind() {
            LandmarkKind::Location => {
                if has_location {
                    can_match_location = true;
                    eligible.push(landmark);
                }
            }
            LandmarkKind::Stop if availability.speed => eligible.push(landmark),
            LandmarkKind::LeftTurn | LandmarkKind::RightTurn if availability.heading => {
                eligible.push(landmark);
            }
            _ => {}
        }
    }
    eligible.sort_by(|left, right| {
        by_time(
            left,
            right,
            |landmark| landmark.video_second(),
            |landmark| landmark.id(),
        )
    });
    Eligibility {
        can_match_all: eligible.len() >= 2,
        can_match_location,
        eligible,
    }
}

/// One typed landmark/event offset interval.
#[derive(Clone, Debug, PartialEq)]
struct OffsetSupport {
    landmark_id: String,
    event_id: String,
    kind: LandmarkKind,
    event_second: f64,
    start_offset: f64,
    end_offset: f64,
    timing_scale_seconds: f64,
}

fn residual(support: &OffsetSupport, offset: f64) -> f64 {
    if offset < support.start_offset {
        support.start_offset - offset
    } else if offset > support.end_offset {
        offset - support.end_offset
    } else {
        0.0
    }
}

/// An event a landmark can match: its id and its ideal interval in activity seconds.
struct EventSpan<'a> {
    id: &'a str,
    start: f64,
    end: f64,
}

fn support(landmark: &Landmark, event: &EventSpan<'_>) -> OffsetSupport {
    let location = landmark.kind() == LandmarkKind::Location;
    OffsetSupport {
        landmark_id: landmark.id().to_string(),
        event_id: event.id.to_string(),
        kind: landmark.kind(),
        event_second: event.start,
        start_offset: event.start - landmark.video_second(),
        end_offset: event.end - landmark.video_second(),
        timing_scale_seconds: if location {
            LOCATION_TIMING_TOLERANCE_SECONDS
        } else {
            USER_TIMING_TOLERANCE_SECONDS
        },
    }
}

fn offset_supports<'a>(
    landmarks: &'a [Landmark],
    detection: &Detection,
) -> (Vec<&'a Landmark>, Vec<OffsetSupport>) {
    let mut stops: Vec<EventSpan> = detection
        .stops
        .iter()
        .map(|stop| EventSpan {
            id: &stop.id,
            start: stop.low_speed_interval.start,
            end: stop.low_speed_interval.end,
        })
        .collect();
    stops.sort_by(|left, right| by_time(left, right, |event| event.start, |event| event.id));
    let turns = |direction: TurnDirection| {
        let mut turns: Vec<EventSpan> = detection
            .turns
            .iter()
            .filter(|turn| turn.direction == direction)
            .map(|turn| EventSpan {
                id: &turn.id,
                start: turn.start,
                end: turn.end,
            })
            .collect();
        turns.sort_by(|left, right| by_time(left, right, |event| event.start, |event| event.id));
        turns
    };
    let (left, right) = (turns(TurnDirection::Left), turns(TurnDirection::Right));
    let eligible = eligibility(landmarks, Some(detection)).eligible;
    let mut supports = Vec::new();
    for landmark in &eligible {
        let events: &[EventSpan] = match landmark.kind() {
            LandmarkKind::Location => {
                let location = detection
                    .location
                    .expect("an eligible location landmark has a detected location");
                supports.push(support(
                    landmark,
                    &EventSpan {
                        id: DETECTED_LOCATION_ID,
                        start: location.time,
                        end: location.time,
                    },
                ));
                continue;
            }
            LandmarkKind::Stop => &stops,
            LandmarkKind::LeftTurn => &left,
            LandmarkKind::RightTurn => &right,
        };
        supports.extend(events.iter().map(|event| support(landmark, event)));
    }
    (eligible, supports)
}

/// A bounded robust quality for a distance outside the ideal interval.
pub fn timing_quality(residual_seconds: f64, timing_scale_seconds: f64) -> f64 {
    let normalized = residual_seconds / timing_scale_seconds;
    1.0 / (1.0 + normalized * normalized)
}

/// The match score's diagnostics.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MatchScore {
    pub matched_count: usize,
    pub eligible_count: usize,
    pub total_quality: f64,
    pub coverage: f64,
    pub match_score: f64,
    pub total_residual_seconds: f64,
}

/// `(residual, quality)` per assigned pair.
pub fn match_score(matches: &[(f64, f64)], eligible_count: usize) -> MatchScore {
    let meaningful: Vec<&(f64, f64)> = matches
        .iter()
        .filter(|(_, quality)| *quality >= MIN_MEANINGFUL_QUALITY)
        .collect();
    let total_quality: f64 = matches.iter().map(|(_, quality)| quality).sum();
    let eligible = eligible_count as f64;
    MatchScore {
        matched_count: meaningful.len(),
        eligible_count,
        total_quality,
        coverage: if eligible_count == 0 {
            0.0
        } else {
            meaningful.len() as f64 / eligible
        },
        match_score: if eligible_count == 0 {
            0.0
        } else {
            (100.0 * total_quality / eligible).round()
        },
        total_residual_seconds: meaningful.iter().map(|(residual, _)| residual).sum(),
    }
}

/// Stronger first.
fn compare_strength(
    left: &MatchScore,
    left_offset: f64,
    right: &MatchScore,
    right_offset: f64,
) -> Ordering {
    let ordering = |value: f64| value.partial_cmp(&0.0).unwrap_or(Ordering::Equal);
    ordering(right.match_score - left.match_score)
        .then(right.matched_count.cmp(&left.matched_count))
        .then(ordering(
            left.total_residual_seconds - right.total_residual_seconds,
        ))
        .then(ordering(left_offset - right_offset))
}

/// `sqrt(1 / MIN_MEANINGFUL_QUALITY - 1)`.
fn max_proposal_normalized_miss() -> f64 {
    (1.0 / MIN_MEANINGFUL_QUALITY - 1.0).sqrt()
}

#[derive(Clone, Debug)]
struct AssignedMatch {
    support: usize,
    residual: f64,
    quality: f64,
}

#[derive(Clone, Debug)]
struct Assignment {
    total_quality: f64,
    total_residual_seconds: f64,
    matches: Vec<AssignedMatch>,
}

fn assignment_key(assignment: &Assignment, supports: &[OffsetSupport]) -> String {
    assignment
        .matches
        .iter()
        .map(|matched| {
            format!(
                "{}:{}",
                supports[matched.support].landmark_id, supports[matched.support].event_id
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

/// `Less` when `candidate` is stronger than `current`.
fn compare_assignments(
    candidate: Option<&Assignment>,
    current: Option<&Assignment>,
    supports: &[OffsetSupport],
) -> Ordering {
    let (Some(candidate), Some(current)) = (candidate, current) else {
        return match (candidate, current) {
            (None, _) => Ordering::Greater,
            _ => Ordering::Less,
        };
    };
    if candidate.total_quality != current.total_quality {
        return current
            .total_quality
            .partial_cmp(&candidate.total_quality)
            .unwrap_or(Ordering::Equal);
    }
    if candidate.total_residual_seconds != current.total_residual_seconds {
        return candidate
            .total_residual_seconds
            .partial_cmp(&current.total_residual_seconds)
            .unwrap_or(Ordering::Equal);
    }
    text_order(
        &assignment_key(candidate, supports),
        &assignment_key(current, supports),
    )
}

fn keep_stronger(
    current: Option<Assignment>,
    candidate: Option<Assignment>,
    supports: &[OffsetSupport],
) -> Option<Assignment> {
    if compare_assignments(candidate.as_ref(), current.as_ref(), supports) == Ordering::Less {
        candidate
    } else {
        current
    }
}

fn proposed_offset(left: &OffsetSupport, right: &OffsetSupport) -> Option<f64> {
    if left.kind == right.kind
        && (left.event_id == right.event_id || left.event_second >= right.event_second)
    {
        return None;
    }
    let overlap_start = left.start_offset.max(right.start_offset);
    let overlap_end = left.end_offset.min(right.end_offset);
    if overlap_start <= overlap_end {
        return Some(f64::midpoint(overlap_start, overlap_end));
    }
    let (earlier, later) = if left.end_offset < right.start_offset {
        (left, right)
    } else {
        (right, left)
    };
    let gap = later.start_offset - earlier.end_offset;
    let combined = earlier.timing_scale_seconds + later.timing_scale_seconds;
    if gap > max_proposal_normalized_miss() * combined {
        return None;
    }
    Some(
        (earlier.end_offset * later.timing_scale_seconds
            + later.start_offset * earlier.timing_scale_seconds)
            / combined,
    )
}

fn supports_of(landmark: &Landmark, supports: &[OffsetSupport]) -> Vec<usize> {
    supports
        .iter()
        .enumerate()
        .filter(|(_, support)| support.landmark_id == landmark.id())
        .map(|(index, _)| index)
        .collect()
}

/// One offset per pair of supports of two landmarks, deduplicated at
/// six decimals and sorted.
fn offset_hypotheses(landmarks: &[&Landmark], supports: &[OffsetSupport]) -> Vec<f64> {
    let by_landmark: Vec<Vec<usize>> = landmarks
        .iter()
        .map(|landmark| supports_of(landmark, supports))
        .collect();
    let mut hypotheses: HashMap<String, f64> = HashMap::new();
    for left_index in 0..landmarks.len() {
        for right_index in left_index + 1..landmarks.len() {
            for &left in &by_landmark[left_index] {
                let left = &supports[left];
                for &right in &by_landmark[right_index] {
                    let right = &supports[right];
                    let maximum_gap = max_proposal_normalized_miss()
                        * (left.timing_scale_seconds + right.timing_scale_seconds);
                    if right.end_offset < left.start_offset - maximum_gap {
                        continue;
                    }
                    if right.start_offset > left.end_offset + maximum_gap {
                        break;
                    }
                    if let Some(offset) = proposed_offset(left, right) {
                        // Keyed to six decimals; a later offset with the same key replaces the earlier one.
                        hypotheses.insert(format!("{offset:.6}"), offset);
                    }
                }
            }
        }
    }
    let mut offsets: Vec<f64> = hypotheses.into_values().collect();
    offsets.sort_by(f64::total_cmp);
    offsets
}

/// One landmark type's assignment problem.
struct AssignmentGroup<'a> {
    landmarks: Vec<&'a Landmark>,
    /// The type's event ids, in the order the supports first name them.
    events: Vec<String>,
    lookup: HashMap<(String, String), usize>,
}

fn assignment_groups<'a>(
    landmarks: &[&'a Landmark],
    supports: &[OffsetSupport],
) -> Vec<AssignmentGroup<'a>> {
    LandmarkKind::ALL
        .into_iter()
        .map(|kind| {
            let mut events: Vec<String> = Vec::new();
            let mut lookup = HashMap::new();
            for (index, support) in supports
                .iter()
                .enumerate()
                .filter(|(_, support)| support.kind == kind)
            {
                lookup.insert(
                    (support.landmark_id.clone(), support.event_id.clone()),
                    index,
                );
                if !events.contains(&support.event_id) {
                    events.push(support.event_id.clone());
                }
            }
            AssignmentGroup {
                landmarks: landmarks
                    .iter()
                    .copied()
                    .filter(|landmark| landmark.kind() == kind)
                    .collect(),
                events,
                lookup,
            }
        })
        .collect()
}

/// The order-preserving assignment with the most quality.
fn assign_group(
    group: &AssignmentGroup<'_>,
    offset: f64,
    supports: &[OffsetSupport],
) -> Assignment {
    let (rows, columns) = (group.landmarks.len() + 1, group.events.len() + 1);
    let mut table: Vec<Vec<Option<Assignment>>> = vec![vec![None; columns]; rows];
    table[0][0] = Some(Assignment {
        total_quality: 0.0,
        total_residual_seconds: 0.0,
        matches: Vec::new(),
    });
    for landmark_index in 0..rows {
        for event_index in 0..columns {
            if landmark_index == 0 && event_index == 0 {
                continue;
            }
            let mut assignment = None;
            if landmark_index > 0 {
                assignment = keep_stronger(
                    assignment,
                    table[landmark_index - 1][event_index].clone(),
                    supports,
                );
            }
            if event_index > 0 {
                assignment = keep_stronger(
                    assignment,
                    table[landmark_index][event_index - 1].clone(),
                    supports,
                );
            }
            if landmark_index > 0 && event_index > 0 {
                let key = (
                    group.landmarks[landmark_index - 1].id().to_string(),
                    group.events[event_index - 1].clone(),
                );
                if let Some(&index) = group.lookup.get(&key) {
                    let previous = table[landmark_index - 1][event_index - 1]
                        .clone()
                        .expect("the diagonal is always assigned");
                    let residual = residual(&supports[index], offset);
                    let quality = timing_quality(residual, supports[index].timing_scale_seconds);
                    let mut matches = previous.matches;
                    matches.push(AssignedMatch {
                        support: index,
                        residual,
                        quality,
                    });
                    let extended = Assignment {
                        total_quality: previous.total_quality + quality,
                        total_residual_seconds: previous.total_residual_seconds + residual,
                        matches,
                    };
                    assignment = keep_stronger(assignment, Some(extended), supports);
                }
            }
            table[landmark_index][event_index] = assignment;
        }
    }
    table[rows - 1][columns - 1]
        .take()
        .expect("the last cell is always assigned")
}

/// How a candidate relates to the location landmark when there is one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocationClassification {
    None,
    Agrees,
    Conflict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateVariant {
    Ordinary,
    LocationConflict,
    LocationOnly,
}

/// One landmark/event pair that supports a candidate.
#[derive(Clone, Debug, PartialEq)]
pub struct Evidence {
    pub landmark_id: String,
    pub event_id: String,
    pub kind: LandmarkKind,
    pub residual_seconds: f64,
    pub quality: f64,
}

/// A candidate offset for the target video.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub variant: CandidateVariant,
    pub offset: f64,
    /// `None` for the location-only candidate.
    pub match_score: Option<f64>,
    pub matched_count: usize,
    pub eligible_count: usize,
    pub evidence: Vec<Evidence>,
    pub location_classification: LocationClassification,
    /// The location landmark a conflicting candidate leaves out.
    pub excluded_landmark_ids: Vec<String>,
}

struct CandidateRecord {
    candidate: Candidate,
    score: MatchScore,
}

fn compare_records(left: &CandidateRecord, right: &CandidateRecord) -> Ordering {
    compare_strength(
        &left.score,
        left.candidate.offset,
        &right.score,
        right.candidate.offset,
    )
}

fn best_assignment(
    offset: f64,
    groups: &[AssignmentGroup<'_>],
    eligible_count: usize,
    supports: &[OffsetSupport],
) -> Option<(Vec<AssignedMatch>, MatchScore)> {
    let matches: Vec<AssignedMatch> = groups
        .iter()
        .flat_map(|group| assign_group(group, offset, supports).matches)
        .collect();
    let pairs: Vec<(f64, f64)> = matches
        .iter()
        .map(|matched| (matched.residual, matched.quality))
        .collect();
    let score = match_score(&pairs, eligible_count);
    (score.matched_count >= 2).then_some((matches, score))
}

fn candidate_record(
    offset: f64,
    matches: &[AssignedMatch],
    score: MatchScore,
    eligible_count: usize,
    supports: &[OffsetSupport],
) -> CandidateRecord {
    let evidence: Vec<Evidence> = matches
        .iter()
        .filter(|matched| matched.quality >= MIN_MEANINGFUL_QUALITY)
        .map(|matched| {
            let support = &supports[matched.support];
            Evidence {
                landmark_id: support.landmark_id.clone(),
                event_id: support.event_id.clone(),
                kind: support.kind,
                residual_seconds: matched.residual,
                quality: matched.quality,
            }
        })
        .collect();
    CandidateRecord {
        candidate: Candidate {
            variant: CandidateVariant::Ordinary,
            offset,
            match_score: Some(score.match_score),
            matched_count: evidence.len(),
            eligible_count,
            evidence,
            location_classification: LocationClassification::None,
            excluded_landmark_ids: Vec::new(),
        },
        score,
    }
}

/// The strongest record within each 10 s run of offsets, strongest first.
fn merge_records(mut records: Vec<CandidateRecord>) -> Vec<CandidateRecord> {
    records.sort_by(|left, right| left.candidate.offset.total_cmp(&right.candidate.offset));
    let mut merged: Vec<CandidateRecord> = Vec::new();
    for record in records {
        match merged.last_mut() {
            Some(previous)
                if record.candidate.offset - previous.candidate.offset
                    <= CANDIDATE_MERGE_TOLERANCE_SECONDS =>
            {
                if compare_records(&record, previous) == Ordering::Less {
                    *previous = record;
                }
            }
            _ => merged.push(record),
        }
    }
    merged.sort_by(compare_records);
    merged
}

fn match_all(landmarks: &[Landmark], detection: &Detection) -> Vec<Candidate> {
    let (eligible, supports) = offset_supports(landmarks, detection);
    let groups = assignment_groups(&eligible, &supports);
    let records = offset_hypotheses(&eligible, &supports)
        .into_iter()
        .filter_map(|offset| {
            best_assignment(offset, &groups, eligible.len(), &supports).map(|(matches, score)| {
                candidate_record(offset, &matches, score, eligible.len(), &supports)
            })
        })
        .collect();
    let merged = merge_records(records);
    let location = detection.location.and_then(|location| {
        landmarks
            .iter()
            .find(|landmark| landmark.kind() == LandmarkKind::Location)
            .map(|landmark| (landmark, location.time - landmark.video_second()))
    });
    merged
        .into_iter()
        .take(MAX_CANDIDATES)
        .map(|record| {
            let mut candidate = record.candidate;
            if let Some((landmark, location_offset)) = location {
                let agrees =
                    (candidate.offset - location_offset).abs() <= LOCATION_TIMING_TOLERANCE_SECONDS;
                candidate.variant = if agrees {
                    CandidateVariant::Ordinary
                } else {
                    CandidateVariant::LocationConflict
                };
                candidate.location_classification = if agrees {
                    LocationClassification::Agrees
                } else {
                    LocationClassification::Conflict
                };
                if !agrees {
                    candidate.excluded_landmark_ids = vec![landmark.id().to_string()];
                }
            }
            candidate
        })
        .collect()
}

fn match_location(landmarks: &[Landmark], detection: &Detection) -> Result<Vec<Candidate>, String> {
    let landmark = landmarks
        .iter()
        .find(|landmark| landmark.kind() == LandmarkKind::Location)
        .ok_or("Location-only sync requires a video location landmark")?;
    let location = detection
        .location
        .ok_or("Location-only sync requires a detected activity location")?;
    Ok(vec![Candidate {
        variant: CandidateVariant::LocationOnly,
        offset: location.time - landmark.video_second(),
        match_score: None,
        matched_count: 1,
        eligible_count: 1,
        evidence: vec![Evidence {
            landmark_id: landmark.id().to_string(),
            event_id: DETECTED_LOCATION_ID.into(),
            kind: LandmarkKind::Location,
            residual_seconds: 0.0,
            quality: 1.0,
        }],
        location_classification: LocationClassification::None,
        excluded_landmark_ids: Vec::new(),
    }])
}

/// Deterministic candidates for one scope. All-landmark matching uses
/// a detected location as ordinary evidence; location-only returns its one exact alignment.
pub fn match_candidates(
    landmarks: &[Landmark],
    detection: &Detection,
    scope: MatchScope,
) -> Result<Vec<Candidate>, String> {
    match scope {
        MatchScope::All => Ok(match_all(landmarks, detection)),
        MatchScope::Location => match_location(landmarks, detection),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::sync::detection::{
        Availability, Interval, LocationEvent, StopEvent, TurnEvent,
    };

    const ALL: Availability = Availability {
        speed: true,
        heading: true,
        course: true,
    };

    fn detection(location: Option<f64>, stops: Vec<StopEvent>, turns: Vec<TurnEvent>) -> Detection {
        Detection {
            availability: ALL,
            stops,
            turns,
            location: location.map(|time| LocationEvent { time }),
            ..Detection::default()
        }
    }

    fn stop(id: &str, time: f64) -> StopEvent {
        StopEvent {
            id: id.into(),
            time,
            low_speed_interval: Interval {
                start: time,
                end: time + 2.0,
            },
        }
    }

    fn turn(id: &str, direction: TurnDirection, start: f64, end: f64) -> TurnEvent {
        TurnEvent {
            id: id.into(),
            direction,
            start,
            end,
            signed_change: 90.0,
            representative_time: f64::midpoint(start, end),
        }
    }

    fn landmark(id: &str, kind: LandmarkKind, second: f64) -> Landmark {
        Landmark::new(id.into(), kind, second)
    }

    fn all(landmarks: &[Landmark], detection: &Detection) -> Vec<Candidate> {
        match_candidates(landmarks, detection, MatchScope::All).unwrap()
    }

    #[test]
    fn aligns_a_stop_and_same_direction_turn_and_rejects_the_opposite() {
        let landmarks = [
            landmark("video-stop", LandmarkKind::Stop, 10.0),
            landmark("video-right", LandmarkKind::RightTurn, 20.0),
        ];
        let found = all(
            &landmarks,
            &detection(
                None,
                vec![stop("activity-stop", 110.0)],
                vec![
                    turn("activity-left", TurnDirection::Left, 119.0, 123.0),
                    turn("activity-right", TurnDirection::Right, 119.0, 123.0),
                ],
            ),
        );
        let first = &found[0];
        assert_eq!(
            (
                first.offset,
                first.match_score,
                first.matched_count,
                first.eligible_count
            ),
            (101.0, Some(100.0), 2, 2)
        );
        assert_eq!(
            first
                .evidence
                .iter()
                .map(|item| item.event_id.as_str())
                .collect::<Vec<_>>(),
            ["activity-stop", "activity-right"]
        );
    }

    #[test]
    fn one_event_explains_one_landmark_and_partial_coverage_is_penalized() {
        let two = [
            landmark("video-stop-1", LandmarkKind::Stop, 0.0),
            landmark("video-stop-2", LandmarkKind::Stop, 1.0),
        ];
        assert!(all(
            &two,
            &detection(None, vec![stop("activity-stop", 100.0)], vec![])
        )
        .is_empty());
        let three = [
            two[0].clone(),
            two[1].clone(),
            landmark("video-stop-3", LandmarkKind::Stop, 20.0),
        ];
        let partial = all(
            &three,
            &detection(
                None,
                vec![
                    stop("activity-stop-1", 100.0),
                    stop("activity-stop-2", 101.0),
                ],
                vec![],
            ),
        );
        assert_eq!(
            (
                partial[0].matched_count,
                partial[0].eligible_count,
                partial[0].match_score
            ),
            (2, 3, Some(67.0))
        );
    }

    #[test]
    fn scores_misses_continuously_and_orders_merges_and_limits() {
        assert_eq!(timing_quality(0.0, 2.0), 1.0);
        assert_eq!(timing_quality(2.0, 2.0), 0.5);
        assert!((timing_quality(4.0, 2.0) - 0.2).abs() < 1e-12);
        let score = match_score(&[(0.0, 1.0), (2.0, 0.5)], 2);
        assert_eq!(
            (score.total_quality, score.coverage, score.match_score),
            (1.5, 1.0, 75.0)
        );

        let landmarks = [
            landmark("video-stop-1", LandmarkKind::Stop, 0.0),
            landmark("video-stop-2", LandmarkKind::Stop, 10.0),
        ];
        let events: Vec<StopEvent> = [
            100.0, 110.0, 200.0, 210.0, 300.0, 310.0, 400.0, 410.0, 500.0, 510.0, 600.0, 610.0,
        ]
        .iter()
        .enumerate()
        .map(|(index, &time)| stop(&format!("activity-stop-{index}"), time))
        .collect();
        let first = all(&landmarks, &detection(None, events.clone(), vec![]));
        assert_eq!(first, all(&landmarks, &detection(None, events, vec![])));
        assert_eq!(first.len(), 5);
        let mut sorted = first.clone();
        sorted.sort_by(|left, right| {
            right
                .match_score
                .partial_cmp(&left.match_score)
                .unwrap()
                .then(left.offset.total_cmp(&right.offset))
        });
        assert_eq!(first, sorted);
    }

    #[test]
    fn treats_periods_as_flat_and_keeps_near_misses() {
        let flat = all(
            &[
                landmark("video-stop-1", LandmarkKind::Stop, 0.0),
                landmark("video-stop-2", LandmarkKind::Stop, 10.0),
            ],
            &detection(
                None,
                vec![
                    stop("activity-stop-1", 100.0),
                    stop("activity-stop-2", 110.0),
                ],
                vec![],
            ),
        );
        assert_eq!(
            (flat[0].offset, flat[0].match_score, flat[0].matched_count),
            (101.0, Some(100.0), 2)
        );
        let near = all(
            &[
                landmark("video-stop", LandmarkKind::Stop, 0.0),
                landmark("video-turn", LandmarkKind::RightTurn, 10.0),
            ],
            &detection(
                None,
                vec![stop("activity-stop", 100.0)],
                vec![turn("activity-turn", TurnDirection::Right, 116.0, 118.0)],
            ),
        );
        assert_eq!(
            (near[0].offset, near[0].match_score, near[0].matched_count),
            (104.0, Some(50.0), 2)
        );
        assert_eq!(
            near[0]
                .evidence
                .iter()
                .map(|item| item.residual_seconds)
                .collect::<Vec<_>>(),
            [2.0, 2.0]
        );
    }

    #[test]
    fn keeps_location_only_independent_from_combined_and_conflict_candidates() {
        let ordinary = [
            landmark("video-stop-1", LandmarkKind::Stop, 10.0),
            landmark("video-stop-2", LandmarkKind::Stop, 20.0),
        ];
        let with_location = |second: f64| {
            let mut landmarks = ordinary.to_vec();
            landmarks.push(landmark("video-location", LandmarkKind::Location, second));
            landmarks
        };
        assert!(all(&with_location(2.0), &detection(None, vec![], vec![])).is_empty());

        let resolved = all(
            &with_location(0.0),
            &detection(
                Some(0.0),
                vec![stop("activity-stop-1", 20.0), stop("activity-stop-2", 30.0)],
                vec![],
            ),
        );
        let only = match_candidates(
            &with_location(0.0),
            &detection(Some(0.0), vec![], vec![]),
            MatchScope::Location,
        )
        .unwrap();
        assert_eq!(
            (only[0].variant, only[0].offset),
            (CandidateVariant::LocationOnly, 0.0)
        );
        assert!(resolved
            .iter()
            .all(|candidate| candidate.variant != CandidateVariant::LocationOnly));
        let agrees = resolved
            .iter()
            .find(|candidate| candidate.location_classification == LocationClassification::Agrees)
            .unwrap();
        assert_eq!(agrees.variant, CandidateVariant::Ordinary);

        let conflicting = all(
            &with_location(0.0),
            &detection(
                Some(0.0),
                vec![stop("activity-stop-1", 30.0), stop("activity-stop-2", 40.0)],
                vec![],
            ),
        );
        let conflict = conflicting
            .iter()
            .find(|candidate| candidate.variant == CandidateVariant::LocationConflict)
            .unwrap();
        assert_eq!(
            (
                conflict.offset,
                conflict.location_classification,
                conflict.excluded_landmark_ids.clone()
            ),
            (
                21.0,
                LocationClassification::Conflict,
                vec!["video-location".to_string()]
            )
        );
    }

    #[test]
    fn uses_location_as_evidence_when_syncing_all_landmarks() {
        let found = all(
            &[
                landmark("video-location", LandmarkKind::Location, 0.0),
                landmark("video-stop", LandmarkKind::Stop, 10.0),
            ],
            &detection(Some(100.0), vec![stop("activity-stop", 110.0)], vec![]),
        );
        let first = &found[0];
        assert_eq!(
            (
                first.offset,
                first.matched_count,
                first.eligible_count,
                first.location_classification
            ),
            (100.0, 2, 2, LocationClassification::Agrees)
        );
        assert_eq!(
            first
                .evidence
                .iter()
                .map(|item| item.kind)
                .collect::<Vec<_>>(),
            [LandmarkKind::Stop, LandmarkKind::Location]
        );
    }

    #[test]
    fn location_only_needs_a_location_landmark_and_a_detected_location() {

        assert!(match_candidates(
            &[],
            &detection(Some(1.0), vec![], vec![]),
            MatchScope::Location
        )
        .is_err());
        assert!(match_candidates(
            &[landmark("l", LandmarkKind::Location, 0.0)],
            &detection(None, vec![], vec![]),
            MatchScope::Location
        )
        .is_err());
    }
}
