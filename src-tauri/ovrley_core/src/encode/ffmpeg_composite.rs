//! FFmpeg argument builder for MP4 compositing mode.
//!
//! This module is intentionally separate from the transparent-overlay FFmpeg
//! builder so composite rendering can evolve as a parallel backend path.
//!
//! Owns: `CompositeProfile` (per-codec encoding profile), `CompositeFfmpegSettings`
//!       (grouped FFmpeg arguments for 3-input composite encodes), `HwAccelInfo`
//!       (hardware acceleration availability), and `build_composite_ffmpeg_settings`
//!       (the main argument construction function).
//! Does not own: encoder profile templates (see
//!       [`crate::encode::ffmpeg_composite_profiles`]), codec detection (see
//!       [`crate::encode::codec_detect`]), actual ffmpeg process spawning (see
//!       [`crate::encode::video_composite_pipeline`]).
//!
//! Allowed dependencies: `crate::encode::codec_detect`, `crate::encode::ffmpeg_composite_profiles`,
//!       `crate::encode::fps`, `crate::error`.
//! Forbidden dependencies: `crate::commands`, `crate::render`,
//!       `crate::encode::video_pipeline`, `crate::encode::video_composite_pipeline`.
//!
//! ## Thread Safety
//! All types are plain data (no shared mutable state). Callers construct
//! `CompositeFfmpegSettings` on the render thread before spawning ffmpeg.

use std::path::Path;

use crate::encode::codec_catalog::{
    composite_codec, CompositeAvailabilityRule, CompositeCodecMetadata,
};
use crate::encode::codec_detect::AvailableCodecs;
use crate::error::{CoreError, CoreResult};

use super::ffmpeg_composite_profiles::composite_profile_template;
use super::fps::Fps;

/// Profile-specific FFmpeg settings for composite encoding.
///
/// Later phases can use this to describe hardware decoder, filter, and encoder
/// variations without changing the software default builder surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositeProfile {
    pub name: &'static str,
    pub codec: &'static str,
    pub input_args: Vec<String>,
    pub filter_complex: Option<String>,
    pub output_args: Vec<String>,
}

/// Grouped FFmpeg arguments needed to spawn a composite render.
///
/// The caller is expected to concatenate these groups in order and append the
/// output path if its process-spawning architecture owns destination handling.
///
/// Composite mode currently uses three inputs:
/// - input 0: unseeked source video for frame-accurate filter-side video trim
/// - input 1: raw RGBA overlay frames from stdin (`pipe:0`)
/// - input 2: separately trimmed source media for audio stream copy
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositeFfmpegSettings {
    pub selected_profile_name: String,
    pub fallback_profile_name: Option<String>,
    pub hw_init_args: Vec<String>,
    pub input_0_args: Vec<String>,
    pub input_1_args: Vec<String>,
    pub input_2_args: Vec<String>,
    pub filter_complex: String,
    pub output_args: Vec<String>,
}

/// Hardware capability flags used by composite profile selection.
///
/// The canonical boolean availability now lives in [`AvailableCodecs`]. This
/// wrapper keeps the builder inputs small while still carrying non-boolean
/// context such as a resolved VAAPI device path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HwAccelInfo {
    pub available_codecs: AvailableCodecs,
    pub vaapi_device: Option<String>,
}

impl HwAccelInfo {
    /// Returns capability flags that trust the already-selected frontend profile.
    ///
    /// The render path uses this when AppBootstrap has already filtered codec
    /// choices, avoiding a second FFmpeg capability probe during export.
    pub fn trust_selected_profile() -> Self {
        Self {
            available_codecs: AvailableCodecs {
                prores_ks: true,
                prores_ks_vulkan: true,
                prores_videotoolbox: true,
                qtrle: true,
                libx264: true,
                libx265: true,
                h264_nvenc: true,
                hevc_nvenc: true,
                h264_qsv: true,
                hevc_qsv: true,
                h264_vaapi: true,
                hevc_vaapi: true,
                h264_amf: true,
                hevc_amf: true,
                h264_videotoolbox: true,
                hevc_videotoolbox: true,
                cuda: true,
                nvdec: true,
                qsv: true,
                vaapi: true,
                videotoolbox: true,
                nvgpu: true,
                nnvgpu: true,
                overlay_cuda: true,
                scale_cuda: true,
                scale_qsv: true,
                hwupload_filter: true,
                overlay_qsv: true,
                hwdownload_filter: true,
                qsv_full: true,
                qsv_full_init_args: Vec::new(),
            },
            vaapi_device: None,
        }
    }

