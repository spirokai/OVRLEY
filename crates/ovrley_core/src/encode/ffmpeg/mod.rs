//! FFmpeg at the process boundary: binary discovery, platform process configuration, spawning,
//! forced termination and a cancellable one-shot runner.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use crate::error::{CoreError, CoreResult};
use std::io::Read;
use std::process::{Child, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub mod assemble;
pub mod catalog;
pub mod composite;
pub mod detect;
pub mod profiles;
pub mod transitions;
pub mod transparent;

/// Resolves the ffmpeg executable used for previews and encoding.
///
/// Search order is: explicit environment override, the bundled vendor path,
/// then `PATH`. Returning a concrete path lets health checks and
/// render failures show actionable messages.
pub fn resolve_ffmpeg_binary(repo_root: &Path) -> CoreResult<PathBuf> {
    let mut candidate_paths = Vec::new();

    if let Some(env_override) = env::var_os("OVRLEY_FFMPEG") {
        candidate_paths.push(PathBuf::from(env_override));
    }

    let local_name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    candidate_paths.push(
        repo_root
            .join("vendor")
            .join("ffmpeg")
            .join("bin")
            .join(local_name),
    );
    for candidate in candidate_paths {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    if let Some(path) = find_in_path(local_name) {
        return Ok(path);
    }

    Err(CoreError::FfmpegNotFound(
        "ffmpeg executable not found. Run pnpm install, install ffmpeg on PATH, or set OVRLEY_FFMPEG."
            .to_string(),
    ))
}

/// Applies platform-specific process configuration for FFmpeg tools.
pub fn configure_ffmpeg_command(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    #[cfg(not(windows))]
    let _ = command;
}

// Searches the process PATH for a binary with the requested platform filename.
fn find_in_path(binary_name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    env::split_paths(&path_var)
        .map(|entry| entry.join(binary_name))
        .find(|candidate| candidate.is_file())
}

pub(crate) const TERMINATE_TIMEOUT: Duration = Duration::from_secs(2);
pub(crate) const POLL_INTERVAL: Duration = Duration::from_millis(25);

fn tool_name(binary: &Path) -> String {
    binary
        .file_stem()
        .expect("resolved tool binaries have file names")
        .to_string_lossy()
        .into_owned()
}

/// Spawns one tool with piped stderr: its diagnostics always have a reader.
pub(crate) fn spawn(
    binary: &Path,
    args: &[String],
    stdin: Stdio,
    stdout: Stdio,
) -> CoreResult<Child> {
    let mut command = Command::new(binary);
    configure_ffmpeg_command(&mut command);
    command
        .args(args)
        .stdin(stdin)
        .stdout(stdout)
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            CoreError::Encode(format!("Could not start {}: {error}", tool_name(binary)))
        })
}

/// Immediately terminates a tool and reaps it. `label` names it in diagnostics.
pub(crate) fn terminate(child: &mut Child, label: &str) -> CoreResult<ExitStatus> {
    if let Some(status) = child
        .try_wait()
        .map_err(|error| CoreError::Encode(format!("{label} process error: {error}")))?
    {
        return Ok(status);
    }
    child
        .kill()
        .map_err(|error| CoreError::Encode(format!("Failed to terminate {label}: {error}")))?;
    let deadline = Instant::now() + TERMINATE_TIMEOUT;
    while Instant::now() < deadline {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| CoreError::Encode(format!("{label} process error: {error}")))?
        {
            return Ok(status);
        }
        thread::sleep(POLL_INTERVAL);
    }
    // Retain native ownership if termination is slow. A timeout is diagnostic,
    // never permission to detach a live process and release the renderer.
    log::warn!("{label} is still stopping after forced termination");
    child
        .wait()
        .map_err(|error| CoreError::Encode(format!("Could not reap {label}: {error}")))
}

/// What a finished one-shot tool left behind. A failed status is the caller's to
/// describe; `stdout` is empty unless captured.
pub(crate) struct Output {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: String,
}

