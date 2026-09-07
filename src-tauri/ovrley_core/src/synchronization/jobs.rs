//! One managed analysis worker, one retained snapshot, and one bounded fingerprint pair.
//! Dropping the controller cancels and joins work before releasing native resources.

use super::*;
use crate::activity::parse_activity_json;
use fingerprint::Fingerprint;
use std::path::PathBuf;
use std::sync::{atomic::AtomicU64, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct InputRevisions {
    pub video_identity: String,
    pub video_revision: String,
    pub activity_identity: String,
    pub activity_revision: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartRequest {
    pub inputs: InputRevisions,
    pub video_path: PathBuf,
    pub parsed_activity_json: String,
    pub analysis_settings: AnalysisSettings,
}

pub fn backend_start_visual_sync(
    paths: &crate::paths::AppPaths,
    jobs: &AnalysisJobs,
    request: StartRequest,
    emit: Arc<dyn Fn(JobSnapshot) + Send + Sync>,
) -> Result<JobSnapshot, String> {
    jobs.start(paths.repo_root.clone(), request, emit)
}

pub fn backend_visual_sync_status(
    jobs: &AnalysisJobs,
    job_id: &str,
) -> Result<JobSnapshot, String> {
    jobs.status(job_id)
}

pub fn backend_cancel_visual_sync(jobs: &AnalysisJobs, job_id: &str) -> Result<(), String> {
    jobs.cancel(job_id)
}

#[derive(Clone, serde::Serialize)]
pub struct JobSnapshot {
    pub job_id: String,
    pub inputs: InputRevisions,
    /// Monotonic revision reconciles event delivery with status responses.
    pub sequence: u64,
    pub stage: AnalysisStage,
    /// Actual analyzed video timestamp; absent during indeterminate work.
    pub analyzed_seconds: Option<f64>,
    pub terminal: Option<JobTerminal>,
}

#[derive(Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobTerminal {
    Result { result: MatchResult },
    Cancelled,
    Error { message: String },
}

struct Worker {
    cancellation: Cancellation,
    handle: JoinHandle<()>,
}

struct CachedFingerprints {
    key: String,
    activity: Arc<Fingerprint>,
    video: Arc<Fingerprint>,
}

#[derive(Default)]
pub struct AnalysisJobs {
    worker: Mutex<Option<Worker>>,
    snapshot: Arc<Mutex<Option<JobSnapshot>>>,
    cache: Arc<Mutex<Option<CachedFingerprints>>>,
    next_id: AtomicU64,
}

impl AnalysisJobs {
    pub fn start(
        &self,
        repo_root: PathBuf,
        request: StartRequest,
        emit: Arc<dyn Fn(JobSnapshot) + Send + Sync>,
    ) -> Result<JobSnapshot, String> {
        request
            .analysis_settings
            .validate()
            .map_err(|e| e.to_string())?;
        for value in [
            &request.inputs.video_identity,
            &request.inputs.video_revision,
            &request.inputs.activity_identity,
            &request.inputs.activity_revision,
        ] {
            if value.trim().is_empty() {
                return Err("Input identities and revisions must be nonempty".into());
            }
        }
        let activity =
            parse_activity_json(&request.parsed_activity_json).map_err(|e| e.to_string())?;
        let metadata = std::fs::metadata(&request.video_path).map_err(|e| e.to_string())?;
        if !metadata.is_file() {
            return Err("Video path must identify a file".into());
        }
        // Version 1 uses uncalibrated motion; settings and file changes invalidate reuse.
        let key = format!(
            "v1:{:?}:{:?}:{}:{:?}:{:?}",
            request.inputs,
            request.video_path,
            metadata.len(),
            metadata.modified().map_err(|e| e.to_string())?,
            request.analysis_settings
        );
        let mut worker = self.worker.lock().expect("analysis worker mutex poisoned");
        if let Some(previous) = worker.as_ref() {
            if !previous.handle.is_finished() {
                return Err("An analysis job is already running".into());
            }
        }
        if let Some(previous) = worker.take() {
            previous
                .handle
                .join()
                .map_err(|_| "Analysis worker panicked")?;
        }
        let initial = JobSnapshot {
            job_id: self.next_id.fetch_add(1, Ordering::Relaxed).to_string(),
            inputs: request.inputs.clone(),
            sequence: 0,
            stage: AnalysisStage::Analyzing,
            analyzed_seconds: None,
            terminal: None,
        };
        *self
            .snapshot
            .lock()
            .expect("analysis snapshot mutex poisoned") = Some(initial.clone());
        let snapshot = self.snapshot.clone();
        let cache = self.cache.clone();
        let cancellation = Cancellation::default();
        let token = cancellation.clone();
        let mut current = initial.clone();
        let handle = std::thread::Builder::new()
            .name("visual-sync".into())
            .spawn(move || {
                let mut publish = |stage, seconds, terminal| {
                    current.sequence += 1;
                    if let Some(stage) = stage {
                        current.stage = stage;
                    }
                    current.analyzed_seconds = seconds;
                    current.terminal = terminal;
                    *snapshot.lock().expect("analysis snapshot mutex poisoned") =
                        Some(current.clone());
                    emit(current.clone());
                };
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                    || -> AnalysisResult<MatchResult> {
                        let cached = cache
                            .lock()
                            .expect("fingerprint cache mutex poisoned")
                            .as_ref()
                            .filter(|entry| entry.key == key)
                            .map(|entry| (entry.activity.clone(), entry.video.clone()));
                        let (activity, video) = match cached {
                            Some(pair) => pair,
                            None => {
                                let mut last = Instant::now();
                                let video = prepare_video_fingerprint(
                                    &repo_root,
                                    &request.video_path,
                                    request.analysis_settings,
                                    &token,
                                    |seconds| {
                                        if last.elapsed() >= Duration::from_millis(150) {
                                            publish(
                                                Some(AnalysisStage::Analyzing),
                                                Some(seconds),
                                                None,
                                            );
                                            last = Instant::now();
                                        }
                                    },
                                )?;
                                if token.is_cancelled() {
                                    return Err(AnalysisError::Cancelled);
                                }
                                let pair = (
                                    Arc::new(fingerprint::prepare_activity(&activity)),
                                    Arc::new(video),
                                );
                                // One pair, at most 32 MiB of retained fingerprint allocations. No images.
                                let bytes = pair.0.heap_bytes() + pair.1.heap_bytes();
                                *cache.lock().expect("fingerprint cache mutex poisoned") =
                                    if bytes <= 32 * 1024 * 1024 {
                                        Some(CachedFingerprints {
                                            key,
                                            activity: pair.0.clone(),
                                            video: pair.1.clone(),
                                        })
                                    } else {
                                        None
                                    };
                                pair
                            }
                        };
                        if token.is_cancelled() {
                            return Err(AnalysisError::Cancelled);
                        }
                        publish(Some(AnalysisStage::Matching), None, None);
                        matcher::match_fingerprints(&activity, &video, &token)
                    },
                ));
                let terminal = match result {
                    Ok(Ok(result)) if !token.is_cancelled() => JobTerminal::Result { result },
                    Ok(Ok(_)) | Ok(Err(AnalysisError::Cancelled)) => JobTerminal::Cancelled,
                    Ok(Err(error)) => JobTerminal::Error {
                        message: error.to_string(),
                    },
                    Err(_) => JobTerminal::Error {
                        message: "Analysis worker panicked".into(),
                    },
                };
                publish(None, None, Some(terminal));
            })
            .map_err(|e| e.to_string())?;
        *worker = Some(Worker {
            cancellation,
            handle,
        });
        Ok(initial)
    }

    pub fn status(&self, job_id: &str) -> Result<JobSnapshot, String> {
        self.snapshot
            .lock()
            .expect("analysis snapshot mutex poisoned")
            .as_ref()
            .filter(|snapshot| snapshot.job_id == job_id)
            .cloned()
            .ok_or_else(|| "Unknown analysis job".into())
    }

    pub fn cancel(&self, job_id: &str) -> Result<(), String> {
        let worker = self.worker.lock().expect("analysis worker mutex poisoned");
        self.status(job_id)?;
        if let Some(worker) = worker.as_ref() {
            worker.cancellation.cancel();
        }
        Ok(())
    }

    pub fn shutdown(&self) {
        if let Some(worker) = self
            .worker
            .lock()
            .expect("analysis worker mutex poisoned")
            .take()
        {
            worker.cancellation.cancel();
            let _ = worker.handle.join();
        }
    }
}

