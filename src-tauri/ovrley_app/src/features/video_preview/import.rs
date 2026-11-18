//! Port of the video import (plan D3, D6, D7): `videoImportOperations.js`, `useVideoImport.js`,
//! `useVideoImportReview.js` and `importReview.js`.
//!
//! One way in, for the dialog, a drop and MCP: every file is probed on a worker with its
//! thumbnail, the probed videos join a draft of the whole collection that the review edits with
//! the store's recipes, the switched-on videos are committed in one action outside the history,
//! and their embedded telemetry is read in the background, one video at a time. A single video
//! joining an empty collection has nothing to review: it is committed at once (a quiet import).
//!
//! Not ported: the preview server registration (webview-only, D3).

use crate::features::app_shell::EditorBackgroundMode;
use crate::features::undo_redo::{change_video_collection, HistoryMode};
use crate::i18n::{catalog, interpolate, Locale};
use crate::lib::file_dialog::{filename_from_selected_path, open_multiple_paths, DialogOptions};
use crate::lib::format::format_seconds_clock;
use crate::lib::update_rate::normalize_update_rate_for_fps;
use crate::lib::video_program::collection::{
    derive_compatibility, find_combined_export_issues, join_video_edits, sort_clips_by_timeline,
    starts_outside_window, CombinedExportIssue, CombinedExportIssueKind,
};
use crate::lib::video_program::edits::{find_video, CollectionDraft};
use crate::lib::video_program::program::build_program;
use crate::lib::video_program::{
    create_imported_video, new_id, next_import_order, Clip, ImportedVideo, TelemetryStatus,
    VideoEditsMap, VideoTelemetry,
};
use crate::lib::video_sync::SyncActivity;
use crate::store::agent::AgentReply;
use crate::store::services::{DialogFilter, Services};
use crate::store::slices::media::ImportedActivity;
use crate::store::slices::video_import::{
    commit_video_collection, set_imported_background_image,
    set_video_telemetry,
};
use crate::store::Store;
use ovrley_core::encode::ffmpeg::transitions::TransitionDefaults;
use ovrley_core::encode::fps::Fps;
use ovrley_core::media::video_edits::{AnchorSide, OffsetSource, TimezoneMode, VideoResolution};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex};

/// MP4, MOV and MKV, by extension.
pub const VIDEO_EXTENSIONS: [&str; 3] = ["mp4", "mov", "mkv"];
/// Background images, which the import takes in debug builds only.
pub const DEBUG_IMAGE_EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];
/// The directory the video dialog remembers.
pub const LAST_VIDEO_IMPORT_DIR_KEY: &str = "last-video-import-dir";
/// `THUMBNAIL_CONCURRENCY`: each thumbnail decodes a frame in its own FFmpeg process.
pub const THUMBNAIL_CONCURRENCY: usize = 4;
/// `FRESH_IMPORT_SPAN_SECONDS`: without an activity or videos already imported, the videos of an
/// import should start within this span.
pub const FRESH_IMPORT_SPAN_SECONDS: f64 = 12.0 * 3600.0;

fn extension(path: &str) -> String {
    filename_from_selected_path(path)
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default()
}

/// `isVideoPath`.
pub fn is_video_path(path: &str) -> bool {
    VIDEO_EXTENSIONS.contains(&extension(path).as_str())
}

/// A debug background image, by extension.
pub fn is_debug_image_path(path: &str) -> bool {
    DEBUG_IMAGE_EXTENSIONS.contains(&extension(path).as_str())
}

fn t(locale: Locale, key: &str, default: &'static str) -> String {
    catalog().t_or(locale, key, default).to_string()
}

// The collection a review edits.

/// A draft of the whole collection (`{importedVideos, videoEdits, parsedActivity, activitySummary}`):
/// the videos, their edits, and the activity they are placed against with its duration.
#[derive(Clone, Debug, PartialEq)]
pub struct ReviewCollection {
    pub videos: Vec<ImportedVideo>,
    pub edits: VideoEditsMap,
    pub activity: Option<SyncActivity>,
    pub activity_duration_seconds: Option<f64>,
}

impl ReviewCollection {
    /// The store's collection.
    pub fn of(store: &Store) -> Self {
        let activity = store.media.sync_activity();
        Self {
            videos: (*store.video_import.imported_videos).clone(),
            edits: (*store.video_import.video_edits).clone(),
            activity_duration_seconds: activity.as_ref().map(|_| {
                store
                    .media
                    .activity_summary
                    .as_ref()
                    .map_or(0.0, |summary| summary.duration_seconds)
            }),
            activity,
        }
    }

    pub fn draft(&mut self) -> CollectionDraft<'_> {
        CollectionDraft {
            videos: &mut self.videos,
            edits: &mut self.edits,
            activity: self.activity.as_ref(),
        }
    }

    /// Applies an edit to a copy, kept only when it succeeds (Immer's `produce`).
    pub fn edited(
        &self,
        edit: impl FnOnce(&mut CollectionDraft) -> Result<(), String>,
    ) -> Result<Self, String> {
        let mut next = self.clone();
        edit(&mut next.draft())?;
        Ok(next)
    }

    pub fn clips(&self) -> Vec<Clip> {
        join_video_edits(&self.videos, &self.edits)
    }
}

/// The rule that decided which videos start far: the activity, the videos already imported, or
/// the span of the new videos.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FarKind {
    Activity,
    Videos,
    Span,
}

/// `findLargestStartGroup`: the largest group of starts within `span_seconds` of each other; the
/// earliest group wins a tie.
pub fn find_largest_start_group(offsets: &[f64], span_seconds: f64) -> Option<(f64, f64)> {
    let mut starts = offsets.to_vec();
    starts.sort_by(f64::total_cmp);
    let mut best: Option<(usize, f64, f64)> = None;
    let mut first = 0;
    for last in 0..starts.len() {
        while starts[last] - starts[first] > span_seconds {
            first += 1;
        }
        if best.is_none_or(|(size, _, _)| last - first > size) {
            best = Some((last - first, starts[first], starts[last]));
        }
    }
    best.map(|(_, from, to)| (from, to))
}

/// `findFarVideos`: the clips that start far from the reference of the import (only placed clips
/// count). With an activity, outside the start window around it; without one, a new video outside
/// the window around the switched-on videos already imported; without either, more than 12 hours
/// from the largest group of switched-on videos within 12 hours of each other.
pub fn find_far_videos(
    clips: &[Clip],
    new_ids: &BTreeSet<String>,
    included_ids: &BTreeSet<String>,
    unplaced_ids: &BTreeSet<String>,
    activity_duration_seconds: Option<f64>,
) -> (FarKind, BTreeSet<String>) {
    let placed: Vec<&Clip> = clips
        .iter()
        .filter(|clip| !unplaced_ids.contains(clip.id()))
        .collect();
    let ids = |clips: Vec<&&Clip>| {
        clips
            .into_iter()
            .map(|clip| clip.id().to_string())
            .collect()
    };
    if let Some(duration) = activity_duration_seconds {
        return (
            FarKind::Activity,
            ids(placed
                .iter()
                .filter(|clip| starts_outside_window(clip.offset_seconds(), 0.0, duration))
                .collect()),
        );
    }
    let reference: Vec<&Clip> = clips
        .iter()
        .filter(|clip| !new_ids.contains(clip.id()) && included_ids.contains(clip.id()))
        .collect();
    if !reference.is_empty() {
        let from = reference
            .iter()
            .map(|clip| clip.offset_seconds())
            .fold(f64::INFINITY, f64::min);
        let to = reference
            .iter()
            .map(|clip| clip.end_second())
            .fold(f64::NEG_INFINITY, f64::max);
        return (
            FarKind::Videos,
            ids(placed
                .iter()
                .filter(|clip| {
                    new_ids.contains(clip.id())
                        && starts_outside_window(clip.offset_seconds(), from, to)
                })
                .collect()),
        );
    }
    let starts: Vec<f64> = placed
        .iter()
        .filter(|clip| included_ids.contains(clip.id()))
        .map(|clip| clip.offset_seconds())
        .collect();
    let far = match find_largest_start_group(&starts, FRESH_IMPORT_SPAN_SECONDS) {
        None => BTreeSet::new(),
        Some((from, to)) => ids(placed
            .iter()
            .filter(|clip| {
                to.max(clip.offset_seconds()) - from.min(clip.offset_seconds())
                    > FRESH_IMPORT_SPAN_SECONDS
            })
            .collect()),
    };
    (FarKind::Span, far)
}

/// `findDefaultIncludedIds`: every video already imported, and every new video with a start near
/// the reference.
pub fn find_default_included_ids(
    clips: &[Clip],
    new_ids: &BTreeSet<String>,
    unplaced_ids: &BTreeSet<String>,
    activity_duration_seconds: Option<f64>,
) -> BTreeSet<String> {
    let candidates: BTreeSet<String> = clips
        .iter()
        .filter(|clip| !unplaced_ids.contains(clip.id()))
        .map(|clip| clip.id().to_string())
        .collect();
    let (_, far) = find_far_videos(
        clips,
        new_ids,
        &candidates,
        unplaced_ids,
        activity_duration_seconds,
    );
    candidates
        .into_iter()
        .filter(|id| !new_ids.contains(id) || !far.contains(id))
        .collect()
}

