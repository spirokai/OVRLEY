//! Shared FFmpeg process lifecycle and pipeline teardown.

use std::path::{Path, PathBuf};
use std::process::{Child, ExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::error::{CoreError, CoreResult};

use super::buffers::FrameBuffer;
use super::buffers::FrameBufferPool;
use super::diagnostics::EncoderMonitor;
use super::FramePipelinePlan;
use crate::debug::{RenderProfiler, TimingBucket};
use crate::encode::ffmpeg::binary::spawn_ffmpeg;
use std::collections::BTreeMap;
use std::io::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::Receiver;

/// Single source of truth for whether the pipeline should stop and why.
///
/// Observes session cancellation without writing it. A pipeline failure stops
/// this item's workers and preserves its first error; it must not request
/// cancellation of the entire reserved operation.
pub(crate) struct PipelineShutdown {
    session_cancel: Arc<AtomicBool>,
    failed: AtomicBool,
    failure: Mutex<Option<PipelineFailure>>,
}

struct PipelineFailure {
    error: CoreError,
    writer: bool,
}

impl PipelineShutdown {
    pub(crate) fn shared(session_cancel: Arc<AtomicBool>) -> Arc<Self> {
        Arc::new(Self {
            session_cancel,
            failed: AtomicBool::new(false),
            failure: Mutex::new(None),
        })
    }

    pub(crate) fn is_stopped(&self) -> bool {
        self.is_cancelled() || self.failed.load(Ordering::Acquire)
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.session_cancel.load(Ordering::Relaxed)
    }

    pub(crate) fn signal_failure(&self, error: CoreError) {
        self.record_failure(error, false);
    }

    pub(crate) fn signal_writer_failure(&self, error: CoreError) {
        self.record_failure(error, true);
    }

    fn record_failure(&self, error: CoreError, writer: bool) {
        // Errors caused by cancellation are cleanup diagnostics, not job failures.
        if self.is_cancelled() || matches!(error, CoreError::Cancelled) {
            return;
        }
        self.failure
            .lock()
            .expect("pipeline failure mutex poisoned")
            .get_or_insert(PipelineFailure { error, writer });
        self.failed.store(true, Ordering::Release);
    }

    /// Observers never consume the failure. Only finalization takes it after joining.
    pub(crate) fn check(&self) -> CoreResult<()> {
        if self.is_stopped() {
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
    writer: Option<JoinHandle<Option<WriterResult>>>,
    monitor: Option<JoinHandle<()>>,
    shutdown: Arc<PipelineShutdown>,
    pipeline: PipelineKind,
    output: PartialOutputGuard,
}

impl PipelineProcesses {
    pub(crate) fn start(
        binary: &Path,
        plan: &FramePipelinePlan,
        output: &Path,
        frames: Receiver<FrameBuffer>,
        buffers: Arc<FrameBufferPool>,
        shutdown: Arc<PipelineShutdown>,
        monitor: Arc<EncoderMonitor>,
    ) -> CoreResult<Self> {
        let child = spawn_ffmpeg(binary, &plan.ffmpeg_args)?;
        let mut processes = Self {
            child,
            writer: None,
            monitor: None,
            shutdown,
            pipeline: plan.kind,
            output: PartialOutputGuard::new(output),
        };
        let stdin = processes
            .child
            .stdin
            .take()
            .expect("FFmpeg was spawned with piped stdin");
        let stderr = processes
            .child
            .stderr
            .take()
            .expect("FFmpeg was spawned with piped stderr");
        processes.monitor = Some(
            thread::Builder::new()
                .name("encoder-monitor".into())
                .spawn(move || monitor.read(stderr))
                .map_err(|error| {
                    CoreError::Encode(format!("Could not start FFmpeg monitor: {error}"))
                })?,
        );
        let shutdown = Arc::clone(&processes.shutdown);
        let kind = plan.kind;
        processes.writer = Some(
            thread::Builder::new()
                .name("encoder-writer".into())
                .spawn(move || writer_worker(stdin, frames, buffers, shutdown, kind))
                .map_err(|error| {
                    CoreError::Encode(format!("Could not start encoder writer: {error}"))
                })?,
        );
        Ok(processes)
    }

    pub(crate) fn preserve_output(&mut self) {
        self.output.preserve();
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

/// Successful producer and writer results after canonical teardown.
pub(crate) struct PipelineOutcome<T> {
    pub(crate) producer: T,
    pub(crate) writer: WriterResult,
}

impl PipelineProcesses {
    /// Reaps the process before joining its stderr reader on every exit path.
    pub(crate) fn finish<T>(
        &mut self,
        producer: CoreResult<T>,
        encoding: &super::VideoEncoding,
        monitor: &super::diagnostics::EncoderMonitor,
    ) -> CoreResult<PipelineOutcome<T>> {
        let producer = match producer {
            Ok(value) => Some(value),
            Err(error) => {
                self.shutdown.signal_failure(error);
                None
            }
        };
        let drain = unblock_stalled_writer(
            self.writer.as_ref().expect("pipeline writer started"),
            &mut self.child,
            self.pipeline,
            &self.shutdown,
        );
        if let Err(error) = drain {
            self.shutdown.signal_failure(error);
        }
        let writer = match join_shutdown_thread(
            self.writer.take().expect("writer joined once"),
            "Encoder writer thread",
        ) {
            Ok(result) => result,
            Err(error) => {
                self.shutdown.signal_writer_failure(error);
                None
            }
        };
        let status = match wait_for_ffmpeg(&mut self.child, self.pipeline, &self.shutdown) {
            Ok(status) => status,
            Err(error) => {
                self.shutdown.signal_failure(error);
                // A polling failure must not leave the stderr reader waiting
                // on a live child. Retain ownership through forced teardown.
                match terminate_ffmpeg(&mut self.child, self.pipeline) {
                    Ok(status) => status,
                    Err(error) => return Err(error),
                }
            }
        };
        if let Err(error) = join_shutdown_thread(
            self.monitor.take().expect("monitor joined once"),
            "FFmpeg monitor thread",
        ) {
            self.shutdown.signal_failure(error);
        }
        let failure = self
            .shutdown
            .failure
            .lock()
            .expect("pipeline failure mutex poisoned")
            .take();
        if let Some(failure) = failure {
            return Err(if failure.writer {
                encoding.writer_failure(failure.error, status, &monitor.stderr())
            } else {
                failure.error
            });
        }
        if self.shutdown.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        if !status.success() {
            return Err(CoreError::Ffmpeg {
                status,
                stderr: super::diagnostics::stderr_tail(&monitor.stderr()),
            });
        }
        Ok(PipelineOutcome {
            producer: producer.expect("successful producer after failure resolution"),
            writer: writer.expect("successful writer after failure resolution"),
        })
    }
}

/// Gives a writer a bounded opportunity to close FFmpeg stdin.
///
/// A stalled pipe write is unblocked by terminating FFmpeg. The caller still
/// owns and joins the writer handle after this function returns.
fn unblock_stalled_writer<T>(
    writer: &JoinHandle<T>,
    child: &mut Child,
    pipeline: PipelineKind,
    shutdown: &PipelineShutdown,
) -> CoreResult<()> {
    let deadline = Instant::now() + WRITER_DRAIN_TIMEOUT;
    while !writer.is_finished() && Instant::now() < deadline {
        if shutdown.is_stopped() {
            let _ = terminate_ffmpeg(child, pipeline)?;
            if wait_for_thread(writer, FFMPEG_TERMINATE_TIMEOUT) {
                return Ok(());
            }
            return Err(CoreError::Encode(format!(
                "{pipeline} encoder writer did not stop after cancellation"
            )));
        }
        thread::sleep(SHUTDOWN_POLL_INTERVAL);
    }
    if writer.is_finished() {
        return Ok(());
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
fn wait_for_ffmpeg(
    child: &mut Child,
    pipeline: PipelineKind,
    shutdown: &PipelineShutdown,
) -> CoreResult<ExitStatus> {
    let deadline = Instant::now() + FFMPEG_FINALIZE_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait().map_err(|error| {
            CoreError::Encode(format!("{pipeline} ffmpeg process error: {error}"))
        })? {
            return Ok(status);
        }
        if shutdown.is_stopped() {
            return terminate_ffmpeg(child, pipeline);
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
fn terminate_ffmpeg(child: &mut Child, pipeline: PipelineKind) -> CoreResult<ExitStatus> {
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
fn join_shutdown_thread<T>(handle: JoinHandle<T>, thread_name: &str) -> CoreResult<T> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writer_io_errors_and_panics_stop_the_pipeline_with_the_original_reason() {
        struct FailedPipe(bool);
        impl std::io::Write for FailedPipe {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                assert!(!self.0, "injected writer panic");
                Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "injected pipe failure",
                ))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        for panic in [false, true] {
            let channels = super::super::buffers::ParallelFramePoolPlan::for_resources(
                crate::render::FrameSize {
                    width: 2,
                    height: 2,
                },
                std::num::NonZeroUsize::new(1).unwrap(),
                2,
            )
            .unwrap()
            .create_channels();
            channels
                .frame_sender
                .send(FrameBuffer {
                    pixels: vec![0; 16],
                })
                .unwrap();
            let shutdown = PipelineShutdown::shared(Arc::new(AtomicBool::new(false)));
            assert!(writer_worker(
                FailedPipe(panic),
                channels.frame_receiver,
                channels.buffers,
                Arc::clone(&shutdown),
                PipelineKind::Transparent,
            )
            .is_none());
            assert!(shutdown.is_stopped());
            let failure = shutdown.failure.lock().unwrap().take().unwrap();
            assert!(failure.writer);
            assert!(failure.error.to_string().contains(if panic {
                "writer thread panicked"
            } else {
                "injected pipe failure"
            }));
        }
    }

    #[test]
    fn polling_and_later_failures_preserve_the_first_failure() {
        let cancel = Arc::new(AtomicBool::new(false));
        let shutdown = PipelineShutdown::shared(Arc::clone(&cancel));
        shutdown.signal_failure(CoreError::Render("original frame failure".into()));
        for _ in 0..10 {
            assert!(shutdown.check().is_err());
        }
        shutdown.signal_writer_failure(CoreError::Encode("secondary pipe failure".into()));
        cancel.store(true, Ordering::Relaxed);
        let failure = shutdown.failure.lock().unwrap().take().unwrap();
        assert!(!failure.writer);
        assert!(failure.error.to_string().contains("original frame failure"));
    }

    #[test]
    fn cancellation_does_not_become_a_pipe_failure() {
        let shutdown = PipelineShutdown::shared(Arc::new(AtomicBool::new(true)));
        shutdown.signal_writer_failure(CoreError::Encode("pipe closed during cancellation".into()));
        assert!(shutdown.check().is_err());
        assert!(shutdown.failure.lock().unwrap().is_none());
    }
}

/// Result returned by the shared ffmpeg stdin writer thread.
pub(crate) struct WriterResult {
    /// Number of complete frames written into ffmpeg stdin.
    pub(crate) written_frames: u64,
    /// Writer-side timing buckets collected while draining the queue.
    pub(crate) timings: BTreeMap<String, TimingBucket>,
}

/// Writes queued frame buffers into ffmpeg stdin and returns buffers to the pool.
pub(crate) fn writer_worker(
    stdin: impl Write,
    receiver: Receiver<FrameBuffer>,
    buffers: Arc<FrameBufferPool>,
    shutdown: Arc<PipelineShutdown>,
    mode: PipelineKind,
) -> Option<WriterResult> {
    let result = catch_unwind(AssertUnwindSafe(|| {
        writer_worker_inner(stdin, &receiver, &buffers, &shutdown, mode)
    }))
    .unwrap_or_else(|_| Err(CoreError::Encode("Encoder writer thread panicked".into())));
    match result {
        Ok(result) => Some(result),
        Err(error) => {
            shutdown.signal_writer_failure(error);
            None
        }
    }
}

fn writer_worker_inner(
    mut stdin: impl Write,
    receiver: &Receiver<FrameBuffer>,
    buffers: &FrameBufferPool,
    shutdown: &PipelineShutdown,
    mode: PipelineKind,
) -> CoreResult<WriterResult> {
    let mut profiler = RenderProfiler::default();
    let mut written_frames = 0u64;
    loop {
        if shutdown.is_stopped() {
            break;
        }
        let queue_started = Instant::now();
        let frame = match receiver.recv_timeout(Duration::from_millis(25)) {
            Ok(frame) => {
                profiler.record_ms(
                    "writer.rendered_frame_wait",
                    queue_started.elapsed().as_secs_f64() * 1000.0,
                );
                frame
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                profiler.record_ms(
                    "writer.rendered_frame_wait",
                    queue_started.elapsed().as_secs_f64() * 1000.0,
                );
                break;
            }
        };
        if shutdown.is_stopped() {
            break;
        }
        let write_started = Instant::now();
        stdin
            .write_all(frame.pixels.as_slice())
            .map_err(|error| match mode {
                PipelineKind::Transparent => {
                    CoreError::Encode(format!("Failed writing frame to ffmpeg: {error}"))
                }
                PipelineKind::Composite => {
                    CoreError::Encode(format!("Failed writing composite overlay frame: {error}"))
                }
            })?;
        profiler.record_ms(
            "ffmpeg.write",
            write_started.elapsed().as_secs_f64() * 1000.0,
        );
        written_frames += 1;

        let release_started = Instant::now();
        buffers.release(frame);
        profiler.record_ms(
            "buffer.release_wait",
            release_started.elapsed().as_secs_f64() * 1000.0,
        );
    }

    stdin
        .flush()
        .map_err(|error| CoreError::Encode(error.to_string()))?;

    Ok(WriterResult {
        written_frames,
        timings: profiler.summary(),
    })
}
