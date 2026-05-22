use crate::encode::ffmpeg::{resolve_ffmpeg_binary, suppress_child_console};
use crate::error::CoreResult;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const CODEC_PROBE_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Default, Serialize, Deserialize)]
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
    pub qsv_full: bool,
    pub qsv_full_init_args: Vec<String>,
}

pub fn detect_codecs(repo_root: &Path) -> CoreResult<AvailableCodecs> {
    let ffmpeg_path = resolve_ffmpeg_binary(repo_root)?;
    let vaapi_device = find_vaapi_device();

    let prores_ks = probe_codec(
        "prores_ks",
        &ffmpeg_path,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=s=256x256:d=1",
            "-c:v",
            "prores_ks",
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ],
    );
    let prores_ks_vulkan = probe_codec(
        "prores_ks_vulkan",
        &ffmpeg_path,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-init_hw_device",
            "vulkan=vk",
            "-filter_hw_device",
            "vk",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=s=256x256:d=1",
            "-vf",
            "format=yuva444p10le,hwupload",
            "-c:v",
            "prores_ks_vulkan",
            "-profile:v",
            "4",
            "-qscale:v",
            "4",
            "-mbs_per_slice",
            "4",
            "-vendor",
            "apl0",
            "-alpha_bits",
            "16",
            "-async_depth",
            "4",
            "-pix_fmt",
            "vulkan",
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ],
    );
    let prores_videotoolbox = probe_codec(
        "prores_videotoolbox",
        &ffmpeg_path,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=s=256x256:d=1",
            "-c:v",
            "prores_videotoolbox",
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ],
    );
    let qtrle = probe_codec(
        "qtrle",
        &ffmpeg_path,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=s=256x256:d=1",
            "-c:v",
            "qtrle",
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ],
    );

    let libx264 = probe_codec(
        "libx264",
        &ffmpeg_path,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=s=256x256:d=1",
            "-c:v",
            "libx264",
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ],
    );
    let libx265 = probe_codec(
        "libx265",
        &ffmpeg_path,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=s=256x256:d=1",
            "-c:v",
            "libx265",
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ],
    );
    let h264_nvenc = probe_codec(
        "h264_nvenc",
        &ffmpeg_path,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=s=256x256:d=1",
            "-c:v",
            "h264_nvenc",
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ],
    );
    let hevc_nvenc = probe_codec(
        "hevc_nvenc",
        &ffmpeg_path,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=s=256x256:d=1",
            "-c:v",
            "hevc_nvenc",
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ],
    );
    let h264_qsv = probe_codec(
        "h264_qsv",
        &ffmpeg_path,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=s=256x256:d=1",
            "-c:v",
            "h264_qsv",
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ],
    );
    let hevc_qsv = probe_codec(
        "hevc_qsv",
        &ffmpeg_path,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=s=256x256:d=1",
            "-c:v",
            "hevc_qsv",
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ],
    );
    let h264_amf = probe_codec(
        "h264_amf",
        &ffmpeg_path,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=s=256x256:d=1",
            "-c:v",
            "h264_amf",
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ],
    );
    let hevc_amf = probe_codec(
        "hevc_amf",
        &ffmpeg_path,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=s=256x256:d=1",
            "-c:v",
            "hevc_amf",
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ],
    );
    let h264_videotoolbox = probe_codec(
        "h264_videotoolbox",
        &ffmpeg_path,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=s=256x256:d=1",
            "-c:v",
            "h264_videotoolbox",
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ],
    );
    let hevc_videotoolbox = probe_codec(
        "hevc_videotoolbox",
        &ffmpeg_path,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=s=256x256:d=1",
            "-c:v",
            "hevc_videotoolbox",
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ],
    );
    let h264_vaapi = vaapi_device.as_ref().is_some_and(|device_path| {
        let args = vec![
            "-hide_banner".to_string(),
            "-loglevel".to_string(),
            "error".to_string(),
            "-vaapi_device".to_string(),
            device_path.to_string_lossy().to_string(),
            "-f".to_string(),
            "lavfi".to_string(),
            "-i".to_string(),
            "nullsrc=s=256x256:d=1".to_string(),
            "-vf".to_string(),
            "format=nv12,hwupload".to_string(),
            "-c:v".to_string(),
            "h264_vaapi".to_string(),
            "-frames:v".to_string(),
            "1".to_string(),
            "-f".to_string(),
            "null".to_string(),
            "-".to_string(),
        ];
        probe_codec_owned("h264_vaapi", &ffmpeg_path, &args)
    });
    let hevc_vaapi = vaapi_device.as_ref().is_some_and(|device_path| {
        let args = vec![
            "-hide_banner".to_string(),
            "-loglevel".to_string(),
            "error".to_string(),
            "-vaapi_device".to_string(),
            device_path.to_string_lossy().to_string(),
            "-f".to_string(),
            "lavfi".to_string(),
            "-i".to_string(),
            "nullsrc=s=256x256:d=1".to_string(),
            "-vf".to_string(),
            "format=nv12,hwupload".to_string(),
            "-c:v".to_string(),
            "hevc_vaapi".to_string(),
            "-frames:v".to_string(),
            "1".to_string(),
            "-f".to_string(),
            "null".to_string(),
            "-".to_string(),
        ];
        probe_codec_owned("hevc_vaapi", &ffmpeg_path, &args)
    });
    let cuda_h264_nvenc = probe_codec(
        "cuda_h264_nvenc",
        &ffmpeg_path,
        &[
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
            "h264_nvenc",
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ],
    );
    let cuda_hevc_nvenc = probe_codec(
        "cuda_hevc_nvenc",
        &ffmpeg_path,
        &[
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
            "hevc_nvenc",
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ],
    );
    let cuda = cuda_h264_nvenc || cuda_hevc_nvenc;
    let filters = detect_ffmpeg_filters(&ffmpeg_path);
    let overlay_cuda = filters.contains("overlay_cuda");
    let scale_cuda = filters.contains("scale_cuda");
    let scale_qsv = filters.contains("scale_qsv");
    let hwupload_filter = filters.contains("hwupload");
    let overlay_qsv = filters.contains("overlay_qsv");
    let hwdownload_filter = filters.contains("hwdownload");
    let cuda_filter_stack = cuda && overlay_cuda && scale_cuda && hwupload_filter;
    let qsv_filter_stack = overlay_qsv && scale_qsv && hwupload_filter;
    let qsv_full_init_args = if (h264_qsv || hevc_qsv) && qsv_filter_stack {
        detect_qsv_full_init_args(&ffmpeg_path).unwrap_or_default()
    } else {
        Vec::new()
    };
    let qsv_full = !qsv_full_init_args.is_empty();

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
        qsv_full,
        qsv_full_init_args,
    })
}