/// `findOtherResolutionVideos`: the overlay's resolution (the largest group of videos already
/// imported with one resolution, or of the new videos in a fresh import; a tie goes to the
/// earliest-imported) and the new videos with another one.
pub fn find_other_resolution_videos(
    collection: &ReviewCollection,
    new_ids: &BTreeSet<String>,
) -> (Option<VideoResolution>, BTreeSet<String>) {
    let existing: Vec<&ImportedVideo> = collection
        .videos
        .iter()
        .filter(|video| !new_ids.contains(&video.id))
        .collect();
    let mut ordered: Vec<&ImportedVideo> = if existing.is_empty() {
        collection.videos.iter().collect()
    } else {
        existing
    };
    ordered.sort_by_key(|video| video.import_order);
    let mut groups: Vec<Vec<&ImportedVideo>> = Vec::new();
    for video in ordered {
        match groups
            .iter_mut()
            .find(|group| group[0].resolution == video.resolution)
        {
            Some(group) => group.push(video),
            None => groups.push(vec![video]),
        }
    }
    let Some(best) = groups
        .iter()
        .fold(None::<&Vec<&ImportedVideo>>, |best, group| match best {
            Some(best) if group.len() <= best.len() => Some(best),
            _ => Some(group),
        })
    else {
        return (None, BTreeSet::new());
    };
    let resolution = best[0].resolution;
    let blocked = collection
        .videos
        .iter()
        .filter(|video| new_ids.contains(&video.id) && video.resolution != resolution)
        .map(|video| video.id.clone())
        .collect();
    (Some(resolution), blocked)
}

/// What the switches resolve to (`resolveImportSwitches`).
#[derive(Clone, Debug, PartialEq)]
pub struct Switches {
    pub included_ids: BTreeSet<String>,
    pub undetermined_ids: BTreeSet<String>,
    /// New videos whose time cannot be determined, still waiting to be switched on.
    pub unplaced_ids: BTreeSet<String>,
    /// New videos with another resolution than the overlay.
    pub blocked_ids: BTreeSet<String>,
    pub resolution: Option<VideoResolution>,
}

/// The review's own state beside its collection.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReviewSelection {
    pub new_ids: BTreeSet<String>,
    /// Switches the user set; an edited video counts as switched on.
    pub switch_overrides: BTreeMap<String, bool>,
    /// New videos whose time cannot be determined that the user switched on once.
    pub placed_ids: BTreeSet<String>,
}

/// `resolveImportSwitches`: the defaults for the current placement, except for the videos the
/// user switched or edited. A waiting video stays off until switched on; a video with another
/// resolution is always off.
pub fn resolve_import_switches(
    collection: &ReviewCollection,
    selection: &ReviewSelection,
) -> Switches {
    let mut copy = collection.clone();
    let undetermined_ids = copy.draft().find_undetermined_video_ids();
    let unplaced_ids: BTreeSet<String> = undetermined_ids
        .iter()
        .filter(|id| selection.new_ids.contains(*id) && !selection.placed_ids.contains(*id))
        .cloned()
        .collect();
    let (resolution, blocked_ids) = find_other_resolution_videos(collection, &selection.new_ids);
    let mut included_ids = find_default_included_ids(
        &collection.clips(),
        &selection.new_ids,
        &unplaced_ids,
        collection.activity_duration_seconds,
    );
    for (video_id, included) in &selection.switch_overrides {
        if *included {
            included_ids.insert(video_id.clone());
        } else {
            included_ids.remove(video_id);
        }
    }
    for video_id in &blocked_ids {
        included_ids.remove(video_id);
    }
    Switches {
        included_ids,
        undetermined_ids,
        unplaced_ids,
        blocked_ids,
        resolution,
    }
}

fn sorted_ids(collection: &ReviewCollection) -> Vec<String> {
    let clips = collection.clips();
    sort_clips_by_timeline(&clips)
        .into_iter()
        .map(|clip| clip.id().to_string())
        .collect()
}

/// `buildShownCollection`: the draft as the dialog shows it, without changing the draft. A
/// switched-on automatic video anchored to a switched-off one sits right after the switched-on
/// video above it (or keeps its start without one); waiting videos queue after the last
/// switched-on video.
pub fn build_shown_collection(
    collection: &ReviewCollection,
    included_ids: &BTreeSet<String>,
    unplaced_ids: &BTreeSet<String>,
) -> ReviewCollection {
    let mut shown = collection.clone();
    let order: Vec<String> = sorted_ids(&shown)
        .into_iter()
        .filter(|id| included_ids.contains(id))
        .collect();
    for (index, video_id) in order.iter().enumerate() {
        let edits = &shown.edits[video_id];
        let keeps = edits.offset_source != OffsetSource::Auto
            || edits
                .placement_anchor
                .as_ref()
                .is_none_or(|anchor| included_ids.contains(&anchor.video_id));
        if keeps {
            continue;
        }
        if index > 0 {
            shown
                .draft()
                .anchor_video(video_id, &order[index - 1], AnchorSide::After)
                .expect("the shown order holds no cycle");
        } else {
            shown
                .edits
                .get_mut(video_id)
                .expect("a clip")
                .placement_anchor = None;
        }
    }
    let Some(last) = sorted_ids(&shown)
        .into_iter()
        .filter(|id| included_ids.contains(id))
        .last()
    else {
        return shown;
    };
    let mut previous = last;
    for video in &collection.videos {
        if unplaced_ids.contains(&video.id) {
            shown
                .draft()
                .anchor_video(&video.id, &previous, AnchorSide::After)
                .expect("waiting videos queue in order");
            previous = video.id.clone();
        }
    }
    shown
}

/// `switchOnWaitingVideo`: a waiting video switched on stays where it waited (right after the
/// switched-on video above it, or at its shown start) and from then on follows the defaults; one
/// that would start far keeps the switch the user set.
pub fn switch_on_waiting_video(
    collection: &ReviewCollection,
    selection: &ReviewSelection,
    video_id: &str,
    shown: &ReviewCollection,
    included_ids: &BTreeSet<String>,
) -> Result<(ReviewCollection, ReviewSelection), String> {
    let order: Vec<Clip> = shown.clips();
    let order = sort_clips_by_timeline(&order);
    let index = order
        .iter()
        .position(|clip| clip.id() == video_id)
        .ok_or_else(|| format!("Unknown imported video: {video_id}"))?;
    let above = order[..index]
        .iter()
        .filter(|clip| included_ids.contains(clip.id()))
        .last()
        .map(|clip| clip.id().to_string());
    let shown_offset = order[index].offset_seconds();
    let next = collection.edited(|draft| match &above {
        Some(above) => draft.move_video_next_to(video_id, above, AnchorSide::After),
        None => {
            let edits = draft.edits.get_mut(video_id).expect("a clip");
            edits.placement_anchor = None;
            edits.offset_seconds = shown_offset;
            Ok(())
        }
    })?;
    let mut next_selection = selection.clone();
    next_selection.placed_ids.insert(video_id.to_string());
    if !resolve_import_switches(&next, &next_selection)
        .included_ids
        .contains(video_id)
    {
        next_selection
            .switch_overrides
            .insert(video_id.to_string(), true);
    }
    Ok((next, next_selection))
}

/// `chooseTimezoneMode`: camera clocks read as local time or UTC, whichever puts more videos near
/// the activity; a tie keeps each video's own inference.
pub fn choose_timezone_mode(collection: ReviewCollection) -> ReviewCollection {
    let near_count = |candidate: &ReviewCollection| {
        let clips = candidate.clips();
        let undetermined = candidate.clone().draft().find_undetermined_video_ids();
        let (_, far) = find_far_videos(
            &clips,
            &BTreeSet::new(),
            &BTreeSet::new(),
            &undetermined,
            candidate.activity_duration_seconds,
        );
        clips
            .iter()
            .filter(|clip| !undetermined.contains(clip.id()) && !far.contains(clip.id()))
            .count()
    };
    let read = |mode| {
        collection
            .edited(|draft| draft.set_videos_timezone_mode(mode))
            .expect("a reading places the collection")
    };
    let (local, utc) = (read(TimezoneMode::Local), read(TimezoneMode::Utc));
    let (local_count, utc_count) = (near_count(&local), near_count(&utc));
    match local_count.cmp(&utc_count) {
        std::cmp::Ordering::Equal => collection,
        std::cmp::Ordering::Greater => local,
        std::cmp::Ordering::Less => utc,
    }
}

/// `openImportReview`: the probed videos join the draft; a first import against an activity reads
/// the camera clocks the way that fits it best.
pub fn open_import_review(
    collection: &ReviewCollection,
    videos: Vec<ImportedVideo>,
) -> Result<(ReviewCollection, ReviewSelection), String> {
    let first_import = collection.videos.is_empty() && !videos.is_empty();
    let new_ids = videos.iter().map(|video| video.id.clone()).collect();
    let with_videos = if videos.is_empty() {
        collection.clone()
    } else {
        collection.edited(|draft| draft.add_videos(videos).map(|_| ()))?
    };
    let added = if first_import && collection.activity.is_some() {
        choose_timezone_mode(with_videos)
    } else {
        with_videos
    };
    Ok((
        added,
        ReviewSelection {
            new_ids,
            ..ReviewSelection::default()
        },
    ))
}

/// `buildIncludedCollection`: the collection of the switched-on videos.
pub fn build_included_collection(
    collection: &ReviewCollection,
    included_ids: &BTreeSet<String>,
) -> ReviewCollection {
    let removed: Vec<String> = collection
        .videos
        .iter()
        .filter(|video| !included_ids.contains(&video.id))
        .map(|video| video.id.clone())
        .collect();
    collection
        .edited(|draft| removed.iter().try_for_each(|id| draft.remove_video(id)))
        .expect("removing listed videos succeeds")
}

/// A video another one overlaps by, among the switched-on videos.
#[derive(Clone, Debug, PartialEq)]
pub struct RowOverlap {
    pub name: String,
    pub seconds: f64,
}

/// One row of the import dialog, in timeline order.
#[derive(Clone, Debug, PartialEq)]
pub struct ReviewRow {
    pub id: String,
    pub filename: String,
    pub path: String,
    pub is_new: bool,
    pub is_included: bool,
    pub has_other_resolution: bool,
    pub is_waiting: bool,
    pub is_far: bool,
    pub offset_seconds: f64,
    pub fps: Fps,
    pub resolution: VideoResolution,
    pub duration_seconds: f64,
    pub is_manual: bool,
    /// A new video whose time cannot be determined: the arrows place it.
    pub is_positioned: bool,
    pub is_incompatible: bool,
    pub overlaps: Vec<RowOverlap>,
}

