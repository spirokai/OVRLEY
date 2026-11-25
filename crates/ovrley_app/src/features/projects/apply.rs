//! A loaded project, or a new one, applied to the store in one action.
//!
//! The callers run these under `HistoryMode::Reset`. Everything that can
//! fail is checked while the project is prepared, so applying it never leaves the session half
//! replaced.

use std::rc::Rc;
use ovrley_core::activity::trim::ActivityTrim;
use ovrley_core::encode::ffmpeg::transitions::transition_defaults;
use ovrley_core::encode::render_settings::RenderSettings;
use ovrley_core::media::manual_video_sync::ManualVideoSync;
use ovrley_services::project::{ProjectDocument, ProjectVideo};
use ovrley_widgets::model::document::OverlayConfig;
use serde_json::{Map, Value};
use crate::activity::{trim_activity_edge, MIN_ACTIVITY_TRIM_SECONDS, TrimEdge};
use crate::features::player::playback::{pause, timeline_minimum};
use crate::features::player::viewport::clamp_to_view;
use crate::features::shell::{self, activate_parsed_activity};
use crate::features::sync::manual::{hydrate_video_sync_state, reset_video_sync_state};
use crate::store::Store;
use crate::store::editor::{PreviewPlaybackState, TimelineViewport};
use crate::store::media::{ActivitySource, ImportedActivity};
use crate::store::template::{create_new_template, hydrate_template_state};
use crate::store::videos::{clear_imported_videos, commit_video_collection, select_clip, ClipLane, SelectedClip};
use crate::video::{ImportedVideo, VideoEditsMap};
use crate::video::collection::{sort_clips_by_timeline, timeline_bounds};
use crate::video::program::PlaybackSource;
use crate::video::segments::fit_cuts_to_frame_count;

/// The timeline a new project starts with when there is no media.
const NEW_PROJECT_DURATION_SECONDS: f64 = 73.0;

/// The saved edits of every loaded video, missing ones included. A
/// found file shorter than the saved video keeps only the cuts inside it. An anchor to a video left
/// out at load (an older project's missing video) goes; the clip keeps its offset.
pub fn create_project_video_edits(
    videos: &[ImportedVideo],
    project_videos: &[ProjectVideo],
) -> Result<VideoEditsMap, String> {
    videos
        .iter()
        .map(|video| {
            let project_video = project_videos
                .iter()
                .find(|candidate| candidate.id == video.id)
                .ok_or_else(|| {
                    format!("Imported video is not part of the project: {}", video.id)
                })?;
            let mut edits = fit_cuts_to_frame_count(&project_video.edits, video.frame_count);
            if edits.placement_anchor.as_ref().is_some_and(|anchor| {
                !videos
                    .iter()
                    .any(|candidate| candidate.id == anchor.video_id)
            }) {
                edits.placement_anchor = None;
            }
            Ok((video.id.clone(), edits))
        })
        .collect()
}

/// When its video was left out at load, its landmarks go and the
/// first loaded video is selected; a found file shorter than the saved video keeps the landmarks
/// inside it.
pub fn create_project_manual_video_sync(
    videos: &[ImportedVideo],
    manual: &ManualVideoSync,
) -> ManualVideoSync {
    let mut manual = manual.clone();
    if let Some(video) = videos.iter().find(|video| Some(&video.id) == manual.video_id.as_ref()) {
        manual.landmarks.retain(|landmark| landmark.video_second() <= video.duration_seconds);
    } else {
        manual.video_id = videos.first().map(|video| video.id.clone());
        manual.landmarks.clear();
    }
    manual
}

/// Laps belong to the activity file; one loaded as absent takes its lap
/// selection with it.
pub fn resolve_disabled_laps(
    disabled: &[usize],
    activity: Option<&ImportedActivity>,
) -> Result<Vec<usize>, String> {
    let Some(activity) = activity else {
        return Ok(Vec::new());
    };
    if disabled
        .last()
        .is_some_and(|last| *last >= activity.activity.lap_start_elapsed_seconds.len())
    {
        return Err("Project laps.disabled must identify laps of the loaded activity".into());
    }
    Ok(disabled.to_vec())
}

/// Kept inside the loaded activity and at least the minimum length apart; a
/// trim of an activity that is gone, or too short to trim, goes.
fn resolve_activity_trim(
    trim: Option<ActivityTrim>,
    activity_duration: f64,
) -> Option<ActivityTrim> {
    let trim = trim?;
    if activity_duration <= MIN_ACTIVITY_TRIM_SECONDS {
        return None;
    }
    let ended = trim_activity_edge(None, activity_duration, TrimEdge::End, trim.end_second);
    trim_activity_edge(ended, activity_duration, TrimEdge::Start, trim.start_second)
}

