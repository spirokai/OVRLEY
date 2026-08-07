//! Encoder codec availability probing.
//!
//! Owns: codec capability detection — `detect_codecs` spawns ffmpeg subprocesses
//!       to discover which H.264, H.265, ProRes, QTRLE, and hardware-accelerated
//!       encoders (NVENC, QSV, VAAPI, AMF, VideoToolbox) are available. Also owns
//!       hardware filter-name parsing (`parse_ffmpeg_filter_names`) and the
//!       `AvailableCodecs` capability struct returned to the frontend.
//! Does not own: ffmpeg binary resolution (see [`crate::encode::ffmpeg::binary`]), encoder
//!       profile selection (see [`crate::encode::ffmpeg::composite_profiles`]),
//!       actual encoding settings construction (see
//!       [`crate::encode::ffmpeg::settings`], [`crate::encode::ffmpeg::composite`]).
//!
//! Allowed dependencies: `crate::encode::ffmpeg::binary`,
//!       `crate::encode::ffmpeg::transparent_profiles`, `crate::error`.
//! Forbidden dependencies: `crate::commands`, `crate::render`, `crate::normalize`.
//!
//! Related modules: [`crate::encode::ffmpeg::composite_profiles`] (consumes detected
//!       codecs to select encoder profiles), [`crate::media::video_probe`]
//!       (video metadata extraction, separate concern).
//!
//! ## Thread Safety
//! Single-threaded. Spawns one ffmpeg probe subprocess per codec, each with an
//! 8-second timeout, and waits synchronously for each. No shared mutable state.
//!
//! ## Performance
//! Heavy one-time operation: probes each unique catalog encoder plus specialized
//! hardware paths sequentially, each
//! with up to 8s timeout. Called once at application startup; result is cached
//! by the frontend. Total worst-case wall time ~160s (unlikely — most probes
//! complete in < 1s). Not on any render hot path.

use crate::encode::ffmpeg::binary::{configure_ffmpeg_command, resolve_ffmpeg_binary};
use crate::encode::ffmpeg::catalog::{
    CompositeAvailabilityRule, CompositeCodecId, EncoderId, TransparentAvailabilityRule,
    TransparentCodecId, ENCODERS,
};
use crate::error::CoreResult;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

/// Collective codec-availability snapshot returned to the frontend.
///
/// Each boolean field corresponds to one encoder probed at startup. Hardware
/// acceleration fields (`nvgpu`, `cuda_filter_stack`, `qsv_full`, etc.) indicate
/// whether the required ffmpeg filters are available, not just the encoder.
/// The frontend uses this struct to grey out unavailable codec options in the
/// export dialog.
///
/// # Thread Safety
/// Constructed once at startup on the calling thread. Immutable after construction.
/// Safe to serialize and send to the frontend.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableCodecs {
    pub prores_ks: bool,
    pub prores_ks_vulkan: bool,
    pub prores_videotoolbox: bool,
    pub qtrle: bool,
    pub libx264: bool,
    pub libx265: bool,
    pub h264_nvenc: bool,
    pub hevc_nvenc: bool,
    pub h264_qsv: bool,
    pub hevc_qsv: bool,
    pub h264_vaapi: bool,
    pub hevc_vaapi: bool,
    pub h264_amf: bool,
    pub hevc_amf: bool,
    pub h264_videotoolbox: bool,
    pub hevc_videotoolbox: bool,
    pub cuda: bool,
    pub nvdec: bool,
    pub qsv: bool,
    pub vaapi: bool,
    pub videotoolbox: bool,
    pub nvgpu: bool,
    pub nnvgpu: bool,
    pub overlay_cuda: bool,
    pub scale_cuda: bool,
    pub scale_qsv: bool,
    pub hwupload_filter: bool,
    pub overlay_qsv: bool,
    pub hwdownload_filter: bool,
    pub transpose_cuda: bool,
    pub vpp_qsv: bool,
    pub scale_vaapi: bool,
    pub overlay_vaapi: bool,
    pub vaapi_full: bool,
    pub qsv_full: bool,
    pub qsv_full_init_args: Vec<String>,
}

impl AvailableCodecs {
    /// Returns whether the requested transparent codec is available.
    ///
    /// This keeps callers on typed catalog identifiers instead of repeating
    /// field-name matches across detection and builder layers.
    pub fn has_transparent_codec(&self, codec: TransparentCodecId) -> bool {
        match codec.metadata().availability_rule {
            TransparentAvailabilityRule::ProresKs => self.prores_ks,
            TransparentAvailabilityRule::ProresKsVulkan => self.prores_ks_vulkan,
            TransparentAvailabilityRule::ProresVideotoolbox => self.prores_videotoolbox,
            TransparentAvailabilityRule::Qtrle => self.qtrle,
        }
    }

