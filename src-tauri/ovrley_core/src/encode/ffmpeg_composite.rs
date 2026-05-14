//! FFmpeg argument builder for MP4 compositing mode.
//!
//! This module is intentionally separate from the existing transparent-overlay
//! FFmpeg builder so composite rendering can evolve as a parallel backend path.

use std::path::Path;

use super::fps::Fps;

/// Profile-specific FFmpeg settings for composite encoding.
///
/// Later phases can use this to describe hardware decoder, filter, and encoder
/// variations without changing the software default builder surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositeProfile {
    pub name: &'static str,
    pub input_args: Vec<String>,
    pub filter_complex: Option<String>,
    pub output_args: Vec<String>,
}

/// Grouped FFmpeg arguments needed to spawn a composite render.
///
/// The caller is expected to concatenate these groups in order and append the
/// output path if its process-spawning architecture owns destination handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositeFfmpegSettings {
    pub hw_init_args: Vec<String>,
    pub input_0_args: Vec<String>,
    pub input_1_args: Vec<String>,
    pub filter_complex: String,
    pub output_args: Vec<String>,
}

/// Hardware capability flags used by composite profile selection.
///
/// Phase 2 only builds the robust software-overlay path, but this placeholder
/// keeps the builder signature ready for later hardware profile selection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HwAccelInfo {
    pub nvenc_available: bool,
    pub cuda_filters_available: bool,
    pub qsv_available: bool,
    pub qsv_filters_available: bool,
    pub videotoolbox_available: bool,
    pub vaapi_available: bool,
}

/// Builds FFmpeg argument groups for the default software composite path.
///
/// The source video is input 0, the raw RGBA overlay stream is input 1 on
/// `pipe:0`, and output FPS is preserved from the exact source-video FPS.
pub fn build_composite_ffmpeg_settings(
    codec_name: &str,
    bitrate: &str,
    video_path: &Path,
    video_trim_start: f64,
    render_duration: f64,
    width: u32,
    height: u32,
    source_fps: Fps,
    overlay_pipe_fps: Fps,
    hwaccel_available: &HwAccelInfo,
) -> Result<CompositeFfmpegSettings, String> {
    let _ = hwaccel_available;
    validate_composite_inputs(
        codec_name,
        bitrate,
        video_path,
        video_trim_start,
        render_duration,
        width,
        height,
        source_fps,
        overlay_pipe_fps,
    )?;

    let source_fps = source_fps.reduced();
    let overlay_pipe_fps = overlay_pipe_fps.reduced();
    let video_path = video_path.to_string_lossy().to_string();

    let mut input_0_args = Vec::new();
    if video_trim_start > 0.0 {
        input_0_args.push("-ss".to_string());
        input_0_args.push(format_seconds_arg(video_trim_start));
    }
    input_0_args.extend([
        "-t".to_string(),
        format_seconds_arg(render_duration),
        "-i".to_string(),
        video_path,
    ]);

    let input_1_args = vec![
        "-thread_queue_size".to_string(),
        "512".to_string(),
        "-f".to_string(),
        "rawvideo".to_string(),
        "-pix_fmt".to_string(),
        "rgba".to_string(),
        "-s".to_string(),
        format!("{width}x{height}"),
        "-r".to_string(),
        overlay_pipe_fps.ffmpeg_arg(),
        "-i".to_string(),
        "pipe:0".to_string(),
    ];

    let filter_complex = software_overlay_filter_complex(width, height);
    let output_args = vec![
        "-map".to_string(),
        "[out]".to_string(),
        "-map".to_string(),
        "0:a?".to_string(),
        "-r".to_string(),
        source_fps.ffmpeg_arg(),
        "-c:v".to_string(),
        codec_name.to_string(),
        "-b:v".to_string(),
        bitrate.to_string(),
        "-c:a".to_string(),
        "copy".to_string(),
        "-movflags".to_string(),
        "faststart".to_string(),
        "-y".to_string(),
    ];

    Ok(CompositeFfmpegSettings {
        hw_init_args: Vec::new(),
        input_0_args,
        input_1_args,
        filter_complex,
        output_args,
    })
}

/// Builds the Phase 2 software overlay filter graph.
///
/// The imported video is scaled to the overlay resolution, the overlay stream
/// is timestamp-normalized, and FFmpeg repeats overlay frames between updates.
fn software_overlay_filter_complex(width: u32, height: u32) -> String {
    format!(
        "[0:v]setpts=PTS-STARTPTS,scale={width}:{height}[base];\
[1:v]setpts=PTS-STARTPTS[ovr];\
[base][ovr]overlay=0:0:eof_action=repeat:shortest=1,format=yuv420p[out]"
    )
}

/// Validates composite FFmpeg builder inputs before any command is produced.
///
/// This fails fast on values that would otherwise produce confusing FFmpeg
/// errors or accidentally round/zero an FPS value.
fn validate_composite_inputs(
    codec_name: &str,
    bitrate: &str,
    video_path: &Path,
    video_trim_start: f64,
    render_duration: f64,
    width: u32,
    height: u32,
    source_fps: Fps,
    overlay_pipe_fps: Fps,
) -> Result<(), String> {
    if codec_name.trim().is_empty() {
        return Err("Composite codec name must not be empty".to_string());
    }
    if bitrate.trim().is_empty() {
        return Err("Composite bitrate must not be empty".to_string());
    }
    if video_path.as_os_str().is_empty() {
        return Err("Composite video path must not be empty".to_string());
    }
    if !render_duration.is_finite() || render_duration <= 0.0 {
        return Err(format!(
            "Composite render duration must be greater than zero: {render_duration}"
        ));
    }
    if !video_trim_start.is_finite() || video_trim_start < 0.0 {
        return Err(format!(
            "Composite video trim start must be zero or greater: {video_trim_start}"
        ));
    }
    if width == 0 {
        return Err("Composite width must be greater than zero".to_string());
    }
    if height == 0 {
        return Err("Composite height must be greater than zero".to_string());
    }
    validate_fps("source FPS", source_fps)?;
    validate_fps("overlay pipe FPS", overlay_pipe_fps)?;
    Ok(())
}

/// Validates a public `Fps` value that may have been constructed directly.
///
/// `Fps::new` is preferred, but the fields are public for simple data passing,
/// so the composite builder also guards against direct zero-valued instances.
fn validate_fps(label: &str, fps: Fps) -> Result<(), String> {
    if fps.num == 0 {
        return Err(format!(
            "Composite {label} numerator must be greater than zero"
        ));
    }
    if fps.den == 0 {
        return Err(format!(
            "Composite {label} denominator must be greater than zero"
        ));
    }
    Ok(())
}

/// Formats seconds for FFmpeg while trimming insignificant decimal zeros.
///
/// This keeps integer trim values readable as `10` while preserving fractional
/// durations when callers pass non-integer values.
fn format_seconds_arg(value: f64) -> String {
    if value.fract().abs() <= f64::EPSILON {
        return format!("{}", value.trunc() as i64);
    }
    let formatted = format!("{value:.6}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

#[cfg(test)]
#[path = "tests/ffmpeg_composite_tests.rs"]
mod tests;
