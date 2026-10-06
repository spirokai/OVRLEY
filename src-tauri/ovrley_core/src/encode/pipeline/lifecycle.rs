//! Shared FFmpeg process lifecycle and pipeline teardown.

use std::path::{Path, PathBuf};
use std::process::{Child, ExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::error::{CoreError, CoreResult};

use super::queue::WriterResult;

/// Single source of truth for whether the pipeline should stop and why.
///
/// Observes session cancellation without writing it. A pipeline failure stops
/// this item's workers and preserves its first error; it must not request
/// cancellation of the entire reserved operation.
pub(crate) struct PipelineShutdown {
    session_cancel: Arc<AtomicBool>,
    failed: AtomicBool,
    error: Mutex<Option<CoreError>>,
}

impl PipelineShutdown {
    pub(crate) fn new(cancel_flag: Arc<AtomicBool>) -> Self {
        Self {
            session_cancel: cancel_flag,
            failed: AtomicBool::new(false),
            error: Mutex::new(None),
        }
    }

    pub(crate) fn shared(cancel_flag: Arc<AtomicBool>) -> Arc<Self> {
        Arc::new(Self::new(cancel_flag))
    }

    pub(crate) fn is_stopped(&self) -> bool {
        self.session_cancel.load(Ordering::SeqCst) || self.failed.load(Ordering::SeqCst)
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.session_cancel.load(Ordering::SeqCst)
    }

    /// Records a failure and signals all observers to stop.
    ///
    /// The error is stored before the flag is set so that any observer
    /// seeing `is_stopped() == true` will also observe the recorded error.
    pub(crate) fn signal_failure(&self, error: CoreError) {
        self.error.lock().unwrap().get_or_insert(error);
        self.failed.store(true, Ordering::SeqCst);
    }

    pub(crate) fn has_error(&self) -> bool {
        self.failed.load(Ordering::SeqCst)
    }

    pub(crate) fn take_error(&self) -> Option<CoreError> {
        self.error.lock().unwrap().take()
    }

    /// Returns `Err` if shutdown has been signalled, distinguishing
    /// cancellation (no error recorded) from pipeline failure.
    pub(crate) fn check(&self) -> CoreResult<()> {
        if self.is_stopped() {
            if let Some(err) = self.take_error() {
                return Err(err);
            }
            if self.has_error() {
                return Err(CoreError::Encode(
                    "Encoder pipeline stopped after failure".into(),
                ));
            }
            return Err(CoreError::Cancelled);
        }
        Ok(())
    }
}

const WRITER_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
const FFMPEG_FINALIZE_TIMEOUT: Duration = Duration::from_secs(600);
const FFMPEG_TERMINATE_TIMEOUT: Duration = Duration::from_secs(2);
const SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Clone, Copy)]
pub(crate) enum PipelineKind {
    Transparent,
    Composite,
}

impl std::fmt::Display for PipelineKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Transparent => "transparent",
            Self::Composite => "composite",
        })
    }
}

/// Removes an incomplete encoder output unless explicitly preserved.
pub(crate) struct PartialOutputGuard {
    path: PathBuf,
    preserve: bool,
}

impl PartialOutputGuard {
    pub(crate) fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            preserve: false,
        }
    }

    pub(crate) fn preserve(&mut self) {
        self.preserve = true;
    }
}

impl Drop for PartialOutputGuard {
    fn drop(&mut self) {
        if self.preserve {
            return;
        }
        if let Err(error) = std::fs::remove_file(&self.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                log::warn!(
                    "Could not remove incomplete encoder output {}: {error}",
                    self.path.display()
                );
            }
        }
    }
}

/// Pipeline-owned process and threads. Early returns and unwinding stop this
/// item, reap FFmpeg, and join both threads before partial-output cleanup runs.
pub(crate) struct PipelineProcesses {
    pub(crate) child: Child,
    pub(crate) writer: Option<JoinHandle<CoreResult<WriterResult>>>,
    pub(crate) monitor: Option<JoinHandle<()>>,
    shutdown: Arc<PipelineShutdown>,
    pipeline: PipelineKind,
}

impl PipelineProcesses {
    pub(crate) fn new(
        child: Child,
        pipeline: PipelineKind,
        shutdown: Arc<PipelineShutdown>,
    ) -> Self {
        Self {
            child,
            writer: None,
            monitor: None,
            shutdown,
            pipeline,
        }
    }
}

