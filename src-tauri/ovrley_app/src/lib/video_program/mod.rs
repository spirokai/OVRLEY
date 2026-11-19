//! Port of `lib/video-program`: the imported-video collection and the program built from it
//! (plan D4).
//!
//! An [`ImportedVideo`] is created from probed metadata once, at import ingress. The user's edits
//! (placement, cuts, segments) live in [`VideoEdits`], keyed by the video's id. A [`Clip`] joins
//! the two for the timeline and the program. Frame arithmetic is `ovrley_core`'s [`Fps`], and the
//! transition catalogue is `ovrley_core`'s.
//!
//! Cuts `{id, frame}` split a video at integer frames, strictly increasing and inside the video.
//! Each resulting segment has [`SegmentEdits`], keyed by the cut it starts at, or
//! [`FIRST_SEGMENT_KEY`] for the first one. A video without cuts is one segment, so excluding a
//! whole video and excluding a part of it are the same edit.
//!
//! The edits themselves (`VideoEdits` and its parts) are `ovrley_core::media::video_edits`, which
//! projects read and write.

pub mod collection;
pub mod edits;
pub mod program;
pub mod segments;
pub mod warnings;

use crate::store::slices::media::ImportedActivity;
use ovrley_core::encode::fps::Fps;
use crate::lib::video_sync::SyncWarning;
use ovrley_core::media::video_edits::{Cut, SegmentEdits, VideoEdits, VideoMetadataSnapshot, VideoResolution};
use std::sync::Arc;
use ovrley_core::media::SourceVideoMetadata;
use std::collections::BTreeMap;

/// A timeline interval, in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SecondsRange {
    pub start_second: f64,
    pub end_second: f64,
}

/// Where a clip's embedded telemetry stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetryStatus {
    Pending,
    Ready,
    Absent,
    Failed,
}

/// A clip's embedded telemetry: only a ready one carries an activity (`requireTelemetry`).
#[derive(Clone, Debug, PartialEq)]
pub struct VideoTelemetry {
    status: TelemetryStatus,
    activity: Option<ImportedActivity>,
}

impl VideoTelemetry {
    pub fn pending() -> Self {
        Self { status: TelemetryStatus::Pending, activity: None }
    }

    pub fn ready(activity: ImportedActivity) -> Self {
        Self { status: TelemetryStatus::Ready, activity: Some(activity) }
    }

    /// A settled result without an activity.
    pub fn without_activity(status: TelemetryStatus) -> Result<Self, String> {
        if status == TelemetryStatus::Ready {
            return Err("Ready video telemetry carries an activity".into());
        }
        Ok(Self { status, activity: None })
    }

    pub fn status(&self) -> TelemetryStatus {
        self.status
    }

    pub fn activity(&self) -> Option<&ImportedActivity> {
        self.activity.as_ref()
    }
}

/// A canonical imported video (`createImportedVideo`). `duration_seconds` is frame-exact
/// (`frame_count × den / num`), `resolution` is display-oriented and `rotation_degrees` is 0, 90,
/// 180 or 270.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedVideo {
    pub id: String,
    pub path: String,
    pub import_order: u32,
    pub duration_seconds: f64,
    pub frame_count: i64,
    pub fps: Fps,
    pub resolution: VideoResolution,
    pub rotation_degrees: u16,
    pub has_audio: bool,
    pub codec_name: Option<String>,
    pub codec_long_name: Option<String>,
    pub bit_rate: Option<String>,
    pub camera_type: Option<String>,
    pub camera_model: Option<String>,
    pub creation_time: Option<String>,
    pub time_source: Option<String>,
    pub detected_creation_time: Option<String>,
    pub detected_time_source: Option<String>,
    pub preview_warnings: Vec<String>,
    /// A project video whose file cannot be found.
    pub missing: bool,
    pub sync_warning: Option<SyncWarning>,
    pub telemetry: VideoTelemetry,
    /// The first frame as PNG, when one could be read (`getVideoThumbnail`).
    pub thumbnail: Option<Arc<[u8]>>,
}

/// The edits of every video, keyed by video id.
pub type VideoEditsMap = BTreeMap<String, VideoEdits>;

/// A timeline clip: a video joined with its edits (`joinVideoEdits`).
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub video: ImportedVideo,
    pub edits: VideoEdits,
}

impl Clip {
    pub fn id(&self) -> &str {
        &self.video.id
    }

    pub fn offset_seconds(&self) -> f64 {
        self.edits.offset_seconds
    }

    pub fn duration_seconds(&self) -> f64 {
        self.video.duration_seconds
    }

