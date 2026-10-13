//! Virtual camera chapters: original media plus a small FFmpeg concat manifest.
//! No media is remuxed and camera data streams never enter an MP4 muxer at import.

use crate::encode::ffmpeg::binary::{configure_ffmpeg_command, resolve_ffmpeg_binary};
use crate::media::{prepared_video::probe_video_metadata, SourceVideoMetadata};
use crate::paths::AppPaths;
use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::HashSet, fs::OpenOptions, io::Write, path::Path, process::Command};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoPart {
    pub path: String,
    pub duration: f64,
    pub start_seconds: f64,
}

#[derive(Serialize)]
pub struct VideoSequence {
    #[serde(flatten)]
    pub metadata: SourceVideoMetadata,
    pub parts: Vec<VideoPart>,
}

/// Adds demuxer options only for our virtual sequence file format.
pub fn input_options(path: &Path) -> Vec<String> {
    if path.extension().and_then(|extension| extension.to_str()) == Some("ffconcat") {
        ["-f", "concat", "-safe", "0"].map(String::from).to_vec()
    } else {
        Vec::new()
    }
}

fn concat_entry(path: &str, duration: f64) -> Result<String, String> {
    if path.contains(['\r', '\n', '\0']) {
        return Err("Video paths must not contain line breaks or NUL characters".into());
    }
    let path = if cfg!(windows) {
        path.replace('\\', "/")
    } else {
        path.to_string()
    };
    Ok(format!(
        "file '{}'\nduration {duration:.9}\n",
        path.replace('\'', "'\\''")
    ))
}

fn stream_signature(root: &Path, path: &str) -> Result<Vec<Value>, String> {
    let ffmpeg = resolve_ffmpeg_binary(root).map_err(|error| error.to_string())?;
    let mut command = Command::new(ffmpeg.with_file_name(if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    }));
    configure_ffmpeg_command(&mut command);
    let output = command
        .args([
            "-v",
            "error",
            "-show_streams",
            "-show_data_hash",
            "sha256",
            "-of",
            "json",
            path,
        ])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "Cannot inspect {path}: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let data: Value = serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())?;
    let streams = data["streams"]
        .as_array()
        .ok_or("ffprobe did not return streams")?;
    // Compare AV stream indices too: concat must not mistake a data track for audio.
    // Unknown camera data codecs are deliberately ignored; export maps only video/audio.
    Ok(streams
        .iter()
        .filter(|stream| stream["codec_type"] == "video" || stream["codec_type"] == "audio")
        .map(|stream| {
            let fields = [
                "index",
                "codec_type",
                "codec_name",
                "codec_tag_string",
                "width",
                "height",
                "pix_fmt",
                "time_base",
                "r_frame_rate",
                "sample_rate",
                "channels",
                "channel_layout",
                "extradata_hash",
            ];
            let mut signature = serde_json::Map::new();
            for field in fields {
                signature.insert(field.into(), stream[field].clone());
            }
            let rotations: Vec<_> = stream["side_data_list"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|data| data.get("rotation").cloned())
                .collect();
            signature.insert("rotation".into(), json!(rotations));
            Value::Object(signature)
        })
        .collect())
}

/// Validates ordered original files and writes only a small internal manifest.
pub fn prepare_video_sequence(
    runtime: &AppPaths,
    paths: &[String],
    manifest: &Path,
) -> Result<VideoSequence, String> {
    if paths.len() < 2 {
        return Err("Select at least two MP4 parts".into());
    }
    let mut seen = HashSet::new();
    let mut reference = None;
    let mut first = None;
    let mut parts = Vec::new();
    let mut contents = String::from("ffconcat version 1.0\n");
    let mut duration = 0.0;
    for path in paths {
        let source = Path::new(path);
        if !source.is_absolute()
            || !source.is_file()
            || !source
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("mp4"))
        {
            return Err(format!("Not an absolute MP4 file: {path}"));
        }
        if !seen.insert(std::fs::canonicalize(source).map_err(|error| error.to_string())?) {
            return Err(format!("Duplicate video part: {path}"));
        }
        let metadata = probe_video_metadata(runtime, path).map_err(|error| error.to_string())?;
        let part_duration = metadata
            .duration
            .filter(|value| value.is_finite() && *value > 0.0)
            .ok_or_else(|| format!("Invalid video duration: {path}"))?;
        if metadata.resolution.is_none()
            || !metadata.fps.is_some_and(|fps| fps.is_finite() && fps > 0.0)
        {
            return Err(format!("Missing video resolution or frame rate: {path}"));
        }
        let signature = stream_signature(&runtime.repo_root, path)?;
        if reference
            .as_ref()
            .is_some_and(|expected| expected != &signature)
        {
            return Err(format!("Incompatible video/audio settings in part: {path}"));
        }
        if first.is_none() {
            first = Some(metadata);
            reference = Some(signature);
        }
        contents.push_str(&concat_entry(path, part_duration)?);
        parts.push(VideoPart {
            path: path.clone(),
            duration: part_duration,
            start_seconds: duration,
        });
        duration += part_duration;
    }
    let mut metadata = first.ok_or("Video sequence is empty")?;
    metadata.path = manifest
        .to_str()
        .ok_or("Invalid manifest path")?
        .to_string();
    metadata.duration = Some(duration);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(manifest)
        .map_err(|error| error.to_string())?;
    if let Err(error) = file.write_all(contents.as_bytes()) {
        drop(file);
        let _ = std::fs::remove_file(manifest);
        return Err(error.to_string());
    }
    Ok(VideoSequence { metadata, parts })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quotes_paths_and_rejects_directive_injection() {
        assert_eq!(
            concat_entry("/camera's files/one.mp4", 2.5).unwrap(),
            "file '/camera'\\''s files/one.mp4'\nduration 2.500000000\n"
        );
        assert!(concat_entry("/camera\nfile other.mp4", 1.0).is_err());
    }
    #[test]
    fn only_sequences_get_concat_demuxer_options() {
        assert!(input_options(Path::new("movie.mp4")).is_empty());
        assert_eq!(
            input_options(Path::new("group.ffconcat")),
            ["-f", "concat", "-safe", "0"]
        );
    }
}