/// `buildImportReview`'s result.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportReviewModel {
    pub window_kind: FarKind,
    pub rows: Vec<ReviewRow>,
    pub included: ReviewCollection,
    pub issues: Vec<CombinedExportIssue>,
    /// The span of the switched-on videos with the activity, or `None` without videos.
    pub timeline: Option<(f64, f64)>,
}

/// `buildImportReview`.
pub fn build_import_review(
    collection: &ReviewCollection,
    selection: &ReviewSelection,
    switches: &Switches,
    defaults: &TransitionDefaults,
) -> ImportReviewModel {
    let included = build_included_collection(collection, &switches.included_ids);
    let included_clips_owned = included.clips();
    let included_clips: Vec<Clip> = sort_clips_by_timeline(&included_clips_owned)
        .into_iter()
        .cloned()
        .collect();
    let clips_owned = collection.clips();
    let clips = sort_clips_by_timeline(&clips_owned);
    let (window_kind, far) = find_far_videos(
        &clips_owned,
        &selection.new_ids,
        &switches.included_ids,
        &switches.unplaced_ids,
        collection.activity_duration_seconds,
    );
    let program = build_program(&included_clips, defaults);
    let names: BTreeMap<&str, String> = clips
        .iter()
        .map(|clip| {
            (
                clip.id(),
                filename_from_selected_path(&clip.video.path)
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect();
    let overlaps_of = |video_id: &str| -> Vec<RowOverlap> {
        program
            .overlaps
            .iter()
            .filter(|overlap| {
                overlap.earlier_video_id == video_id || overlap.later_video_id == video_id
            })
            .map(|overlap| RowOverlap {
                name: names[if overlap.earlier_video_id == video_id {
                    overlap.later_video_id.as_str()
                } else {
                    overlap.earlier_video_id.as_str()
                }]
                .clone(),
                seconds: overlap.end_second - overlap.start_second,
            })
            .collect()
    };
    let included_refs: Vec<&Clip> = included_clips.iter().collect();
    let issues = find_combined_export_issues(&program, &included_refs, false);
    let incompatible: BTreeSet<&str> = issues
        .iter()
        .filter(|issue| issue.kind == CombinedExportIssueKind::Incompatible)
        .flat_map(|issue| issue.video_ids.iter().map(String::as_str))
        .collect();
    let rows = clips
        .iter()
        .map(|clip| ReviewRow {
            id: clip.id().to_string(),
            filename: names[clip.id()].clone(),
            path: clip.video.path.clone(),
            is_new: selection.new_ids.contains(clip.id()),
            is_included: switches.included_ids.contains(clip.id()),
            has_other_resolution: switches.blocked_ids.contains(clip.id()),
            is_waiting: switches.unplaced_ids.contains(clip.id()),
            is_far: far.contains(clip.id()),
            offset_seconds: clip.offset_seconds(),
            fps: clip.fps(),
            resolution: clip.video.resolution,
            duration_seconds: clip.duration_seconds(),
            is_manual: clip.edits.offset_source == OffsetSource::Manual,
            is_positioned: selection.new_ids.contains(clip.id())
                && switches.undetermined_ids.contains(clip.id()),
            is_incompatible: incompatible.contains(clip.id()),
            overlaps: if switches.included_ids.contains(clip.id()) {
                overlaps_of(clip.id())
            } else {
                Vec::new()
            },
        })
        .collect();
    let mut bounds: Vec<f64> = included_clips
        .iter()
        .flat_map(|clip| [clip.offset_seconds(), clip.end_second()])
        .collect();
    if let (Some(duration), false) = (
        collection.activity_duration_seconds,
        included_clips.is_empty(),
    ) {
        bounds.extend([0.0, duration]);
    }
    let timeline = (!bounds.is_empty()).then(|| {
        (
            bounds.iter().copied().fold(f64::INFINITY, f64::min),
            bounds.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        )
    });
    ImportReviewModel {
        window_kind,
        rows,
        included,
        issues,
        timeline,
    }
}

// The review state (`useVideoImportReview`).

/// Where a review stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewPhase {
    Probing,
    Reviewing,
    Confirming,
    Importing,
}

/// A file that cannot join.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportFailure {
    pub path: String,
    pub message: String,
}

/// An agent's import: its reply, and with `include`, exactly the listed paths join without a
/// dialog.
#[derive(Debug, Default)]
pub struct ImportRequest {
    pub reply: Option<AgentReply>,
    pub include: Option<Vec<String>>,
}

/// `useVideoImportReview`'s state.
#[derive(Debug)]
pub struct ImportReview {
    /// Results of a cancelled review are discarded by their token.
    pub token: u64,
    pub phase: ReviewPhase,
    pub file_count: usize,
    /// No dialog: the import indicator shows instead.
    pub quiet: bool,
    pub collection: ReviewCollection,
    pub selection: ReviewSelection,
    pub failures: Vec<ImportFailure>,
    pub transition_defaults: TransitionDefaults,
    /// The first import adopts its format.
    pub adopts_format: bool,
    pub error: Option<String>,
    pub request: ImportRequest,
}

/// The review, if one is open, and the next token.
#[derive(Debug, Default)]
pub struct VideoImportReviewState {
    pub review: Option<ImportReview>,
    next_token: u64,
}

/// The review's derived state, for its dialog (the `derived` memo).
#[derive(Clone, Debug, PartialEq)]
pub struct ReviewDerived {
    pub switches: Switches,
    pub shown: ReviewCollection,
    pub model: ImportReviewModel,
}

impl ImportReview {
    pub fn derived(&self) -> ReviewDerived {
        let switches = resolve_import_switches(&self.collection, &self.selection);
        let shown = build_shown_collection(
            &self.collection,
            &switches.included_ids,
            &switches.unplaced_ids,
        );
        let model = build_import_review(
            &shown,
            &self.selection,
            &switches,
            &self.transition_defaults,
        );
        ReviewDerived {
            switches,
            shown,
            model,
        }
    }
}

/// `handleImportVideo`: the dialog (with images in debug builds), then the review of the chosen
/// files, or the background image.
pub fn open_video_import_dialog(store: &mut Store, services: &mut Services) {
    let debug = store.editor_shell.debug_mode_enabled;
    let locale = store.i18n.language;
    let filter = if debug {
        DialogFilter::new(
            &t(locale, "video-preview.videoOrImage", "Video or Image"),
            &["mp4", "mov", "mkv", "png", "jpg", "jpeg", "webp"],
        )
    } else {
        DialogFilter::new("Video", &VIDEO_EXTENSIONS)
    };
    let opened = open_multiple_paths(
        services,
        &[filter],
        DialogOptions {
            default_path: None,
            last_directory_key: Some(LAST_VIDEO_IMPORT_DIR_KEY),
        },
        Box::new(move |store, services, selected| {
            if selected.is_empty() {
                return;
            }
            import_selected_files(store, services, selected);
        }),
    );
    if let Err(error) = opened {
        store
            .media
            .set_error_message(format!("Video import failed: {error}"));
    }
}

/// The files of the dialog or a drop: a single image in debug builds sets the background, else
/// the review starts.
pub fn import_selected_files(store: &mut Store, services: &mut Services, paths: Vec<String>) {
    if store.editor_shell.debug_mode_enabled && paths.len() == 1 && is_debug_image_path(&paths[0]) {
        apply_background_image(store, paths.into_iter().next().expect("one path"));
        return;
    }
    if let Err(error) = start_import(store, services, paths, ImportRequest::default()) {
        store
            .media
            .set_error_message(format!("Video import failed: {error}"));
    }
}

/// `handleClearImportedVideos`: "Remove all", outside the history; the background returns to
/// checker.
pub fn clear_videos(store: &mut Store) {
    crate::features::player::playback::pause(store);
    change_video_collection(store, &[], crate::store::slices::video_import::clear_imported_videos);
    store.editor_shell.set_editor_background_mode(EditorBackgroundMode::Checker);
}

/// `applyBackgroundImage`: the image replaces the videos, outside the history.
pub fn apply_background_image(store: &mut Store, path: String) {
    change_video_collection(store, &[], |store| {
        set_imported_background_image(store, path)
    });
    store
        .editor_shell
        .set_editor_background_mode(EditorBackgroundMode::Image);
}

/// `start`: probes the files and opens the review. Ignored (an error) while a review is open.
pub fn start_import(
    store: &mut Store,
    services: &mut Services,
    paths: Vec<String>,
    request: ImportRequest,
) -> Result<(), String> {
    if store.video_import_review.review.is_some() {
        return Err("A video import is already open.".into());
    }
    if paths.is_empty() {
        return Err("At least one video is required".into());
    }
    let quiet = paths.len() == 1 && store.video_import.imported_videos.is_empty();
    let locale = store.i18n.language;
    let state = &mut store.video_import_review;
    state.next_token += 1;
    let token = state.next_token;
    let mut known: BTreeSet<String> = store
        .video_import
        .imported_videos
        .iter()
        .map(|video| video.path.clone())
        .collect();
    let mut import_order = next_import_order(&store.video_import.imported_videos);
    let mut failures = Vec::new();
    let mut candidates = Vec::new();
    for path in &paths {
        if !known.insert(path.clone()) {
            failures.push(ImportFailure {
                path: path.clone(),
                message: t(locale, "video-preview.alreadyImported", "Already imported"),
            });
            continue;
        }
        candidates.push((path.clone(), new_id(), import_order));
        import_order += 1;
    }
    store.video_import_review.review = Some(ImportReview {
        token,
        phase: ReviewPhase::Probing,
        file_count: paths.len(),
        quiet,
        collection: ReviewCollection::of(store),
        selection: ReviewSelection::default(),
        failures: Vec::new(),
        transition_defaults: store.video_import.transition_defaults.clone(),
        adopts_format: false,
        error: None,
        request,
    });
    if quiet {
        store.media.importing_video = true;
    }
    store.seconds_fields.clear_import();
    let tools = Arc::clone(&services.video_tools);
    let unsupported = t(
        locale,
        "video-preview.unsupportedVideoFile",
        "Not a supported video file (MP4, MOV, MKV).",
    );
    services.workers.spawn("video-import", move || {
        let probes = probe_videos(&*tools, candidates, &unsupported);
        Box::new(move |store: &mut Store, services: &mut Services| {
            open_review(store, services, token, quiet, probes, failures)
        })
    });
    Ok(())
}

/// Bounds the thumbnails rendering at once across every import (`THUMBNAIL_CONCURRENCY`).
struct Semaphore {
    running: Mutex<usize>,
    freed: Condvar,
}

static THUMBNAILS: Semaphore = Semaphore {
    running: Mutex::new(0),
    freed: Condvar::new(),
};

impl Semaphore {
    fn run<R>(&self, job: impl FnOnce() -> R) -> R {
        let mut running = self
            .freed
            .wait_while(self.running.lock().unwrap(), |running| {
                *running >= THUMBNAIL_CONCURRENCY
            })
            .unwrap();
        *running += 1;
        drop(running);
        let result = job();
        *self.running.lock().unwrap() -= 1;
        self.freed.notify_one();
        result
    }
}

/// `probeVideo`: the probe and the thumbnail of one file, then its entity.
pub fn probe_video(
    tools: &dyn crate::store::services::VideoTools,
    path: &str,
    id: String,
    import_order: u32,
    unsupported: &str,
) -> Result<ImportedVideo, String> {
    if !is_video_path(path) {
        return Err(unsupported.to_string());
    }
    let metadata = tools.probe(path)?;
    let thumbnail = THUMBNAILS.run(|| tools.thumbnail(path));
    let mut video = create_imported_video(&metadata, id, import_order, Vec::new())?;
    video.thumbnail = thumbnail.map(Arc::from);
    Ok(video)
}

/// Every file probed in parallel; results in selection order.
fn probe_videos(
    tools: &dyn crate::store::services::VideoTools,
    candidates: Vec<(String, String, u32)>,
    unsupported: &str,
) -> Vec<(String, Result<ImportedVideo, String>)> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = candidates
            .into_iter()
            .map(|(path, id, import_order)| {
                let handle = scope.spawn({
                    let path = path.clone();
                    move || probe_video(tools, &path, id, import_order, unsupported)
                });
                (path, handle)
            })
            .collect();
        handles
            .into_iter()
            .map(|(path, handle)| {
                (
                    path,
                    handle
                        .join()
                        .unwrap_or_else(|_| Err("The probe failed".into())),
                )
            })
            .collect()
    })
}