    /// `getVideoEndSecond`: the exclusive timeline end.
    pub fn end_second(&self) -> f64 {
        self.edits.offset_seconds + self.video.duration_seconds
    }

    pub fn fps(&self) -> Fps {
        self.video.fps
    }

    pub fn frame_count(&self) -> i64 {
        self.video.frame_count
    }

    pub fn cuts(&self) -> &[Cut] {
        &self.edits.cuts
    }

    /// The edits of a segment of this clip; segment keys come from its cuts.
    pub fn segment(&self, key: &str) -> &SegmentEdits {
        &self.edits.segments[key]
    }
}

/// `createImportedVideo`: validates probed metadata once at import ingress. Malformed metadata
/// fails, as the probe is the app's own boundary.
pub fn create_imported_video(metadata: &SourceVideoMetadata, id: String, import_order: u32, preview_warnings: Vec<String>) -> Result<ImportedVideo, String> {
    if metadata.path.is_empty() {
        return Err("Imported video metadata must include a path".into());
    }
    let duration = metadata
        .duration
        .filter(|duration| duration.is_finite() && *duration > 0.0)
        .ok_or("Imported video metadata contains an invalid duration")?;
    let fps_num = metadata.fps_num.filter(|value| *value > 0).ok_or("Imported video metadata contains an invalid frame rate numerator")?;
    let fps_den = metadata.fps_den.filter(|value| *value > 0).ok_or("Imported video metadata contains an invalid frame rate denominator")?;
    let fps = Fps::new(fps_num, fps_den).map_err(|error| error.to_string())?;
    // A positive duration always has at least one frame.
    let frame_count = i64::try_from(fps.frame_count_for_duration(duration).map_err(|error| error.to_string())?)
        .map_err(|_| "Imported video metadata contains an invalid duration".to_string())?;
    let rotation_degrees = match metadata.rotation_degrees.unwrap_or(0).rem_euclid(360) {
        rotation @ (0 | 90 | 180 | 270) => rotation as u16,
        _ => return Err("Imported video metadata contains an unsupported rotation".into()),
    };
    let resolution = metadata.resolution.as_ref().ok_or("Imported video metadata must include a resolution")?;
    let invalid = || "Imported video metadata contains an invalid resolution".to_string();
    let (width, height) = (u32::try_from(resolution.width).map_err(|_| invalid())?, u32::try_from(resolution.height).map_err(|_| invalid())?);
    if width == 0 || height == 0 {
        return Err(invalid());
    }
    let resolution = if rotation_degrees == 90 || rotation_degrees == 270 { VideoResolution { width: height, height: width } } else { VideoResolution { width, height } };
    Ok(ImportedVideo {
        id,
        path: metadata.path.clone(),
        import_order,
        duration_seconds: fps.frames_to_seconds(frame_count),
        frame_count,
        fps,
        resolution,
        rotation_degrees,
        has_audio: metadata.has_audio,
        codec_name: metadata.codec_name.clone(),
        codec_long_name: metadata.codec_long_name.clone(),
        bit_rate: metadata.bit_rate.clone(),
        camera_type: metadata.camera_type.clone(),
        camera_model: metadata.camera_model.clone(),
        creation_time: metadata.creation_time.clone(),
        time_source: metadata.time_source.clone(),
        detected_creation_time: metadata.creation_time.clone(),
        detected_time_source: metadata.time_source.clone(),
        preview_warnings,
        missing: false,
        sync_warning: None,
        telemetry: VideoTelemetry::pending(),
        thumbnail: None,
    })
}

/// `createMissingVideo`: the entity of a project video whose file cannot be found, from the
/// metadata the project saved (validated when the project was read). It has no thumbnail, and no
/// embedded telemetry can be read.
pub fn create_missing_video(metadata: &VideoMetadataSnapshot, id: String, path: String, import_order: u32) -> ImportedVideo {
    ImportedVideo {
        id,
        path,
        import_order,
        duration_seconds: metadata.fps.frames_to_seconds(metadata.frame_count),
        frame_count: metadata.frame_count,
        fps: metadata.fps,
        resolution: metadata.resolution,
        rotation_degrees: metadata.rotation_degrees,
        has_audio: metadata.has_audio,
        codec_name: metadata.codec_name.clone(),
        codec_long_name: metadata.codec_long_name.clone(),
        bit_rate: metadata.bit_rate.clone(),
        camera_type: metadata.camera_type.clone(),
        camera_model: metadata.camera_model.clone(),
        creation_time: metadata.creation_time.clone(),
        time_source: metadata.time_source.clone(),
        detected_creation_time: metadata.creation_time.clone(),
        detected_time_source: metadata.time_source.clone(),
        preview_warnings: Vec::new(),
        missing: true,
        sync_warning: None,
        telemetry: VideoTelemetry::without_activity(TelemetryStatus::Failed).expect("failed telemetry has no activity"),
        thumbnail: None,
    }
}

