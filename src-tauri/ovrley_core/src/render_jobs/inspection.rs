//! Configuration-session inspection and the source-freshness acceptance seam.
//! Disposal drops session descriptors only; accepted sources own their Arcs.
//! This service never registers previews, extracts activities, or reserves the renderer.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};
use crate::media::prepared_video::{
    canonical_source_path, check_source_freshness, normalize_inspected_metadata,
    probe_video_metadata, source_file_stamp, InspectedVideoSource,
};
use crate::media::SourceVideoMetadata;
use crate::paths::AppPaths;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn identity(prefix: &str) -> String {
    format!("{prefix}-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed))
}

/// External-system seam: tests can pause/change a file during metadata probing.
pub trait SourceMetadataProbe: Send + Sync {
    fn probe(&self, paths: &AppPaths, path: &str) -> CoreResult<SourceVideoMetadata>;
}

struct MediaMetadataProbe;

impl SourceMetadataProbe for MediaMetadataProbe {
    fn probe(&self, paths: &AppPaths, path: &str) -> CoreResult<SourceVideoMetadata> {
        probe_video_metadata(paths, path)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectionSession {
    pub inspection_id: String,
}

/// Also used by submission to select the queue and an optional out-of-folder
/// calibration source. Effective editor timestamp overrides remain calibration
/// inputs: they must never replace this descriptor's actual file metadata/stamp.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InspectionSourceSelection {
    pub inspection_id: String,
    pub sources: Vec<InspectedVideoSource>,
    pub calibration_source: Option<InspectedVideoSource>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ReinspectionReason {
    ClosedSession,
    UnknownDescriptor,
    DescriptorMismatch,
    SourceChanged,
    SourceUnavailable,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInspectionIssue {
    pub source_id: String,
    pub path: String,
    pub reason: ReinspectionReason,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectionRejection {
    pub inspection_id: String,
    pub issues: Vec<SourceInspectionIssue>,
}

/// Immutable, owned sources to retain in accepted jobs. No session lookup is
/// needed after acceptance; freshness is checked again before each source runs.
pub struct AcceptedInspectionSources {
    sources: Vec<Arc<InspectedVideoSource>>,
    calibration_source: Option<Arc<InspectedVideoSource>>,
}

impl AcceptedInspectionSources {
    pub fn sources(&self) -> &[Arc<InspectedVideoSource>] {
        &self.sources
    }

    pub fn calibration_source(&self) -> Option<&InspectedVideoSource> {
        self.calibration_source.as_deref()
    }
}

pub enum InspectionValidation {
    Valid(AcceptedInspectionSources),
    Rejected(InspectionRejection),
}

type SessionSources = HashMap<String, Arc<InspectedVideoSource>>;

pub struct VideoInspectionService {
    sessions: Mutex<HashMap<String, SessionSources>>,
    // One metadata probe at a time bounds parser memory and ffprobe subprocesses.
    // Disposal uses only sessions, so closing never waits for a long probe.
    probe_gate: Mutex<()>,
    probe: Arc<dyn SourceMetadataProbe>,
}

impl Default for VideoInspectionService {
    fn default() -> Self {
        Self::with_probe(Arc::new(MediaMetadataProbe))
    }
}

impl VideoInspectionService {
    pub fn with_probe(probe: Arc<dyn SourceMetadataProbe>) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            probe_gate: Mutex::new(()),
            probe,
        }
    }

    pub fn create_session(&self) -> InspectionSession {
        let inspection_id = identity("inspection");
        self.sessions
            .lock()
            .expect("inspection mutex poisoned")
            .insert(inspection_id.clone(), HashMap::new());
        InspectionSession { inspection_id }
    }

    /// Idempotent disposal also prevents in-flight probes from publishing.
    pub fn dispose_session(&self, inspection_id: &str) {
        self.sessions
            .lock()
            .expect("inspection mutex poisoned")
            .remove(inspection_id);
    }

    pub fn inspect_source(
        &self,
        paths: &AppPaths,
        inspection_id: &str,
        path: &str,
    ) -> CoreResult<InspectedVideoSource> {
        self.require_session(inspection_id)?;
        let _probe_guard = self
            .probe_gate
            .lock()
            .expect("inspection probe mutex poisoned");
        self.require_session(inspection_id)?;
        let original_path = Path::new(path);
        let canonical_path = canonical_source_path(original_path)?;
        let before = source_file_stamp(Path::new(&canonical_path))?;
        let probed = self.probe.probe(paths, &canonical_path);
        // Check even when probing failed: changed versions must not be reported
        // as an ordinary probe error or published as a mixed-version descriptor.
        let unchanged = canonical_source_path(original_path)
            .is_ok_and(|current| current == canonical_path)
            && source_file_stamp(Path::new(&canonical_path)).is_ok_and(|current| current == before);
        if !unchanged {
            return Err(CoreError::Config(format!(
                "Source changed during inspection: {path}"
            )));
        }
        let (metadata, display_resolution) =
            normalize_inspected_metadata(probed?, &canonical_path)?;
        let descriptor = InspectedVideoSource {
            source_id: identity("source"),
            metadata,
            display_resolution,
            stamp: before,
        };
        let mut sessions = self.sessions.lock().expect("inspection mutex poisoned");
        let sources = sessions
            .get_mut(inspection_id)
            .ok_or_else(|| closed_session(inspection_id))?;
        // Reinspection replaces the prior descriptor for this path in this session.
        sources.retain(|_, source| source.metadata.path != canonical_path);
        sources.insert(descriptor.source_id.clone(), Arc::new(descriptor.clone()));
        Ok(descriptor)
    }

    pub fn validate_sources(&self, selection: &InspectionSourceSelection) -> InspectionValidation {
        let sessions = self.sessions.lock().expect("inspection mutex poisoned");
        let session = sessions.get(&selection.inspection_id);
        let mut issues = Vec::new();
        let mut owned = Vec::new();
        for source in selection
            .sources
            .iter()
            .chain(selection.calibration_source.iter())
        {
            let stored = session.and_then(|sources| sources.get(&source.source_id));
            let reason = match (session, stored) {
                (None, _) => Some(ReinspectionReason::ClosedSession),
                (_, None) => Some(ReinspectionReason::UnknownDescriptor),
                (_, Some(stored)) if stored.as_ref() != source => {
                    Some(ReinspectionReason::DescriptorMismatch)
                }
                (_, Some(stored)) => match check_source_freshness(stored) {
                    Ok(()) => {
                        owned.push(stored.clone());
                        None
                    }
                    Err(CoreError::Io { .. }) => Some(ReinspectionReason::SourceUnavailable),
                    Err(_) => Some(ReinspectionReason::SourceChanged),
                },
            };
            if let Some(reason) = reason {
                issues.push(SourceInspectionIssue {
                    source_id: source.source_id.clone(),
                    path: source.metadata.path.clone(),
                    reason,
                });
            }
        }
        if session.is_none() || !issues.is_empty() {
            return InspectionValidation::Rejected(InspectionRejection {
                inspection_id: selection.inspection_id.clone(),
                issues,
            });
        }
        let calibration_source = selection.calibration_source.as_ref().map(|_| {
            owned
                .pop()
                .expect("validated calibration descriptor is owned")
        });
        InspectionValidation::Valid(AcceptedInspectionSources {
            sources: owned,
            calibration_source,
        })
    }

    fn require_session(&self, inspection_id: &str) -> CoreResult<()> {
        if !self
            .sessions
            .lock()
            .expect("inspection mutex poisoned")
            .contains_key(inspection_id)
        {
            return Err(closed_session(inspection_id));
        }
        Ok(())
    }
}

fn closed_session(inspection_id: &str) -> CoreError {
    CoreError::Config(format!("Inspection session is closed: {inspection_id}"))
}
