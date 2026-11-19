//! Port of `features/toolbar/utils/videoSyncControls.js` (`deriveVideoSyncControls`), which the
//! import dialog and the video panel share; Phase 8's Sync Doctor builds on it.

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
