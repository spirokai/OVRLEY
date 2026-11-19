//! Sync Doctor (`features/video-sync`): the automatic sync controls the import dialog and the video
//! panel share (`deriveVideoSyncControls`), and the manual sync workspace (Phase 8).

pub mod detection;
pub mod matching;

use crate::i18n::catalog;
use crate::store::agent::AgentReply;
use crate::store::services::Services;
use crate::store::slices::manual_video_sync::{self as slice, CandidateStatus};
use crate::store::Store;
use detection::{create_sync_input, detect_activity_events, DetectionSettings, LocationEvent, SyncInput};
use matching::{match_candidates, MatchScope};
use ovrley_core::activity::schema::ParsedActivity;
use std::sync::Arc;

use crate::lib::video_program::collection::{join_video_edits, sort_clips_by_timeline};
use crate::lib::video_program::{ImportedVideo, VideoEditsMap};
use crate::lib::video_sync::{parse_video_filename_creation_time, SyncWarning};
use ovrley_core::encode::fps::Fps;
use ovrley_core::media::video_edits::TimezoneMode;

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

/// `deriveVideoSyncControls`.
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

/// What a detection reads (`useVideoSyncCalculation`'s effect inputs).
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

/// The workspace's hook state (`useVideoSyncWorkspace`, `useVideoSyncCalculation`).
#[derive(Debug, Default)]
pub struct VideoSyncWorkspace {
    /// The detector input, cached per activity pointer.
    input: Option<(Arc<ParsedActivity>, Arc<SyncInput>)>,
    /// The inputs the last detection (or rerun) acted on.
    detected: Option<DetectionInputs>,
    /// An agent waiting for the running search (D13).
    search_reply: Option<AgentReply>,
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

/// Whether each search may run, and why "Sync All Landmarks" may not (`useVideoSyncCalculation`).
#[derive(Clone, Debug, PartialEq)]
pub struct CalculationEligibility {
    pub can_calculate_all: bool,
    pub can_calculate_location: bool,
    pub all_explanation: Option<String>,
}

/// The searches' eligibility.
pub fn calculation_eligibility(store: &Store) -> CalculationEligibility {
    let has_video = store.manual_video_sync.video_id.is_some();
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

/// Starts a search for `scope` on a worker (D3). Fails with the button's explanation when the scope
/// is not eligible, or while a search runs. `reply` answers once the result is in.
pub fn calculate(store: &mut Store, services: &mut Services, scope: MatchScope, reply: Option<AgentReply>) -> Result<(), String> {
    let eligibility = calculation_eligibility(store);
    match scope {
        MatchScope::All if !eligibility.can_calculate_all => return Err(eligibility.all_explanation.unwrap_or_default()),
        MatchScope::Location if !eligibility.can_calculate_location => return Err("Match Location Only needs a location landmark and a course location on the map".into()),
        _ => {}
    }
    let input = sync_input(store);
    let Some(revision) = slice::begin_calculation(store, scope) else {
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

/// A search's result arrives: applied when its revision is current, else dropped. A waiting agent
/// learns which.
fn finish_search(store: &mut Store, scope: MatchScope, revision: u64, detection: detection::Detection, outcome: Result<Vec<matching::Candidate>, String>) {
    let applied = match outcome {
        Ok(candidates) => slice::complete_calculation(store, scope, revision, detection, candidates),
        Err(message) => slice::fail_calculation(store, scope, revision, message),
    };
    if let Some(reply) = store.video_sync.search_reply.take() {
        let result = store.manual_video_sync_results.get(scope);
        reply.send(match (applied, result.status) {
            (false, _) => Err("The landmarks or the sensitivity changed while the search ran, so its result was dropped. Run it again.".into()),
            (true, CandidateStatus::Error) => Err(result.error.clone().unwrap_or_default()),
            (true, _) => Ok(serde_json::Value::Null),
        });
    }
}

/// The `video_sync` reconcile step, first half (D3): detection refreshes by itself when the
/// activity, a threshold or the detected location changed, and after a threshold commit a search
/// that ran runs again. Nothing runs while a search does; its inputs are read again after it.
pub fn reconcile_video_sync(store: &mut Store, services: &mut Services) {
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
    // Inline: a detection takes about 0.5 ms on an 8-hour ride (step 0, item 6).
    let input = sync_input(store).expect("an activity has an input");
    let detection = detect_activity_events(&input, inputs.settings, location(store));
    slice::complete_detection(store, detection);
}

#[cfg(test)]
mod tests;
