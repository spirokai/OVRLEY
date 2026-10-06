//! Canonical batch wire vocabulary. Public DTOs serialize directly to the
//! frontend's camelCase shape, with no command-layer compatibility mapping.
//!
//! Required fields have no defaults. Optional absence is documented below.
//! These are ingress DTOs, not evidence of acceptance: shared configuration,
//! resource ownership, source freshness and destination checks belong to the
//! acceptance service. External metadata/activity keep their existing types
//! and tolerant parsing rules. Inspection state never represents execution.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::activity::schema::ParsedActivity;
use crate::encode::quality::QualityType;
use crate::media::source_video_metadata::SourceVideoMetadata;
use crate::normalize::raw::{BackdropConfig, LabelConfig, RasterConfig, ValueConfig};

/// Session-scoped inspected source, independent of preview registration.
/// Metadata uses the shared probe shape. Inspection must resolve required
/// duration, display geometry, rotation and exact FPS before publishing it.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InspectedVideoSource {
    pub source_id: String,
    /// `metadata.path` is the canonical absolute input path.
    pub metadata: SourceVideoMetadata,
    pub stamp: SourceFileStamp,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceFileStamp {
    pub size_bytes: u64,
    /// Decimal Unix nanoseconds as text, preserving precision across JS IPC.
    pub modified_at_unix_nanos: String,
}