/// `probeAndOpen`'s second half: the probed videos join the draft. A quiet import that read its
/// one file commits at once; an agent's `include` commits the listed paths.
fn open_review(
    store: &mut Store,
    services: &mut Services,
    token: u64,
    quiet: bool,
    probes: Vec<(String, Result<ImportedVideo, String>)>,
    mut failures: Vec<ImportFailure>,
) {
    if quiet {
        store.media.importing_video = false;
    }
    if store
        .video_import_review
        .review
        .as_ref()
        .is_none_or(|review| review.token != token)
    {
        return;
    }
    let mut videos = Vec::new();
    for (path, probe) in probes {
        match probe {
            Ok(video) => videos.push(video),
            Err(message) => failures.push(ImportFailure { path, message }),
        }
    }
    let current = ReviewCollection::of(store);
    let empty = store.video_import.imported_videos.is_empty();
    let quiet = quiet && videos.len() == 1 && empty;
    let opened = open_import_review(&current, videos);
    let review = store
        .video_import_review
        .review
        .as_mut()
        .expect("the review is open");
    match opened {
        Err(error) => {
            review.phase = ReviewPhase::Reviewing;
            review.error = Some(error);
        }
        Ok((collection, selection)) => {
            review.collection = collection;
            review.selection = selection;
            review.failures = failures;
            review.transition_defaults = store.video_import.transition_defaults.clone();
            review.adopts_format = empty;
            review.quiet = quiet;
            review.phase = ReviewPhase::Reviewing;
            review.error = None;
        }
    }
    if let Some(include) = review.request.include.take() {
        return commit_listed(store, services, include);
    }
    if store
        .video_import_review
        .review
        .as_ref()
        .is_some_and(|review| review.quiet)
    {
        let collection = store
            .video_import_review
            .review
            .as_ref()
            .expect("open")
            .collection
            .clone();
        commit(store, services, collection);
    }
}

/// An agent's `include`: exactly the listed paths join, placed as the review places them. A listed
/// path that cannot join fails the call with its reason, and nothing is committed.
fn commit_listed(store: &mut Store, services: &mut Services, include: Vec<String>) {
    let review = store
        .video_import_review
        .review
        .as_mut()
        .expect("the review is open");
    let failure = include.iter().find_map(|path| {
        if let Some(failure) = review.failures.iter().find(|failure| &failure.path == path) {
            return Some(format!("{path}: {}", failure.message));
        }
        let Some(video) = review
            .collection
            .videos
            .iter()
            .find(|video| &video.path == path)
        else {
            return Some(format!("{path}: not one of the imported paths"));
        };
        (!review.selection.new_ids.contains(&video.id)).then(|| format!("{path}: Already imported"))
    });
    if let Some(failure) = failure {
        let reply = review.request.reply.take();
        store.video_import_review.review = None;
        if let Some(reply) = reply {
            reply.send(Err(failure));
        }
        return;
    }
    for video in &review.collection.videos {
        if review.selection.new_ids.contains(&video.id) {
            review
                .selection
                .switch_overrides
                .insert(video.id.clone(), include.contains(&video.path));
        }
    }
    let derived = review.derived();
    if let Some(path) =
        include.iter().find(|path| {
            review.collection.videos.iter().any(|video| {
                &video.path == *path && derived.switches.blocked_ids.contains(&video.id)
            })
        })
    {
        let reply = review.request.reply.take();
        store.video_import_review.review = None;
        if let Some(reply) = reply {
            reply.send(Err(format!(
                "{path}: has another resolution than the overlay"
            )));
        }
        return;
    }
    commit(store, services, derived.model.included);
}

/// `commit`: imports the reviewed collection in one action outside the history; a failure returns
/// to the review with its reason.
fn commit(store: &mut Store, services: &mut Services, reviewed: ReviewCollection) {
    let review = store
        .video_import_review
        .review
        .as_mut()
        .expect("the review is open");
    review.phase = ReviewPhase::Importing;
    review.error = None;
    let adopted = if review.adopts_format {
        let clips = reviewed.clips();
        derive_compatibility(&clips.iter().collect::<Vec<_>>()).reference
    } else {
        None
    };
    let defaults = review.transition_defaults.clone();
    let new_ids = review.selection.new_ids.clone();
    let result = commit_reviewed_import(store, reviewed, defaults, adopted);
    match result {
        Ok(added) => {
            let review = store
                .video_import_review
                .review
                .take()
                .expect("the review is open");
            let added_ids: Vec<String> = added
                .iter()
                .filter(|video| new_ids.contains(&video.id))
                .map(|video| video.id.clone())
                .collect();
            if !added_ids.is_empty() {
                store
                    .editor_shell
                    .set_editor_background_mode(EditorBackgroundMode::Video);
            }
            queue_telemetry_extraction(store, services, &added_ids);
            if let Some(reply) = review.request.reply {
                reply.send(Ok(describe_import(store, &added_ids, &review.failures)));
            }
        }
        Err(error) => {
            let review = store
                .video_import_review
                .review
                .as_mut()
                .expect("the review is open");
            review.phase = ReviewPhase::Reviewing;
            review.quiet = false;
            review.error = Some(error);
        }
    }
}

/// `commitReviewedImport`: the collection and the transition defaults in one change outside the
/// history; the first import adopts its format. Returns the committed videos.
fn commit_reviewed_import(
    store: &mut Store,
    reviewed: ReviewCollection,
    defaults: TransitionDefaults,
    adopted: Option<(VideoResolution, Fps)>,
) -> Result<Vec<ImportedVideo>, String> {
    // Everything that can fail is checked before the collection changes.
    defaults.validate()?;
    let render_settings = match adopted {
        Some((_, fps)) => Some(adopted_render_settings(store, fps)?),
        None => None,
    };
    let videos = reviewed.videos.clone();
    change_video_collection(store, &[], |store| {
        commit_video_collection(store, reviewed.videos, reviewed.edits);
        store.video_import.transition_defaults = defaults;
        if let (Some((resolution, _)), Some(render_settings)) = (adopted, render_settings) {
            store.editor.edit_config(|config| {
                config.scene.insert("width".into(), serde_json::Value::from(resolution.width));
                config.scene.insert("height".into(), serde_json::Value::from(resolution.height));
            });
            store.render_settings = render_settings;
        }
    });
    Ok(videos)
}

/// `adoptVideoFormat`'s render settings: the fps rounded, the update rate normalized for it. The
/// scene takes the videos' size.
fn adopted_render_settings(store: &Store, fps: Fps) -> Result<ovrley_core::encode::render_settings::RenderSettings, String> {
    let rate = crate::js::round(fps.as_f64());
    let update_rate = normalize_update_rate_for_fps(rate, f64::from(store.render_settings.widget_update_rate));
    let mut settings = store.render_settings.clone();
    settings.set_fps_and_update_rate(rate, update_rate)?;
    Ok(settings)
}