    /// Returns whether the requested composite profile is available.
    pub fn has_composite_codec(&self, codec: super::codec_catalog::CompositeCodecId) -> bool {
        self.available_codecs.has_composite_codec(codec)
    }

    /// Returns the detected QSV full-path hardware init args, if any.
    pub fn qsv_full_init_args(&self) -> &[String] {
        &self.available_codecs.qsv_full_init_args
    }
}

impl From<&AvailableCodecs> for HwAccelInfo {
    /// Converts the existing codec detector response into composite profile flags.
    ///
    /// The frontend and backend share the same codec names, so the
    /// hardware-info adapter can use the detector output directly for
    /// explicit hardware profile validation.
    fn from(codecs: &AvailableCodecs) -> Self {
        Self {
            available_codecs: codecs.clone(),
            vaapi_device: None,
        }
    }
}

/// Bundled inputs for building composite FFmpeg settings.
///
/// Consolidates what was previously 9–10 separate parameters shared between
/// `build_composite_ffmpeg_settings` and `validate_composite_inputs`.
pub struct CompositeFfmpegBuildRequest<'a> {
    pub codec_name: &'a str,
    pub bitrate: &'a str,
    pub video_path: &'a Path,
    pub video_trim_start: f64,
    pub render_duration: f64,
    pub width: u32,
    pub height: u32,
    pub source_fps: Fps,
    pub overlay_pipe_fps: Fps,
    pub hwaccel_available: &'a HwAccelInfo,
}

/// Builds FFmpeg argument groups for the default software composite path.
///
/// Input 0 keeps the original source video unseeked so the filter graph can
/// apply frame-accurate video trimming. Input 1 is the raw RGBA overlay stream
/// on `pipe:0`, and input 2 is a separately trimmed source-media input used for
/// copying audio without re-encoding it.
///
/// The function validates inputs, selects and configures an encoder profile,
/// then assembles four argument groups that callers concatenate in order: HW
/// init args, input 0 (unseeked video), input 1 (overlay pipe), input 2 (trimmed
/// audio source), filter_complex, and output args.
pub fn build_composite_ffmpeg_settings(
    request: &CompositeFfmpegBuildRequest<'_>,
) -> CoreResult<CompositeFfmpegSettings> {
    // ── PHASE 1: VALIDATE INPUTS & REDUCE FPS ──
    validate_composite_inputs(request)?;

    let source_fps = request.source_fps.reduced();
    let overlay_pipe_fps = request.overlay_pipe_fps.reduced();
    let video_path = request.video_path.to_string_lossy().to_string();

    // ── PHASE 2: SELECT & CONFIGURE PROFILE ──
    let mut selected_profile =
        select_composite_profile(request.codec_name, request.hwaccel_available)?;
    if selected_profile.name.starts_with("qsv_full_") {
        if request.hwaccel_available.qsv_full_init_args().is_empty() {
            return Err(CoreError::Encode(format!(
                "Requested experimental QSV overlay profile {} is unavailable; codec detection did not provide working QSV hardware-device init args.",
                selected_profile.name
            )));
        }
        selected_profile.input_args = request.hwaccel_available.qsv_full_init_args().to_vec();
    }

    // ── PHASE 3: BUILD INPUT 0 ARGS (unseeked source video for filter-side trim) ──
    let mut input_0_args = selected_profile.input_args.clone();
    input_0_args.extend(["-i".to_string(), video_path.clone()]);

    // ── PHASE 4: BUILD INPUT 1 ARGS (raw RGBA overlay via stdin pipe) ──
    let input_1_args = vec![
        "-thread_queue_size".to_string(),
        "512".to_string(),
        "-f".to_string(),
        "rawvideo".to_string(),
        "-pix_fmt".to_string(),
        "rgba".to_string(),
        "-s".to_string(),
        format!(
            "{width}x{height}",
            width = request.width,
            height = request.height
        ),
        "-r".to_string(),
        overlay_pipe_fps.ffmpeg_arg(),
        "-i".to_string(),
        "pipe:0".to_string(),
    ];

    // ── PHASE 5: BUILD INPUT 2 ARGS (trimmed audio source for stream copy) ──
    let mut input_2_args = Vec::new();
    if request.video_trim_start > 0.0 {
        input_2_args.push("-ss".to_string());
        input_2_args.push(format_seconds_arg(request.video_trim_start));
    }
    input_2_args.extend([
        "-t".to_string(),
        format_seconds_arg(request.render_duration),
        "-i".to_string(),
        video_path,
    ]);

    // ── PHASE 6: BUILD FILTER COMPLEX (video trim + scale + overlay + format) ──
    let filter_complex = composite_filter_complex(
        request.width,
        request.height,
        request.video_trim_start,
        request.render_duration,
        &selected_profile,
    )?;

    // ── PHASE 7: BUILD OUTPUT ARGS (map, codec, bitrate, audio copy, mux flags) ──
    let mut output_args = vec![
        "-map".to_string(),
        "[out]".to_string(),
        "-map".to_string(),
        "2:a?".to_string(),
        "-r".to_string(),
        source_fps.ffmpeg_arg(),
    ];
    output_args.extend(selected_profile.output_args.clone());
    output_args.extend([
        "-b:v".to_string(),
        request.bitrate.to_string(),
        "-c:a".to_string(),
        "copy".to_string(),
        "-shortest".to_string(),
        "-movflags".to_string(),
        "faststart".to_string(),
        "-y".to_string(),
    ]);

    Ok(CompositeFfmpegSettings {
        selected_profile_name: selected_profile.name.to_string(),
        fallback_profile_name: fallback_profile_name(&selected_profile),
        hw_init_args: profile_hw_init_args(&selected_profile, request.hwaccel_available),
        input_0_args,
        input_1_args,
        input_2_args,
        filter_complex,
        output_args,
    })
}

