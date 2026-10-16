//! Batch acceptance: validates and pins inputs before queue execution.

use super::batch::BatchServiceError;
use super::contracts::*;
use super::inspection::{InspectionSourceSelection, InspectionValidation, VideoInspectionService};
use super::planning::{
    validate_batch_encoding, validate_batch_template, PlannedBatchOutput, ValidatedBatchTemplate,
};
use crate::activity::schema::ParsedActivity;
use crate::activity::validate_render_activity;
use crate::error::{CoreError, CoreResult};
use crate::raster::RasterResourceResolver;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

pub(super) struct AcceptedJob {
    pub(super) id: String,
    pub(super) source: Arc<InspectedVideoSource>,
    pub(super) offset: f64,
    pub(super) skip_overlay: bool,
    pub(super) output: PlannedBatchOutput,
}

pub(super) struct AcceptedBatch {
    pub(super) template: ValidatedBatchTemplate,
    pub(super) external_activity: Option<(ParsedActivity, f64)>,
    pub(super) jobs: Vec<AcceptedJob>,
}

pub(super) fn accept_batch(
    inspection: &VideoInspectionService,
    request: BatchRenderRequest,
    resources: Option<&dyn RasterResourceResolver>,
) -> Result<AcceptedBatch, BatchServiceError> {
    if request.jobs.is_empty() || request.jobs.len() > u32::MAX as usize {
        return Err(
            CoreError::Config("Batch must contain a nonempty supported queue".into()).into(),
        );
    }
    let mut ids = HashSet::new();
    for job in &request.jobs {
        if job.id.trim().is_empty() || !ids.insert(&job.id) {
            return Err(CoreError::Config(
                "Batch job identities must be nonempty and distinct".into(),
            )
            .into());
        }
    }
    let offsets = validate_batch_activity(&request.activity, &request.jobs)?;
    let owned = match inspection.validate_sources(&InspectionSourceSelection {
        inspection_id: request.inspection_id.clone(),
        source_ids: request
            .jobs
            .iter()
            .map(|job| job.source_id.clone())
            .collect(),
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
    let outputs = encoding.plan_outputs(&owned, Path::new(&request.output_directory))?;
    let template = validate_batch_template(request.template, encoding, resources)?;
    let external_activity = match request.activity {
        BatchActivity::EmbeddedActivity {} => None,
        BatchActivity::ExternalActivity { activity, .. } => {
            let activity = crate::activity::normalize_parsed_activity(activity)?;
            let end = validate_render_activity(&activity)?;
            Some((activity, end))
        }
    };
    let jobs = request
        .jobs
        .into_iter()
        .zip(owned.sources())
        .zip(outputs)
        .zip(offsets)
        .map(|(((job, source), output), offset)| AcceptedJob {
            id: job.id,
            source: source.clone(),
            offset,
            skip_overlay: job.skip_overlay,
            output,
        })
        .collect();
    Ok(AcceptedBatch {
        template,
        external_activity,
        jobs,
    })
}

/// Validate the committed offsets at submission: exactly one finite offset per queued source.
fn validate_batch_activity(
    activity: &BatchActivity,
    jobs: &[BatchRenderJob],
) -> CoreResult<Vec<f64>> {
    let BatchActivity::ExternalActivity { offsets, .. } = activity else {
        return Ok(vec![0.0; jobs.len()]);
    };
    if offsets.len() != jobs.len() {
        return Err(CoreError::Config(
            "Batch offsets must identify exactly the queued sources".into(),
        ));
    }
    jobs.iter()
        .map(|job| {
            let offset = offsets.get(&job.source_id).ok_or_else(|| {
                CoreError::Config(format!("Missing offset for {}", job.source_id))
            })?;
            if !offset.is_finite() {
                return Err(CoreError::Config(
                    "Batch activity offsets must be finite".into(),
                ));
            }
            Ok(*offset)
        })
        .collect()
}
