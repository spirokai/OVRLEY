use crate::encode::ffmpeg::{resolve_ffmpeg_binary, suppress_child_console};
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
}

pub fn detect_codecs(repo_root: &Path) -> Result<AvailableCodecs, String> {
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
            "-mbs_per_slice",
            "2",
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
        nnvgpu: cuda,
    })
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

#[cfg(test)]
mod tests {}
