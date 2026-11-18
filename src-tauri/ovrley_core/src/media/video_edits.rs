//! The user's edits of an imported video, and what a project saves about the video itself.
//!
//! These are the one model of this state: the editor's store holds them, and projects read and
//! write them (`ovrley_services::project_file`). Their serde shape is the project file's: camelCase
//! names, `null` where the file has `null`. Types with invariants deserialize through their
//! validating constructors, so reading a project is the one validation.
//!
//! Cuts `{id, frame}` split a video at integer frames, strictly increasing and inside the video.
//! Each resulting segment has [`SegmentEdits`], keyed by the cut it starts at, or
//! [`FIRST_SEGMENT_KEY`] for the first one.

use crate::encode::ffmpeg::transitions::{is_xfade_transition, xfade_transitions, NO_TRANSITION};
use crate::encode::fps::Fps;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, HashSet};

/// The key of a video's first segment, which no cut begins.
pub const FIRST_SEGMENT_KEY: &str = "start";

/// Deserializes a nullable field that must be present.
pub(crate) fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

/// A transition into a segment: a hard cut, or an `xfade` from the core catalogue.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransitionKind {
    HardCut,
    Xfade(&'static str),
}

impl TransitionKind {
    /// Validates a transition type name against the catalogue (`requireTransitionType`).
    pub fn from_name(name: &str) -> Result<Self, String> {
        if name == NO_TRANSITION {
            return Ok(Self::HardCut);
        }
        xfade_transitions()
            .iter()
            .find(|candidate| candidate.as_str() == name)
            .map(|candidate| Self::Xfade(candidate.as_str()))
            .ok_or_else(|| format!("Unknown transition type: {name}"))
    }

    /// The name stored in projects: `none` or the `xfade` name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::HardCut => NO_TRANSITION,
            Self::Xfade(name) => name,
        }
    }
}

/// `{type, durationSeconds}`. A hard cut lasts 0 s; an `xfade` a positive number of seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transition {
    kind: TransitionKind,
    duration_seconds: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TransitionFields {
    #[serde(rename = "type")]
    transition_type: String,
    duration_seconds: f64,
}

impl Transition {
    /// `HARD_CUT`.
    pub const HARD_CUT: Transition = Transition {
        kind: TransitionKind::HardCut,
        duration_seconds: 0.0,
    };

    /// `requireTransition`: validates a stored override.
    pub fn new(kind: TransitionKind, duration_seconds: f64) -> Result<Self, String> {
        match kind {
            TransitionKind::HardCut if duration_seconds != 0.0 => {
                Err("No transition has a duration of 0 seconds".into())
            }
            TransitionKind::Xfade(_)
                if !(duration_seconds.is_finite() && duration_seconds > 0.0) =>
            {
                Err("Transition duration must be a positive number of seconds".into())
            }
            _ => Ok(Self {
                kind,
                duration_seconds,
            }),
        }
    }

    /// An `xfade` of a duration already known to be positive (snapped to frames).
    pub fn xfade(name: &'static str, duration_seconds: f64) -> Self {
        debug_assert!(is_xfade_transition(name) && duration_seconds > 0.0);
        Self {
            kind: TransitionKind::Xfade(name),
            duration_seconds,
        }
    }

    pub fn kind(&self) -> TransitionKind {
        self.kind
    }

    pub fn duration_seconds(&self) -> f64 {
        self.duration_seconds
    }

    pub fn is_hard_cut(&self) -> bool {
        self.kind == TransitionKind::HardCut
    }
}

impl Serialize for Transition {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        TransitionFields {
            transition_type: self.kind.name().to_string(),
            duration_seconds: self.duration_seconds,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Transition {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields = TransitionFields::deserialize(deserializer)?;
        let kind =
            TransitionKind::from_name(&fields.transition_type).map_err(serde::de::Error::custom)?;
        Transition::new(kind, fields.duration_seconds).map_err(serde::de::Error::custom)
    }
}

/// The edits of one segment.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SegmentEdits {
    /// Left out of the export; it stays on the timeline and in the preview.
    pub excluded: bool,
    /// The transition of the join into the segment; `None` follows the project defaults.
    #[serde(deserialize_with = "required_option")]
    pub transition_in: Option<Transition>,
    /// Hides the activity widgets over the segment, in the preview and the render.
    pub skip_overlay: bool,
}

impl SegmentEdits {
    pub const INCLUDED: SegmentEdits = SegmentEdits {
        excluded: false,
        transition_in: None,
        skip_overlay: false,
    };
}

/// A split of a video at a source frame.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cut {
    pub id: String,
    pub frame: i64,
}