/// What `video_import` answers: the new videos and the files that failed.
fn describe_import(
    store: &Store,
    added_ids: &[String],
    failures: &[ImportFailure],
) -> serde_json::Value {
    let videos: Vec<serde_json::Value> = added_ids
        .iter()
        .filter_map(|id| find_video(&store.video_import.imported_videos, id).ok())
        .map(|video| serde_json::json!({ "id": video.id, "path": video.path, "offsetSeconds": store.video_import.video_edits[&video.id].offset_seconds }))
        .collect();
    let failed: Vec<serde_json::Value> = failures
        .iter()
        .map(|failure| serde_json::json!({ "path": failure.path, "message": failure.message }))
        .collect();
    serde_json::json!({ "videos": videos, "failures": failed })
}

/// The review's transition picker closes with the review.
fn close_review_popovers(store: &mut Store) {
    use crate::features::transitions::TransitionSelectTarget;
    if store.transition_select.as_ref().is_some_and(|select| select.target == TransitionSelectTarget::ImportDefaults) {
        store.transition_select = None;
    }
}

/// `onCancel`: closes the review; an agent waiting on it reads that the user cancelled.
pub fn cancel_import_review(store: &mut Store) {
    close_review_popovers(store);
    let Some(review) = store.video_import_review.review.take() else {
        return;
    };
    if review.quiet {
        store.media.importing_video = false;
    }
    if let Some(reply) = review.request.reply {
        reply.send(Err("The user cancelled the import.".into()));
    }
}

/// `confirm`: with a switched-on video far from the reference, asks first.
pub fn confirm_import_review(store: &mut Store, services: &mut Services) {
    let Some(review) = store.video_import_review.review.as_mut() else {
        return;
    };
    let derived = review.derived();
    match review.phase {
        ReviewPhase::Reviewing
            if derived
                .model
                .rows
                .iter()
                .any(|row| row.is_included && row.is_far) =>
        {
            review.phase = ReviewPhase::Confirming;
            review.error = None;
        }
        ReviewPhase::Reviewing | ReviewPhase::Confirming => {
            commit(store, services, derived.model.included)
        }
        ReviewPhase::Probing | ReviewPhase::Importing => {}
    }
}

/// `onBack`: from the far question to the review.
pub fn back_to_import_review(store: &mut Store) {
    if let Some(review) = store
        .video_import_review
        .review
        .as_mut()
        .filter(|review| review.phase == ReviewPhase::Confirming)
    {
        review.phase = ReviewPhase::Reviewing;
    }
}

/// An edit of the review (`update`): applied, or its reason shown as the dialog's error.
fn update_review(
    store: &mut Store,
    edit: impl FnOnce(&ImportReview) -> Result<(ReviewCollection, ReviewSelection), String>,
) -> Result<(), String> {
    let review = store
        .video_import_review
        .review
        .as_mut()
        .ok_or("No video import is open.")?;
    match edit(review) {
        Ok((collection, selection)) => {
            review.collection = collection;
            review.selection = selection;
            review.phase = ReviewPhase::Reviewing;
            review.error = None;
            Ok(())
        }
        Err(error) => {
            review.error = Some(error.clone());
            Err(error)
        }
    }
}

/// `change`: a recipe on the draft, with switch changes.
fn change_review(
    store: &mut Store,
    switch_changes: &[(&str, bool)],
    edit: impl FnOnce(&mut CollectionDraft) -> Result<(), String>,
) -> Result<(), String> {
    update_review(store, |review| {
        let collection = review.collection.edited(edit)?;
        let mut selection = review.selection.clone();
        for (video_id, included) in switch_changes {
            selection
                .switch_overrides
                .insert(video_id.to_string(), *included);
        }
        Ok((collection, selection))
    })
}

/// A row's sync, set by hand; editing a video keeps it switched on.
pub fn set_review_row_offset(
    store: &mut Store,
    video_id: &str,
    seconds: f64,
) -> Result<(), String> {
    change_review(store, &[(video_id, true)], |draft| {
        draft.set_video_offset(video_id, seconds)
    })
}

/// A row's sync, placed automatically again; editing a video keeps it switched on.
pub fn reset_review_row_offset(store: &mut Store, video_id: &str) -> Result<(), String> {
    change_review(store, &[(video_id, true)], |draft| {
        draft.reset_video_offset(video_id)
    })
}

/// Moves a positioned row up or down among the switched-on rows. Moving only places the video.
pub fn move_review_row(store: &mut Store, video_id: &str, up: bool) -> Result<(), String> {
    let review = store
        .video_import_review
        .review
        .as_ref()
        .ok_or("No video import is open.")?;
    let derived = review.derived();
    let included: Vec<&ReviewRow> = derived
        .model
        .rows
        .iter()
        .filter(|row| row.is_included)
        .collect();
    let index = included
        .iter()
        .position(|row| row.id == video_id)
        .ok_or_else(|| format!("Video {video_id} is not switched on"))?;
    if !included[index].is_positioned {
        return Err(format!(
            "Video {video_id} has a recording time and is not placed by hand"
        ));
    }
    let target = if up {
        index.checked_sub(1)
    } else {
        Some(index + 1).filter(|next| *next < included.len())
    }
    .ok_or("The video cannot move further")?;
    let others: Vec<&str> = included
        .iter()
        .filter(|row| row.id != video_id)
        .map(|row| row.id.as_str())
        .collect();
    let (anchor, side) = if target == 0 {
        (others[0].to_string(), AnchorSide::Before)
    } else {
        (others[target - 1].to_string(), AnchorSide::After)
    };
    change_review(store, &[], |draft| {
        draft.move_video_next_to(video_id, &anchor, side)
    })
}

/// A row's switch. A waiting video switched on joins the others where it waited.
pub fn set_review_row_included(
    store: &mut Store,
    video_id: &str,
    included: bool,
) -> Result<(), String> {
    let review = store
        .video_import_review
        .review
        .as_ref()
        .ok_or("No video import is open.")?;
    let derived = review.derived();
    let row = derived
        .model
        .rows
        .iter()
        .find(|row| row.id == video_id)
        .ok_or_else(|| format!("Unknown imported video: {video_id}"))?;
    if row.has_other_resolution {
        return Err("A video with another resolution than the overlay cannot be imported".into());
    }
    if row.is_waiting && included {
        let (shown, included_ids) = (derived.shown, derived.switches.included_ids);
        return update_review(store, |review| {
            switch_on_waiting_video(
                &review.collection,
                &review.selection,
                video_id,
                &shown,
                &included_ids,
            )
        });
    }
    change_review(store, &[(video_id, included)], |_| Ok(()))
}

/// The review's global sync: detect times again.
pub fn review_compute_video_sync(store: &mut Store) -> Result<(), String> {
    change_review(store, &[], |draft| draft.place_automatic_videos())
}

/// The review's shared offset: the start of the first switched-on video.
pub fn review_commit_shared_offset(store: &mut Store, seconds: f64) -> Result<(), String> {
    let review = store
        .video_import_review
        .review
        .as_ref()
        .ok_or("No video import is open.")?;
    let current = review.derived().model.included.shared_offset_seconds();
    change_review(store, &[], |draft| {
        draft.shift_video_offsets(seconds - current)
    })
}

pub fn review_reset_creation_times(store: &mut Store) -> Result<(), String> {
    change_review(store, &[], |draft| draft.reset_creation_times())
}

pub fn review_set_creation_times_from_filenames(store: &mut Store) -> Result<(), String> {
    change_review(store, &[], |draft| draft.apply_filename_creation_times())
}

pub fn review_set_timezone_mode(store: &mut Store, mode: TimezoneMode) -> Result<(), String> {
    change_review(store, &[], |draft| draft.set_videos_timezone_mode(mode))
}

/// The review's default transitions, checked when the import commits.
pub fn set_review_transition_defaults(
    store: &mut Store,
    change: impl FnOnce(&mut TransitionDefaults),
) -> Result<(), String> {
    let review = store
        .video_import_review
        .review
        .as_mut()
        .ok_or("No video import is open.")?;
    change(&mut review.transition_defaults);
    Ok(())
}

impl ReviewCollection {
    /// The shared offset: where the earliest video starts.
    pub fn shared_offset_seconds(&self) -> f64 {
        let clips = self.clips();
        sort_clips_by_timeline(&clips)
            .first()
            .map_or(0.0, |clip| clip.offset_seconds())
    }
}

/// The far confirmation's text values: the far videos' names, and the timeline from and to.
pub fn far_confirmation(derived: &ReviewDerived) -> (Vec<String>, String, String) {
    let names = derived
        .model
        .rows
        .iter()
        .filter(|row| row.is_included && row.is_far)
        .map(|row| row.filename.clone())
        .collect();
    let (from, to) = derived.model.timeline.unwrap_or((0.0, 0.0));
    (names, format_seconds_clock(from), format_seconds_clock(to))
}

/// A row's far status line, by the rule that decided.
pub fn far_label(kind: FarKind, locale: Locale) -> String {
    match kind {
        FarKind::Activity => t(
            locale,
            "video-preview.importOutsideActivity",
            "Starts more than an hour before or after the activity",
        ),
        FarKind::Videos => t(
            locale,
            "video-preview.importOutsideVideos",
            "Starts more than an hour away from the other videos",
        ),
        FarKind::Span => t(
            locale,
            "video-preview.importSpanTooLong",
            "Starts more than 12 hours away from the other videos",
        ),
    }
}

/// A row's overlap line.
pub fn overlap_label(overlap: &RowOverlap, locale: Locale) -> String {
    interpolate(
        &t(
            locale,
            "video-preview.importOverlap",
            "Overlaps {{name}} by {{duration}}",
        ),
        &[
            ("name", &overlap.name),
            ("duration", &format_seconds_clock(overlap.seconds)),
        ],
    )
}

// The dialog's view (`useVideoImportReview`'s `dialog` model, texts in the app language).