impl Drop for AnalysisJobs {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs() -> InputRevisions {
        InputRevisions {
            video_identity: "clip.mp4".into(),
            video_revision: "video-1".into(),
            activity_identity: "ride.fit".into(),
            activity_revision: "activity-1".into(),
        }
    }

    #[test]
    fn cancellation_is_job_specific_and_shutdown_joins() {
        let jobs = AnalysisJobs::default();
        let cancellation = Cancellation::default();
        let token = cancellation.clone();
        let exited = Arc::new(AtomicBool::new(false));
        let thread_exited = exited.clone();
        let handle = std::thread::spawn(move || {
            while !token.is_cancelled() {
                std::thread::sleep(Duration::from_millis(1));
            }
            thread_exited.store(true, Ordering::Release);
        });
        *jobs.worker.lock().unwrap() = Some(Worker {
            cancellation: cancellation.clone(),
            handle,
        });
        *jobs.snapshot.lock().unwrap() = Some(JobSnapshot {
            job_id: "current".into(),
            inputs: inputs(),
            sequence: 0,
            stage: AnalysisStage::Analyzing,
            analyzed_seconds: None,
            terminal: None,
        });
        assert!(jobs.cancel("stale").is_err());
        assert!(!cancellation.is_cancelled());
        jobs.cancel("current").unwrap();
        jobs.shutdown();
        assert!(exited.load(Ordering::Acquire));
        assert!(jobs.worker.lock().unwrap().is_none());
        jobs.shutdown();
    }

