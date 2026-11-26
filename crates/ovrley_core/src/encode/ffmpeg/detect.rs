//! Encoder availability: FFmpeg probes of every catalog encoder (H.264, HEVC, ProRes, QTRLE and
//! the NVENC, QSV, VAAPI, AMF and VideoToolbox hardware paths) and of the filters the hardware
//! filter stacks need.
//!
//! Probes run sequentially, each with an 8 s timeout, once at startup; most finish in under a
//! second. Detection is never on a render path.

use crate::encode::ffmpeg::{configure_ffmpeg_command, resolve_ffmpeg_binary};
use crate::encode::ffmpeg::catalog::{
    CompositeAvailabilityRule, CompositeCodecId, EncoderId, TransparentAvailabilityRule,
    TransparentCodecId, ENCODERS,
};
use crate::error::CoreResult;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;
use crate::encode::ffmpeg::catalog::{
    CompositeFilterStackKind, EncoderMetadata, ProbeKind, };
use crate::encode::ffmpeg::composite::composite_filter_complex;
use crate::encode::ffmpeg::profiles::composite_profile;
use crate::encode::ffmpeg::profiles::transparent_profile;
use crate::render::{AtlasPlan, FrameSize};
use skia_safe::IRect;
use std::fs;
use std::process::Stdio;
use std::thread;
use std::time::{Duration, Instant};