/// A row of the dialog as it shows.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportRowModel {
    pub row: ReviewRow,
    pub can_switch: bool,
    pub switch_label: String,
    pub format_label: String,
    pub status: Option<String>,
    pub far_label: Option<String>,
    pub overlap_labels: Vec<String>,
    pub can_move_up: bool,
    pub can_move_down: bool,
    pub move_up_label: String,
    pub move_down_label: String,
    pub reset_label: String,
    pub sync_label: String,
}

/// The dialog while it is open.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportDialogView {
    pub phase: ReviewPhase,
    pub reading_label: String,
    pub rows: Vec<ImportRowModel>,
    /// The failures: file name, path and "Not imported: …".
    pub failures: Vec<(String, String, String)>,
    pub issues: Vec<String>,
    pub far_text: Option<String>,
    pub error: Option<String>,
    pub import_label: String,
    pub import_label_alternatives: Vec<String>,
    pub import_count: usize,
    pub sync: crate::features::video_sync::VideoSyncControls,
    pub transition_label: String,
    /// The previewed transition, `None` for no transition.
    pub transition_preview: Option<String>,
}

/// The dialog's view, or `None` while no dialog shows (no review, or a quiet one).
pub fn import_dialog_view(store: &Store) -> Option<ImportDialogView> {
    let review = store.video_import_review.review.as_ref().filter(|review| !review.quiet)?;
    let locale = store.i18n.language;
    let names = |key: &str, default: &'static str, name: &str| interpolate(&t(locale, key, default), &[("name", name)]);
    let import_label = |count: usize| catalog().t_count(locale, "video-preview.importConfirm", count, "Import {{count}} videos");
    let reading_label = catalog().t_count(locale, "video-preview.importReading", review.file_count, "Reading {{count}} files…");
    if review.phase == ReviewPhase::Probing {
        let label = import_label(review.file_count);
        return Some(ImportDialogView {
            phase: review.phase,
            reading_label,
            rows: Vec::new(),
            failures: Vec::new(),
            issues: Vec::new(),
            far_text: None,
            error: None,
            import_label: label.clone(),
            import_label_alternatives: vec![label],
            import_count: 0,
            sync: crate::features::video_sync::derive_video_sync_controls(&[], &VideoEditsMap::new(), None),
            transition_label: String::new(),
            transition_preview: None,
        });
    }
    let derived = review.derived();
    let resolution = derived.switches.resolution.map(|resolution| format!("{}×{}", resolution.width, resolution.height)).unwrap_or_default();
    let included_rows: Vec<&ReviewRow> = derived.model.rows.iter().filter(|row| row.is_included).collect();
    let rows = derived
        .model
        .rows
        .iter()
        .map(|row| {
            let index = included_rows.iter().position(|included| included.id == row.id);
            let can_move = row.is_positioned && row.is_included;
            let status = if row.has_other_resolution {
                Some(interpolate(&t(locale, "video-preview.importOtherResolution", "Cannot be imported: the overlay is laid out for {{resolution}}"), &[("resolution", &resolution)]))
            } else if row.is_new || row.is_included {
                None
            } else {
                Some(t(locale, "video-preview.importRemoved", "Will be removed from the project"))
            };
            ImportRowModel {
                can_switch: !row.has_other_resolution,
                switch_label: if row.is_new { names("video-preview.importInclude", "Import {{name}}", &row.filename) } else { names("video-preview.importKeep", "Keep {{name}} in the project", &row.filename) },
                format_label: crate::lib::format::format_video_details(row.resolution, row.fps, row.duration_seconds, locale),
                status,
                far_label: row.is_far.then(|| far_label(derived.model.window_kind, locale)),
                overlap_labels: row.overlaps.iter().map(|overlap| overlap_label(overlap, locale)).collect(),
                can_move_up: can_move && index.is_some_and(|index| index > 0),
                can_move_down: can_move && index.is_some_and(|index| index + 1 < included_rows.len()),
                move_up_label: names("video-preview.importMoveUp", "Move {{name}} up", &row.filename),
                move_down_label: names("video-preview.importMoveDown", "Move {{name}} down", &row.filename),
                reset_label: names("toolbar.resetVideoStart", "Place {{name}} automatically again", &row.filename),
                sync_label: names("video-preview.importSyncOf", "Sync of {{name}}", &row.filename),
                row: row.clone(),
            }
        })
        .collect();
    let import_count = review.selection.new_ids.iter().filter(|id| derived.switches.included_ids.contains(*id)).count();
    let new_count = derived.model.rows.iter().filter(|row| row.is_new).count();
    let mut alternatives: Vec<String> = Vec::new();
    for count in [1, 2, 5, new_count] {
        let label = import_label(count);
        if !alternatives.contains(&label) {
            alternatives.push(label);
        }
    }
    alternatives.push(t(locale, "video-preview.importAnyway", "Import anyway"));
    let far_text = (review.phase == ReviewPhase::Confirming).then(|| {
        let (names, from, to) = far_confirmation(&derived);
        interpolate(
            &t(locale, "video-preview.importFarConfirm", "These videos start far from the activity or the other videos: {{names}}. The timeline will run from {{from}} to {{to}}. Import them anyway?"),
            &[("names", &names.join(", ")), ("from", &from), ("to", &to)],
        )
    });
    let clips = review.collection.clips();
    let mut sync = crate::features::video_sync::derive_video_sync_controls(&review.collection.videos, &review.collection.edits, review.collection.activity.as_ref().and_then(|activity| activity.timezone.as_deref()));
    // The shared offset is the start of the first switched-on video; the rows show their own warnings.
    let shared = crate::features::video_sync::derive_video_sync_controls(&derived.model.included.videos, &derived.model.included.edits, None);
    sync.shared_offset_seconds = shared.shared_offset_seconds;
    sync.shared_offset_fps = shared.shared_offset_fps;
    sync.warning = None;
    let transition_type = review.transition_defaults.transition_type.as_str();
    Some(ImportDialogView {
        phase: review.phase,
        reading_label,
        rows,
        failures: review
            .failures
            .iter()
            .map(|failure| {
                let name = filename_from_selected_path(&failure.path).unwrap_or(&failure.path).to_string();
                (name, failure.path.clone(), interpolate(&t(locale, "video-preview.importFailed", "Not imported: {{message}}"), &[("message", &failure.message)]))
            })
            .collect(),
        issues: derived.model.issues.iter().map(|issue| crate::lib::video_program::warnings::format_combined_export_issue(issue, &clips, locale)).collect(),
        far_text,
        error: review.error.clone(),
        import_label: import_label(import_count),
        import_label_alternatives: alternatives,
        import_count,
        sync,
        transition_label: crate::features::transitions::default_transition_label(transition_type, locale),
        transition_preview: (transition_type != ovrley_core::encode::ffmpeg::transitions::NO_TRANSITION).then(|| transition_type.to_string()),
    })
}

// Embedded telemetry (D7).

/// `queueTelemetryExtraction`'s queue: one extraction at a time across every import.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VideoTelemetryQueue {
    pub queued: VecDeque<String>,
    pub running: Option<String>,
}

/// Queues the extraction of videos that joined the collection, in timeline order.
pub fn queue_telemetry_extraction(
    store: &mut Store,
    services: &mut Services,
    video_ids: &[String],
) {
    let clips = store.video_import.clips();
    let ordered = sort_clips_by_timeline(&clips)
        .into_iter()
        .filter(|clip| video_ids.iter().any(|id| id == clip.id()))
        .map(|clip| clip.id().to_string());
    store.telemetry_queue.queued.extend(ordered);
    pump_telemetry(store, services);
}

