//! Port of `store/slices/createManualVideoSyncSlice.js`: the landmarks, the detected location, the
//! sensitivity, and the derived detection and candidate results (Phase 8 D2).
//!
//! The durable state is core's `ManualVideoSync` (`Store::manual_video_sync`), which projects save.
//! The slice is outside the editor history, as React's: only `apply_candidate` makes an undo step,
//! through the video edits it moves.

use crate::features::video_sync::detection::{Detection, LocationEvent};
use crate::features::video_sync::matching::{Candidate, MatchScope};
use crate::lib::video_program::edits::find_video;
use crate::lib::video_program::Clip;
use crate::store::Store;
use ovrley_core::media::manual_video_sync::{
    Landmark, LandmarkKind, ManualVideoSync, MAX_MANUAL_LANDMARKS, MAX_MANUAL_LOCATION_LANDMARKS,
    MAX_MANUAL_SPEED_THRESHOLD_KMH, MAX_MANUAL_TURN_THRESHOLD_DEGREES,
    MIN_MANUAL_SPEED_THRESHOLD_KMH, MIN_MANUAL_TURN_THRESHOLD_DEGREES,
};
use std::sync::Arc;

/// `MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CandidateStatus {
    #[default]
    Idle,
    Calculating,
    Fresh,
    Stale,
    Error,
}

impl CandidateStatus {
    /// The React constant, as agents read it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Calculating => "calculating",
            Self::Fresh => "fresh",
            Self::Stale => "stale",
            Self::Error => "error",
        }
    }
}

/// One search's result.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CandidateResult {
    pub candidates: Vec<Candidate>,
    pub error: Option<String>,
    pub has_searched: bool,
    /// Rises with every invalidation, so a search that began before it is dropped.
    pub revision: u64,
    pub status: CandidateStatus,
}

impl CandidateResult {
    /// Hidden until a search ran (`VideoSyncCandidateList`).
    pub fn is_shown(&self) -> bool {
        self.has_searched || self.status != CandidateStatus::Idle
    }
}

/// `manualVideoSyncResults`: one result per scope.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CandidateResults {
    pub all: CandidateResult,
    pub location: CandidateResult,
}

impl CandidateResults {
    pub fn get(&self, scope: MatchScope) -> &CandidateResult {
        match scope {
            MatchScope::All => &self.all,
            MatchScope::Location => &self.location,
        }
    }

    fn get_mut(&mut self, scope: MatchScope) -> &mut CandidateResult {
        match scope {
            MatchScope::All => &mut self.all,
            MatchScope::Location => &mut self.location,
        }
    }

    pub fn is_calculating(&self) -> bool {
        self.all.status == CandidateStatus::Calculating
            || self.location.status == CandidateStatus::Calculating
    }

    /// `createCandidateResults(previous)`: empty results whose revisions moved on.
    fn renewed(&self) -> Self {
        Self {
            all: CandidateResult {
                revision: self.all.revision + 1,
                ..CandidateResult::default()
            },
            location: CandidateResult {
                revision: self.location.revision + 1,
                ..CandidateResult::default()
            },
        }
    }
}

/// The two thresholds' ranges, read by the sliders, the actions and MCP (D11).
pub const SPEED_THRESHOLD_RANGE: (f64, f64) = (
    MIN_MANUAL_SPEED_THRESHOLD_KMH,
    MAX_MANUAL_SPEED_THRESHOLD_KMH,
);
pub const TURN_THRESHOLD_RANGE: (f64, f64) = (
    MIN_MANUAL_TURN_THRESHOLD_DEGREES,
    MAX_MANUAL_TURN_THRESHOLD_DEGREES,
);

/// `findVideoSyncTarget`: the clip whose landmarks the workspace matches.
pub fn find_video_sync_target(store: &Store) -> Option<Clip> {
    let video_id = store.manual_video_sync.video_id.as_deref()?;
    let video = store
        .video_import
        .imported_videos
        .iter()
        .find(|video| video.id == video_id)?;
    Some(Clip {
        video: video.clone(),
        edits: store.video_import.video_edits[video_id].clone(),
    })
}

