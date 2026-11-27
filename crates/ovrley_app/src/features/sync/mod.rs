//! Sync Doctor: the automatic sync controls the import dialog and the video panel share, and the
//! manual sync workspace.

pub mod detection;
pub mod maps;
pub mod matching;
pub mod manual;
pub mod readout;
pub mod timeline;

use std::sync::Arc;
use detection::{create_sync_input, detect_activity_events, DetectionSettings, LocationEvent, SyncInput};
use manual::CandidateStatus;
use matching::{match_candidates, MatchScope};
use ovrley_core::activity::schema::ParsedActivity;
use ovrley_core::encode::timing::Fps;
use ovrley_core::media::sync::{LandmarkKind, MAX_MANUAL_LANDMARKS};
use ovrley_core::media::edits::TimezoneMode;
use crate::i18n::catalog;
use crate::store::Store;
use crate::store::agent::AgentReply;
use crate::store::services::Services;
use crate::video::{Clip, ImportedVideo, VideoEditsMap};
use crate::video::collection::{join_video_edits, sort_clips_by_timeline};
use crate::video::sync::{parse_video_filename_creation_time, SyncWarning};

/// The automatic sync controls' state.
#[derive(Clone, Debug, PartialEq)]
pub struct VideoSyncControls {
    pub can_reset_creation_time: bool,
    pub filename_creation_time_available: bool,
    /// Where the earliest video starts, on that video's frames.
    pub shared_offset_seconds: f64,
    pub shared_offset_fps: Option<Fps>,
    /// The activity's timezone; Apply Timezone shows only with one.
    pub timezone: Option<String>,
    pub timezone_disabled: bool,
    /// Apply Timezone is on when every video with a camera clock reads it as UTC.
    pub utc: bool,
    pub warning: Option<SyncWarning>,
}

pub fn derive_video_sync_controls(videos: &[ImportedVideo], edits: &VideoEditsMap, timezone: Option<&str>) -> VideoSyncControls {
    let clips = join_video_edits(videos, edits);
    let ordered = sort_clips_by_timeline(&clips);
    let camera_clocks: Vec<_> = ordered.iter().filter(|clip| matches!(clip.video.time_source.as_deref(), Some("ffprobe" | "filename"))).collect();
    VideoSyncControls {
        can_reset_creation_time: videos.iter().any(|video| video.time_source.as_deref() == Some("filename")),
        filename_creation_time_available: videos.iter().any(|video| parse_video_filename_creation_time(&video.path).is_some()),
        shared_offset_seconds: ordered.first().map_or(0.0, |clip| clip.offset_seconds()),
        shared_offset_fps: ordered.first().map(|clip| clip.fps()),
        timezone: timezone.map(str::to_string),
        timezone_disabled: !videos.is_empty() && videos.iter().all(|video| video.time_source.as_deref() == Some("filename")),
        utc: !camera_clocks.is_empty() && camera_clocks.iter().all(|clip| clip.edits.timezone_mode == Some(TimezoneMode::Utc)),
        warning: ordered.iter().find_map(|clip| clip.video.sync_warning),
    }
}

/// The gap between the two screens, in scene units.
pub const PREVIEW_SCREEN_GAP: f64 = 16.0;

/// The store's collection, with the external activity's timezone.
pub fn store_video_sync_controls(store: &Store) -> VideoSyncControls {
    let timezone = store.media.parsed_activity.as_ref().and(store.media.activity_summary.as_ref()).and_then(|summary| summary.timezone.as_deref());
    derive_video_sync_controls(&store.video_import.imported_videos, &store.video_import.video_edits, timezone)
}

/// What a detection reads.
#[derive(Clone, Debug)]
struct DetectionInputs {
    activity: Option<Arc<ParsedActivity>>,
    settings: DetectionSettings,
    location_second: Option<f64>,
}

impl PartialEq for DetectionInputs {
    fn eq(&self, other: &Self) -> bool {
        let same_activity = match (&self.activity, &other.activity) {
            (Some(left), Some(right)) => Arc::ptr_eq(left, right),
            (None, None) => true,
            _ => false,
        };
        same_activity && self.settings == other.settings && self.location_second == other.location_second
    }
}