    /// Returns whether the requested composite profile is available.
    ///
    /// Composite profiles can depend on both encoder availability and a
    /// specific hardware filter stack, so the catalog rule stays canonical.
    pub fn has_composite_codec(&self, codec: CompositeCodecId) -> bool {
        match codec.metadata().availability_rule {
            CompositeAvailabilityRule::Always => true,
            CompositeAvailabilityRule::H264Nvenc => self.h264_nvenc,
            CompositeAvailabilityRule::HevcNvenc => self.hevc_nvenc,
            CompositeAvailabilityRule::H264Qsv => self.h264_qsv,
            CompositeAvailabilityRule::HevcQsv => self.hevc_qsv,
            CompositeAvailabilityRule::H264Amf => self.h264_amf,
            CompositeAvailabilityRule::HevcAmf => self.hevc_amf,
            CompositeAvailabilityRule::H264Videotoolbox => self.h264_videotoolbox,
            CompositeAvailabilityRule::HevcVideotoolbox => self.hevc_videotoolbox,
            CompositeAvailabilityRule::H264VaapiWithFullFilters => {
                self.h264_vaapi && self.vaapi_full
            }
            CompositeAvailabilityRule::HevcVaapiWithFullFilters => {
                self.hevc_vaapi && self.vaapi_full
            }
            CompositeAvailabilityRule::H264NvencWithCudaFilters => self.h264_nvenc && self.nnvgpu,
            CompositeAvailabilityRule::HevcNvencWithCudaFilters => self.hevc_nvenc && self.nnvgpu,
            CompositeAvailabilityRule::H264QsvWithFullFilters => self.h264_qsv && self.qsv_full,
            CompositeAvailabilityRule::HevcQsvWithFullFilters => self.hevc_qsv && self.qsv_full,
        }
    }
}