fn require_target(store: &Store) -> Result<Clip, String> {
    find_video_sync_target(store)
        .ok_or_else(|| "A video must be loaded before adding a manual video sync landmark".into())
}

/// `validateVideoSecond` against the target video.
fn validate_video_second(second: f64, duration_seconds: f64) -> Result<(), String> {
    if !second.is_finite() {
        return Err("Manual video sync landmark videoSecond must be a finite number".into());
    }
    if second < 0.0 {
        return Err("Manual video sync landmark videoSecond must not be negative".into());
    }
    if duration_seconds <= 0.0 || second > duration_seconds {
        return Err(
            "Manual video sync landmark videoSecond must be within the imported video duration"
                .into(),
        );
    }
    Ok(())
}

fn location_limit_error() -> String {
    format!("Manual video sync supports at most {MAX_MANUAL_LOCATION_LANDMARKS} location landmark")
}

fn find_landmark<'a>(store: &'a Store, id: &str) -> Result<&'a Landmark, String> {
    if id.trim().is_empty() {
        return Err("Manual video sync landmark id must be a non-empty string".into());
    }
    store
        .manual_video_sync
        .landmarks
        .iter()
        .find(|landmark| landmark.id() == id)
        .ok_or_else(|| format!("Manual video sync landmark was not found: {id}"))
}

/// `invalidateResult`: a result that searched becomes stale; one that has not stays idle.
fn invalidate(store: &mut Store, scope: MatchScope) {
    let result = store.manual_video_sync_results.get_mut(scope);
    result.revision += 1;
    result.error = None;
    result.status = if result.has_searched {
        CandidateStatus::Stale
    } else {
        CandidateStatus::Idle
    };
}

/// A location edit invalidates both results; any other landmark edit only the all-scope one.
fn invalidate_for(store: &mut Store, location: bool) {
    invalidate(store, MatchScope::All);
    if location {
        invalidate(store, MatchScope::Location);
    }
}

/// `detectionWithSavedLocation`.
fn with_saved_location(mut detection: Detection, location_second: Option<f64>) -> Arc<Detection> {
    detection.location = location_second.map(|time| LocationEvent { time });
    Arc::new(detection)
}

/// `resetDerivedState`.
fn reset_derived_state(store: &mut Store) {
    let location = store.manual_video_sync.detected_location_second;
    store.manual_video_sync_detection =
        location.map(|_| Arc::new(Detection::with_location(location)));
    store.manual_video_sync_results = store.manual_video_sync_results.renewed();
}

/// `addVideoSyncLandmark`: inside the target video, at most five, at most one location. Returns
/// the new landmark's id.
pub fn add_landmark(
    store: &mut Store,
    kind: LandmarkKind,
    video_second: f64,
) -> Result<String, String> {
    let video = require_target(store)?;
    let id = crate::lib::video_program::new_id();
    validate_video_second(video_second, video.duration_seconds())?;
    let landmarks = &store.manual_video_sync.landmarks;
    if landmarks.len() >= MAX_MANUAL_LANDMARKS {
        return Err(format!(
            "Manual video sync supports at most {MAX_MANUAL_LANDMARKS} landmarks"
        ));
    }
    if kind == LandmarkKind::Location
        && landmarks
            .iter()
            .any(|landmark| landmark.kind() == LandmarkKind::Location)
    {
        return Err(location_limit_error());
    }
    store
        .manual_video_sync
        .landmarks
        .push(Landmark::new(id.clone(), kind, video_second));
    invalidate_for(store, kind == LandmarkKind::Location);
    Ok(id)
}

/// `moveVideoSyncLandmark`.
pub fn move_landmark(store: &mut Store, id: &str, video_second: f64) -> Result<(), String> {
    let kind = find_landmark(store, id)?.kind();
    validate_video_second(video_second, require_target(store)?.duration_seconds())?;
    let landmark = store
        .manual_video_sync
        .landmarks
        .iter_mut()
        .find(|landmark| landmark.id() == id)
        .expect("found above");
    landmark.set_video_second(video_second);
    invalidate_for(store, kind == LandmarkKind::Location);
    Ok(())
}