/// Lists FFmpeg filter names advertised by the bundled FFmpeg binary.
///
/// A failed probe returns an empty set so hardware-only filter profiles are
/// conservatively disabled instead of being shown optimistically.
fn detect_ffmpeg_filters(ffmpeg_path: &Path) -> std::collections::BTreeSet<String> {
    let output = Command::new(ffmpeg_path)
        .args(["-hide_banner", "-filters"])
        .output();
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
    // test seam
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

/// Returns the first QSV hardware-device argument set that can run `overlay_qsv`.
///
/// The probe mirrors the composite render shape with two video inputs, hardware
/// upload for the raw overlay leg, `scale_qsv`, `overlay_qsv`, and a one-frame
/// QSV encode without downloading the filtered frames.
fn detect_qsv_full_init_args(ffmpeg_path: &Path) -> Option<Vec<String>> {
    qsv_full_init_arg_candidates()
        .into_iter()
        .find(|args| probe_qsv_overlay_path(ffmpeg_path, args))
}

/// Lists platform-specific QSV hardware-device initialization candidates.
///
/// Windows tries explicit DXVA2/D3D11 derivation first because adapter binding
/// can differ on systems with both integrated and dedicated GPUs.
fn qsv_full_init_arg_candidates() -> Vec<Vec<String>> {
    let candidates: &[&[&str]] = if cfg!(windows) {
        &[
            &[
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
            &[
                "-init_hw_device",
                "d3d11va=dx",
                "-init_hw_device",
                "qsv=qs@dx",
                "-filter_hw_device",
                "qs",
                "-hwaccel",
                "qsv",
                "-hwaccel_output_format",
                "qsv",
            ],
            &[
                "-init_hw_device",
                "d3d11va=dx:0",
                "-init_hw_device",
                "qsv=qs@dx",
                "-filter_hw_device",
                "qs",
                "-hwaccel",
                "qsv",
                "-hwaccel_output_format",
                "qsv",
            ],
            &[
                "-init_hw_device",
                "d3d11va=dx:1",
                "-init_hw_device",
                "qsv=qs@dx",
                "-filter_hw_device",
                "qs",
                "-hwaccel",
                "qsv",
                "-hwaccel_output_format",
                "qsv",
            ],
            &[
                "-init_hw_device",
                "qsv=qs",
                "-filter_hw_device",
                "qs",
                "-hwaccel",
                "qsv",
                "-hwaccel_output_format",
                "qsv",
            ],
        ]
    } else {
        &[&[
            "-init_hw_device",
            "qsv=qs",
            "-filter_hw_device",
            "qs",
            "-hwaccel",
            "qsv",
            "-hwaccel_output_format",
            "qsv",
        ]]
    };

    candidates
        .iter()
        .map(|candidate| candidate.iter().map(|arg| (*arg).to_string()).collect())
        .collect()
}

/// Probes whether a QSV device can run the performance QSV filter path.
///
/// This is intentionally small but exercises QSV scaling, raw overlay upload,
/// QSV overlay, and QSV encode without a hardware-frame download.
fn probe_qsv_overlay_path(ffmpeg_path: &Path, init_args: &[String]) -> bool {
    let mut args = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
    ];
    args.extend(init_args.iter().cloned());
    args.extend([
        "-f".to_string(),
        "lavfi".to_string(),
        "-i".to_string(),
        "color=c=black:s=128x128:r=30:d=0.1,format=yuv420p".to_string(),
        "-f".to_string(),
        "lavfi".to_string(),
        "-i".to_string(),
        "color=c=red@0.35:s=128x128:r=30:d=0.1,format=rgba".to_string(),
        "-filter_complex".to_string(),
        "[0:v]format=nv12,hwupload=extra_hw_frames=64[main_hw];[1:v]format=bgra,hwupload=extra_hw_frames=64[overlay_hw];[main_hw][overlay_hw]overlay_qsv=x=0:y=0,hwdownload,format=nv12[out]"
            .to_string(),
        "-map".to_string(),
        "[out]".to_string(),
        "-c:v".to_string(),
        "h264_qsv".to_string(),
        "-frames:v".to_string(),
        "1".to_string(),
        "-f".to_string(),
        "null".to_string(),
        "-".to_string(),
    ]);

    probe_codec_owned("qsv_overlay", ffmpeg_path, &args)
}

fn probe_codec(name: &str, ffmpeg_path: &Path, args: &[&str]) -> bool {
    let owned_args = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    probe_codec_owned(name, ffmpeg_path, &owned_args)
}

fn probe_codec_owned(name: &str, ffmpeg_path: &Path, args: &[String]) -> bool {
    let mut command = Command::new(ffmpeg_path);
    command.arg("-nostdin");
    command.args(args);
    suppress_child_console(&mut command);
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

fn find_vaapi_device() -> Option<std::path::PathBuf> {
    let dri_dir = Path::new("/dev/dri");
    if !dri_dir.is_dir() {
        return None;
    }

    let preferred = dri_dir.join("renderD128");
    if preferred.is_file() {
        return Some(preferred);
    }

    let mut entries = fs::read_dir(dri_dir).ok()?;
    entries.find_map(|entry| {
        let path = entry.ok()?.path();
        let name = path.file_name()?.to_string_lossy();
        if name.starts_with("renderD") || name.starts_with("card") {
            Some(path)
        } else {
            None
        }
    })
}
