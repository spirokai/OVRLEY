//! FFmpeg subprocess probes used by codec detection.

use crate::encode::ffmpeg::binary::configure_ffmpeg_command;
use crate::encode::ffmpeg::catalog::{
    CompositeCodecId, EncoderMetadata, ProbeKind, TransparentCodecId,
};
use crate::encode::ffmpeg::composite_filters::composite_filter_complex;
use crate::encode::ffmpeg::composite_profiles::composite_profile;
use crate::encode::ffmpeg::transparent_profiles::transparent_profile;
use crate::render::{AtlasPlan, FrameSize};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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
pub(super) fn detect_qsv_full_init_args(ffmpeg_path: &Path) -> Option<Vec<String>> {
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
/// graph and direct hardware-frame encoding for 30 frames without video decode.
/// Main-frame preparation belongs to each candidate: the preferred path uploads
/// NV12 directly, while DXVA2 uses a fixed BGRA upload and GPU conversion.
/// The live render graph owns the overlay upload's 16-frame pool on both paths.
fn probe_qsv_overlay_path(ffmpeg_path: &Path, candidate: &QsvProbeCandidate) -> bool {
    probe_codec(
        "qsv_overlay",
        ffmpeg_path,
        &qsv_overlay_probe_args(candidate),
    )
}

fn qsv_overlay_probe_args(candidate: &QsvProbeCandidate) -> Vec<String> {
    let render_graph = composite_filter_complex(
        128,
        128,
        &AtlasPlan::full_frame(
            FrameSize {
                width: 128,
                height: 128,
            },
            0,
        ),
        0.0,
        1.0,
        composite_profile(CompositeCodecId::QsvFullH264),
        None,
        None,
        None,
        None,
    )
    .expect("fixed QSV probe dimensions and timing produce a valid render graph");
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
        "color=c=red@0.35:s=128x128:r=30:d=1,format=rgba".to_string(),
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
            assert!(graph.contains("[probe_main]trim=start=0:end=1,setpts=PTS-STARTPTS,scale_qsv=w=128:h=128:format=nv12[main_hw]"));
            assert!(
                graph.contains("[1:v]setpts=PTS-STARTPTS,hwupload=extra_hw_frames=16[overlay_hw]")
            );
            assert!(graph.contains("[main_hw][overlay_hw]overlay_qsv=x=0:y=0[out]"));
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
