//! Batch acceptance: validates and pins inputs before queue execution.

use super::batch::BatchServiceError;
use super::contracts::*;
use super::inspection::{InspectionSourceSelection, InspectionValidation, VideoInspectionService};
use super::planning::{
    referenced_source_ids, validate_batch_encoding, validate_batch_outputs,
    validate_batch_template, PlannedBatchOutput, ValidatedBatchTemplate,
};
use crate::activity::schema::ParsedActivity;
use crate::activity::validate_render_activity;
use crate::error::{CoreError, CoreResult};
use crate::raster::RasterResourceResolver;
use std::collections::{HashMap, HashSet};

pub(super) struct AcceptedJob {
    pub(super) id: String,
    pub(super) output: PlannedBatchOutput,
    /// Per piece, in piece order: the activity second where frame 0 of the
    /// piece's source sits (0 for embedded telemetry).
    pub(super) piece_offsets: Vec<f64>,
}

pub(super) struct AcceptedBatch {
    pub(super) template: ValidatedBatchTemplate,
    /// External activity, its timeline end, and its disabled laps.
    pub(super) external_activity: Option<(ParsedActivity, f64, Vec<usize>)>,
    pub(super) jobs: Vec<AcceptedJob>,
}

pub(super) fn accept_batch(
    inspection: &VideoInspectionService,
    request: BatchRenderRequest,
    resources: Option<&dyn RasterResourceResolver>,
) -> Result<AcceptedBatch, BatchServiceError> {
    if request.outputs.is_empty() || request.outputs.len() > u32::MAX as usize {
        return Err(
            CoreError::Config("Batch must contain a nonempty supported queue".into()).into(),
        );
    }
    let mut ids = HashSet::new();
    for output in &request.outputs {
        if output.id.trim().is_empty() || !ids.insert(&output.id) {
            return Err(CoreError::Config(
                "Batch output identities must be nonempty and distinct".into(),
            )
            .into());
        }
    }
    validate_batch_outputs(&request.outputs, &request.destination)?;
    let source_ids = referenced_source_ids(&request.outputs);
    let offsets = validate_batch_activity(&request.activity, &source_ids)?;
    let owned = match inspection.validate_sources(&InspectionSourceSelection {
        inspection_id: request.inspection_id.clone(),
        source_ids,
    }) {
        InspectionValidation::Valid(sources) => sources,
        InspectionValidation::Rejected(rejection) => {
            return Err(BatchServiceError::ReinspectionRequired {
                message: "Batch sources require fresh inspection before submission".into(),
                rejection,
            })
        }
    };
    let encoding = validate_batch_encoding(&request.encoding)?;
    let planned = encoding.plan_outputs(&request.outputs, &owned, &request.destination)?;
    let template = validate_batch_template(request.template, encoding, resources)?;
    let external_activity = match request.activity {
        BatchActivity::EmbeddedActivity {} => None,
        BatchActivity::ExternalActivity {
            activity,
            disabled_laps,
            ..
        } => {
            let activity = crate::activity::normalize_parsed_activity(activity)?;
            let end = validate_render_activity(&activity)?;
            let disabled_laps =
                crate::normalize::validate_disabled_laps(disabled_laps, "activity.disabledLaps")?;
            Some((activity, end, disabled_laps))
        }
    };
    let jobs = request
        .outputs
        .into_iter()
        .zip(planned)
        .map(|(output, planned)| AcceptedJob {
            // Embedded telemetry is video-local: frame 0 is activity second 0.
            piece_offsets: output
                .pieces
                .iter()
                .map(|piece| {
                    offsets
                        .as_ref()
                        .map_or(0.0, |offsets| offsets[&piece.source_id])
                })
                .collect(),
            id: output.id,
            output: planned,
        })
        .collect();
    Ok(AcceptedBatch {
        template,
        external_activity,
        jobs,
    })
}

/// Validate the committed offsets at submission: exactly one finite offset per
/// referenced source with an external activity; None in embedded mode.
fn validate_batch_activity(
    activity: &BatchActivity,
    source_ids: &[String],
) -> CoreResult<Option<HashMap<String, f64>>> {
    let BatchActivity::ExternalActivity { offsets, .. } = activity else {
        return Ok(None);
    };
    if offsets.len() != source_ids.len() || source_ids.iter().any(|id| !offsets.contains_key(id)) {
        return Err(CoreError::Config(
            "Batch offsets must identify exactly the queued sources".into(),
        ));
    }
    if offsets.values().any(|offset| !offset.is_finite()) {
        return Err(CoreError::Config(
            "Batch activity offsets must be finite".into(),
        ));
    }
    Ok(Some(offsets.clone()))
}