/// Builds global FFmpeg hardware initialization args for the selected profile.
///
/// Most hardware profiles use hardware encode only and need no global setup;
/// VAAPI is the exception because FFmpeg requires a render device.
fn profile_hw_init_args(
    profile: &CompositeProfile,
    hwaccel_available: &HwAccelInfo,
) -> Vec<String> {
    if !matches!(profile.codec, "h264_vaapi" | "hevc_vaapi") {
        return Vec::new();
    }
    hwaccel_available
        .vaapi_device
        .as_ref()
        .map(|device| vec!["-vaapi_device".to_string(), device.clone()])
        .unwrap_or_default()
}

/// Selects the requested composite profile and validates explicit hardware asks.
///
/// Unknown codec names remain pass-through so FFmpeg can produce detailed
/// process-level diagnostics for custom encoders and failure tests.
fn select_composite_profile(
    codec_name: &str,
    hwaccel_available: &HwAccelInfo,
) -> CoreResult<CompositeProfile> {
    let requested_profile = composite_codec(codec_name);
    let profile = composite_profile_template(codec_name).unwrap_or_else(|_| CompositeProfile {
        name: "custom_passthrough",
        codec: "custom",
        input_args: Vec::new(),
        filter_complex: None,
        output_args: vec!["-c:v".to_string(), codec_name.to_string()],
    });

    if let Some(metadata) = requested_profile {
        validate_catalog_profile_availability(metadata, hwaccel_available)?;
    }

    Ok(profile)
}