/// A project read, its sources loaded and its saved state fitted to them, ready to commit.
pub struct PreparedProject {
    /// The document as read, its editor normalized and its loaded videos described as loaded
    ///: the saved baseline.
    pub document: ProjectDocument,
    /// The editor's document through the template ingress, the raster load results attached.
    pub config: OverlayConfig,
    pub global_defaults: Map<String, Value>,
    /// The activity file and its parsed activity, when the project has one that was found.
    pub activity: Option<(String, ImportedActivity)>,
    pub videos: Vec<ImportedVideo>,
    pub edits: VideoEditsMap,
    pub manual_video_sync: ManualVideoSync,
    pub disabled_laps: Vec<usize>,
    /// The video folder of a version 3 to 5 batch project, which is no longer imported.
    pub legacy_batch_video_folder: Option<String>,
}

/// The activity is activated before the videos join, so detecting
/// video times on activation never moves the saved placements; then the project-owned state. The
/// first video on the timeline starts selected.
pub fn apply_project(store: &mut Store, prepared: PreparedProject) {
    pause(store);
    store.media.activity_source = None;
    store.media.parsed_activity = None;
    store.media.activity_summary = None;
    clear_imported_videos(store);
    if let Some((path, activity)) = prepared.activity {
        activate_parsed_activity(store, ActivitySource::File { path }, activity);
    }
    commit_video_collection(store, prepared.videos, prepared.edits);
    apply_owned_state(
        store,
        prepared.document,
        prepared.config,
        prepared.global_defaults,
        prepared.manual_video_sync,
        prepared.disabled_laps,
    );
    let first = sort_clips_by_timeline(&store.video_import.clips())
        .first()
        .map(|clip| SelectedClip { lane: ClipLane::Video(clip.id().to_string()), segment_key: None });
    select_clip(store, first).expect("the first clip is imported");
}

/// The editor, the settings and the timeline, after the media.
fn apply_owned_state(
    store: &mut Store,
    document: ProjectDocument,
    config: OverlayConfig,
    global_defaults: Map<String, Value>,
    manual: ManualVideoSync,
    disabled_laps: Vec<usize>,
) {
    hydrate_video_sync_state(store, manual);
    let bounds = timeline_bounds(&store.video_import.clips());
    let minimum = timeline_minimum(bounds);
    let activity_duration = store
        .media
        .activity_summary
        .as_ref()
        .map_or(0.0, |summary| summary.duration_seconds);
    let video_end = bounds.map_or(0.0, |bounds| bounds.end_second);
    let timeline_end = activity_duration.max(video_end).max(minimum + 0.001);
    let timeline = document.timeline;

    store.editor.set_config(config);
    store.template.global_defaults = Rc::new(global_defaults);
    store.template.loaded_template_source = None;
    store.template.last_saved_template_state = None;
    store.media.disabled_laps = disabled_laps;
    store.video_import.transition_defaults = document.transition_defaults;
    store.video_import.video_drag_preview = None;
    store.video_import.cut_removal_request = None;
    store.render_settings = document.render.settings;
    store.media.activity_trim = resolve_activity_trim(document.activity_trim, activity_duration);
    store.batch_render.batch_output_folder = document.render.batch_output_folder;
    store.editor.selected_second =
        timeline.playhead_second.max(minimum).min(timeline_end);
    store.editor.timeline_viewport = clamp_to_view(
        timeline.view_start,
        timeline.view_end,
        timeline_end,
        minimum,
    );
    store.editor.skip_next_timeline_viewport_reset = true;
    store.editor.preview_playback_state = PreviewPlaybackState::Paused;
    store.editor.preview_playback_source = PlaybackSource::Timeline;
}

