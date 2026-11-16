//! Joins the rendered parts of one output with the commands built by
//! [`crate::encode::ffmpeg::assemble`]. Every FFmpeg step stops on cancellation;
//! the target is removed unless the joined output is verified. The caller owns
//! the parts directory and its retention.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use serde::Deserialize;

use super::diagnostics::stderr_tail;
use super::process::PartialOutputGuard;
use crate::encode::ffmpeg::assemble::{
    concat_manifest, join_args, probe_output_args, remux_part_args, transition_segment_args,
    AssemblyKind, AssemblySegment, OutputAssembly,
};
use crate::encode::ffmpeg::binary::resolve_ffmpeg_binary;
use crate::encode::ffmpeg::process::run;
use crate::encode::progress::RenderController;
use crate::error::{CoreError, CoreResult};
use crate::paths::AppPaths;

/// Encodes composite transitions, remuxes every composite segment, joins them
/// into the target and verifies its frame count and audio track.
pub(crate) fn assemble_output(
    paths: &AppPaths,
    assembly: &OutputAssembly,
    controller: &RenderController,
) -> CoreResult<()> {
    let ffmpeg = resolve_ffmpeg_binary(&paths.repo_root)?;
    let cancel = controller.cancel_flag();
    if let AssemblyKind::Composite {
        hevc,
        transition_encoding,
        ..
    } = &assembly.kind
    {
        for segment in &assembly.segments {
            if matches!(segment, AssemblySegment::Transition { .. }) {
                run_join_step(
                    &ffmpeg,
                    &transition_segment_args(transition_encoding, segment),
                    &cancel,
                )?;
            }
        }
        for (part, remux) in assembly.remuxes() {
            run_join_step(&ffmpeg, &remux_part_args(&part, &remux, *hevc), &cancel)?;
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
    run_join_step(&ffmpeg, &join_args(assembly), &cancel)?;
    verify_joined_output(&ffmpeg, assembly, &cancel)?;
    output.preserve();
    Ok(())
}

fn run_join_step(ffmpeg: &Path, args: &[String], cancel: &AtomicBool) -> CoreResult<()> {
    let step = run(ffmpeg, args, false, cancel)?;
    if !step.status.success() {
        return Err(CoreError::Encode(format!(
            "ffmpeg failed while joining output parts ({}). FFmpeg stderr:\n{}",
            step.status,
            stderr_tail(&step.stderr)
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
fn verify_joined_output(
    ffmpeg: &Path,
    assembly: &OutputAssembly,
    cancel: &AtomicBool,
) -> CoreResult<()> {
    let ffprobe = ffmpeg.with_file_name(if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    });
    let output = run(&ffprobe, &probe_output_args(&assembly.target), true, cancel)?;
    if !output.status.success() {
        return Err(CoreError::Encode(format!(
            "ffprobe could not read the joined output {}: {}",
            assembly.target.display(),
            output.stderr.trim()
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

    fn ffmpeg() -> std::path::PathBuf {
        resolve_ffmpeg_binary(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap()
    }

    #[test]
    fn failed_steps_report_ffmpeg_stderr() {
        let args = ["-nostdin", "-i", "/nonexistent/part.ts", "-f", "null", "-"].map(String::from);
        let error = run_join_step(&ffmpeg(), &args, &AtomicBool::new(false)).unwrap_err();
        assert!(
            error.to_string().contains("/nonexistent/part.ts"),
            "{error}"
        );
    }

    #[test]
    fn cancellation_stops_output_verification() {
        let assembly = OutputAssembly {
            target: "/nonexistent/joined.mov".into(),
            parts_directory: "/nonexistent".into(),
            segments: Vec::new(),
            frames: 1,
            kind: AssemblyKind::Transparent,
        };
        assert!(matches!(
            verify_joined_output(&ffmpeg(), &assembly, &AtomicBool::new(true)),
            Err(CoreError::Cancelled)
        ));
    }
}