/// The workspace's state.
#[derive(Debug, Default)]
pub struct VideoSyncWorkspace {
    /// The detector input, cached per activity pointer.
    input: Option<(Arc<ParsedActivity>, Arc<SyncInput>)>,
    /// The inputs the last detection (or rerun) acted on.
    detected: Option<DetectionInputs>,
    /// An agent waiting for the running search.
    search_reply: Option<AgentReply>,
    /// The sensitivity sliders' drafts while they move; `None` shows the committed value.
    pub speed_draft_kmh: Option<f64>,
    pub turn_draft_degrees: Option<f64>,
    /// The committed thresholds the drafts follow.
    drafts_follow: Option<(f64, f64)>,
    /// A landmark handle being dragged on the timeline.
    pub landmark_drag: Option<timeline::LandmarkDrag>,
    /// Both maps, while the mode is on.
    pub maps: Option<maps::SyncMaps>,
    /// The speed readout, while the mode is on and the activity has speed.
    pub speed: Option<readout::SpeedReadout>,
}

// --- The workspace ----------------------------------------------------------------------------

/// A slider moved: its draft shows. Release commits it.
pub fn set_speed_draft(store: &mut Store, kmh: f64) {
    store.video_sync.speed_draft_kmh = Some(kmh);
}

pub fn set_turn_draft(store: &mut Store, degrees: f64) {
    store.video_sync.turn_draft_degrees = Some(degrees);
}

/// The drafts follow the committed thresholds, the reconcile step's last part.
fn follow_committed_thresholds(store: &mut Store) {
    let committed = (store.manual_video_sync.speed_threshold_kmh, store.manual_video_sync.turn_threshold_degrees);
    let previous = store.video_sync.drafts_follow.replace(committed);
    if let Some((speed, turn)) = previous {
        if speed != committed.0 {
            store.video_sync.speed_draft_kmh = None;
        }
        if turn != committed.1 {
            store.video_sync.turn_draft_degrees = None;
        }
    }
}

/// The clip the workspace acts on: the selected one, even with the playhead outside it, else the
/// one the clip commands at the playhead act on. Marking needs the playhead inside it.
pub fn work_clip(store: &Store) -> Option<&Clip> {
    match crate::features::toolbar::selected_video_id(store) {
        Some(id) => store.player.timeline.timeline().committed.clips.iter().find(|clip| clip.id() == id),
        None => crate::features::player::edits::resolve_edit_target(store).map(|(clip, _)| clip),
    }
}

/// The file name of the video that holds the sync while the workspace acts on another one: marking
/// is disabled, and the drawer offers to clear the landmarks or to go to that video.
pub fn held_by_other_video(store: &Store) -> Option<String> {
    let target = manual::find_video_sync_target(store)?;
    if work_clip(store).is_some_and(|clip| clip.id() == target.id()) {
        return None;
    }
    Some(crate::dialogs::filename_from_selected_path(&target.video.path).unwrap_or_default().to_string())
}

/// "Go to that video": selects the video that holds the sync and brings the playhead into it.
pub fn show_sync_video(store: &mut Store) -> Result<(), String> {
    let target = manual::find_video_sync_target(store).ok_or("Manual video sync has no landmarks")?;
    crate::store::videos::select_clip_revealing_playhead(store, target.id())
}

/// The four mark buttons' state at the playhead.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MarkControls {
    pub can_mark: bool,
    pub can_mark_location: bool,
    /// The tooltip of the stop and turn buttons when they are disabled.
    pub mark_disabled_reason: Option<String>,
    pub location_disabled_reason: Option<String>,
}

pub fn mark_controls(store: &Store) -> MarkControls {
    let language = store.i18n.language;
    let t = |key: &str| catalog().t(language, key).to_string();
    let work = work_clip(store);
    let held_by = held_by_other_video(store);
    let landmarks = &store.manual_video_sync.landmarks;
    let has_location = landmarks.iter().any(|landmark| landmark.kind() == LandmarkKind::Location);
    let has_capacity = landmarks.len() < MAX_MANUAL_LANDMARKS;
    let inside = work.is_some_and(|clip| {
        let second = store.editor.selected_second - clip.offset_seconds();
        second >= 0.0 && second < clip.duration_seconds()
    });
    let mark_disabled_reason = if !store.video_import.has_videos() {
        Some(t("videoSync.videoRequired"))
    } else if let Some(name) = &held_by {
        Some(catalog().t_with(language, "videoSync.landmarksInOtherVideo", &[("name", name)]))
    } else if !inside {
        Some(t("videoSync.playheadInsideVideo"))
    } else if !has_capacity {
        Some(t("videoSync.landmarkLimit"))
    } else {
        None
    };
    let can_mark = inside && held_by.is_none() && has_capacity;
    MarkControls {
        can_mark,
        can_mark_location: can_mark && !has_location,
        location_disabled_reason: if has_location && mark_disabled_reason.is_none() { Some(t("videoSync.locationLimit")) } else { mark_disabled_reason.clone() },
        mark_disabled_reason,
    }
}