/// `setVideoSyncLandmarkType`.
pub fn set_landmark_kind(store: &mut Store, id: &str, kind: LandmarkKind) -> Result<(), String> {
    let landmark = find_landmark(store, id)?;
    let previous = landmark.kind();
    let retyped = landmark.retyped(kind);
    validate_video_second(
        retyped.video_second(),
        require_target(store)?.duration_seconds(),
    )?;
    if kind == LandmarkKind::Location
        && store
            .manual_video_sync
            .landmarks
            .iter()
            .any(|item| item.id() != id && item.kind() == LandmarkKind::Location)
    {
        return Err(location_limit_error());
    }
    if previous == kind {
        return Ok(());
    }
    let slot = store
        .manual_video_sync
        .landmarks
        .iter_mut()
        .find(|landmark| landmark.id() == id)
        .expect("found above");
    *slot = retyped;
    invalidate_for(
        store,
        previous == LandmarkKind::Location || kind == LandmarkKind::Location,
    );
    Ok(())
}

/// `removeVideoSyncLandmark`.
pub fn remove_landmark(store: &mut Store, id: &str) -> Result<(), String> {
    let kind = find_landmark(store, id)?.kind();
    store
        .manual_video_sync
        .landmarks
        .retain(|landmark| landmark.id() != id);
    invalidate_for(store, kind == LandmarkKind::Location);
    Ok(())
}

/// `clearVideoSyncLandmarks`.
pub fn clear_landmarks(store: &mut Store) {
    if store.manual_video_sync.landmarks.is_empty() {
        return;
    }
    store.manual_video_sync.landmarks.clear();
    invalidate_for(store, true);
}

/// `setVideoSyncDetectedLocation`: "Set location" on the map.
pub fn set_detected_location(store: &mut Store, activity_second: f64) -> Result<(), String> {
    if !activity_second.is_finite() {
        return Err(
            "Manual video sync detected location activitySecond must be a finite number".into(),
        );
    }
    if activity_second < 0.0 {
        return Err(
            "Manual video sync detected location activitySecond must not be negative".into(),
        );
    }
    if store.manual_video_sync.detected_location_second == Some(activity_second) {
        return Ok(());
    }
    store.manual_video_sync.detected_location_second = Some(activity_second);
    let mut detection = store
        .manual_video_sync_detection
        .as_deref()
        .cloned()
        .unwrap_or_default();
    detection.location = Some(LocationEvent {
        time: activity_second,
    });
    store.manual_video_sync_detection = Some(Arc::new(detection));
    invalidate_for(store, true);
    Ok(())
}

/// `clearVideoSyncDetectedLocation`.
pub fn clear_detected_location(store: &mut Store) {
    if store.manual_video_sync.detected_location_second.is_none() {
        return;
    }
    store.manual_video_sync.detected_location_second = None;
    if let Some(detection) = &mut store.manual_video_sync_detection {
        Arc::make_mut(detection).location = None;
    }
    invalidate_for(store, true);
}

fn validate_threshold(value: f64, (min, max): (f64, f64), label: &str) -> Result<(), String> {
    if !value.is_finite() {
        return Err(format!("{label} must be a finite number"));
    }
    if value < min || value > max {
        return Err(format!("{label} must be between {min} and {max}"));
    }
    Ok(())
}

/// `setVideoSyncSpeedThreshold`.
pub fn set_speed_threshold(store: &mut Store, kmh: f64) -> Result<(), String> {
    validate_threshold(
        kmh,
        SPEED_THRESHOLD_RANGE,
        "Manual video sync speedThresholdKmh",
    )?;
    if store.manual_video_sync.speed_threshold_kmh != kmh {
        store.manual_video_sync.speed_threshold_kmh = kmh;
        invalidate(store, MatchScope::All);
    }
    Ok(())
}

