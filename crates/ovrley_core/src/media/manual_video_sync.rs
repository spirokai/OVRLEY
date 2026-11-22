//! The manual video sync's durable state: the video whose workspace is selected, its landmarks
//! (in video seconds), the detected location and the detection thresholds
//! (`manualVideoSyncContract.js`). The editor's store holds it and projects save it; deserializing
//! validates it, so reading a project is the one validation. Cross-field rules that need the whole
//! project (the video exists, an activity for the detected location) stay with the project.

use super::video_edits::required_option;
use serde::{Deserialize, Deserializer, Serialize};

pub const MAX_MANUAL_LANDMARKS: usize = 5;
pub const MAX_MANUAL_LOCATION_LANDMARKS: usize = 1;
pub const DEFAULT_MANUAL_SPEED_THRESHOLD_KMH: f64 = 5.0;
pub const MIN_MANUAL_SPEED_THRESHOLD_KMH: f64 = 1.0;
pub const MAX_MANUAL_SPEED_THRESHOLD_KMH: f64 = 10.0;
pub const DEFAULT_MANUAL_TURN_THRESHOLD_DEGREES: f64 = 80.0;
pub const MIN_MANUAL_TURN_THRESHOLD_DEGREES: f64 = 70.0;
pub const MAX_MANUAL_TURN_THRESHOLD_DEGREES: f64 = 180.0;

/// What a landmark marks: the four types the user can mark (`VIDEO_SYNC_LANDMARK_TYPES`), in the
/// type select's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum LandmarkKind {
    Stop,
    LeftTurn,
    RightTurn,
    Location,
}

impl LandmarkKind {
    pub const ALL: [Self; 4] = [Self::Stop, Self::LeftTurn, Self::RightTurn, Self::Location];
}

/// A landmark the user placed in the video.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Landmark {
    #[serde(rename = "stop")]
    Stop {
        id: String,
        #[serde(rename = "videoSecond")]
        video_second: f64,
    },
    #[serde(rename = "leftTurn")]
    LeftTurn {
        id: String,
        #[serde(rename = "videoSecond")]
        video_second: f64,
    },
    #[serde(rename = "rightTurn")]
    RightTurn {
        id: String,
        #[serde(rename = "videoSecond")]
        video_second: f64,
    },
    #[serde(rename = "location")]
    Location {
        id: String,
        #[serde(rename = "videoSecond")]
        video_second: f64,
        #[serde(rename = "activitySecond", deserialize_with = "required_option")]
        activity_second: Option<f64>,
    },
}

impl Landmark {
    /// A new landmark of `kind`. A location landmark's activity second is always `None`: nothing
    /// sets it, but projects carry the field (a React oddity kept).
    pub fn new(id: String, kind: LandmarkKind, video_second: f64) -> Self {
        match kind {
            LandmarkKind::Stop => Self::Stop { id, video_second },
            LandmarkKind::LeftTurn => Self::LeftTurn { id, video_second },
            LandmarkKind::RightTurn => Self::RightTurn { id, video_second },
            LandmarkKind::Location => Self::Location { id, video_second, activity_second: None },
        }
    }

    pub fn kind(&self) -> LandmarkKind {
        match self {
            Self::Stop { .. } => LandmarkKind::Stop,
            Self::LeftTurn { .. } => LandmarkKind::LeftTurn,
            Self::RightTurn { .. } => LandmarkKind::RightTurn,
            Self::Location { .. } => LandmarkKind::Location,
        }
    }

    /// The same landmark retyped (`setVideoSyncLandmarkType`): a new location's activity second is
    /// `None`, and another type drops it.
    pub fn retyped(&self, kind: LandmarkKind) -> Self {
        Self::new(self.id().to_string(), kind, self.video_second())
    }

    pub fn set_video_second(&mut self, second: f64) {
        match self {
            Self::Stop { video_second, .. }
            | Self::LeftTurn { video_second, .. }
            | Self::RightTurn { video_second, .. }
            | Self::Location { video_second, .. } => *video_second = second,
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Stop { id, .. }
            | Self::LeftTurn { id, .. }
            | Self::RightTurn { id, .. }
            | Self::Location { id, .. } => id,
        }
    }

    pub fn video_second(&self) -> f64 {
        match self {
            Self::Stop { video_second, .. }
            | Self::LeftTurn { video_second, .. }
            | Self::RightTurn { video_second, .. }
            | Self::Location { video_second, .. } => *video_second,
        }
    }
}

/// The manual sync's durable state.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualVideoSync {
    pub video_id: Option<String>,
    pub landmarks: Vec<Landmark>,
    pub detected_location_second: Option<f64>,
    pub speed_threshold_kmh: f64,
    pub turn_threshold_degrees: f64,
}