/// `createVideoMetadataSnapshot`: the video as a project saves it, with its *detected* recording
/// time, so it can be shown and kept on the timeline when its file cannot be found.
pub fn video_metadata_snapshot(video: &ImportedVideo) -> VideoMetadataSnapshot {
    VideoMetadataSnapshot {
        frame_count: video.frame_count,
        fps: video.fps,
        resolution: video.resolution,
        rotation_degrees: video.rotation_degrees,
        has_audio: video.has_audio,
        codec_name: video.codec_name.clone(),
        codec_long_name: video.codec_long_name.clone(),
        bit_rate: video.bit_rate.clone(),
        camera_type: video.camera_type.clone(),
        camera_model: video.camera_model.clone(),
        creation_time: video.detected_creation_time.clone(),
        time_source: video.detected_time_source.clone(),
    }
}

/// A new video or cut id, as `crypto.randomUUID`.
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// `nextImportOrder`.
pub fn next_import_order(videos: &[ImportedVideo]) -> u32 {
    videos.iter().map(|video| video.import_order + 1).max().unwrap_or(0)
}

#[cfg(test)]
pub(crate) mod fixtures {
    //! `tests/helpers/videoFixtures.js`.
    use super::*;
    use ovrley_core::media::Resolution as ProbedResolution;
    use ovrley_core::media::video_edits::FIRST_SEGMENT_KEY;

    pub struct TestVideo {
        pub id: &'static str,
        pub path: &'static str,
        pub duration_seconds: f64,
        pub fps: (u32, u32),
        pub width: u32,
        pub height: u32,
        pub import_order: u32,
    }

    impl Default for TestVideo {
        fn default() -> Self {
            Self { id: "video-1", path: "/videos/ride.mp4", duration_seconds: 60.0, fps: (30, 1), width: 1920, height: 1080, import_order: 0 }
        }
    }

    pub fn metadata(path: &str, duration: f64, fps: (u32, u32), width: u32, height: u32) -> SourceVideoMetadata {
        SourceVideoMetadata {
            path: path.into(),
            duration: Some(duration),
            fps_num: Some(fps.0),
            fps_den: Some(fps.1),
            resolution: Some(ProbedResolution { width: width.into(), height: height.into() }),
            has_audio: true,
            codec_name: Some("h264".into()),
            ..Default::default()
        }
    }

    /// `createTestVideo`, through the production ingress.
    pub fn video(test: TestVideo) -> ImportedVideo {
        create_imported_video(&metadata(test.path, test.duration_seconds, test.fps, test.width, test.height), test.id.into(), test.import_order, Vec::new()).unwrap()
    }

    /// A video of `duration_seconds` at 30 fps with an id and import order.
    pub fn simple(id: &'static str, duration_seconds: f64, import_order: u32) -> ImportedVideo {
        video(TestVideo { id, duration_seconds, import_order, ..TestVideo::default() })
    }

    /// One cut of `cutEdits`: its id, frame and the edits of the segment it starts.
    pub fn cut(id: &str, frame: i64, edits: SegmentEdits) -> (String, i64, SegmentEdits) {
        (id.into(), frame, edits)
    }

    pub fn excluded() -> SegmentEdits {
        SegmentEdits { excluded: true, ..SegmentEdits::INCLUDED }
    }

    /// `cutEdits`: edits that split a video.
    pub fn cut_edits(cuts: &[(String, i64, SegmentEdits)], start: SegmentEdits) -> VideoEdits {
        let mut edits = VideoEdits::default();
        edits.segments.insert(FIRST_SEGMENT_KEY.into(), start);
        for (id, frame, segment) in cuts {
            edits.cuts.push(Cut { id: id.clone(), frame: *frame });
            edits.segments.insert(id.clone(), *segment);
        }
        edits
    }

    /// `videoStoreState`: edits placed at explicit offsets.
    pub fn edits_at(videos: &[ImportedVideo], offsets: &[(&str, f64)]) -> VideoEditsMap {
        videos
            .iter()
            .map(|video| {
                let offset = offsets.iter().find(|(id, _)| *id == video.id).map_or(0.0, |(_, offset)| *offset);
                (video.id.clone(), VideoEdits { offset_seconds: offset, ..VideoEdits::default() })
            })
            .collect()
    }