impl Drop for PipelineProcesses {
    fn drop(&mut self) {
        if self.writer.is_some() || self.monitor.is_some() {
            self.shutdown.signal_failure(CoreError::Encode(
                "Encoder pipeline ended before teardown".into(),
            ));
        }
        let pipeline = self.pipeline;
        if let Err(error) = terminate_ffmpeg(&mut self.child, pipeline) {
            log::warn!("Could not stop {pipeline} ffmpeg during pipeline cleanup: {error}");
        }
        if let Some(writer) = self.writer.take() {
            if let Err(error) = join_shutdown_thread(writer, "Encoder writer thread") {
                log::warn!("Could not join {pipeline} writer during cleanup: {error}");
            }
        }
        if let Some(monitor) = self.monitor.take() {
            if let Err(error) = join_shutdown_thread(monitor, "FFmpeg monitor thread") {
                log::warn!("Could not join {pipeline} monitor during cleanup: {error}");
            }
        }
    }
}

/// Mode-specific diagnostic formatting for the shared FFmpeg lifecycle.
pub(crate) trait PipelineFailurePolicy {
    fn writer_failure(&self, error: CoreError, status: Option<ExitStatus>) -> CoreError;
    fn ffmpeg_failure(&self, status: ExitStatus) -> CoreError;
}

/// Successful producer, writer, and FFmpeg results after canonical teardown.
pub(crate) struct PipelineOutcome<T> {
    pub(crate) producer: T,
    pub(crate) writer: WriterResult,
}

/// Drains the writer and resolves cancellation, producer, writer, monitor, and
/// FFmpeg results in one canonical order for every encode mode.
pub(crate) fn finalize_pipeline<T, P: PipelineFailurePolicy>(
    processes: &mut PipelineProcesses,
    producer_result: CoreResult<T>,
    shutdown: &PipelineShutdown,
    pipeline: PipelineKind,
    failure_policy: &P,
) -> CoreResult<PipelineOutcome<T>> {
    let child = &mut processes.child;
    let (writer_thread_name, monitor_thread_name) = match pipeline {
        PipelineKind::Transparent => ("Encoder writer thread", "FFmpeg monitor thread"),
        PipelineKind::Composite => (
            "Composite encoder writer thread",
            "Composite ffmpeg monitor thread",
        ),
    };
    let mut was_cancelled = shutdown.is_cancelled();
    let producer_failed = producer_result.is_err();
    let writer_failed_before_teardown = shutdown.has_error();
    let mut shutdown_error = None;
    let mut status = None;

    if was_cancelled || producer_failed || writer_failed_before_teardown {
        match terminate_ffmpeg(child, pipeline) {
            Ok(exit_status) => status = Some(exit_status),
            Err(error) => shutdown_error = Some(error),
        }
    } else {
        match unblock_stalled_writer(
            processes
                .writer
                .as_ref()
                .expect("pipeline writer must be started"),
            child,
            pipeline,
            shutdown,
        ) {
            Ok(cancelled) => was_cancelled |= cancelled,
            Err(error) => shutdown_error = Some(error),
        }
    }

    let writer_result = join_shutdown_thread(
        processes
            .writer
            .take()
            .expect("pipeline writer must be joined once"),
        writer_thread_name,
    );
    if status.is_none()
        && shutdown_error.is_none()
        && !was_cancelled
        && !producer_failed
        && !writer_failed_before_teardown
    {
        match wait_for_ffmpeg(child, pipeline, shutdown) {
            Ok((exit_status, cancelled)) => {
                status = Some(exit_status);
                was_cancelled |= cancelled;
            }
            Err(error) => shutdown_error = Some(error),
        }
    }
    let monitor_result = join_shutdown_thread(
        processes
            .monitor
            .take()
            .expect("pipeline monitor must be joined once"),
        monitor_thread_name,
    );

    if was_cancelled {
        return Err(CoreError::Cancelled);
    }
    let writer_result = match writer_result {
        Ok(result) => result,
        Err(error) => {
            if !writer_failed_before_teardown {
                if let Err(producer_error) = producer_result {
                    return Err(producer_error);
                }
            }
            return Err(error);
        }
    };
    if writer_failed_before_teardown {
        let error = match writer_result {
            Err(error) => error,
            Ok(_) => {
                producer_result?;
                return Err(shutdown
                    .take_error()
                    .expect("failed pipeline must report its error"));
            }
        };
        return Err(failure_policy.writer_failure(error, status));
    }
    let producer = producer_result?;
    if let Some(error) = shutdown_error {
        return Err(error);
    }
    monitor_result?;
    let status = status.ok_or_else(|| {
        CoreError::Encode(format!("{pipeline} ffmpeg did not report an exit status"))
    })?;
    let writer =
        writer_result.map_err(|error| failure_policy.writer_failure(error, Some(status)))?;
    if !status.success() {
        return Err(failure_policy.ffmpeg_failure(status));
    }

    Ok(PipelineOutcome { producer, writer })
}