/// Starts the next extraction when none runs; videos removed meanwhile are skipped.
fn pump_telemetry(store: &mut Store, services: &mut Services) {
    if store.telemetry_queue.running.is_some() {
        return;
    }
    while let Some(video_id) = store.telemetry_queue.queued.pop_front() {
        let Ok(video) = find_video(&store.video_import.imported_videos, &video_id) else {
            continue;
        };
        let path = video.path.clone();
        store.telemetry_queue.running = Some(video_id.clone());
        let tools = Arc::clone(&services.video_tools);
        services.workers.spawn("video-telemetry", move || {
            let telemetry = match tools
                .extract_telemetry(&path)
                .and_then(|response| response.map(ImportedActivity::from_response).transpose())
            {
                Ok(Some(activity)) => VideoTelemetry::ready(activity),
                Ok(None) => {
                    VideoTelemetry::without_activity(TelemetryStatus::Absent).expect("absent")
                }
                Err(error) => {
                    log::warn!("MP4 telemetry extraction failed (non-fatal): {error}");
                    VideoTelemetry::without_activity(TelemetryStatus::Failed).expect("failed")
                }
            };
            Box::new(move |store: &mut Store, services: &mut Services| {
                store.history.set_mode(HistoryMode::Skip);
                if let Err(error) = set_video_telemetry(store, &video_id, telemetry) {
                    store
                        .media
                        .set_error_message(format!("Video telemetry failed: {error}"));
                }
                store.telemetry_queue.running = None;
                pump_telemetry(store, services);
            })
        });
        return;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::lib::video_program::fixtures::*;
    use crate::store::services::fakes::{fake_services, Fakes};
    use crate::store::Dispatcher;
    use serde_json::json;

    fn timed(
        id: &'static str,
        creation_time: Option<&str>,
        duration_seconds: f64,
        import_order: u32,
    ) -> ImportedVideo {
        let path: &'static str = Box::leak(format!("/videos/{id}.mp4").into_boxed_str());
        let mut video = video(TestVideo {
            id,
            path,
            duration_seconds,
            import_order,
            ..TestVideo::default()
        });
        video.creation_time = creation_time.map(str::to_string);
        video.time_source = creation_time.map(|_| "gps".to_string());
        video.detected_creation_time = video.creation_time.clone();
        video.detected_time_source = video.time_source.clone();
        video
    }

    fn empty(activity: Option<(SyncActivity, f64)>) -> ReviewCollection {
        ReviewCollection {
            videos: Vec::new(),
            edits: VideoEditsMap::new(),
            activity_duration_seconds: activity.as_ref().map(|(_, duration)| *duration),
            activity: activity.map(|(activity, _)| activity),
        }
    }

    fn sofia() -> (SyncActivity, f64) {
        (
            SyncActivity {
                sync_time: Some("2026-07-18T08:00:00Z".into()),
                end_time: Some("2026-07-18T10:00:00Z".into()),
                timezone: Some("Europe/Sofia".into()),
            },
            7200.0,
        )
    }

    fn rows(collection: &ReviewCollection, selection: &ReviewSelection) -> Vec<ReviewRow> {
        let switches = resolve_import_switches(collection, selection);
        let shown =
            build_shown_collection(collection, &switches.included_ids, &switches.unplaced_ids);
        build_import_review(
            &shown,
            selection,
            &switches,
            ovrley_core::encode::ffmpeg::transitions::transition_defaults(),
        )
        .rows
    }

    #[test]
    fn the_largest_start_group_wins_and_ties_go_to_the_earliest() {
        assert_eq!(find_largest_start_group(&[], 10.0), None);
        assert_eq!(
            find_largest_start_group(&[50.0, 0.0, 5.0, 100.0, 105.0], 10.0),
            Some((0.0, 5.0))
        );
        assert_eq!(
            find_largest_start_group(&[0.0, 100.0, 101.0, 102.0], 10.0),
            Some((100.0, 102.0))
        );
    }

    #[test]
    fn videos_far_from_the_activity_start_switched_off() {
        let videos = vec![
            timed("near", Some("2026-07-18T08:30:00Z"), 60.0, 0),
            timed("far", Some("2026-07-18T14:00:00Z"), 60.0, 1),
        ];
        let (collection, selection) = open_import_review(&empty(Some(sofia())), videos).unwrap();
        let rows = rows(&collection, &selection);
        let state = |id: &str| {
            rows.iter()
                .find(|row| row.id == id)
                .map(|row| (row.is_included, row.is_far))
                .unwrap()
        };
        assert_eq!(state("near"), (true, false));
        assert_eq!(state("far"), (false, true));
    }

    #[test]
    fn without_a_reference_the_largest_group_of_new_videos_within_12_hours_is_switched_on() {
        let videos = vec![
            timed("a", Some("2026-07-18T08:00:00Z"), 60.0, 0),
            timed("b", Some("2026-07-18T09:00:00Z"), 60.0, 1),
            timed("c", Some("2026-07-19T12:00:00Z"), 60.0, 2),
        ];
        let (collection, selection) = open_import_review(&empty(None), videos).unwrap();
        let included: Vec<String> = rows(&collection, &selection)
            .into_iter()
            .filter(|row| row.is_included)
            .map(|row| row.id)
            .collect();
        assert_eq!(included, ["a", "b"]);
    }

    #[test]
    fn a_video_of_another_resolution_stays_off_even_when_switched_on() {
        let mut portrait = timed("portrait", Some("2026-07-18T08:01:00Z"), 60.0, 1);
        portrait.resolution = VideoResolution {
            width: 1080,
            height: 1920,
        };
        let (collection, mut selection) = open_import_review(
            &empty(None),
            vec![
                timed("landscape", Some("2026-07-18T08:00:00Z"), 60.0, 0),
                portrait,
            ],
        )
        .unwrap();
        selection.switch_overrides.insert("portrait".into(), true);
        let switches = resolve_import_switches(&collection, &selection);
        assert_eq!(
            switches.resolution,
            Some(VideoResolution {
                width: 1920,
                height: 1080
            })
        );
        assert!(
            switches.blocked_ids.contains("portrait")
                && !switches.included_ids.contains("portrait")
        );
    }

    #[test]
    fn a_waiting_video_waits_switched_off_after_the_last_switched_on_one_and_joins_where_it_waited()
    {
        let (collection, selection) = open_import_review(
            &empty(None),
            vec![
                timed("known", Some("2026-07-18T08:00:00Z"), 20.0, 0),
                timed("unknown", None, 20.0, 1),
            ],
        )
        .unwrap();
        let switches = resolve_import_switches(&collection, &selection);
        assert!(
            switches.unplaced_ids.contains("unknown") && !switches.included_ids.contains("unknown")
        );
        let shown =
            build_shown_collection(&collection, &switches.included_ids, &switches.unplaced_ids);
        assert_eq!(shown.edits["unknown"].offset_seconds, 20.0);
        let rows = build_import_review(
            &shown,
            &selection,
            &switches,
            ovrley_core::encode::ffmpeg::transitions::transition_defaults(),
        )
        .rows;
        assert!(
            rows.iter()
                .find(|row| row.id == "unknown")
                .unwrap()
                .is_positioned
        );
        let (collection, selection) = switch_on_waiting_video(
            &collection,
            &selection,
            "unknown",
            &shown,
            &switches.included_ids,
        )
        .unwrap();
        let switches = resolve_import_switches(&collection, &selection);
        assert!(switches.included_ids.contains("unknown"));
        assert_eq!(collection.edits["unknown"].offset_seconds, 20.0);
    }

    #[test]
    fn a_switched_off_video_leaves_no_gap_before_the_one_anchored_to_it() {
        let (collection, mut selection) = open_import_review(
            &empty(None),
            vec![
                timed("known", Some("2026-07-18T08:00:00Z"), 20.0, 0),
                timed("first", None, 20.0, 1),
                timed("second", None, 20.0, 2),
            ],
        )
        .unwrap();
        selection
            .placed_ids
            .extend(["first".to_string(), "second".to_string()]);
        selection.switch_overrides.insert("first".into(), false);
        selection.switch_overrides.insert("second".into(), true);
        let switches = resolve_import_switches(&collection, &selection);
        let shown =
            build_shown_collection(&collection, &switches.included_ids, &switches.unplaced_ids);
        assert_eq!(
            shown.edits["second"].offset_seconds, 20.0,
            "second moves up next to known"
        );
        assert_eq!(
            collection.edits["second"].offset_seconds, 40.0,
            "the draft is unchanged"
        );
    }

    #[test]
    fn a_first_import_reads_camera_clocks_the_way_that_fits_the_activity() {
        // In Sofia (UTC+3), 08:30 clock text read locally is 05:30Z, before the activity;
        // read as UTC it is inside.
        let mut clock = timed("clock", Some("2026-07-18T08:30:00"), 60.0, 0);
        clock.time_source = Some("ffprobe".into());
        clock.detected_time_source = clock.time_source.clone();
        let (collection, _) = open_import_review(&empty(Some(sofia())), vec![clock]).unwrap();
        assert_eq!(
            collection.edits["clock"].timezone_mode,
            Some(TimezoneMode::Utc)
        );
        assert_eq!(collection.edits["clock"].offset_seconds, 1800.0);
    }

    // The pipeline, with the fake video tools.

    pub(crate) fn add_probe(fakes: &Fakes, path: &str, creation_time: Option<&str>) {
        let mut probed = metadata(path, 10.0, (30, 1), 1920, 1080);
        probed.creation_time = creation_time.map(str::to_string);
        probed.time_source = creation_time.map(|_| "gps".to_string());
        fakes.video_tools.add_video(path, probed);
    }

    pub(crate) fn settle(dispatcher: &Dispatcher, fakes: &Fakes, done: impl Fn(&Store) -> bool) {
        for _ in 0..400 {
            fakes.ui_post.drain(dispatcher);
            if dispatcher.with_store(&done) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("timed out");
    }

    fn dispatcher() -> (Dispatcher, Fakes) {
        let (parts, fakes) = fake_services(json!({}));
        (Dispatcher::new(Store::default(), parts), fakes)
    }

    fn start(dispatcher: &Dispatcher, paths: &[&str]) {
        let paths: Vec<String> = paths.iter().map(|path| path.to_string()).collect();
        dispatcher.dispatch(move |store, services| {
            start_import(store, services, paths, ImportRequest::default()).unwrap()
        });
    }

    fn reviewing(store: &Store) -> bool {
        store
            .video_import_review
            .review
            .as_ref()
            .is_some_and(|review| review.phase != ReviewPhase::Probing)
    }

    #[test]
    fn a_quiet_import_commits_at_once_adopts_its_format_and_reads_telemetry() {
        let (dispatcher, fakes) = dispatcher();
        add_probe(&fakes, "/videos/ride.mp4", Some("2026-07-18T08:00:00Z"));
        fakes
            .video_tools
            .thumbnails
            .lock()
            .unwrap()
            .insert("/videos/ride.mp4".into(), vec![1, 2, 3]);
        dispatcher.dispatch(|store, _| {
            store
                .render_settings
                .set_fps_and_update_rate(60.0, 2)
                .unwrap()
        });
        start(&dispatcher, &["/videos/ride.mp4"]);
        assert!(dispatcher.with_store(|store| store.media.importing_video));
        settle(&dispatcher, &fakes, |store| {
            store.video_import.has_videos() && store.telemetry_queue.running.is_none()
        });
        dispatcher.with_store(|store| {
            assert!(store.video_import_review.review.is_none() && !store.media.importing_video);
            assert_eq!(
                store.video_import.imported_videos[0].thumbnail.as_deref(),
                Some(&[1u8, 2, 3][..])
            );
            assert_eq!(
                (
                    store.editor.config.scene["width"].clone(),
                    store.editor.config.scene["height"].clone()
                ),
                (json!(1920), json!(1080))
            );
            assert_eq!(store.render_settings.fps, 30.0);
            assert_eq!(
                store.editor_shell.editor_background_mode,
                EditorBackgroundMode::Video
            );
            assert_eq!(
                store.video_import.imported_videos[0].telemetry.status(),
                TelemetryStatus::Absent
            );
            assert_eq!(store.history.past.len(), 1, "only the render settings edit before the import is recorded");
        });
        assert_eq!(
            *fakes.video_tools.telemetry_calls.lock().unwrap(),
            ["/videos/ride.mp4"]
        );
    }

    #[test]
    fn an_already_imported_path_and_a_probe_failure_open_the_review_with_their_reasons() {
        let (dispatcher, fakes) = dispatcher();
        add_probe(&fakes, "/videos/a.mp4", Some("2026-07-18T08:00:00Z"));
        start(&dispatcher, &["/videos/a.mp4"]);
        settle(&dispatcher, &fakes, |store| store.video_import.has_videos());
        add_probe(&fakes, "/videos/b.mp4", Some("2026-07-18T08:01:00Z"));
        start(
            &dispatcher,
            &[
                "/videos/a.mp4",
                "/videos/b.mp4",
                "/videos/gone.mp4",
                "/videos/notes.txt",
            ],
        );
        settle(&dispatcher, &fakes, reviewing);
        dispatcher.with_store(|store| {
            let review = store.video_import_review.review.as_ref().unwrap();
            let failures: Vec<(&str, &str)> = review
                .failures
                .iter()
                .map(|failure| (failure.path.as_str(), failure.message.as_str()))
                .collect();
            assert_eq!(failures[0], ("/videos/a.mp4", "Already imported"));
            assert!(failures.iter().any(
                |(path, message)| *path == "/videos/gone.mp4" && message.contains("not a file")
            ));
            assert!(failures
                .iter()
                .any(|(path, message)| *path == "/videos/notes.txt"
                    && message.contains("Not a supported video file")));
            assert_eq!(review.collection.videos.len(), 2);
            assert!(!review.quiet);
        });
    }

    #[test]
    fn cancelling_while_probing_discards_the_results() {
        let (dispatcher, fakes) = dispatcher();
        add_probe(&fakes, "/videos/a.mp4", None);
        add_probe(&fakes, "/videos/b.mp4", None);
        start(&dispatcher, &["/videos/a.mp4", "/videos/b.mp4"]);
        dispatcher.dispatch(|store, _| cancel_import_review(store));
        std::thread::sleep(std::time::Duration::from_millis(50));
        fakes.ui_post.drain(&dispatcher);
        dispatcher.with_store(|store| {
            assert!(store.video_import_review.review.is_none() && !store.video_import.has_videos())
        });
    }

    #[test]
    fn a_far_video_switched_on_asks_first_and_import_anyway_commits() {
        let (dispatcher, fakes) = dispatcher();
        add_probe(&fakes, "/videos/a.mp4", Some("2026-07-18T08:00:00Z"));
        add_probe(&fakes, "/videos/b.mp4", Some("2026-07-20T08:00:00Z"));
        start(&dispatcher, &["/videos/a.mp4", "/videos/b.mp4"]);
        settle(&dispatcher, &fakes, reviewing);
        let far_id = dispatcher.with_store(|store| {
            store
                .video_import_review
                .review
                .as_ref()
                .unwrap()
                .collection
                .videos[1]
                .id
                .clone()
        });
        let id = far_id.clone();
        dispatcher.dispatch(move |store, _| set_review_row_included(store, &id, true).unwrap());
        dispatcher.dispatch(confirm_import_review);
        assert_eq!(
            dispatcher.with_store(|store| store.video_import_review.review.as_ref().unwrap().phase),
            ReviewPhase::Confirming
        );
        dispatcher.dispatch(|store, _| back_to_import_review(store));
        assert_eq!(
            dispatcher.with_store(|store| store.video_import_review.review.as_ref().unwrap().phase),
            ReviewPhase::Reviewing
        );
        dispatcher.dispatch(confirm_import_review);
        dispatcher.dispatch(confirm_import_review);
        dispatcher.with_store(|store| {
            assert!(store.video_import_review.review.is_none());
            assert_eq!(store.video_import.imported_videos.len(), 2);
        });
    }

    #[test]
    fn the_format_is_adopted_once_and_a_rejected_edit_shows_its_reason() {
        let (dispatcher, fakes) = dispatcher();
        add_probe(&fakes, "/videos/a.mp4", Some("2026-07-18T08:00:00Z"));
        start(&dispatcher, &["/videos/a.mp4"]);
        settle(&dispatcher, &fakes, |store| store.video_import.has_videos());
        dispatcher.dispatch(|store, _| set_scene_width(store, 640));
        add_probe(&fakes, "/videos/b.mp4", Some("2026-07-18T08:01:00Z"));
        start(&dispatcher, &["/videos/b.mp4"]);
        settle(&dispatcher, &fakes, reviewing);
        let error = dispatcher
            .dispatch_with_result(|store, _| set_review_row_offset(store, "nobody", 1.0))
            .unwrap()
            .unwrap_err();
        assert!(
            dispatcher.with_store(|store| store
                .video_import_review
                .review
                .as_ref()
                .unwrap()
                .error
                .clone())
                == Some(error)
        );
        dispatcher.dispatch(confirm_import_review);
        dispatcher.with_store(|store| {
            assert_eq!(store.video_import.imported_videos.len(), 2);
            assert_eq!(
                store.editor.config.scene["width"],
                json!(640),
                "only the first import adopts its format"
            );
        });
    }

    fn set_scene_width(store: &mut Store, width: u32) {
        store.editor.edit_config(|config| {
            config.scene.insert("width".into(), json!(width));
        });
    }

    #[test]
    fn telemetry_runs_one_at_a_time_in_timeline_order_and_skips_removed_videos() {
        let (dispatcher, fakes) = dispatcher();
        add_probe(&fakes, "/videos/late.mp4", Some("2026-07-18T09:00:00Z"));
        add_probe(&fakes, "/videos/early.mp4", Some("2026-07-18T08:00:00Z"));
        add_probe(&fakes, "/videos/gone.mp4", Some("2026-07-18T08:30:00Z"));
        start(
            &dispatcher,
            &["/videos/late.mp4", "/videos/early.mp4", "/videos/gone.mp4"],
        );
        settle(&dispatcher, &fakes, reviewing);
        dispatcher.dispatch(confirm_import_review);
        // The first extraction is running; the removed video's turn is skipped.
        let gone = dispatcher.with_store(|store| {
            store
                .video_import
                .imported_videos
                .iter()
                .find(|video| video.path.ends_with("gone.mp4"))
                .unwrap()
                .id
                .clone()
        });
        dispatcher
            .dispatch(move |store, _| crate::features::player::remove_video(store, &gone).unwrap());
        settle(&dispatcher, &fakes, |store| {
            store.telemetry_queue.running.is_none() && store.telemetry_queue.queued.is_empty()
        });
        assert_eq!(
            *fakes.video_tools.telemetry_calls.lock().unwrap(),
            ["/videos/early.mp4", "/videos/late.mp4"]
        );
    }

    #[test]
    fn thumbnails_render_at_most_four_at_once() {
        let (dispatcher, fakes) = dispatcher();
        *fakes.video_tools.thumbnail_delay.lock().unwrap() = std::time::Duration::from_millis(20);
        let paths: Vec<String> = (0..12)
            .map(|index| format!("/videos/clip-{index}.mp4"))
            .collect();
        for path in &paths {
            add_probe(&fakes, path, Some("2026-07-18T08:00:00Z"));
        }
        let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
        start(&dispatcher, &refs);
        settle(&dispatcher, &fakes, reviewing);
        let peak = fakes
            .video_tools
            .thumbnails_peak
            .load(std::sync::atomic::Ordering::SeqCst);
        assert!(peak <= THUMBNAIL_CONCURRENCY && peak > 1, "peak {peak}");
    }

    #[test]
    fn an_agent_import_with_include_commits_exactly_those_paths_or_fails_whole() {
        let (dispatcher, fakes) = dispatcher();
        add_probe(&fakes, "/videos/a.mp4", Some("2026-07-18T08:00:00Z"));
        add_probe(&fakes, "/videos/b.mp4", Some("2026-07-18T08:01:00Z"));
        let replies = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&replies);
        dispatcher.dispatch(move |store, services| {
            let request = ImportRequest {
                reply: Some(AgentReply::new(move |result| {
                    log.lock().unwrap().push(result)
                })),
                include: Some(vec!["/videos/b.mp4".into()]),
            };
            start_import(
                store,
                services,
                vec!["/videos/a.mp4".into(), "/videos/b.mp4".into()],
                request,
            )
            .unwrap();
        });
        settle(&dispatcher, &fakes, |store| {
            store.video_import_review.review.is_none()
        });
        assert_eq!(
            dispatcher.with_store(|store| store
                .video_import
                .imported_videos
                .iter()
                .map(|video| video.path.clone())
                .collect::<Vec<_>>()),
            ["/videos/b.mp4"]
        );
        let reply = replies.lock().unwrap().remove(0).unwrap();
        assert_eq!(reply["videos"][0]["path"], json!("/videos/b.mp4"));
        let log = Arc::clone(&replies);
        dispatcher.dispatch(move |store, services| {
            let request = ImportRequest {
                reply: Some(AgentReply::new(move |result| {
                    log.lock().unwrap().push(result)
                })),
                include: Some(vec!["/videos/gone.mp4".into()]),
            };
            start_import(
                store,
                services,
                vec!["/videos/a.mp4".into(), "/videos/gone.mp4".into()],
                request,
            )
            .unwrap();
        });
        settle(&dispatcher, &fakes, |store| {
            store.video_import_review.review.is_none()
        });
        assert!(replies
            .lock()
            .unwrap()
            .remove(0)
            .unwrap_err()
            .contains("/videos/gone.mp4"));
        assert_eq!(
            dispatcher.with_store(|store| store.video_import.imported_videos.len()),
            1,
            "nothing was committed"
        );
    }
}