/// The encoders and hardware paths available on this machine, detected once at startup.
///
/// Each boolean is one probed encoder. Hardware fields (`nvgpu`, `cuda_filter_stack`,
/// `qsv_full`, ...) also require the FFmpeg filters their stack uses. The export dialog disables
/// unavailable codecs from it.
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
/// # Steps
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
    let vaapi_device = super::detect::find_vaapi_device();

    // Every unique catalog encoder.
    let encoder_availability = ENCODERS
        .iter()
        .map(|encoder| {
            // Linux exports through VAAPI and NVENC. The editor hides QSV and AMF there, so skip
            // probing them rather than initialising a GPU path nothing can select.
            let hidden_on_linux = cfg!(target_os = "linux")
                && matches!(
                    encoder.id,
                    EncoderId::H264Qsv | EncoderId::HevcQsv | EncoderId::H264Amf | EncoderId::HevcAmf
                );
            (
                encoder.id,
                !hidden_on_linux
                    && super::detect::probe_encoder(&ffmpeg_path, vaapi_device.as_deref(), encoder),
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

    // The CUDA upload path.
    let cuda_h264_nvenc = super::detect::probe_codec(
        "cuda_h264_nvenc",
        &ffmpeg_path,
        &cuda_upload_probe_args("h264_nvenc"),
    );
    let cuda_hevc_nvenc = super::detect::probe_codec(
        "cuda_hevc_nvenc",
        &ffmpeg_path,
        &cuda_upload_probe_args("hevc_nvenc"),
    );
    let cuda = cuda_h264_nvenc || cuda_hevc_nvenc;

    // Filter capabilities.
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

    // The experimental QSV full-overlay path.
    let qsv_full_init_args = if (h264_qsv || hevc_qsv) && qsv_filter_stack {
        super::detect::detect_qsv_full_init_args(&ffmpeg_path).unwrap_or_default()
    } else {
        Vec::new()
    };
    let qsv_full = !qsv_full_init_args.is_empty();
    let vaapi_full = (h264_vaapi || hevc_vaapi) && vaapi_filter_stack;

    // The result.
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

const CODEC_PROBE_TIMEOUT: Duration = Duration::from_secs(8);

struct QsvProbeCandidate {
    init_args: &'static [&'static str],
    main_preparation: &'static str,
}

/// Returns the first QSV device arguments that can scale, composite, and encode.
///
/// Keeps 2.49.2's automatic QSV selection (D3D11 with the bundled oneVPL build)
/// and NV12 upload first. Windows then tries DXVA2 with a fixed BGRA upload.
/// The live composite encoder reuses only the successful device arguments.
fn detect_qsv_full_init_args(ffmpeg_path: &Path) -> Option<Vec<String>> {
    select_qsv_full_init_args(|candidate| probe_qsv_overlay_path(ffmpeg_path, candidate))
}

fn select_qsv_full_init_args(
    mut probe: impl FnMut(&QsvProbeCandidate) -> bool,
) -> Option<Vec<String>> {
    qsv_full_probe_candidates()
        .into_iter()
        .find(|candidate| probe(candidate))
        .map(|candidate| {
            candidate
                .init_args
                .iter()
                .map(|arg| (*arg).to_string())
                .collect()
        })
}

fn qsv_full_probe_candidates() -> Vec<QsvProbeCandidate> {
    let mut candidates = vec![QsvProbeCandidate {
        init_args: &[
            "-init_hw_device",
            "qsv=qs",
            "-filter_hw_device",
            "qs",
            "-hwaccel",
            "qsv",
            "-hwaccel_output_format",
            "qsv",
        ],
        main_preparation: "format=nv12,hwupload",
    }];
    if cfg!(windows) {
        candidates.push(QsvProbeCandidate {
            init_args: &[
                "-init_hw_device",
                "dxva2=dx",
                "-init_hw_device",
                "qsv=qs@dx",
                "-filter_hw_device",
                "qs",
                "-hwaccel",
                "qsv",
                "-hwaccel_output_format",
                "qsv",
            ],
            main_preparation: "format=bgra,hwupload=extra_hw_frames=16,scale_qsv=format=nv12",
        });
    }

    candidates
}

/// Probes whether a QSV device can run the performance QSV filter path.
///
/// Prepares synthetic NV12 hardware frames, then exercises the live render
/// graph over the export's single-slot atlas and direct hardware-frame encoding for 30
/// frames without video decode.
/// Main-frame preparation belongs to each candidate: the preferred path uploads
/// NV12 directly, while DXVA2 uses a fixed BGRA upload and GPU conversion.
/// The live render graph owns the slot upload's 16-frame pool on both paths.
fn probe_qsv_overlay_path(ffmpeg_path: &Path, candidate: &QsvProbeCandidate) -> bool {
    probe_codec(
        "qsv_overlay",
        ffmpeg_path,
        &qsv_overlay_probe_args(candidate),
    )
}

fn qsv_overlay_probe_args(candidate: &QsvProbeCandidate) -> Vec<String> {
    // Two layers merge into the one offset slot the export graph overlays.
    let atlas = AtlasPlan::from_layer_bounds(
        FrameSize {
            width: 128,
            height: 128,
        },
        &[
            Some(IRect::new(8, 8, 40, 40)),
            Some(IRect::new(72, 80, 120, 120)),
        ],
        CompositeFilterStackKind::QsvFullOverlay.slot_placement(),
    )
    .expect("fixed QSV probe layer bounds fit the probe frame");
    let render_graph = composite_filter_complex(
        128,
        128,
        &atlas,
        0.0,
        1.0,
        composite_profile(CompositeCodecId::QsvFullH264),
        None,
        None,
        None,
        None,
    );
    let FrameSize {
        width: atlas_width,
        height: atlas_height,
    } = atlas.size();
    let filter_complex = format!(
        "[0:v]{}[probe_main];{}",
        candidate.main_preparation,
        render_graph.replacen("[0:v]", "[probe_main]", 1),
    );
    let mut args = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
    ];
    args.extend(candidate.init_args.iter().map(|arg| (*arg).to_string()));
    args.extend([
        "-f".to_string(),
        "lavfi".to_string(),
        "-i".to_string(),
        "color=c=black:s=256x256:r=30:d=1,format=yuv420p".to_string(),
        "-f".to_string(),
        "lavfi".to_string(),
        "-i".to_string(),
        format!("color=c=red@0.35:s={atlas_width}x{atlas_height}:r=30:d=1,format=rgba"),
        "-filter_complex".to_string(),
        filter_complex,
        "-map".to_string(),
        "[out]".to_string(),
        "-c:v".to_string(),
        "h264_qsv".to_string(),
        "-frames:v".to_string(),
        "30".to_string(),
        "-f".to_string(),
        "null".to_string(),
        "-".to_string(),
    ]);

    args
}

/// Executes the probe strategy declared by the unique encoder catalog entry.
fn probe_encoder(
    ffmpeg_path: &Path,
    vaapi_device: Option<&Path>,
    encoder: &EncoderMetadata,
) -> bool {
    match encoder.probe_kind {
        ProbeKind::NullSource => probe_null_source_encoder(ffmpeg_path, encoder),
        ProbeKind::TransparentProfile(codec_id) => {
            probe_transparent_profile_encoder(ffmpeg_path, encoder, codec_id)
        }
        ProbeKind::VaapiDevice => {
            vaapi_device.is_some_and(|device| probe_vaapi_encoder(ffmpeg_path, device, encoder))
        }
    }
}

/// Runs the common one-frame null-source probe used by ordinary encoders.
fn probe_null_source_encoder(ffmpeg_path: &Path, encoder: &EncoderMetadata) -> bool {
    let args = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
        "-f".to_string(),
        "lavfi".to_string(),
        "-i".to_string(),
        "nullsrc=s=256x256:d=1".to_string(),
        "-c:v".to_string(),
        encoder.ffmpeg_name.to_string(),
        "-frames:v".to_string(),
        "1".to_string(),
        "-f".to_string(),
        "null".to_string(),
        "-".to_string(),
    ];
    probe_codec(encoder.ffmpeg_name, ffmpeg_path, &args)
}