/// Validates one catalog-backed composite profile request.
///
/// This keeps availability messages centralized on typed catalog metadata
/// instead of relying on string prefixes or codec-name match trees.
fn validate_catalog_profile_availability(
    metadata: &CompositeCodecMetadata,
    hwaccel_available: &HwAccelInfo,
) -> CoreResult<()> {
    match metadata.availability_rule {
        CompositeAvailabilityRule::Always => Ok(()),
        CompositeAvailabilityRule::H264Nvenc => validate_simple_catalog_profile(
            hwaccel_available,
            metadata.id,
            metadata.ffmpeg_codec_name,
        ),
        CompositeAvailabilityRule::HevcNvenc => validate_simple_catalog_profile(
            hwaccel_available,
            metadata.id,
            metadata.ffmpeg_codec_name,
        ),
        CompositeAvailabilityRule::H264Qsv => validate_simple_catalog_profile(
            hwaccel_available,
            metadata.id,
            metadata.ffmpeg_codec_name,
        ),
        CompositeAvailabilityRule::HevcQsv => validate_simple_catalog_profile(
            hwaccel_available,
            metadata.id,
            metadata.ffmpeg_codec_name,
        ),
        CompositeAvailabilityRule::H264Amf => validate_simple_catalog_profile(
            hwaccel_available,
            metadata.id,
            metadata.ffmpeg_codec_name,
        ),
        CompositeAvailabilityRule::HevcAmf => validate_simple_catalog_profile(
            hwaccel_available,
            metadata.id,
            metadata.ffmpeg_codec_name,
        ),
        CompositeAvailabilityRule::H264Videotoolbox => validate_simple_catalog_profile(
            hwaccel_available,
            metadata.id,
            metadata.ffmpeg_codec_name,
        ),
        CompositeAvailabilityRule::HevcVideotoolbox => validate_simple_catalog_profile(
            hwaccel_available,
            metadata.id,
            metadata.ffmpeg_codec_name,
        ),
        CompositeAvailabilityRule::H264Vaapi => validate_simple_catalog_profile(
            hwaccel_available,
            metadata.id,
            metadata.ffmpeg_codec_name,
        ),
        CompositeAvailabilityRule::HevcVaapi => validate_simple_catalog_profile(
            hwaccel_available,
            metadata.id,
            metadata.ffmpeg_codec_name,
        ),
        CompositeAvailabilityRule::H264NvencWithCudaFilters => validate_stacked_catalog_profile(
            hwaccel_available,
            super::codec_catalog::CompositeCodecId::NvgpuH264,
            metadata.id,
            metadata.ffmpeg_codec_name,
            metadata.profile_name,
            unavailable_cuda_overlay_profile,
        ),
        CompositeAvailabilityRule::HevcNvencWithCudaFilters => validate_stacked_catalog_profile(
            hwaccel_available,
            super::codec_catalog::CompositeCodecId::NvgpuHevc,
            metadata.id,
            metadata.ffmpeg_codec_name,
            metadata.profile_name,
            unavailable_cuda_overlay_profile,
        ),
        CompositeAvailabilityRule::H264QsvWithFullFilters => validate_stacked_catalog_profile(
            hwaccel_available,
            super::codec_catalog::CompositeCodecId::QsvH264,
            metadata.id,
            metadata.ffmpeg_codec_name,
            metadata.profile_name,
            unavailable_qsv_overlay_profile,
        ),
        CompositeAvailabilityRule::HevcQsvWithFullFilters => validate_stacked_catalog_profile(
            hwaccel_available,
            super::codec_catalog::CompositeCodecId::QsvHevc,
            metadata.id,
            metadata.ffmpeg_codec_name,
            metadata.profile_name,
            unavailable_qsv_overlay_profile,
        ),
    }
}

/// Validates a profile whose availability is represented by one canonical rule.
fn validate_simple_catalog_profile(
    hwaccel_available: &HwAccelInfo,
    codec_id: super::codec_catalog::CompositeCodecId,
    codec_name: &str,
) -> CoreResult<()> {
    if hwaccel_available.has_composite_codec(codec_id) {
        Ok(())
    } else {
        unavailable_encoder(codec_name)
    }
}

/// Validates an experimental stacked profile while preserving legacy errors.
fn validate_stacked_catalog_profile(
    hwaccel_available: &HwAccelInfo,
    safe_codec_id: super::codec_catalog::CompositeCodecId,
    stacked_codec_id: super::codec_catalog::CompositeCodecId,
    codec_name: &str,
    profile_name: &str,
    unavailable_stack: fn(&str) -> CoreResult<()>,
) -> CoreResult<()> {
    if !hwaccel_available.has_composite_codec(safe_codec_id) {
        return unavailable_encoder(codec_name);
    }
    if hwaccel_available.has_composite_codec(stacked_codec_id) {
        Ok(())
    } else {
        unavailable_stack(profile_name)
    }
}

/// Returns the standard "hardware encoder unavailable" error.
fn unavailable_encoder(codec_name: &str) -> CoreResult<()> {
    Err(CoreError::Encode(format!(
        "Requested hardware encoder {codec_name} is unavailable."
    )))
}

/// Returns the standard "CUDA overlay stack unavailable" error.
fn unavailable_cuda_overlay_profile(profile_name: &str) -> CoreResult<()> {
    Err(CoreError::Encode(format!(
        "Requested experimental CUDA overlay profile {profile_name} is unavailable; FFmpeg must support overlay_cuda, scale_cuda, and hwupload."
    )))
}