/// Materialized shared presentation, with no activity/custom export window,
/// source video fields or encoding settings. Scene presentation fields remain
/// in the existing template vocabulary; each job later supplies render timing.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BatchTemplate {
    pub scene: BatchTemplateScene,
    pub backdrops: Vec<BackdropConfig>,
    pub rasters: Vec<RasterConfig>,
    pub labels: Vec<LabelConfig>,
    pub values: Vec<ValueConfig>,
    pub plots: Vec<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BatchTemplateScene {
    pub width: u32,
    pub height: u32,
    #[serde(flatten)]
    pub presentation: BTreeMap<String, Value>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BatchExportMode {
    Composite,
    Transparent,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchEncodingSettings {
    pub export_mode: BatchExportMode,
    pub export_codec: String,
    /// Transparent layout FPS; composite uses each inspected source's FPS.
    pub fps: u32,
    pub update_rate: u32,
    pub quality_type: QualityType,
    pub quality_value: f64,
    /// Absent unless the selected QSV profile requires detected init args.
    pub qsv_full_init_args: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VideoSyncTimezoneMode {
    Local,
    Utc,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchCalibrationReference {
    pub path: String,
    /// Effective editor timestamp, including an override when selected.
    pub creation_time: String,
    /// Null when the external probe did not identify timestamp provenance.
    pub time_source: Option<String>,
    pub committed_offset_seconds: f64,
    pub automatic_offset_seconds: f64,
}

/// Embedded mode has no shared activity calibration. With external activity,
/// absent reference means exactly zero correction; a present reference must
/// have a resolved signed baseline. Drag preview offsets never enter this DTO.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum BatchCalibration {
    EmbeddedActivity,
    ExternalActivity {
        timezone_mode: VideoSyncTimezoneMode,
        correction_seconds: f64,
        reference: Option<BatchCalibrationReference>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum BatchJobTiming {
    /// Required to be zero; full embedded telemetry is resolved per job later.
    EmbeddedActivity { offset_seconds: f64 },
    ExternalActivity {
        automatic_offset_seconds: f64,
        /// Signed automatic offset plus the shared correction, exactly once.
        offset_seconds: f64,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchRenderJob {
    pub id: String,
    pub source: InspectedVideoSource,
    pub timing: BatchJobTiming,
    /// Suppresses metric widgets and plots, retaining the job and static art.
    pub skip_overlay: bool,
    /// Planned destination; acceptance verifies naming, collisions and aliases.
    pub output_path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchRenderRequest {
    pub inspection_id: String,
    pub template: BatchTemplate,
    pub encoding: BatchEncodingSettings,
    /// Absent only in per-video embedded telemetry mode; never editor telemetry.
    pub external_activity: Option<ParsedActivity>,
    pub calibration: BatchCalibration,
    /// Absent without an external-activity reference. May be outside the queue.
    pub calibration_source: Option<InspectedVideoSource>,
    pub output_directory: String,
    /// Ordered, eligible jobs only. Row removal excludes a source; skipOverlay
    /// does not. Accepted queue order and inputs are immutable.
    pub jobs: Vec<BatchRenderJob>,
}

/// Owned inputs after acceptance, distinct from an inspection session. The
/// acceptance service will retain validated resources for this lifetime.
pub struct AcceptedBatchRequest {
    pub batch_id: String,
    pub request: BatchRenderRequest,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BatchPhase {
    Accepted,
    Preparing,
    Rendering,
    Cancelling,
    /// All submitted jobs succeeded, after cleanup.
    Completed,
    /// At least one success and at least one item error, after cleanup.
    CompletedWithErrors,
    /// All attempted jobs failed, after cleanup.
    Failed,
    /// Cancellation complete, after native worker/subprocess cleanup.
    Cancelled,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BatchItemPhase {
    Queued,
    Preparing,
    Rendering,
    Finished,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum BatchItemOutcome {
    Succeeded {
        output_path: String,
    },
    Failed {
        message: String,
    },
    /// Active operation was interrupted and its partial output cleaned up.
    Cancelled,
    /// Never started because the batch was cancelled.
    Unstarted,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchItemSnapshot {
    pub id: String,
    pub phase: BatchItemPhase,
    /// Absent until terminal. Inspection estimates are not authoritative totals.
    pub planned_frames: Option<u64>,
    pub rendered_frames: u64,
    pub encoded_frames: u64,
    /// Absent while queued or active; present only after item cleanup.
    pub outcome: Option<BatchItemOutcome>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchItemProgress {
    pub item_id: String,
    pub planned_frames: u64,
    /// Planned output work traversed, distinct from produced frame counts.
    pub current_frames: u64,
    pub rendered_frames: u64,
    pub encoded_frames: u64,
    pub elapsed_seconds: f64,
    /// Absent when throughput is unavailable or zero.
    pub estimated_seconds_remaining: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchResultCounts {
    pub succeeded: u32,
    pub failed: u32,
    pub cancelled: u32,
    pub unstarted: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchOutput {
    pub item_id: String,
    pub output_path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchSnapshot {
    pub batch_id: String,
    /// Monotonically increasing within a batch, shared by reads and events.
    /// Consumers ignore snapshots with revisions <= their current revision.
    pub revision: u64,
    pub phase: BatchPhase,
    /// Reservation remains busy through preparation, gaps and cancellation cleanup.
    pub renderer_busy: bool,
    pub items: Vec<BatchItemSnapshot>,
    /// Absent between items and after terminal cleanup.
    pub active_item_id: Option<String>,
    /// Absent until authoritative planning has resolved every submitted job.
    pub planned_frames: Option<u64>,
    /// Frame-weighted work settled, including failed jobs' planned work. Does
    /// not claim that failed/cancelled work produced successful output frames.
    pub processed_frames: u64,
    pub rendered_frames: u64,
    pub encoded_frames: u64,
    /// Absent while no item is active, including terminal snapshots.
    pub current_item_progress: Option<BatchItemProgress>,
    pub elapsed_seconds: f64,
    /// Absent when throughput is unavailable or zero.
    pub estimated_seconds_remaining: Option<f64>,
    /// Successful outputs only, retained across subsequent failure/cancellation.
    pub outputs: Vec<BatchOutput>,
    pub result_counts: BatchResultCounts,
}

/// Submission response includes the first authoritative snapshot; later
/// snapshot retrieval and events use exactly the same public shape.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchAcceptance {
    pub batch_id: String,
    pub snapshot: BatchSnapshot,
}
