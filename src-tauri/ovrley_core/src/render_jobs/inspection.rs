//! Configuration-session inspection and the source-freshness acceptance seam.
//! Disposal drops session descriptors only; accepted sources own their Arcs.
//! This service never registers previews, extracts activities, or reserves the renderer.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};
use crate::media::prepared_video::{
    canonical_source_path, check_source_freshness, normalize_inspected_metadata,
    probe_video_metadata, source_file_stamp, InspectedVideoSource,
};
use crate::media::SourceVideoMetadata;
use crate::paths::AppPaths;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
const MAX_CONCURRENT_PROBES: usize = 5;

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
    pub source_ids: Vec<String>,
    pub calibration_source_id: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ReinspectionReason {
    ClosedSession,
    UnknownDescriptor,
    SourceChanged,
    SourceUnavailable,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInspectionIssue {
    pub source_id: String,
    /// Absent when the submitted identity is unknown or its session is closed.
    pub path: Option<String>,
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
    // The native service is the sole owner of inspection concurrency.
    // Disposal uses only sessions, so closing never waits for a long probe.
    active_probes: Mutex<usize>,
    probe_available: Condvar,
    probe: Arc<dyn SourceMetadataProbe>,
}

struct ProbePermit<'a>(&'a VideoInspectionService);

impl Drop for ProbePermit<'_> {
    fn drop(&mut self) {
        let mut active = self
            .0
            .active_probes
            .lock()
            .expect("inspection probe mutex poisoned");
        *active -= 1;
        self.0.probe_available.notify_one();
    }
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
            active_probes: Mutex::new(0),
            probe_available: Condvar::new(),
            probe,
        }
    }

    fn acquire_probe(&self) -> ProbePermit<'_> {
        let active = self
            .active_probes
            .lock()
            .expect("inspection probe mutex poisoned");
        let mut active = self
            .probe_available
            .wait_while(active, |active| *active >= MAX_CONCURRENT_PROBES)
            .expect("inspection probe mutex poisoned");
        *active += 1;
        ProbePermit(self)
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
        let _probe_permit = self.acquire_probe();
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
        for source_id in selection
            .source_ids
            .iter()
            .chain(selection.calibration_source_id.iter())
        {
            let stored = session.and_then(|sources| sources.get(source_id));
            let reason = match (session, stored) {
                (None, _) => Some(ReinspectionReason::ClosedSession),
                (_, None) => Some(ReinspectionReason::UnknownDescriptor),
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
                    source_id: source_id.clone(),
                    path: stored.map(|source| source.metadata.path.clone()),
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
        let calibration_source = selection.calibration_source_id.as_ref().map(|_| {
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

/// Source metadata required by composite encoding, resolved once before execution.
#[derive(Clone, Copy, Debug)]
pub struct CompositeSourceMetadata {
    pub has_audio: bool,
    pub rotation_degrees: Option<i32>,
}

pub(crate) fn verify_composite_source_resolution(
    paths: &AppPaths,
    composite_video_path: &Path,
    scene_width: u32,
    scene_height: u32,
) -> CoreResult<CompositeSourceMetadata> {
    if !composite_video_path.is_file() {
        return Err(CoreError::Config(format!(
            "Composite video does not exist: {}",
            composite_video_path.display()
        )));
    }

    let video_path = composite_video_path.to_str().ok_or_else(|| {
        CoreError::Config(format!(
            "Composite video path is not valid Unicode: {}",
            composite_video_path.display()
        ))
    })?;
    let metadata = crate::media::video_probe::probe_video(&paths.repo_root, video_path)?;
    let resolution = metadata.resolution.ok_or_else(|| {
        CoreError::Config(format!(
            "Could not read composite video resolution for {}",
            composite_video_path.display()
        ))
    })?;

    let rotation = metadata
        .rotation_degrees
        .map(|degrees| degrees.rem_euclid(360));
    let (display_width, display_height) = if matches!(rotation, Some(90 | 270)) {
        (resolution.height, resolution.width)
    } else {
        (resolution.width, resolution.height)
    };

    if u64::from(scene_width) != display_width || u64::from(scene_height) != display_height {
        return Err(CoreError::Config(format!(
            "scene resolution {scene_width}x{scene_height} must match display-oriented composite video resolution {display_width}x{display_height} (coded {}x{}, rotation {})",
            resolution.width,
            resolution.height,
            metadata
                .rotation_degrees
                .map(|degrees| degrees.to_string())
                .unwrap_or_else(|| "none".to_string())
        )));
    }

    Ok(CompositeSourceMetadata {
        has_audio: metadata.has_audio,
        rotation_degrees: metadata.rotation_degrees,
    })
}