    #[test]
    fn malformed_options_fail_before_file_io_or_job_creation() {
        let jobs = AnalysisJobs::default();
        let request = StartRequest {
            inputs: inputs(),
            video_path: "does-not-exist.mp4".into(),
            parsed_activity_json: "{}".into(),
            analysis_settings: AnalysisSettings {
                frames_per_second: 0.0,
                long_edge_pixels: 640,
                start_seconds: 0.0,
                end_seconds: None,
            },
        };
        let error = jobs
            .start(PathBuf::from("."), request, Arc::new(|_| {}))
            .err()
            .unwrap();
        assert!(error.contains("Invalid analysis settings"));
        assert!(jobs.snapshot.lock().unwrap().is_none());
        assert!(jobs.worker.lock().unwrap().is_none());
    }

    #[test]
    fn status_retains_terminal_outcome_and_revisions() {
        let jobs = AnalysisJobs::default();
        *jobs.snapshot.lock().unwrap() = Some(JobSnapshot {
            job_id: "completed".into(),
            inputs: inputs(),
            sequence: 7,
            stage: AnalysisStage::Matching,
            analyzed_seconds: None,
            terminal: Some(JobTerminal::Result {
                result: MatchResult {
                    accepted: false,
                    bin_seconds: 4.0,
                    evaluated_offsets: 0,
                    video_observed_seconds: 0.0,
                    video_binned_observed_seconds: 0.0,
                    candidates: vec![],
                },
            }),
        });
        let status = jobs.status("completed").unwrap();
        assert_eq!(status.inputs, inputs());
        assert_eq!(status.sequence, 7);
        assert!(matches!(
            status.terminal,
            Some(JobTerminal::Result {
                result: MatchResult {
                    accepted: false,
                    ..
                }
            })
        ));
        assert!(jobs.status("other").is_err());
    }
}