/// Whether a clip's start follows its detected recording time or was set by hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OffsetSource {
    Auto,
    Manual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnchorSide {
    Before,
    After,
}

/// Places a clip whose time cannot be determined directly before or after another clip.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlacementAnchor {
    pub video_id: String,
    pub side: AnchorSide,
}

/// How a clip's camera clock is read against the activity: local time or UTC.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum TimezoneMode {
    Local,
    Utc,
}

/// The user's edits of one video (`createVideoEdits`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoEdits {
    /// The committed timeline start.
    pub offset_seconds: f64,
    pub offset_source: OffsetSource,
    pub cuts: Vec<Cut>,
    pub segments: BTreeMap<String, SegmentEdits>,
    pub placement_anchor: Option<PlacementAnchor>,
    /// `None` until inferred or chosen.
    pub timezone_mode: Option<TimezoneMode>,
}

impl Default for VideoEdits {
    /// `createVideoEdits`.
    fn default() -> Self {
        Self {
            offset_seconds: 0.0,
            offset_source: OffsetSource::Auto,
            cuts: Vec::new(),
            segments: BTreeMap::from([(FIRST_SEGMENT_KEY.to_string(), SegmentEdits::INCLUDED)]),
            placement_anchor: None,
            timezone_mode: None,
        }
    }
}

impl VideoEdits {
    /// The edits' own invariants: a finite offset; cuts at strictly increasing frames after the
    /// first, inside `frame_count` when it is known; one segment per cut and one for
    /// [`FIRST_SEGMENT_KEY`]. Transitions validated themselves when they were built.
    pub fn validate(&self, video_id: &str, frame_count: Option<i64>) -> Result<(), String> {
        if !self.offset_seconds.is_finite() {
            return Err("sources.videos offsetSeconds must be finite".into());
        }
        let mut expected_keys = HashSet::from([FIRST_SEGMENT_KEY]);
        let mut previous_frame = 0;
        for cut in &self.cuts {
            if cut.id.trim().is_empty() || cut.id == FIRST_SEGMENT_KEY {
                return Err(format!("sources.videos cuts id is not valid: {:?}", cut.id));
            }
            if !expected_keys.insert(cut.id.as_str()) {
                return Err(format!("sources.videos cuts id is duplicated: {}", cut.id));
            }
            if cut.frame <= previous_frame {
                return Err(
                    "sources.videos cuts require strictly increasing frames above 0".into(),
                );
            }
            previous_frame = cut.frame;
        }
        if frame_count.is_some_and(|frame_count| previous_frame >= frame_count) {
            return Err("sources.videos cuts must lie inside the video".into());
        }
        let keys: HashSet<&str> = self.segments.keys().map(String::as_str).collect();
        if keys != expected_keys {
            return Err(format!("sources.videos segments of {video_id} must have one entry per cut and one for {FIRST_SEGMENT_KEY}"));
        }
        Ok(())
    }
}

/// A display-oriented resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoResolution {
    pub width: u32,
    pub height: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FpsFields {
    num: u32,
    den: u32,
}

/// What a video was when it was last loaded (`createVideoMetadataSnapshot`), so a file that cannot
/// be found keeps its place, length and details. A found file's probe replaces it.
#[derive(Clone, Debug, PartialEq)]
pub struct VideoMetadataSnapshot {
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
    /// The detected recording time, before any change made in the editor.
    pub creation_time: Option<String>,
    pub time_source: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VideoMetadataFields {
    frame_count: i64,
    fps: FpsFields,
    resolution: VideoResolution,
    rotation_degrees: u16,
    has_audio: bool,
    #[serde(deserialize_with = "required_option")]
    codec_name: Option<String>,
    #[serde(deserialize_with = "required_option")]
    codec_long_name: Option<String>,
    #[serde(deserialize_with = "required_option")]
    bit_rate: Option<String>,
    #[serde(deserialize_with = "required_option")]
    camera_type: Option<String>,
    #[serde(deserialize_with = "required_option")]
    camera_model: Option<String>,
    #[serde(deserialize_with = "required_option")]
    creation_time: Option<String>,
    #[serde(deserialize_with = "required_option")]
    time_source: Option<String>,
}

impl TryFrom<VideoMetadataFields> for VideoMetadataSnapshot {
    type Error = String;

