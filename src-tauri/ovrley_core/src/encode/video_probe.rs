use crate::encode::ffmpeg::{resolve_ffmpeg_binary, suppress_child_console};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoMetadata {
    pub path: String,
    pub duration: Option<f64>,
    pub fps: Option<f64>,
    pub resolution: Option<Resolution>,
    pub creation_time: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u64,
    pub height: u64,
}

pub fn probe_video(repo_root: &Path, file_path: &str) -> Result<VideoMetadata, String> {
    let ffmpeg_path = resolve_ffmpeg_binary(repo_root)?;
    let ffprobe_name = if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" };
    let ffprobe_path = ffmpeg_path.with_file_name(ffprobe_name);

    let mut command = Command::new(ffprobe_path);
    command.args([
        "-v", "quiet",
        "-print_format", "json",
        "-show_format",
        "-show_streams",
        file_path,
    ]);
    suppress_child_console(&mut command);

    let output = command.output().map_err(|e| format!("Failed to run ffprobe: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let json: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse ffprobe JSON: {e}"))?;

    let mut metadata = VideoMetadata {
        path: file_path.to_string(),
        duration: None,
        fps: None,
        resolution: None,
        creation_time: None,
    };

    let format = json.get("format");
    let streams = json.get("streams").and_then(|v| v.as_array());

    if let Some(format_obj) = format {
        if let Some(duration_str) = format_obj.get("duration").and_then(|v| v.as_str()) {
            metadata.duration = duration_str.parse::<f64>().ok();
        }
    }

    let mut video_stream = None;
    if let Some(streams) = streams {
        for stream in streams {
            if stream.get("codec_type").and_then(|v| v.as_str()) == Some("video") {
                video_stream = Some(stream);
                break;
            }
        }
    }

    if let Some(stream) = video_stream {
        if let (Some(w), Some(h)) = (
            stream.get("width").and_then(|v| v.as_u64()),
            stream.get("height").and_then(|v| v.as_u64()),
        ) {
            metadata.resolution = Some(Resolution { width: w, height: h });
        }

        if let Some(avg_frame_rate) = stream.get("avg_frame_rate").and_then(|v| v.as_str()) {
            let parts: Vec<&str> = avg_frame_rate.split('/').collect();
            if parts.len() == 2 {
                if let (Ok(num), Ok(den)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                    if den != 0.0 {
                        metadata.fps = Some(num / den);
                    }
                }
            }
        }
    }

    // println!("[OVRLEY] Probing video: {}", file_path);

    // Priority order:
    // 1. format.tags.creation_time
    let format_creation_time = format
        .and_then(|f| f.get("tags"))
        .and_then(|t| t.get("creation_time"))
        .and_then(|v| v.as_str());
    // if let Some(t) = format_creation_time { println!("[OVRLEY] Found format.tags.creation_time: {}", t); }

    // 2. streams[0].tags.creation_time
    let stream_creation_time = video_stream
        .and_then(|s| s.get("tags"))
        .and_then(|t| t.get("creation_time"))
        .and_then(|v| v.as_str());
    // if let Some(t) = stream_creation_time { println!("[OVRLEY] Found streams[0].tags.creation_time: {}", t); }

    // 3. format.tags.com.apple.quicktime.creationdate
    let apple_creation_date = format
        .and_then(|f| f.get("tags"))
        .and_then(|t| t.get("com.apple.quicktime.creationdate"))
        .and_then(|v| v.as_str());
    // if let Some(t) = apple_creation_date { println!("[OVRLEY] Found format.tags.com.apple.quicktime.creationdate: {}", t); }

    metadata.creation_time = format_creation_time
        .or(stream_creation_time)
        .or(apple_creation_date)
        .map(|s| s.to_string());

    if metadata.creation_time.is_none() {
        // println!("[OVRLEY] No creation time found in video metadata. Using file system modified time as fallback.");
        if let Ok(fs_meta) = std::fs::metadata(file_path) {
            if let Ok(modified) = fs_meta.modified() {
                let dt: chrono::DateTime<chrono::Utc> = modified.into();
                let rfc3339 = dt.to_rfc3339();
                // println!("[OVRLEY] Fallback file modified time: {}", rfc3339);
                metadata.creation_time = Some(rfc3339);
            }
        }
    }

    // println!("[OVRLEY] Final selected creation time: {:?}", metadata.creation_time);

    Ok(metadata)
}