/// A mark button: a landmark in the work video at the playhead's video second.
pub fn add_landmark_at_playhead(store: &mut Store, kind: LandmarkKind) -> Result<String, String> {
    let marks = mark_controls(store);
    let disabled = if kind == LandmarkKind::Location { marks.location_disabled_reason } else { marks.mark_disabled_reason };
    if let Some(reason) = disabled {
        return Err(reason);
    }
    let (video_id, offset) = work_clip(store).map(|clip| (clip.id().to_string(), clip.offset_seconds())).expect("marking is enabled only on a video");
    manual::add_landmark(store, &video_id, kind, store.editor.selected_second - offset)
}

/// A landmark type's label key (`VIDEO_SYNC_LANDMARK_PRESENTATION`).
pub fn landmark_label_key(kind: LandmarkKind) -> &'static str {
    match kind {
        LandmarkKind::Stop => "videoSync.stop",
        LandmarkKind::LeftTurn => "videoSync.leftTurn",
        LandmarkKind::RightTurn => "videoSync.rightTurn",
        LandmarkKind::Location => "videoSync.location",
    }
}

/// One landmark row of the drawer.
#[derive(Clone, Debug, PartialEq)]
pub struct LandmarkCard {
    pub id: String,
    pub kind: LandmarkKind,
    pub label: String,
    pub video_time_label: String,
}

/// One candidate card.
#[derive(Clone, Debug, PartialEq)]
pub struct CandidateCard {
    pub offset_label: String,
    /// "{matched} / {eligible} landmarks matched", or "Location only".
    pub subline: String,
    pub score: Option<f64>,
    /// `hsl(score × 1.2, 72%, 48%)`'s hue.
    pub score_hue: f64,
    pub applied: bool,
}

/// One scope's list (`VideoSyncCandidateList`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CandidateListView {
    /// Hidden until a search ran.
    pub shown: bool,
    pub error: Option<String>,
    pub stale: bool,
    /// Fresh and empty: "No candidate aligns at least two landmarks".
    pub empty: bool,
    /// Cards apply only from a fresh result.
    pub disabled: bool,
    pub cards: Vec<CandidateCard>,
}

/// The detected events' counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DetectionCounts {
    pub left_turns: usize,
    pub stops: usize,
    pub right_turns: usize,
    pub locations: usize,
}

/// The Manual Sync tab.
#[derive(Clone, Debug, PartialEq)]
pub struct DrawerView {
    pub landmarks: Vec<LandmarkCard>,
    pub has_location_landmark: bool,
    pub speed_draft_kmh: f64,
    pub turn_draft_degrees: f64,
    pub speed_label: String,
    pub turn_label: String,
    pub counts: DetectionCounts,
    pub eligibility: CalculationEligibility,
    pub calculating: bool,
    pub location: CandidateListView,
    pub all: CandidateListView,
    /// `held_by_other_video`: the searches and candidates give way to clearing the landmarks or
    /// going to that video.
    pub held_by_other_video: Option<String>,
}