/// Gives a writer a bounded opportunity to close FFmpeg stdin.
///
/// A stalled pipe write is unblocked by terminating FFmpeg. The caller still
/// owns and joins the writer handle after this function returns.
pub(crate) fn unblock_stalled_writer<T>(
    writer: &JoinHandle<T>,
    child: &mut Child,
    pipeline: PipelineKind,
    shutdown: &PipelineShutdown,
) -> CoreResult<bool> {
    let deadline = Instant::now() + WRITER_DRAIN_TIMEOUT;
    while !writer.is_finished() && Instant::now() < deadline {
        if shutdown.is_stopped() {
            let _ = terminate_ffmpeg(child, pipeline)?;
            if wait_for_thread(writer, FFMPEG_TERMINATE_TIMEOUT) {
                return Ok(shutdown.is_cancelled());
            }
            return Err(CoreError::Encode(format!(
                "{pipeline} encoder writer did not stop after cancellation"
            )));
        }
        thread::sleep(SHUTDOWN_POLL_INTERVAL);
    }
    if writer.is_finished() {
        return Ok(false);
    }

    let _ = terminate_ffmpeg(child, pipeline)?;
    if wait_for_thread(writer, FFMPEG_TERMINATE_TIMEOUT) {
        return Err(CoreError::Encode(format!(
            "{pipeline} ffmpeg did not drain stdin within {} seconds and was terminated",
            WRITER_DRAIN_TIMEOUT.as_secs()
        )));
    }

    Err(CoreError::Encode(format!(
        "{pipeline} encoder writer did not stop after ffmpeg termination"
    )))
}

/// Waits a bounded time for FFmpeg finalization while observing cancellation.
pub(crate) fn wait_for_ffmpeg(
    child: &mut Child,
    pipeline: PipelineKind,
    shutdown: &PipelineShutdown,
) -> CoreResult<(ExitStatus, bool)> {
    let deadline = Instant::now() + FFMPEG_FINALIZE_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait().map_err(|error| {
            CoreError::Encode(format!("{pipeline} ffmpeg process error: {error}"))
        })? {
            return Ok((status, shutdown.is_cancelled()));
        }
        if shutdown.is_stopped() {
            return terminate_ffmpeg(child, pipeline)
                .map(|status| (status, shutdown.is_cancelled()));
        }
        if Instant::now() >= deadline {
            break;
        }
        thread::sleep(SHUTDOWN_POLL_INTERVAL);
    }

    let _ = terminate_ffmpeg(child, pipeline)?;
    Err(CoreError::Encode(format!(
        "{pipeline} ffmpeg did not finalize within {} seconds and was terminated",
        FFMPEG_FINALIZE_TIMEOUT.as_secs()
    )))
}

/// Immediately terminates FFmpeg and waits a bounded time for it to exit.
pub(crate) fn terminate_ffmpeg(
    child: &mut Child,
    pipeline: PipelineKind,
) -> CoreResult<ExitStatus> {
    if let Some(status) = child
        .try_wait()
        .map_err(|error| CoreError::Encode(format!("{pipeline} ffmpeg process error: {error}")))?
    {
        return Ok(status);
    }
    child.kill().map_err(|error| {
        CoreError::Encode(format!("Failed to terminate {pipeline} ffmpeg: {error}"))
    })?;
    if let Some(status) = poll_child_exit(child, FFMPEG_TERMINATE_TIMEOUT, pipeline)? {
        return Ok(status);
    }
    // Retain native ownership if termination is slow. A timeout is diagnostic,
    // never permission to detach a live process and release the renderer.
    log::warn!("{pipeline} ffmpeg is still stopping after forced termination");
    child
        .wait()
        .map_err(|error| CoreError::Encode(format!("Could not reap {pipeline} ffmpeg: {error}")))
}

/// Joins an encoder-owned thread before releasing native execution ownership.
pub(crate) fn join_shutdown_thread<T>(handle: JoinHandle<T>, thread_name: &str) -> CoreResult<T> {
    if !wait_for_thread(&handle, FFMPEG_TERMINATE_TIMEOUT) {
        log::warn!("{thread_name} is still stopping after encoder shutdown");
    }
    handle
        .join()
        .map_err(|_| CoreError::Encode(format!("{thread_name} panicked")))
}

fn wait_for_thread<T>(handle: &JoinHandle<T>, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while !handle.is_finished() && Instant::now() < deadline {
        thread::sleep(SHUTDOWN_POLL_INTERVAL);
    }
    handle.is_finished()
}

fn poll_child_exit(
    child: &mut Child,
    timeout: Duration,
    pipeline: PipelineKind,
) -> CoreResult<Option<ExitStatus>> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().map_err(|error| {
            CoreError::Encode(format!("{pipeline} ffmpeg process error: {error}"))
        })? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        thread::sleep(SHUTDOWN_POLL_INTERVAL);
    }
}