/// Reuses a transparent profile's device, filter, and encoder fragments.
fn probe_transparent_profile_encoder(
    ffmpeg_path: &Path,
    encoder: &EncoderMetadata,
    codec_id: TransparentCodecId,
) -> bool {
    let profile = transparent_profile(codec_id);
    assert_eq!(codec_id.metadata().encoder_id, encoder.id);

    let mut args = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
    ];
    args.extend(profile.input_args.iter().map(|arg| arg.to_string()));
    args.extend([
        "-f".to_string(),
        "lavfi".to_string(),
        "-i".to_string(),
        "nullsrc=s=256x256:d=1,format=rgba".to_string(),
    ]);
    if let Some(filter) = profile.filter_complex {
        args.extend(["-vf".to_string(), filter.to_string()]);
    }
    args.extend(["-c:v".to_string(), encoder.ffmpeg_name.to_string()]);
    args.extend(profile.output_args.iter().map(|arg| arg.to_string()));
    args.extend([
        "-frames:v".to_string(),
        "1".to_string(),
        "-f".to_string(),
        "null".to_string(),
        "-".to_string(),
    ]);
    probe_codec(encoder.ffmpeg_name, ffmpeg_path, &args)
}

/// Exercises VAAPI device initialization, upload, and encode as one boundary probe.
fn probe_vaapi_encoder(ffmpeg_path: &Path, device: &Path, encoder: &EncoderMetadata) -> bool {
    let args = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
        "-vaapi_device".to_string(),
        device.to_string_lossy().to_string(),
        "-f".to_string(),
        "lavfi".to_string(),
        "-i".to_string(),
        "nullsrc=s=256x256:d=1".to_string(),
        "-vf".to_string(),
        "format=nv12,hwupload".to_string(),
        "-c:v".to_string(),
        encoder.ffmpeg_name.to_string(),
        "-frames:v".to_string(),
        "1".to_string(),
        "-f".to_string(),
        "null".to_string(),
        "-".to_string(),
    ];
    probe_codec(encoder.ffmpeg_name, ffmpeg_path, &args)
}

fn probe_codec(name: &str, ffmpeg_path: &Path, args: &[String]) -> bool {
    let mut command = Command::new(ffmpeg_path);
    command.arg("-nostdin");
    command.args(args);
    configure_ffmpeg_command(&mut command);
    command.stdout(Stdio::null());
    command.stderr(Stdio::null());

    match command.spawn() {
        Ok(mut child) => {
            let started_at = Instant::now();
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => return status.success(),
                    Ok(None) if started_at.elapsed() >= CODEC_PROBE_TIMEOUT => {
                        eprintln!(
                            "[OVRLEY] ffmpeg codec probe timed out after {}s: {name}",
                            CODEC_PROBE_TIMEOUT.as_secs()
                        );
                        let _ = child.kill();
                        let _ = child.wait();
                        return false;
                    }
                    Ok(None) => thread::sleep(Duration::from_millis(50)),
                    Err(_) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        return false;
                    }
                }
            }
        }
        Err(_) => false,
    }
}