fn candidate_list(store: &Store, scope: MatchScope, applied_offset: f64) -> CandidateListView {
    let language = store.i18n.language;
    let result = store.manual_video_sync_results.get(scope);
    let cards = result
        .candidates
        .iter()
        .map(|candidate| CandidateCard {
            offset_label: crate::format::format_clock_duration(candidate.offset),
            subline: if candidate.variant == matching::CandidateVariant::LocationOnly {
                catalog().t(language, "videoSync.locationOnly").to_string()
            } else {
                catalog().t_with(language, "videoSync.landmarksMatched", &[("matched", &candidate.matched_count.to_string()), ("eligible", &candidate.eligible_count.to_string())])
            },
            score: candidate.match_score,
            score_hue: candidate.match_score.unwrap_or(0.0) * 1.2,
            applied: candidate.offset == applied_offset,
        })
        .collect::<Vec<_>>();
    CandidateListView {
        shown: result.is_shown(),
        error: (result.status == CandidateStatus::Error).then(|| result.error.clone().unwrap_or_default()),
        stale: result.status == CandidateStatus::Stale,
        empty: result.has_searched && result.status == CandidateStatus::Fresh && cards.is_empty(),
        disabled: result.status != CandidateStatus::Fresh,
        cards,
    }
}

pub fn drawer_view(store: &Store) -> DrawerView {
    let language = store.i18n.language;
    let mut landmarks: Vec<&ovrley_core::media::sync::Landmark> = store.manual_video_sync.landmarks.iter().collect();
    landmarks.sort_by(|left, right| left.video_second().total_cmp(&right.video_second()).then_with(|| crate::format::compare_text(left.id(), right.id())));
    let detection = store.manual_video_sync_detection.as_deref();
    let counts = detection.map_or_else(DetectionCounts::default, |detection| DetectionCounts {
        left_turns: detection.turn_count(detection::TurnDirection::Left),
        stops: detection.stops.len(),
        right_turns: detection.turn_count(detection::TurnDirection::Right),
        locations: usize::from(detection.location.is_some()),
    });
    let speed = store.video_sync.speed_draft_kmh.unwrap_or(store.manual_video_sync.speed_threshold_kmh);
    let turn = store.video_sync.turn_draft_degrees.unwrap_or(store.manual_video_sync.turn_threshold_degrees);
    let applied = manual::find_video_sync_target(store).map_or(0.0, |clip| clip.offset_seconds());
    DrawerView {
        landmarks: landmarks
            .iter()
            .map(|landmark| LandmarkCard {
                id: landmark.id().to_string(),
                kind: landmark.kind(),
                label: catalog().t(language, landmark_label_key(landmark.kind())).to_string(),
                video_time_label: crate::format::format_clock_duration(landmark.video_second()),
            })
            .collect(),
        has_location_landmark: store.manual_video_sync.landmarks.iter().any(|landmark| landmark.kind() == LandmarkKind::Location),
        speed_draft_kmh: speed,
        turn_draft_degrees: turn,
        speed_label: format!("{speed:.1}km/h"),
        turn_label: format!("{turn:.1}°"),
        counts,
        eligibility: calculation_eligibility(store),
        calculating: store.manual_video_sync_results.is_calculating(),
        location: candidate_list(store, MatchScope::Location, applied),
        all: candidate_list(store, MatchScope::All, applied),
        held_by_other_video: held_by_other_video(store),
    }
}

/// A candidate card: applies that scope's candidate at `index` of its fresh result.
pub fn apply_candidate_at(store: &mut Store, scope: MatchScope, index: usize) -> Result<(), String> {
    let offset = store.manual_video_sync_results.get(scope).candidates.get(index).map(|candidate| candidate.offset).ok_or("Manual video sync candidate is not part of the current result")?;
    manual::apply_candidate(store, scope, offset)
}

/// The external activity's detector input, built once per activity.
fn sync_input(store: &mut Store) -> Option<Arc<SyncInput>> {
    let activity = store.media.parsed_activity.as_ref()?.activity.clone();
    match &store.video_sync.input {
        Some((cached, input)) if Arc::ptr_eq(cached, &activity) => Some(input.clone()),
        _ => {
            let input = Arc::new(create_sync_input(&activity));
            store.video_sync.input = Some((activity, input.clone()));
            Some(input)
        }
    }
}

fn settings(store: &Store) -> DetectionSettings {
    DetectionSettings { speed_threshold_kmh: store.manual_video_sync.speed_threshold_kmh, turn_threshold_degrees: store.manual_video_sync.turn_threshold_degrees }
}

fn location(store: &Store) -> Option<LocationEvent> {
    store.manual_video_sync_detection.as_ref().and_then(|detection| detection.location)
}