/// Runs one tool to completion, draining its pipes so a chatty tool cannot block.
/// A set `cancel` terminates and reaps it, and returns [`CoreError::Cancelled`].
pub(crate) fn run(
    binary: &Path,
    args: &[String],
    capture_stdout: bool,
    cancel: &AtomicBool,
) -> CoreResult<Output> {
    if cancel.load(Ordering::SeqCst) {
        return Err(CoreError::Cancelled);
    }
    let stdout = if capture_stdout {
        Stdio::piped()
    } else {
        Stdio::null()
    };
    let mut child = spawn(binary, args, Stdio::null(), stdout)?;
    let stderr = read_to_end(child.stderr.take().expect("spawn pipes stderr"));
    let stdout = child.stdout.take().map(read_to_end);
    let name = tool_name(binary);
    // The readers end with the process, so an early return may leave them running.
    let status = wait(&mut child, &name, cancel)?;
    Ok(Output {
        status,
        stdout: stdout.map_or(Ok(Vec::new()), join_reader)?,
        stderr: String::from_utf8_lossy(&join_reader(stderr)?).into_owned(),
    })
}

fn wait(child: &mut Child, name: &str, cancel: &AtomicBool) -> CoreResult<ExitStatus> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Err(error) => {
                terminate(child, name)?;
                return Err(CoreError::Encode(format!("{name} process error: {error}")));
            }
            Ok(None) if cancel.load(Ordering::SeqCst) => {
                terminate(child, name)?;
                return Err(CoreError::Cancelled);
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
        }
    }
}

fn read_to_end(mut pipe: impl Read + Send + 'static) -> JoinHandle<std::io::Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes).map(|_| bytes)
    })
}

fn join_reader(reader: JoinHandle<std::io::Result<Vec<u8>>>) -> CoreResult<Vec<u8>> {
    reader
        .join()
        .expect("tool pipe reader panicked")
        .map_err(|error| CoreError::Encode(format!("Could not read tool output: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::ffmpeg::resolve_ffmpeg_binary;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn ffmpeg() -> PathBuf {
        resolve_ffmpeg_binary(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap()
    }

    #[test]
    fn cancellation_kills_a_running_tool() {
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let canceller = thread::spawn(move || {
            thread::sleep(Duration::from_millis(200));
            flag.store(true, Ordering::SeqCst);
        });
        let started = Instant::now();
        let args = [
            "-nostdin",
            "-f",
            "lavfi",
            "-re",
            "-i",
            "testsrc2=duration=600",
            "-f",
            "null",
            "-",
        ]
        .map(String::from);
        assert!(matches!(
            run(&ffmpeg(), &args, false, &cancel),
            Err(CoreError::Cancelled)
        ));
        assert!(started.elapsed() < Duration::from_secs(10));
        canceller.join().unwrap();
    }

    #[test]
    fn a_set_cancel_flag_never_starts_the_tool() {
        let missing = Path::new("/nonexistent/ffmpeg");
        assert!(matches!(
            run(missing, &[], false, &AtomicBool::new(true)),
            Err(CoreError::Cancelled)
        ));
    }

    #[test]
    fn failed_tools_report_their_status_and_stderr() {
        let args = ["-nostdin", "-i", "/nonexistent/part.ts", "-f", "null", "-"].map(String::from);
        let output = run(&ffmpeg(), &args, false, &AtomicBool::new(false)).unwrap();
        assert!(!output.status.success());
        assert!(
            output.stderr.contains("/nonexistent/part.ts"),
            "{}",
            output.stderr
        );
    }

    #[test]
    fn stdout_is_returned_only_when_captured() {
        let args = ["-hide_banner", "-version"].map(String::from);
        let cancel = AtomicBool::new(false);
        let captured = run(&ffmpeg(), &args, true, &cancel).unwrap();
        assert!(String::from_utf8_lossy(&captured.stdout).contains("ffmpeg version"));
        assert!(run(&ffmpeg(), &args, false, &cancel)
            .unwrap()
            .stdout
            .is_empty());
    }

    #[test]
    fn a_tool_that_cannot_start_is_named() {
        let error = run(
            Path::new("/nonexistent/ffprobe"),
            &[],
            false,
            &AtomicBool::new(false),
        )
        .err()
        .unwrap();
        assert!(
            error.to_string().contains("Could not start ffprobe"),
            "{error}"
        );
    }
}