    pub fn clip(video: ImportedVideo, edits: VideoEdits) -> Clip {
        Clip { video, edits }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    #[test]
    fn ingress_stores_a_reduced_fps_a_frame_exact_duration_and_a_display_resolution() {
        let mut probed = metadata("/videos/ride.mp4", 10.0103, (60000, 2002), 1920, 1080);
        probed.rotation_degrees = Some(90);
        let video = create_imported_video(&probed, "v".into(), 3, vec!["slow seeking".into()]).unwrap();
        assert_eq!(video.fps, Fps::new(30000, 1001).unwrap());
        // 300.09 frames: the partial last frame counts, as in the native renderer.
        assert_eq!(video.frame_count, 301);
        assert!((video.duration_seconds - 301.0 * 1001.0 / 30000.0).abs() < 1e-12);
        assert_eq!(video.resolution, VideoResolution { width: 1080, height: 1920 });
        assert_eq!((video.import_order, video.preview_warnings.clone(), video.sync_warning), (3, vec!["slow seeking".to_string()], None));
        assert_eq!(video.telemetry.status(), TelemetryStatus::Pending);
    }

    #[test]
    fn ingress_rejects_malformed_metadata() {
        let base = || metadata("/videos/ride.mp4", 60.0, (30, 1), 1920, 1080);
        type Change = Box<dyn Fn(&mut SourceVideoMetadata)>;
        let cases: [(&str, Change); 5] = [
            ("must include a path", Box::new(|m| m.path.clear())),
            ("invalid duration", Box::new(|m| m.duration = Some(0.0))),
            ("invalid frame rate numerator", Box::new(|m| m.fps_num = None)),
            ("invalid frame rate denominator", Box::new(|m| m.fps_den = Some(0))),
            ("unsupported rotation", Box::new(|m| m.rotation_degrees = Some(45))),
        ];
        for (message, change) in cases {
            let mut probed = base();
            change(&mut probed);
            let error = create_imported_video(&probed, "v".into(), 0, Vec::new()).unwrap_err();
            assert!(error.contains(message), "{error}");
        }
    }

    #[test]
    fn frames_are_counted_like_the_native_renderer() {
        let frames = |duration, fps| create_imported_video(&metadata("/v.mp4", duration, fps, 1920, 1080), "v".into(), 0, Vec::new()).unwrap().frame_count;
        assert_eq!(frames(10.0, (30, 1)), 300);
        assert_eq!(frames(10.000001, (30, 1)), 300);
        assert_eq!(frames(10.01, (30, 1)), 301);
        assert_eq!(frames(0.001, (30, 1)), 1);
        assert_eq!(frames(10.01, (30000, 1001)), 300);
    }

    #[test]
    fn telemetry_carries_an_activity_only_when_ready() {
        assert!(VideoTelemetry::without_activity(TelemetryStatus::Ready).is_err());
        assert_eq!(VideoTelemetry::without_activity(TelemetryStatus::Absent).unwrap().activity(), None);
    }

    #[test]
    fn a_missing_video_keeps_its_saved_place_and_details_and_snapshots_its_detected_time() {
        let mut probed = metadata("/videos/ride.mp4", 10.0, (30000, 1001), 1920, 1080);
        probed.creation_time = Some("2024-01-01T10:00:00".into());
        probed.time_source = Some("ffprobe".into());
        let mut video = create_imported_video(&probed, "v".into(), 2, Vec::new()).unwrap();
        video.creation_time = Some("2024-01-01T11:00:00".into());
        video.time_source = Some("filename".into());
        let snapshot = video_metadata_snapshot(&video);
        assert_eq!((snapshot.creation_time.as_deref(), snapshot.time_source.as_deref()), (Some("2024-01-01T10:00:00"), Some("ffprobe")));
        let missing = create_missing_video(&snapshot, "v".into(), "/gone/ride.mp4".into(), 2);
        assert!(missing.missing);
        assert_eq!(missing.telemetry.status(), TelemetryStatus::Failed);
        assert_eq!((missing.frame_count, missing.fps, missing.duration_seconds), (video.frame_count, video.fps, video.duration_seconds));
        assert_eq!((missing.creation_time.clone(), missing.detected_creation_time.clone()), (snapshot.creation_time.clone(), snapshot.creation_time.clone()));
        assert_eq!(missing.thumbnail, None);
    }

    #[test]
    fn import_orders_continue_after_the_highest() {
        assert_eq!(next_import_order(&[]), 0);
        assert_eq!(next_import_order(&[simple("a", 1.0, 4), simple("b", 1.0, 1)]), 5);
    }
}