/// Whether each search may run, and why "Sync All Landmarks" may not.
#[derive(Clone, Debug, PartialEq)]
pub struct CalculationEligibility {
    pub can_calculate_all: bool,
    pub can_calculate_location: bool,
    pub all_explanation: Option<String>,
}

/// The searches' eligibility.
pub fn calculation_eligibility(store: &Store) -> CalculationEligibility {
    let has_video = store.video_import.has_videos();
    let domain = matching::eligibility(&store.manual_video_sync.landmarks, store.manual_video_sync_detection.as_deref());
    let language = store.i18n.language;
    let all_explanation = if !has_video {
        Some(catalog().t(language, "videoSync.videoRequiredForSync").to_string())
    } else if !domain.can_match_all {
        let key = if store.media.parsed_activity.is_none() { "videoSync.activityRequiredForSync" } else { "videoSync.minimumLandmarksForSync" };
        Some(catalog().t(language, key).to_string())
    } else {
        None
    };
    CalculationEligibility { can_calculate_all: has_video && domain.can_match_all, can_calculate_location: has_video && domain.can_match_location, all_explanation }
}

/// Starts a search for `scope` on a worker. Fails with the button's explanation when the scope
/// is not eligible, or while a search runs. `reply` answers once the result is in.
pub fn calculate(store: &mut Store, services: &mut Services, scope: MatchScope, reply: Option<AgentReply>) -> Result<(), String> {
    let eligibility = calculation_eligibility(store);
    match scope {
        MatchScope::All if !eligibility.can_calculate_all => return Err(eligibility.all_explanation.unwrap_or_default()),
        MatchScope::Location if !eligibility.can_calculate_location => return Err("Match Location Only needs a location landmark and a course location on the map".into()),
        _ => {}
    }
    let input = sync_input(store);
    let Some(revision) = manual::begin_calculation(store, scope) else {
        return Err("A sync search is already running".into());
    };
    store.video_sync.search_reply = reply;
    let landmarks = store.manual_video_sync.landmarks.clone();
    let detection = store.manual_video_sync_detection.as_deref().cloned().unwrap_or_default();
    let (settings, location) = (settings(store), location(store));
    services.workers.spawn("video-sync-search", move || {
        let detection = match (scope, input) {
            (MatchScope::All, Some(input)) => detect_activity_events(&input, settings, location),
            _ => detection,
        };
        let outcome = match_candidates(&landmarks, &detection, scope);
        Box::new(move |store: &mut Store, _: &mut Services| finish_search(store, scope, revision, detection, outcome))
    });
    Ok(())
}

/// One search's result as agents read it: its status, error and candidates.
pub fn describe_result(store: &Store, scope: MatchScope) -> serde_json::Value {
    use serde_json::json;
    let result = store.manual_video_sync_results.get(scope);
    let applied = manual::find_video_sync_target(store).map(|clip| clip.offset_seconds());
    let candidates: Vec<serde_json::Value> = result
        .candidates
        .iter()
        .map(|candidate| {
            json!({
                "offsetSeconds": candidate.offset,
                "score": candidate.match_score,
                "matchedCount": candidate.matched_count,
                "eligibleCount": candidate.eligible_count,
                "variant": match candidate.variant {
                    matching::CandidateVariant::Ordinary => "ordinary",
                    matching::CandidateVariant::LocationConflict => "locationConflict",
                    matching::CandidateVariant::LocationOnly => "locationOnly",
                },
                "applied": applied == Some(candidate.offset),
            })
        })
        .collect();
    json!({ "scope": scope, "status": result.status.name(), "error": result.error, "candidates": candidates })
}

