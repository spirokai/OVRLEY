//! Joins the rendered parts of one output with the commands built by
//! [`crate::encode::ffmpeg::assemble`]. Every FFmpeg step stops on cancellation;
//! the target is removed unless the joined output is verified. The caller owns
//! the parts directory and its retention.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use serde::Deserialize;

use super::diagnostics::stderr_tail;
use super::process::PartialOutputGuard;
use crate::encode::ffmpeg::assemble::{
    concat_manifest, join_args, probe_output_args, remux_part_args, AssemblyKind, OutputAssembly,
};
use crate::encode::ffmpeg::binary::{configure_ffmpeg_command, resolve_ffmpeg_binary};
use crate::encode::progress::RenderController;
use crate::error::{CoreError, CoreResult};
use crate::paths::AppPaths;

const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Remuxes composite parts, joins all parts into the target and verifies its
/// frame count and audio track.
pub(crate) fn assemble_output(
    paths: &AppPaths,
    assembly: &OutputAssembly,
    controller: &RenderController,
) -> CoreResult<()> {
    let ffmpeg = resolve_ffmpeg_binary(&paths.repo_root)?;
    if let AssemblyKind::Composite { hevc, .. } = assembly.kind {
        for (part, remux) in assembly.parts.iter().zip(assembly.joined_parts()) {
            run_ffmpeg(&ffmpeg, &remux_part_args(part, &remux, hevc), controller)?;
        }
    }
    let manifest = assembly.manifest_path();
    std::fs::write(&manifest, concat_manifest(&assembly.joined_parts())).map_err(|source| {
        CoreError::Io {
            path: manifest.clone(),
            source,
        }
    })?;
    let mut output = PartialOutputGuard::new(&assembly.target);
    run_ffmpeg(&ffmpeg, &join_args(assembly), controller)?;
    verify_joined_output(&ffmpeg, assembly)?;
    output.preserve();
    Ok(())
}

/// Runs one FFmpeg command to completion; kills and reaps it on cancellation.
fn run_ffmpeg(binary: &Path, args: &[String], controller: &RenderController) -> CoreResult<()> {
    controller.check_cancelled()?;
    let mut command = Command::new(binary);
    configure_ffmpeg_command(&mut command);
    let mut child = command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| CoreError::Encode(format!("Could not start ffmpeg: {error}")))?;
    let mut stderr = child.stderr.take().expect("piped ffmpeg stderr");
    let reader = thread::spawn(move || {
        let mut text = String::new();
        stderr.read_to_string(&mut text).map(|_| text)
    });
    let process_error =
        |error: std::io::Error| CoreError::Encode(format!("ffmpeg process error: {error}"));
    let status = loop {
        if let Some(status) = child.try_wait().map_err(process_error)? {
            break status;
        }
        if controller.check_cancelled().is_err() {
            if let Err(error) = child.kill() {
                log::warn!("Could not terminate ffmpeg while joining parts: {error}");
            }
            child.wait().map_err(process_error)?;
            let _ = reader.join().expect("ffmpeg stderr reader panicked");
            return Err(CoreError::Cancelled);
        }
        thread::sleep(CANCEL_POLL_INTERVAL);
    };
    let stderr = reader
        .join()
        .expect("ffmpeg stderr reader panicked")
        .map_err(process_error)?;
    if !status.success() {
        return Err(CoreError::Encode(format!(
            "ffmpeg failed while joining output parts ({status}). FFmpeg stderr:\n{}",
            stderr_tail(&stderr)
        )));
    }
    Ok(())
}

#[derive(Deserialize)]
struct ProbedOutput {
    streams: Vec<ProbedStream>,
}

#[derive(Deserialize)]
struct ProbedStream {
    codec_type: String,
    nb_read_packets: Option<String>,
}

/// The joined output must hold exactly the planned video frames, and an audio
/// track exactly when the assembly encodes one.
fn verify_joined_output(ffmpeg: &Path, assembly: &OutputAssembly) -> CoreResult<()> {
    let ffprobe = ffmpeg.with_file_name(if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    });
    let mut command = Command::new(&ffprobe);
    configure_ffmpeg_command(&mut command);
    let output = command
        .args(probe_output_args(&assembly.target))
        .stdin(Stdio::null())
        .output()
        .map_err(|error| CoreError::Encode(format!("Could not start ffprobe: {error}")))?;
    if !output.status.success() {
        return Err(CoreError::Encode(format!(
            "ffprobe could not read the joined output {}: {}",
            assembly.target.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let probed: ProbedOutput = serde_json::from_slice(&output.stdout).map_err(|error| {
        CoreError::Encode(format!("ffprobe returned unreadable stream data: {error}"))
    })?;
    let video_frames = probed
        .streams
        .iter()
        .filter(|stream| stream.codec_type == "video")
        .map(|stream| stream.nb_read_packets.as_deref().map(str::parse::<u64>))
        .collect::<Vec<_>>();
    let frames = match video_frames.as_slice() {
        [Some(Ok(frames))] => *frames,
        _ => {
            return Err(CoreError::Encode(format!(
                "Joined output {} must contain one counted video stream",
                assembly.target.display()
            )))
        }
    };
    if frames != assembly.frames {
        return Err(CoreError::Encode(format!(
            "Joined output {} has {frames} video frames instead of {}",
            assembly.target.display(),
            assembly.frames
        )));
    }
    let expects_audio = matches!(
        assembly.kind,
        AssemblyKind::Composite { audio: Some(_), .. }
    );
    let audio_streams = probed
        .streams
        .iter()
        .filter(|stream| stream.codec_type == "audio")
        .count();
    if audio_streams != usize::from(expects_audio) {
        return Err(CoreError::Encode(format!(
            "Joined output {} has {audio_streams} audio streams instead of {}",
            assembly.target.display(),
            usize::from(expects_audio)
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    use std::time::Instant;

    #[test]
    fn cancellation_kills_a_running_ffmpeg_step() {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let ffmpeg = resolve_ffmpeg_binary(&repo_root).unwrap();
        let controller = RenderController::default();
        let flag = controller.cancel_flag();
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
            run_ffmpeg(&ffmpeg, &args, &controller),
            Err(CoreError::Cancelled)
        ));
        assert!(started.elapsed() < Duration::from_secs(10));
        canceller.join().unwrap();
    }

    #[test]
    fn failed_steps_report_ffmpeg_stderr() {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let ffmpeg = resolve_ffmpeg_binary(&repo_root).unwrap();
        let args = ["-nostdin", "-i", "/nonexistent/part.ts", "-f", "null", "-"].map(String::from);
        let error = run_ffmpeg(&ffmpeg, &args, &RenderController::default()).unwrap_err();
        assert!(
            error.to_string().contains("/nonexistent/part.ts"),
            "{error}"
        );
    }
}
