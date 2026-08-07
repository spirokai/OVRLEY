//! FFmpeg subprocess probes used by codec detection.

use crate::encode::ffmpeg::binary::configure_ffmpeg_command;
use crate::encode::ffmpeg::catalog::{EncoderMetadata, ProbeKind, TransparentCodecId};
use crate::encode::ffmpeg::transparent_profiles::transparent_profile;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const CODEC_PROBE_TIMEOUT: Duration = Duration::from_secs(8);

/// Returns the first QSV hardware-device argument set that can run `overlay_qsv`.
///
/// The probe mirrors the composite render shape with two video inputs, hardware
/// upload for the raw overlay leg, `scale_qsv`, `overlay_qsv`, and a one-frame
/// QSV encode without downloading the filtered frames.
pub(super) fn detect_qsv_full_init_args(ffmpeg_path: &Path) -> Option<Vec<String>> {
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

    probe_codec("qsv_overlay", ffmpeg_path, &args)
}

/// Executes the probe strategy declared by the unique encoder catalog entry.
pub(super) fn probe_encoder(
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

pub(super) fn probe_codec(name: &str, ffmpeg_path: &Path, args: &[String]) -> bool {
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

pub(super) fn find_vaapi_device() -> Option<std::path::PathBuf> {
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