    fn try_from(fields: VideoMetadataFields) -> Result<Self, String> {
        if fields.frame_count <= 0 {
            return Err("sources.videos metadata frameCount must be positive".into());
        }
        let fps = Fps::new(fields.fps.num, fields.fps.den)
            .map_err(|_| "sources.videos metadata fps must be positive".to_string())?;
        if fields.resolution.width == 0 || fields.resolution.height == 0 {
            return Err("sources.videos metadata resolution must be positive".into());
        }
        if ![0, 90, 180, 270].contains(&fields.rotation_degrees) {
            return Err("sources.videos metadata rotationDegrees must be 0, 90, 180 or 270".into());
        }
        Ok(Self {
            frame_count: fields.frame_count,
            fps,
            resolution: fields.resolution,
            rotation_degrees: fields.rotation_degrees,
            has_audio: fields.has_audio,
            codec_name: fields.codec_name,
            codec_long_name: fields.codec_long_name,
            bit_rate: fields.bit_rate,
            camera_type: fields.camera_type,
            camera_model: fields.camera_model,
            creation_time: fields.creation_time,
            time_source: fields.time_source,
        })
    }
}

impl Serialize for VideoMetadataSnapshot {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (num, den) = self.fps.components();
        VideoMetadataFields {
            frame_count: self.frame_count,
            fps: FpsFields { num, den },
            resolution: self.resolution,
            rotation_degrees: self.rotation_degrees,
            has_audio: self.has_audio,
            codec_name: self.codec_name.clone(),
            codec_long_name: self.codec_long_name.clone(),
            bit_rate: self.bit_rate.clone(),
            camera_type: self.camera_type.clone(),
            camera_model: self.camera_model.clone(),
            creation_time: self.creation_time.clone(),
            time_source: self.time_source.clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for VideoMetadataSnapshot {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        VideoMetadataFields::deserialize(deserializer)?
            .try_into()
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn transitions_are_checked_against_the_catalogue() {
        assert_eq!(
            TransitionKind::from_name("spin").unwrap_err(),
            "Unknown transition type: spin"
        );
        assert_eq!(
            TransitionKind::from_name("none").unwrap(),
            TransitionKind::HardCut
        );
        let fade = TransitionKind::from_name("fade").unwrap();
        assert!(Transition::new(TransitionKind::HardCut, 1.0)
            .unwrap_err()
            .contains("duration of 0"));
        assert!(Transition::new(fade, 0.0).unwrap_err().contains("positive"));
        assert_eq!(Transition::new(fade, 1.0).unwrap().kind().name(), "fade");
    }

    #[test]
    fn segments_keep_the_file_shape() {
        let value = json!({"excluded": true, "transitionIn": {"type": "fade", "durationSeconds": 0.5}, "skipOverlay": false});
        let segment: SegmentEdits = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(segment).unwrap(), value);
        assert!(
            serde_json::from_value::<SegmentEdits>(json!({"excluded": true, "skipOverlay": false}))
                .is_err(),
            "transitionIn is required"
        );
        let error = serde_json::from_value::<SegmentEdits>(json!({"excluded": true, "transitionIn": {"type": "none", "durationSeconds": 1}, "skipOverlay": false})).unwrap_err();
        assert!(error.to_string().contains("duration of 0"), "{error}");
    }

    #[test]
    fn cuts_and_segments_follow_the_rules() {
        let mut edits = VideoEdits::default();
        assert_eq!(edits.validate("v", Some(10)), Ok(()));
        edits.cuts.push(Cut {
            id: "c".into(),
            frame: 10,
        });
        assert!(edits
            .validate("v", None)
            .unwrap_err()
            .contains("one entry per cut"));
        edits.segments.insert("c".into(), SegmentEdits::INCLUDED);
        assert_eq!(edits.validate("v", None), Ok(()));
        assert_eq!(
            edits.validate("v", Some(10)),
            Err("sources.videos cuts must lie inside the video".into())
        );
        edits.cuts[0].frame = 0;
        assert!(edits
            .validate("v", None)
            .unwrap_err()
            .contains("strictly increasing"));
    }

    #[test]
    fn metadata_round_trips_and_rejects_bad_numbers() {
        let value = json!({
            "frameCount": 300, "fps": {"num": 30000, "den": 1001}, "resolution": {"width": 1920, "height": 1080},
            "rotationDegrees": 0, "hasAudio": true, "codecName": "h264", "codecLongName": null, "bitRate": null,
            "cameraType": null, "cameraModel": null, "creationTime": "2024-01-01T10:00:00Z", "timeSource": "metadata"
        });
        let metadata: VideoMetadataSnapshot = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(&metadata).unwrap(), value);
        let mut bad = value.clone();
        bad["fps"]["den"] = json!(0);
        assert!(serde_json::from_value::<VideoMetadataSnapshot>(bad)
            .unwrap_err()
            .to_string()
            .contains("fps must be positive"));
        let mut bad = value;
        bad["rotationDegrees"] = json!(45);
        assert!(serde_json::from_value::<VideoMetadataSnapshot>(bad).is_err());
    }
}