/// The manual sync as agents read it (`video_sync_get`).
pub fn describe_video_sync(store: &Store) -> serde_json::Value {
    use serde_json::json;
    let target = manual::find_video_sync_target(store);
    let offset = target.as_ref().map_or(0.0, |clip| clip.offset_seconds());
    let landmarks: Vec<serde_json::Value> = store
        .manual_video_sync
        .landmarks
        .iter()
        .map(|landmark| json!({ "id": landmark.id(), "type": landmark.kind(), "videoSecond": landmark.video_second(), "timelineSecond": offset + landmark.video_second() }))
        .collect();
    let detection = store.manual_video_sync_detection.as_deref();
    let eligibility = calculation_eligibility(store);
    let (speed, turn) = (manual::SPEED_THRESHOLD_RANGE, manual::TURN_THRESHOLD_RANGE);
    json!({
        "targetVideo": target.as_ref().map(|clip| json!({ "id": clip.id(), "offsetSeconds": clip.offset_seconds(), "durationSeconds": clip.duration_seconds() })),
        "markVideoId": work_clip(store).map(Clip::id),
        "landmarks": landmarks,
        "detectedLocationSecond": store.manual_video_sync.detected_location_second,
        "sensitivity": {
            "speedThresholdKmh": store.manual_video_sync.speed_threshold_kmh,
            "speedRange": [speed.0, speed.1],
            "turnThresholdDegrees": store.manual_video_sync.turn_threshold_degrees,
            "turnRange": [turn.0, turn.1],
        },
        "detection": detection.map(|detection| json!({
            "speedAvailable": detection.availability.speed,
            "headingAvailable": detection.availability.heading,
            "stops": detection.stops.len(),
            "leftTurns": detection.turn_count(detection::TurnDirection::Left),
            "rightTurns": detection.turn_count(detection::TurnDirection::Right),
        })),
        "searches": {
            "all": { "eligible": eligibility.can_calculate_all, "reason": eligibility.all_explanation, "result": describe_result(store, MatchScope::All) },
            "location": { "eligible": eligibility.can_calculate_location, "result": describe_result(store, MatchScope::Location) },
        },
    })
}

/// A search's result arrives: applied when its revision is current, else dropped. A waiting agent
/// learns which.
fn finish_search(store: &mut Store, scope: MatchScope, revision: u64, detection: detection::Detection, outcome: Result<Vec<matching::Candidate>, String>) {
    let applied = match outcome {
        Ok(candidates) => manual::complete_calculation(store, scope, revision, detection, candidates),
        Err(message) => manual::fail_calculation(store, scope, revision, message),
    };
    if let Some(reply) = store.video_sync.search_reply.take() {
        let result = store.manual_video_sync_results.get(scope);
        reply.send(match (applied, result.status) {
            (false, _) => Err("The landmarks or the sensitivity changed while the search ran, so its result was dropped. Run it again.".into()),
            (true, CandidateStatus::Error) => Err(result.error.clone().unwrap_or_default()),
            (true, _) => Ok(describe_result(store, scope)),
        });
    }
}

/// The `video_sync` reconcile step, first half: detection refreshes by itself when the
/// activity, a threshold or the detected location changed, and after a threshold commit a search
/// that ran runs again. Nothing runs while a search does; its inputs are read again after it.
pub fn reconcile_video_sync(store: &mut Store, services: &mut Services) {
    reconcile_calculation(store, services);
    // Leaving the mode ends a landmark drag.
    if !crate::features::player::video_sync_mode(store) {
        store.video_sync.landmark_drag = None;
    }
    follow_committed_thresholds(store);
    maps::reconcile_sync_maps(store, services, std::time::Instant::now());
    readout::reconcile_speed_readout(store, services);
}

fn reconcile_calculation(store: &mut Store, services: &mut Services) {
    let inputs = DetectionInputs {
        activity: store.media.parsed_activity.as_ref().map(|activity| activity.activity.clone()),
        settings: settings(store),
        location_second: store.manual_video_sync.detected_location_second,
    };
    if store.video_sync.detected.as_ref() == Some(&inputs) || store.manual_video_sync_results.is_calculating() {
        return;
    }
    let previous = store.video_sync.detected.replace(inputs.clone());
    let Some(activity) = &inputs.activity else { return };
    let rerun = previous.is_some_and(|previous| {
        previous.settings != inputs.settings && previous.activity.as_ref().is_some_and(|previous| Arc::ptr_eq(previous, activity))
    }) && store.manual_video_sync_results.all.has_searched
        && calculation_eligibility(store).can_calculate_all;
    if rerun {
        let _ = calculate(store, services, MatchScope::All, None);
        return;
    }
    // Inline: a detection takes about 0.5 ms on an 8-hour ride.
    let input = sync_input(store).expect("an activity has an input");
    let detection = detect_activity_events(&input, inputs.settings, location(store));
    manual::complete_detection(store, detection);
}

#[cfg(test)]
mod tests;