impl Default for ManualVideoSync {
    /// No video, no landmarks, the default thresholds.
    fn default() -> Self {
        Self {
            video_id: None,
            landmarks: Vec::new(),
            detected_location_second: None,
            speed_threshold_kmh: DEFAULT_MANUAL_SPEED_THRESHOLD_KMH,
            turn_threshold_degrees: DEFAULT_MANUAL_TURN_THRESHOLD_DEGREES,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManualVideoSyncFields {
    #[serde(deserialize_with = "required_option")]
    video_id: Option<String>,
    landmarks: Vec<Landmark>,
    // Older projects omit it.
    #[serde(default)]
    detected_location_second: Option<f64>,
    speed_threshold_kmh: f64,
    turn_threshold_degrees: f64,
}

impl ManualVideoSync {
    /// The state's own rules: thresholds in range, at most five landmarks with unique ids and
    /// finite non-negative video seconds, at most one location landmark, and landmarks only with
    /// a video.
    pub fn validate(&self) -> Result<(), String> {
        if self.video_id.is_none() && !self.landmarks.is_empty() {
            return Err("sync.manual.landmarks require sync.manual.videoId".into());
        }
        if let Some(activity_second) = self.detected_location_second {
            if !activity_second.is_finite() || activity_second < 0.0 {
                return Err("sync.manual.detectedLocationSecond must be a finite non-negative number or null".into());
            }
        }
        if self.landmarks.len() > MAX_MANUAL_LANDMARKS {
            return Err(format!(
                "sync.manual.landmarks must contain at most {MAX_MANUAL_LANDMARKS} landmarks"
            ));
        }
        if !(MIN_MANUAL_SPEED_THRESHOLD_KMH..=MAX_MANUAL_SPEED_THRESHOLD_KMH)
            .contains(&self.speed_threshold_kmh)
        {
            return Err(format!(
                "sync.manual.speedThresholdKmh must be finite and between {MIN_MANUAL_SPEED_THRESHOLD_KMH} and {MAX_MANUAL_SPEED_THRESHOLD_KMH}"
            ));
        }
        if !(MIN_MANUAL_TURN_THRESHOLD_DEGREES..=MAX_MANUAL_TURN_THRESHOLD_DEGREES)
            .contains(&self.turn_threshold_degrees)
        {
            return Err(format!(
                "sync.manual.turnThresholdDegrees must be finite and between {MIN_MANUAL_TURN_THRESHOLD_DEGREES} and {MAX_MANUAL_TURN_THRESHOLD_DEGREES}"
            ));
        }
        let mut ids: Vec<&str> = Vec::with_capacity(self.landmarks.len());
        let mut location_count = 0;
        for landmark in &self.landmarks {
            let id = landmark.id();
            if id.trim().is_empty() {
                return Err("sync.manual.landmark id must not be empty".into());
            }
            if ids.contains(&id) {
                return Err(format!("sync.manual.landmark id is duplicated: {id}"));
            }
            ids.push(id);
            let video_second = landmark.video_second();
            if !video_second.is_finite() || video_second < 0.0 {
                return Err(
                    "sync.manual.landmark videoSecond must be a finite non-negative number".into(),
                );
            }
            if let Landmark::Location {
                activity_second, ..
            } = landmark
            {
                location_count += 1;
                if activity_second.is_some_and(|second| !second.is_finite()) {
                    return Err(
                        "sync.manual.location landmark activitySecond must be finite or null"
                            .into(),
                    );
                }
            }
        }
        if location_count > MAX_MANUAL_LOCATION_LANDMARKS {
            return Err(format!("sync.manual.landmarks must contain at most {MAX_MANUAL_LOCATION_LANDMARKS} location landmark"));
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for ManualVideoSync {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields = ManualVideoSyncFields::deserialize(deserializer)?;
        let manual = Self {
            video_id: fields.video_id,
            landmarks: fields.landmarks,
            detected_location_second: fields.detected_location_second,
            speed_threshold_kmh: fields.speed_threshold_kmh,
            turn_threshold_degrees: fields.turn_threshold_degrees,
        };
        manual.validate().map_err(serde::de::Error::custom)?;
        Ok(manual)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn round_trips_and_validates() {
        let value = json!({
            "videoId": "a", "landmarks": [{"type": "location", "id": "l", "videoSecond": 2.0, "activitySecond": null}],
            "detectedLocationSecond": null, "speedThresholdKmh": 5.0, "turnThresholdDegrees": 80.0
        });
        let manual: ManualVideoSync = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(&manual).unwrap(), value);
        let mut bad = value.clone();
        bad["videoId"] = json!(null);
        assert!(serde_json::from_value::<ManualVideoSync>(bad)
            .unwrap_err()
            .to_string()
            .contains("require sync.manual.videoId"));
        let mut bad = value;
        bad["speedThresholdKmh"] = json!(11);
        assert!(serde_json::from_value::<ManualVideoSync>(bad).is_err());
        assert_eq!(ManualVideoSync::default().validate(), Ok(()));
    }

    #[test]
    fn the_turn_threshold_reaches_180_degrees() {
        // React's slider goes to 180 degrees; projects it saved above 160 failed to open (Q3: A).
        let value = |degrees: f64| json!({
            "videoId": null, "landmarks": [], "speedThresholdKmh": 5.0, "turnThresholdDegrees": degrees
        });
        assert_eq!(serde_json::from_value::<ManualVideoSync>(value(170.0)).unwrap().turn_threshold_degrees, 170.0);
        assert!(serde_json::from_value::<ManualVideoSync>(value(180.0)).is_ok());
        assert!(serde_json::from_value::<ManualVideoSync>(value(185.0)).is_err());
        assert!(serde_json::from_value::<ManualVideoSync>(value(65.0)).is_err());
    }

    #[test]
    fn retyping_a_landmark_keeps_its_id_and_second() {
        let stop = Landmark::new("a".into(), LandmarkKind::Stop, 3.0);
        let location = stop.retyped(LandmarkKind::Location);
        assert_eq!(location, Landmark::Location { id: "a".into(), video_second: 3.0, activity_second: None });
        assert_eq!(location.retyped(LandmarkKind::RightTurn).kind(), LandmarkKind::RightTurn);
    }
}