/// Returns the standard "QSV overlay stack unavailable" error.
fn unavailable_qsv_overlay_profile(profile_name: &str) -> CoreResult<()> {
    Err(CoreError::Encode(format!(
        "Requested experimental QSV overlay profile {profile_name} is unavailable; FFmpeg must support overlay_qsv, scale_qsv, and hwupload."
    )))
}

/// Returns the safe CPU-overlay profile that corresponds to an experimental path.
///
/// This is diagnostic only for explicit full-GPU renders, which fail loudly
/// instead of silently producing a fallback output when FFmpeg rejects the graph.
pub fn fallback_profile_name(profile: &CompositeProfile) -> Option<String> {
    // test seam
    match profile.name {
        "nnvgpu_h264" => Some("nvgpu_h264".to_string()),
        "nnvgpu_hevc" => Some("nvgpu_hevc".to_string()),
        "qsv_full_h264" => Some("qsv_h264".to_string()),
        "qsv_full_hevc" => Some("qsv_hevc".to_string()),
        _ => None,
    }
}

/// Builds the selected profile's composite filter graph.
///
/// Profile templates use `{base_video_filters}`, `{width}`, and `{height}`
/// placeholders. The base-video filter chain owns exact video trimming so the
/// decoded frame boundary matches the segment plan more closely than input-side
/// seek alone.
fn composite_filter_complex(
    width: u32,
    height: u32,
    video_trim_start: f64,
    render_duration: f64,
    profile: &CompositeProfile,
) -> CoreResult<String> {
    let template = profile.filter_complex.as_deref().unwrap_or(
        "[0:v]{base_video_filters}scale={width}:{height}[base];\
[1:v]setpts=PTS-STARTPTS[ovr];\
[base][ovr]overlay=0:0:eof_action=repeat:shortest=1,format=yuv420p[out]",
    );
    let base_video_filters = format!(
        "trim=start={}:end={},setpts=PTS-STARTPTS,",
        format_seconds_arg(video_trim_start),
        format_seconds_arg(video_trim_start + render_duration),
    );
    Ok(template
        .replace("{base_video_filters}", &base_video_filters)
        .replace("{width}", &width.to_string())
        .replace("{height}", &height.to_string()))
}

/// Validates composite FFmpeg builder inputs before any command is produced.
///
/// This fails fast on values that would otherwise produce confusing FFmpeg
/// errors or accidentally round/zero an FPS value.
fn validate_composite_inputs(request: &CompositeFfmpegBuildRequest<'_>) -> CoreResult<()> {
    if request.codec_name.trim().is_empty() {
        return Err(CoreError::Encode(
            "Composite codec name must not be empty".to_string(),
        ));
    }
    if request.bitrate.trim().is_empty() {
        return Err(CoreError::Encode(
            "Composite bitrate must not be empty".to_string(),
        ));
    }
    if request.video_path.as_os_str().is_empty() {
        return Err(CoreError::Encode(
            "Composite video path must not be empty".to_string(),
        ));
    }
    if !request.render_duration.is_finite() || request.render_duration <= 0.0 {
        return Err(CoreError::Encode(format!(
            "Composite render duration must be greater than zero: {}",
            request.render_duration
        )));
    }
    if !request.video_trim_start.is_finite() || request.video_trim_start < 0.0 {
        return Err(CoreError::Encode(format!(
            "Composite video trim start must be zero or greater: {}",
            request.video_trim_start
        )));
    }
    if request.width == 0 {
        return Err(CoreError::Encode(
            "Composite width must be greater than zero".to_string(),
        ));
    }
    if request.height == 0 {
        return Err(CoreError::Encode(
            "Composite height must be greater than zero".to_string(),
        ));
    }
    validate_fps("source FPS", request.source_fps)?;
    validate_fps("overlay pipe FPS", request.overlay_pipe_fps)?;
    Ok(())
}

/// Validates a public `Fps` value that may have been constructed directly.
///
/// `Fps::new` is preferred, but the fields are public for simple data passing,
/// so the composite builder also guards against direct zero-valued instances.
fn validate_fps(label: &str, fps: Fps) -> CoreResult<()> {
    if fps.num == 0 {
        return Err(CoreError::Encode(format!(
            "Composite {label} numerator must be greater than zero"
        )));
    }
    if fps.den == 0 {
        return Err(CoreError::Encode(format!(
            "Composite {label} denominator must be greater than zero"
        )));
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