/// `setVideoSyncTurnThreshold`.
pub fn set_turn_threshold(store: &mut Store, degrees: f64) -> Result<(), String> {
    validate_threshold(
        degrees,
        TURN_THRESHOLD_RANGE,
        "Manual video sync turnThresholdDegrees",
    )?;
    if store.manual_video_sync.turn_threshold_degrees != degrees {
        store.manual_video_sync.turn_threshold_degrees = degrees;
        invalidate(store, MatchScope::All);
    }
    Ok(())
}

/// Both thresholds, either kept when `None`: both change, or neither.
///
/// # Errors
///
/// When either value is outside its range.
pub fn set_thresholds(store: &mut Store, kmh: Option<f64>, degrees: Option<f64>) -> Result<(), String> {
    // The second one's range first, so that its refusal leaves the first unchanged.
    if let Some(degrees) = degrees {
        validate_threshold(degrees, TURN_THRESHOLD_RANGE, "Manual video sync turnThresholdDegrees")?;
    }
    if let Some(kmh) = kmh {
        set_speed_threshold(store, kmh)?;
    }
    if let Some(degrees) = degrees {
        set_turn_threshold(store, degrees)?;
    }
    Ok(())
}

/// Two offsets this close name one candidate: JSON can change an offset's last bit, and a frame is
/// thousands of times longer.
const SAME_OFFSET_SECONDS: f64 = 1e-6;

/// `applyVideoSyncCandidate`: only from a fresh result holding that offset. Candidates place the
/// target clip; every clip moves by the same amount, and the playhead with them.
pub fn apply_candidate(store: &mut Store, scope: MatchScope, offset: f64) -> Result<(), String> {
    let result = store.manual_video_sync_results.get(scope);
    if result.status != CandidateStatus::Fresh {
        return Err("Cannot apply a stale manual video sync candidate".into());
    }
    let offset = result
        .candidates
        .iter()
        .map(|candidate| candidate.offset)
        .find(|candidate| (candidate - offset).abs() <= SAME_OFFSET_SECONDS)
        .ok_or("Manual video sync candidate is not part of the current result")?;
    let target = require_target(store)?;
    crate::store::slices::video_import::shift_video_offsets(
        store,
        offset - target.offset_seconds(),
        true,
    )
}

/// `beginVideoSyncCalculation`: the revision the search answers, or `None` while one runs.
pub fn begin_calculation(store: &mut Store, scope: MatchScope) -> Option<u64> {
    if store.manual_video_sync_results.is_calculating() {
        return None;
    }
    let result = store.manual_video_sync_results.get_mut(scope);
    result.status = CandidateStatus::Calculating;
    result.error = None;
    Some(result.revision)
}

fn answers(store: &Store, scope: MatchScope, revision: u64) -> bool {
    let result = store.manual_video_sync_results.get(scope);
    result.revision == revision && result.status == CandidateStatus::Calculating
}

/// `completeVideoSyncCalculation`: ignored (`false`) when the landmarks changed meanwhile.
pub fn complete_calculation(
    store: &mut Store,
    scope: MatchScope,
    revision: u64,
    detection: Detection,
    candidates: Vec<Candidate>,
) -> bool {
    if !answers(store, scope, revision) {
        return false;
    }
    store.manual_video_sync_detection = Some(with_saved_location(
        detection,
        store.manual_video_sync.detected_location_second,
    ));
    let result = store.manual_video_sync_results.get_mut(scope);
    result.candidates = candidates;
    result.status = CandidateStatus::Fresh;
    result.error = None;
    result.has_searched = true;
    true
}

/// `failVideoSyncCalculation`.
pub fn fail_calculation(
    store: &mut Store,
    scope: MatchScope,
    revision: u64,
    message: String,
) -> bool {
    if !answers(store, scope, revision) {
        return false;
    }
    let result = store.manual_video_sync_results.get_mut(scope);
    result.status = CandidateStatus::Error;
    result.error = Some(message);
    result.has_searched = true;
    true
}

/// `completeVideoSyncDetection`.
pub fn complete_detection(store: &mut Store, detection: Detection) {
    store.manual_video_sync_detection = Some(with_saved_location(
        detection,
        store.manual_video_sync.detected_location_second,
    ));
}