/// A new project: the loaded template's last saved state, or a
/// new template; no activity, no videos, the 73 s timeline and the default settings.
pub fn apply_new_project(store: &mut Store) -> Result<(), String> {
    let template = match (
        &store.template.loaded_template_source,
        &store.template.last_saved_template_state,
    ) {
        (Some(source), Some(state)) => Some((source.clone(), state.clone())),
        (Some(_), None) => return Err("the loaded template has no saved widget state".into()),
        (None, _) => None,
    };
    crate::features::import::clear_videos(store);
    shell::clear_activity_file(store);
    match template {
        Some((source, state)) => hydrate_template_state(
            &mut store.editor,
            &mut store.template,
            state.config,
            state.global_defaults,
            Some(source),
        ),
        None => create_new_template(&mut store.editor, &mut store.template),
    }
    store.media.activity_source = None;
    store.media.parsed_activity = None;
    store.media.activity_summary = None;
    store.editor.fallback_duration_seconds = NEW_PROJECT_DURATION_SECONDS;
    store.editor.selected_second = 0.0;
    store.editor.timeline_viewport = TimelineViewport {
        view_start: 0.0,
        view_end: NEW_PROJECT_DURATION_SECONDS,
    };
    store.editor.preview_playback_state = PreviewPlaybackState::Paused;
    store.editor.preview_playback_source = PlaybackSource::Timeline;
    store.video_import.video_drag_preview = None;
    store.video_import.transition_defaults = transition_defaults().clone();
    store.render_settings = RenderSettings::default();
    store.media.clear_error();
    store.batch_render.batch_output_folder = None;
    reset_video_sync_state(store);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::fixtures::{cut, cut_edits, simple};
    use ovrley_core::media::video_edits::{AnchorSide, PlacementAnchor, SegmentEdits, VideoEdits};
    use ovrley_services::project::PathLocator;

    fn project_video(id: &str, edits: VideoEdits) -> ProjectVideo {
        ProjectVideo {
            id: id.into(),
            path: PathLocator::Absolute(format!("/videos/{id}.mp4")),
            edits,
            metadata: None,
        }
    }

    #[test]
    fn a_shorter_file_keeps_the_cuts_inside_it() {
        let saved = cut_edits(
            &[
                cut("c1", 300, SegmentEdits::INCLUDED),
                cut("c2", 1500, SegmentEdits::INCLUDED),
            ],
            SegmentEdits::INCLUDED,
        );
        let edits =
            create_project_video_edits(&[simple("a", 20.0, 0)], &[project_video("a", saved)])
                .unwrap();
        assert_eq!(
            edits["a"]
                .cuts
                .iter()
                .map(|cut| cut.frame)
                .collect::<Vec<_>>(),
            [300],
            "frame 1500 lies past 600 frames"
        );
    }

    #[test]
    fn an_anchor_to_a_video_left_out_goes() {
        let anchored = VideoEdits {
            placement_anchor: Some(PlacementAnchor {
                video_id: "gone".into(),
                side: AnchorSide::After,
            }),
            offset_seconds: 12.0,
            ..VideoEdits::default()
        };
        let edits =
            create_project_video_edits(&[simple("a", 20.0, 0)], &[project_video("a", anchored)])
                .unwrap();
        assert_eq!(edits["a"].placement_anchor, None);
        assert_eq!(edits["a"].offset_seconds, 12.0, "the clip keeps its offset");
    }

    #[test]
    fn a_video_outside_the_project_is_an_error() {
        assert!(create_project_video_edits(&[simple("a", 20.0, 0)], &[]).is_err());
    }

    #[test]
    fn the_manual_sync_moves_to_the_first_video_when_its_video_was_left_out() {
        let manual = ManualVideoSync {
            video_id: Some("gone".into()),
            ..ManualVideoSync::default()
        };
        let resolved = create_project_manual_video_sync(&[simple("a", 20.0, 0)], &manual);
        assert_eq!(resolved.video_id.as_deref(), Some("a"));
        assert_eq!(
            create_project_manual_video_sync(&[], &manual).video_id,
            None
        );
    }

    #[test]
    fn a_trim_is_kept_inside_the_loaded_activity() {
        let trim = ActivityTrim { start_second: 10.0, end_second: 500.0 };
        assert_eq!(resolve_activity_trim(Some(trim), 100.0), Some(ActivityTrim { start_second: 10.0, end_second: 100.0 }));
        assert_eq!(resolve_activity_trim(Some(ActivityTrim { start_second: 0.0, end_second: 500.0 }), 100.0), None, "a trim covering the activity goes");
        assert_eq!(resolve_activity_trim(Some(trim), 0.5), None, "too short to trim");
        assert_eq!(resolve_activity_trim(None, 100.0), None);
    }

    #[test]
    fn laps_must_belong_to_the_loaded_activity() {
        assert_eq!(
            resolve_disabled_laps(&[3], None),
            Ok(Vec::new()),
            "no activity drops the selection"
        );
    }

    #[test]
    fn a_new_project_resets_the_session_to_the_loaded_template() {
        let mut store = Store::default();
        store.editor.fallback_duration_seconds = 300.0;
        store
            .render_settings
            .set_fps_and_update_rate(25.0, 5)
            .unwrap();
        store.batch_render.batch_output_folder = Some("/out".into());
        apply_new_project(&mut store).unwrap();
        assert_eq!(store.editor.fallback_duration_seconds, 73.0);
        assert_eq!(store.render_settings, RenderSettings::default());
        assert_eq!(store.batch_render.batch_output_folder, None);
        store.template.loaded_template_source =
            Some(crate::store::template::TemplateSource::File {
                path: "/t.json".into(),
            });
        assert!(
            apply_new_project(&mut store).is_err(),
            "a loaded template needs its saved state"
        );
    }
}
