use crate::encode::ffmpeg::{resolve_ffmpeg_binary, suppress_child_console};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableCodecs {
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
    let encoders = run_ffmpeg_list(&ffmpeg_path, "-encoders")?;
    let hwaccels = run_ffmpeg_list(&ffmpeg_path, "-hwaccels")?;

    let encoder_names = parse_encoder_names(&encoders);
    let hwaccel_names = parse_hwaccel_names(&hwaccels);

    let h264_nvenc = encoder_names.contains("h264_nvenc");
    let hevc_nvenc = encoder_names.contains("hevc_nvenc");
    let cuda = hwaccel_names.contains("cuda");
    let nvdec = hwaccel_names.contains("nvdec");
    let qsv = hwaccel_names.contains("qsv");
    let vaapi = hwaccel_names.contains("vaapi");
    let videotoolbox = hwaccel_names.contains("videotoolbox");

    Ok(AvailableCodecs {
        libx264: encoder_names.contains("libx264"),
        libx265: encoder_names.contains("libx265"),
        h264_nvenc,
        hevc_nvenc,
        h264_qsv: encoder_names.contains("h264_qsv"),
        hevc_qsv: encoder_names.contains("hevc_qsv"),
        h264_vaapi: encoder_names.contains("h264_vaapi"),
        hevc_vaapi: encoder_names.contains("hevc_vaapi"),
        h264_amf: encoder_names.contains("h264_amf"),
        hevc_amf: encoder_names.contains("hevc_amf"),
        h264_videotoolbox: encoder_names.contains("h264_videotoolbox"),
        hevc_videotoolbox: encoder_names.contains("hevc_videotoolbox"),
        cuda,
        nvdec,
        qsv,
        vaapi,
        videotoolbox,
        nvgpu: (h264_nvenc || hevc_nvenc) && (nvdec || cuda),
        nnvgpu: (h264_nvenc || hevc_nvenc) && cuda,
    })
}

fn run_ffmpeg_list(ffmpeg_path: &Path, list_arg: &str) -> Result<String, String> {
    let mut command = Command::new(ffmpeg_path);
    command.arg(list_arg);
    suppress_child_console(&mut command);

    let output = command
        .output()
        .map_err(|error| format!("Failed to run ffmpeg {list_arg}: {error}"))?;

    if !output.status.success() {
        return Err(format!(
            "ffmpeg {list_arg} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    if text.is_empty() {
        text = String::from_utf8_lossy(&output.stderr).to_string();
    }
    Ok(text)
}

fn parse_encoder_names(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if !trimmed.starts_with('V') {
                return None;
            }

            trimmed
                .split_whitespace()
                .nth(1)
                .map(|name| name.to_ascii_lowercase())
        })
        .collect()
}

fn parse_hwaccel_names(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                && !line.starts_with("Hardware")
                && !line.starts_with("ffmpeg")
                && !line.starts_with("configuration:")
                && !line.starts_with("lib")
        })
        .map(|line| line.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{parse_encoder_names, parse_hwaccel_names};

    #[test]
    fn parses_encoder_names_from_ffmpeg_listing() {
        let names = parse_encoder_names(
            "Encoders:\n V....D libx264             libx264 H.264\n V..... h264_nvenc          NVIDIA NVENC H.264\n A..... aac\n",
        );

        assert!(names.contains("libx264"));
        assert!(names.contains("h264_nvenc"));
        assert!(!names.contains("aac"));
    }

    #[test]
    fn parses_hwaccel_names_from_ffmpeg_listing() {
        let names = parse_hwaccel_names("Hardware acceleration methods:\ncuda\nqsv\nvaapi\n");

        assert!(names.contains("cuda"));
        assert!(names.contains("qsv"));
        assert!(names.contains("vaapi"));
    }
}