/// `selectVideoSyncVideo`: the video whose landmarks the manual sync matches; landmarks belong to
/// one video and are cleared on a change.
pub fn select_video_sync_video(store: &mut Store, video_id: Option<String>) -> Result<(), String> {
    if store.manual_video_sync.video_id == video_id {
        return Ok(());
    }
    if let Some(video_id) = &video_id {
        find_video(&store.video_import.imported_videos, video_id)
            .map_err(|_| format!("Unknown manual video sync video: {video_id}"))?;
    }
    store.manual_video_sync.video_id = video_id;
    store.manual_video_sync.landmarks.clear();
    store.manual_video_sync_results = store.manual_video_sync_results.renewed();
    Ok(())
}

/// `clearVideoSyncForActivity`: the detected location belongs to the replaced activity.
pub fn clear_video_sync_for_activity(store: &mut Store) {
    store.manual_video_sync.detected_location_second = None;
    reset_derived_state(store);
}

/// `hydrateVideoSyncState`: a project's manual sync, validated when it was read.
pub fn hydrate_video_sync_state(store: &mut Store, manual: ManualVideoSync) {
    store.manual_video_sync = manual;
    reset_derived_state(store);
}

/// `resetVideoSyncState`.
pub fn reset_video_sync_state(store: &mut Store) {
    store.manual_video_sync = ManualVideoSync::default();
    reset_derived_state(store);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::video_sync::matching::{CandidateVariant, LocationClassification};
    use crate::lib::video_program::fixtures::simple;
    use crate::store::slices::video_import::tests::store_with;

    /// A store with one 60 s video at offset 0, selected for the manual sync.
    fn store() -> Store {
        let mut store = store_with(vec![simple("ride", 60.0, 0)], &[]);
        select_video_sync_video(&mut store, Some("ride".into())).unwrap();
        store
    }

    fn candidate(offset: f64) -> Candidate {
        Candidate {
            variant: CandidateVariant::Ordinary,
            offset,
            match_score: Some(100.0),
            matched_count: 2,
            eligible_count: 2,
            evidence: Vec::new(),
            location_classification: LocationClassification::None,
            excluded_landmark_ids: Vec::new(),
        }
    }

    fn searched(store: &mut Store, scope: MatchScope, candidates: Vec<Candidate>) {
        let revision = begin_calculation(store, scope).unwrap();
        assert!(complete_calculation(
            store,
            scope,
            revision,
            Detection::default(),
            candidates
        ));
    }

    #[test]
    fn adds_typed_landmarks_inside_the_target_video_with_the_limits() {
        let mut store = store();
        let id = add_landmark(&mut store, LandmarkKind::Location, 4.0).unwrap();
        assert_eq!(
            store.manual_video_sync.landmarks,
            vec![Landmark::Location {
                id,
                video_second: 4.0,
                activity_second: None
            }]
        );
        assert!(add_landmark(&mut store, LandmarkKind::Location, 5.0)
            .unwrap_err()
            .contains("at most 1 location"));
        assert!(add_landmark(&mut store, LandmarkKind::Stop, -1.0)
            .unwrap_err()
            .contains("must not be negative"));
        assert!(add_landmark(&mut store, LandmarkKind::Stop, 61.0)
            .unwrap_err()
            .contains("within the imported video duration"));
        for second in [1.0, 2.0, 3.0, 4.0] {
            add_landmark(&mut store, LandmarkKind::Stop, second).unwrap();
        }
        assert!(add_landmark(&mut store, LandmarkKind::Stop, 5.0)
            .unwrap_err()
            .contains("at most 5 landmarks"));
    }

    #[test]
    fn a_landmark_needs_a_target_video() {
        let mut store = Store::default();
        assert!(add_landmark(&mut store, LandmarkKind::Stop, 1.0)
            .unwrap_err()
            .contains("A video must be loaded"));
    }

    #[test]
    fn moves_retypes_and_removes_landmarks() {
        let mut store = store();
        let id = add_landmark(&mut store, LandmarkKind::Stop, 1.0).unwrap();
        move_landmark(&mut store, &id, 7.5).unwrap();
        assert_eq!(store.manual_video_sync.landmarks[0].video_second(), 7.5);
        assert!(move_landmark(&mut store, &id, 70.0).is_err());
        assert!(move_landmark(&mut store, "nope", 1.0)
            .unwrap_err()
            .contains("not found"));
        set_landmark_kind(&mut store, &id, LandmarkKind::Location).unwrap();
        assert_eq!(
            store.manual_video_sync.landmarks[0].kind(),
            LandmarkKind::Location
        );
        let other = add_landmark(&mut store, LandmarkKind::LeftTurn, 2.0).unwrap();
        assert!(
            set_landmark_kind(&mut store, &other, LandmarkKind::Location)
                .unwrap_err()
                .contains("location landmark")
        );
        set_landmark_kind(&mut store, &id, LandmarkKind::RightTurn).unwrap();
        assert_eq!(
            store.manual_video_sync.landmarks[0],
            Landmark::RightTurn {
                id: id.clone(),
                video_second: 7.5
            }
        );
        remove_landmark(&mut store, &id).unwrap();
        assert_eq!(store.manual_video_sync.landmarks.len(), 1);
        clear_landmarks(&mut store);
        assert!(store.manual_video_sync.landmarks.is_empty());
    }

    #[test]
    fn landmark_edits_invalidate_the_all_scope_and_location_edits_both() {
        let mut store = store();
        searched(&mut store, MatchScope::All, vec![candidate(1.0)]);
        searched(&mut store, MatchScope::Location, vec![candidate(2.0)]);
        let stop = add_landmark(&mut store, LandmarkKind::Stop, 1.0).unwrap();
        assert_eq!(
            (
                store.manual_video_sync_results.all.status,
                store.manual_video_sync_results.location.status
            ),
            (CandidateStatus::Stale, CandidateStatus::Fresh)
        );
        searched(&mut store, MatchScope::All, vec![]);
        add_landmark(&mut store, LandmarkKind::Location, 2.0).unwrap();
        assert_eq!(
            (
                store.manual_video_sync_results.all.status,
                store.manual_video_sync_results.location.status
            ),
            (CandidateStatus::Stale, CandidateStatus::Stale)
        );
        let mut fresh = self::store();
        add_landmark(&mut fresh, LandmarkKind::Stop, 1.0).unwrap();
        assert_eq!(
            fresh.manual_video_sync_results.all.status,
            CandidateStatus::Idle,
            "a result that never searched stays idle"
        );
        set_speed_threshold(&mut store, 6.0).unwrap();
        assert!(set_turn_threshold(&mut store, 181.0)
            .unwrap_err()
            .contains("between 70 and 180"));
        assert!(set_speed_threshold(&mut store, 0.5).is_err());
        let _ = stop;
    }

    #[test]
    fn the_detected_location_is_set_once_and_cleared() {
        let mut store = store();
        assert!(set_detected_location(&mut store, -1.0).is_err());
        set_detected_location(&mut store, 42.0).unwrap();
        assert_eq!(store.manual_video_sync.detected_location_second, Some(42.0));
        assert_eq!(
            store.manual_video_sync_detection.as_ref().unwrap().location,
            Some(LocationEvent { time: 42.0 })
        );
        let revision = store.manual_video_sync_results.location.revision;
        set_detected_location(&mut store, 42.0).unwrap();
        assert_eq!(
            store.manual_video_sync_results.location.revision, revision,
            "the same second changes nothing"
        );
        clear_detected_location(&mut store);
        assert_eq!(
            store.manual_video_sync_detection.as_ref().unwrap().location,
            None
        );
        assert_eq!(store.manual_video_sync.detected_location_second, None);
    }

    #[test]
    fn a_calculation_answers_only_its_revision_and_one_runs_at_a_time() {
        let mut store = store();
        let revision = begin_calculation(&mut store, MatchScope::All).unwrap();
        assert_eq!(
            begin_calculation(&mut store, MatchScope::Location),
            None,
            "one search at a time"
        );
        add_landmark(&mut store, LandmarkKind::Stop, 1.0).unwrap();
        assert!(
            !complete_calculation(
                &mut store,
                MatchScope::All,
                revision,
                Detection::default(),
                vec![candidate(1.0)]
            ),
            "the landmarks changed meanwhile"
        );
        let revision = begin_calculation(&mut store, MatchScope::All).unwrap();
        assert!(fail_calculation(
            &mut store,
            MatchScope::All,
            revision,
            "boom".into()
        ));
        let result = &store.manual_video_sync_results.all;
        assert_eq!(
            (result.status, result.error.as_deref(), result.has_searched),
            (CandidateStatus::Error, Some("boom"), true)
        );
    }

    #[test]
    fn a_completed_detection_keeps_the_saved_location() {
        let mut store = store();
        set_detected_location(&mut store, 9.0).unwrap();
        complete_detection(&mut store, Detection::default());
        assert_eq!(
            store.manual_video_sync_detection.as_ref().unwrap().location,
            Some(LocationEvent { time: 9.0 })
        );
    }

    #[test]
    fn applying_a_fresh_candidate_moves_every_clip_and_the_playhead() {
        let mut store = store_with(
            vec![simple("ride", 60.0, 0), simple("second", 30.0, 1)],
            &[],
        );
        select_video_sync_video(&mut store, Some("ride".into())).unwrap();
        let before: Vec<f64> = ["ride", "second"]
            .iter()
            .map(|id| store.video_import.video_edits[*id].offset_seconds)
            .collect();
        store.media.activity_summary = Some(crate::store::slices::media::ActivitySummary {
            duration_seconds: 500.0,
            ..crate::store::slices::media::ActivitySummary::of(&Default::default())
        });
        store.editor.selected_second = 10.0;
        assert!(apply_candidate(&mut store, MatchScope::All, 25.0)
            .unwrap_err()
            .contains("stale"));
        searched(&mut store, MatchScope::All, vec![candidate(25.0)]);
        assert!(apply_candidate(&mut store, MatchScope::All, 26.0)
            .unwrap_err()
            .contains("not part"));
        apply_candidate(&mut store, MatchScope::All, 25.0).unwrap();
        let after: Vec<f64> = ["ride", "second"]
            .iter()
            .map(|id| store.video_import.video_edits[*id].offset_seconds)
            .collect();
        assert_eq!(
            after,
            vec![before[0] + 25.0 - before[0], before[1] + 25.0 - before[0]]
        );
        assert_eq!(store.editor.selected_second, 35.0);
    }

    #[test]
    fn the_manual_sync_selects_known_videos_and_keeps_landmarks_across_activities() {
        let mut store = store_with(vec![simple("ride", 10.0, 0)], &[]);
        assert!(select_video_sync_video(&mut store, Some("gone".into()))
            .unwrap_err()
            .contains("Unknown manual video sync video"));
        hydrate_video_sync_state(
            &mut store,
            ManualVideoSync {
                video_id: Some("ride".into()),
                landmarks: vec![Landmark::Stop {
                    id: "s".into(),
                    video_second: 1.0,
                }],
                detected_location_second: Some(5.0),
                ..ManualVideoSync::default()
            },
        );
        assert_eq!(
            store.manual_video_sync_detection.as_ref().unwrap().location,
            Some(LocationEvent { time: 5.0 }),
            "hydration keeps the saved location"
        );
        clear_video_sync_for_activity(&mut store);
        assert_eq!(
            (
                store.manual_video_sync.detected_location_second,
                store.manual_video_sync.landmarks.len()
            ),
            (None, 1)
        );
        assert_eq!(store.manual_video_sync_detection, None);
        select_video_sync_video(&mut store, Some("ride".into())).unwrap();
        assert_eq!(
            store.manual_video_sync.landmarks.len(),
            1,
            "selecting the same video keeps its landmarks"
        );
        select_video_sync_video(&mut store, None).unwrap();
        assert!(store.manual_video_sync.landmarks.is_empty());
        reset_video_sync_state(&mut store);
        assert_eq!(store.manual_video_sync, ManualVideoSync::default());
    }
}