/// Returns the lowest-numbered DRM render node, which is the device VAAPI opens.
///
/// Render nodes are character devices, so `Path::is_file` is false for them.
/// Primary `card*` nodes are not used: libva cannot initialise a driver on them.
fn find_vaapi_device() -> Option<std::path::PathBuf> {
    let mut render_nodes: Vec<std::path::PathBuf> = fs::read_dir("/dev/dri")
        .ok()?
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            path.file_name()?
                .to_string_lossy()
                .starts_with("renderD")
                .then_some(path)
        })
        .collect();
    render_nodes.sort();
    render_nodes.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qsv_probe_exercises_nv12_scaling_and_the_render_overlay_graph_for_30_frames() {
        for (index, candidate) in qsv_full_probe_candidates().iter().enumerate() {
            let args = qsv_overlay_probe_args(candidate);
            let init_args: Vec<_> = candidate
                .init_args
                .iter()
                .map(|arg| (*arg).to_string())
                .collect();
            assert!(args[3..].starts_with(&init_args));
            let graph = &args[args
                .iter()
                .position(|arg| arg == "-filter_complex")
                .unwrap()
                + 1];
            assert!(graph.contains("[probe_main]trim=start=0:end=1,setpts=PTS-STARTPTS,scale_qsv=w=128:h=128:format=nv12[base]"));
            assert!(graph.contains("[1:v]setpts=PTS-STARTPTS,crop=112:112:0:0,hwupload=extra_hw_frames=16[slot0];"));
            assert_eq!(graph.matches(",hwupload=extra_hw_frames=16[slot").count(), 1);
            assert!(graph.contains("[base][slot0]overlay_qsv=x=8:y=8[out]"));
            assert!(!graph.contains("hwdownload"));
            if index == 0 {
                assert!(graph.starts_with("[0:v]format=nv12,hwupload[probe_main];"));
            } else {
                assert!(graph.starts_with("[0:v]format=bgra,hwupload=extra_hw_frames=16,scale_qsv=format=nv12[probe_main];"));
            }
            assert!(args.windows(2).any(|pair| pair == ["-frames:v", "30"]));
            assert!(args.windows(2).any(|pair| pair == ["-c:v", "h264_qsv"]));
        }
    }

    #[test]
    fn qsv_selection_stops_after_preferred_backend_succeeds() {
        let mut attempts = Vec::new();
        let selected = select_qsv_full_init_args(|candidate| {
            attempts.push(
                candidate
                    .init_args
                    .iter()
                    .map(|arg| (*arg).to_string())
                    .collect::<Vec<_>>(),
            );
            true
        })
        .unwrap();

        assert_eq!(attempts, vec![selected.clone()]);
        assert_eq!(selected[1], "qsv=qs");
    }

    #[cfg(windows)]
    #[test]
    fn qsv_selection_returns_dxva2_arguments_after_default_backend_fails() {
        let mut attempts = Vec::new();
        let selected = select_qsv_full_init_args(|candidate| {
            attempts.push(
                candidate
                    .init_args
                    .iter()
                    .map(|arg| (*arg).to_string())
                    .collect::<Vec<_>>(),
            );
            attempts.len() == 2
        })
        .unwrap();

        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0][1], "qsv=qs");
        assert_eq!(attempts[1], selected);
        assert_eq!(selected[1], "dxva2=dx");
        assert_eq!(selected[3], "qsv=qs@dx");
    }

    #[test]
    fn qsv_selection_reports_unavailable_when_every_backend_fails() {
        let mut attempts = 0;
        let selected = select_qsv_full_init_args(|_| {
            attempts += 1;
            false
        });

        assert!(selected.is_none());
        assert_eq!(attempts, if cfg!(windows) { 2 } else { 1 });
    }
}
