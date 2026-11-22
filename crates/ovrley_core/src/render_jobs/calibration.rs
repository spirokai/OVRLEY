//! Dormant: calibrated offsets for batch rendering of videos that are not loaded
//! into the editor.
//!
//! Editor renders submit the offset the user committed for each video
//! (`BatchActivity::ExternalActivity::offsets`). Videos that never entered the
//! editor have no committed offset, only an automatic baseline from their
//! timestamp. One calibration reference — a video whose committed and automatic
//! offsets are both known — yields a single correction that is applied to every
//! baseline. [`calibrated_offsets`] produces the same per-source offset map the
//! committed path consumes, so batch rendering can feed it in when it returns.
//!
//! Nothing calls this module yet; the DTOs below are not part of any request.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::{CoreError, CoreResult};

/// A video whose committed editor offset calibrates the automatic baselines.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchCalibrationReference {
    pub source_id: String,
    /// Effective editor timestamp, including an override when selected.
    pub creation_time: String,
    /// Null when the external probe did not identify timestamp provenance.
    pub time_source: Option<String>,
    pub committed_offset_seconds: f64,
    pub automatic_offset_seconds: f64,
}

/// Applies the reference correction once to each queued source's automatic
/// baseline. Without a reference the correction is exactly zero. The baselines
/// must identify exactly `source_ids`; a reference that is also queued must agree
/// with its own baseline.
pub fn calibrated_offsets(
    reference: Option<&BatchCalibrationReference>,
    automatic_offsets: &HashMap<String, f64>,
    source_ids: &[String],
) -> CoreResult<HashMap<String, f64>> {
    let correction = match reference {
        None => 0.0,
        Some(reference) => {
            if reference.creation_time.trim().is_empty()
                || !reference.committed_offset_seconds.is_finite()
                || !reference.automatic_offset_seconds.is_finite()
                || automatic_offsets
                    .get(&reference.source_id)
                    .is_some_and(|offset| *offset != reference.automatic_offset_seconds)
            {
                return Err(CoreError::Config(
                    "Batch calibration reference must have a consistent finite baseline".into(),
                ));
            }
            reference.committed_offset_seconds - reference.automatic_offset_seconds
        }
    };
    if automatic_offsets.len() != source_ids.len() {
        return Err(CoreError::Config(
            "Batch automatic offsets must identify exactly the queued sources".into(),
        ));
    }
    source_ids
        .iter()
        .map(|source_id| {
            let automatic = automatic_offsets.get(source_id).ok_or_else(|| {
                CoreError::Config(format!("Missing automatic offset for {source_id}"))
            })?;
            let offset = automatic + correction;
            if !automatic.is_finite() || !offset.is_finite() {
                return Err(CoreError::Config(
                    "Batch activity offsets must be finite".into(),
                ));
            }
            Ok((source_id.clone(), offset))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> Vec<String> {
        vec!["a".into(), "b".into()]
    }

    fn baselines() -> HashMap<String, f64> {
        [("a".into(), -5.0), ("b".into(), 20.0)].into()
    }

    fn reference(committed: f64, automatic: f64) -> BatchCalibrationReference {
        BatchCalibrationReference {
            source_id: "a".into(),
            creation_time: "2024-01-01T00:00:00Z".into(),
            time_source: None,
            committed_offset_seconds: committed,
            automatic_offset_seconds: automatic,
        }
    }

    #[test]
    fn applies_one_correction_to_each_automatic_baseline() {
        let offsets =
            calibrated_offsets(Some(&reference(7.0, -5.0)), &baselines(), &ids()).unwrap();
        assert_eq!(offsets, [("a".into(), 7.0), ("b".into(), 32.0)].into());
    }

    #[test]
    fn without_a_reference_the_baselines_are_the_offsets() {
        assert_eq!(
            calibrated_offsets(None, &baselines(), &ids()).unwrap(),
            baselines()
        );
    }

    #[test]
    fn rejects_invalid_references_and_baselines() {
        let mut missing = baselines();
        missing.remove("b");
        let mut infinite = baselines();
        infinite.insert("b".into(), f64::INFINITY);
        let mut foreign = missing.clone();
        foreign.insert("foreign".into(), 0.0);
        let mut blank_time = reference(7.0, -5.0);
        blank_time.creation_time = " ".into();
        for (reference, automatic) in [
            (None, HashMap::new()),
            (None, missing),
            (None, infinite),
            (None, foreign),
            (Some(reference(7.0, 1.0)), baselines()),
            (Some(reference(f64::NAN, -5.0)), baselines()),
            (Some(blank_time), baselines()),
        ] {
            assert!(calibrated_offsets(reference.as_ref(), &automatic, &ids()).is_err());
        }
    }
}