/// Probes every known encoder and hardware filter via ffmpeg subprocesses.
///
/// Each probe spawns ffmpeg with a minimal null-source encode for 1 frame and
/// checks the exit code. Hardware filters are detected via `ffmpeg -filters`.
/// The function is intentionally sequential — parallel probing adds complexity
/// without a meaningful speedup since probe time is dominated by subprocess
/// startup, not encode work.
///
/// # Phases
/// 1. Probe each unique catalog encoder through its declared [`ProbeKind`]
/// 2. Probe the specialized CUDA upload path
/// 3. Detect hardware filter availability via `ffmpeg -filters`
/// 4. Probe experimental QSV full-overlay hardware init arguments
/// 5. Assemble the `AvailableCodecs` result struct
///
/// # Performance
/// Called once at application startup. Worst case ~160s for all ~20 probes if
/// every subprocess hits the 8-second timeout. Typical time is < 10s because
/// most codecs either succeed or fail quickly.
///
/// # Errors
/// Returns [`CoreError::FfmpegNotFound`] if the ffmpeg binary cannot be located.
/// Individual probe failures are silently recorded as `false` — a missing codec
/// is not a fatal error.
#[must_use = "expensive subprocess call; codec capabilities must be consumed"]
pub fn detect_codecs(repo_root: &Path) -> CoreResult<AvailableCodecs> {
    let ffmpeg_path = resolve_ffmpeg_binary(repo_root)?;
    let vaapi_device = super::probes::find_vaapi_device();

    // ── PHASE 1: PROBE UNIQUE CATALOG ENCODERS ──
    let encoder_availability = ENCODERS
        .iter()
        .map(|encoder| {
            (
                encoder.id,
                super::probes::probe_encoder(&ffmpeg_path, vaapi_device.as_deref(), encoder),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let available = |encoder_id| encoder_availability[&encoder_id];

    let prores_ks = available(EncoderId::ProresKs);
    let prores_ks_vulkan = available(EncoderId::ProresKsVulkan);
    let prores_videotoolbox = available(EncoderId::ProresVideotoolbox);
    let qtrle = available(EncoderId::Qtrle);
    let libx264 = available(EncoderId::Libx264);
    let libx265 = available(EncoderId::Libx265);
    let h264_nvenc = available(EncoderId::H264Nvenc);
    let hevc_nvenc = available(EncoderId::HevcNvenc);
    let h264_qsv = available(EncoderId::H264Qsv);
    let hevc_qsv = available(EncoderId::HevcQsv);
    let h264_vaapi = available(EncoderId::H264Vaapi);
    let hevc_vaapi = available(EncoderId::HevcVaapi);
    let h264_amf = available(EncoderId::H264Amf);
    let hevc_amf = available(EncoderId::HevcAmf);
    let h264_videotoolbox = available(EncoderId::H264Videotoolbox);
    let hevc_videotoolbox = available(EncoderId::HevcVideotoolbox);

    // ── PHASE 2: PROBE CUDA UPLOAD PATH ──
    let cuda_h264_nvenc = super::probes::probe_codec(
        "cuda_h264_nvenc",
        &ffmpeg_path,
        &cuda_upload_probe_args("h264_nvenc"),
    );
    let cuda_hevc_nvenc = super::probes::probe_codec(
        "cuda_hevc_nvenc",
        &ffmpeg_path,
        &cuda_upload_probe_args("hevc_nvenc"),
    );
    let cuda = cuda_h264_nvenc || cuda_hevc_nvenc;

    // ── PHASE 3: DETECT FILTER CAPABILITIES ──
    let filters = detect_ffmpeg_filters(&ffmpeg_path);
    let overlay_cuda = filters.contains("overlay_cuda");
    let scale_cuda = filters.contains("scale_cuda");
    let scale_qsv = filters.contains("scale_qsv");
    let hwupload_filter = filters.contains("hwupload");
    let overlay_qsv = filters.contains("overlay_qsv");
    let hwdownload_filter = filters.contains("hwdownload");
    let transpose_cuda = filters.contains("transpose_cuda");
    let vpp_qsv = filters.contains("vpp_qsv");
    let scale_vaapi = filters.contains("scale_vaapi");
    let overlay_vaapi = filters.contains("overlay_vaapi");
    let cuda_filter_stack = cuda && overlay_cuda && scale_cuda && hwupload_filter;
    let qsv_filter_stack = overlay_qsv && scale_qsv && hwupload_filter;
    let vaapi_filter_stack = scale_vaapi && overlay_vaapi && hwupload_filter;

    // ── PHASE 4: PROBE EXPERIMENTAL QSV FULL-OVERLAY PATH ──
    let qsv_full_init_args = if (h264_qsv || hevc_qsv) && qsv_filter_stack {
        super::probes::detect_qsv_full_init_args(&ffmpeg_path).unwrap_or_default()
    } else {
        Vec::new()
    };
    let qsv_full = !qsv_full_init_args.is_empty();
    let vaapi_full = (h264_vaapi || hevc_vaapi) && vaapi_filter_stack;

    // ── PHASE 5: ASSEMBLE RESULT ──
    Ok(AvailableCodecs {
        prores_ks,
        prores_ks_vulkan,
        prores_videotoolbox,
        qtrle,
        libx264,
        libx265,
        h264_nvenc,
        hevc_nvenc,
        h264_qsv,
        hevc_qsv,
        h264_vaapi,
        hevc_vaapi,
        h264_amf,
        hevc_amf,
        h264_videotoolbox,
        hevc_videotoolbox,
        cuda,
        nvdec: h264_nvenc || hevc_nvenc,
        qsv: h264_qsv || hevc_qsv,
        vaapi: h264_vaapi || hevc_vaapi,
        videotoolbox: prores_videotoolbox || h264_videotoolbox || hevc_videotoolbox,
        nvgpu: h264_nvenc || hevc_nvenc,
        nnvgpu: cuda_filter_stack,
        overlay_cuda,
        scale_cuda,
        scale_qsv,
        hwupload_filter,
        overlay_qsv,
        hwdownload_filter,
        transpose_cuda,
        vpp_qsv,
        scale_vaapi,
        overlay_vaapi,
        vaapi_full,
        qsv_full,
        qsv_full_init_args,
    })
}

fn cuda_upload_probe_args(encoder: &str) -> Vec<String> {
    [
        "-hide_banner",
        "-loglevel",
        "error",
        "-f",
        "lavfi",
        "-i",
        "nullsrc=s=256x256:d=1",
        "-vf",
        "format=nv12,hwupload_cuda",
        "-c:v",
        encoder,
        "-frames:v",
        "1",
        "-f",
        "null",
        "-",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

/// Lists FFmpeg filter names advertised by the bundled FFmpeg binary.
///
/// A failed probe returns an empty set so hardware-only filter profiles are
/// conservatively disabled instead of being shown optimistically.
fn detect_ffmpeg_filters(ffmpeg_path: &Path) -> std::collections::BTreeSet<String> {
    let mut command = Command::new(ffmpeg_path);
    configure_ffmpeg_command(&mut command);
    let output = command.args(["-hide_banner", "-filters"]).output();
    let Ok(output) = output else {
        return std::collections::BTreeSet::new();
    };

    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    parse_ffmpeg_filter_names(&text)
}

/// Parses the human-readable `ffmpeg -filters` table into filter names.
///
/// Only rows with a media-flow signature are treated as filter entries, which
/// avoids capturing headings and legend text.
pub fn parse_ffmpeg_filter_names(filters_output: &str) -> std::collections::BTreeSet<String> {
    filters_output
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let _flags = parts.next()?;
            let name = parts.next()?;
            let signature = parts.next()?;
            signature.contains("->").then(|| name.to_string())
        })
        .collect()
}
