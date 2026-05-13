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
    pub codec_name: Option<String>,
    pub codec_long_name: Option<String>,
    pub codec_profile: Option<String>,
    pub pix_fmt: Option<String>,
    pub bits_per_raw_sample: Option<u32>,
    pub has_audio: bool,
    pub container_format: Option<String>,
    pub rotation_degrees: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u64,
    pub height: u64,
}

pub fn probe_video(repo_root: &Path, file_path: &str) -> Result<VideoMetadata, String> {
    let ffmpeg_path = resolve_ffmpeg_binary(repo_root)?;
    let ffprobe_name = if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    };
    let ffprobe_path = ffmpeg_path.with_file_name(ffprobe_name);

    let mut command = Command::new(ffprobe_path);
    command.args([
        "-v",
        "quiet",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
        file_path,
    ]);
    suppress_child_console(&mut command);

    let output = command
        .output()
        .map_err(|e| format!("Failed to run ffprobe: {e}"))?;

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
        codec_name: None,
        codec_long_name: None,
        codec_profile: None,
        pix_fmt: None,
        bits_per_raw_sample: None,
        has_audio: false,
        container_format: None,
        rotation_degrees: None,
    };

    let format = json.get("format");
    let streams = json.get("streams").and_then(|v| v.as_array());

    if let Some(format_obj) = format {
        if let Some(duration_str) = format_obj.get("duration").and_then(|v| v.as_str()) {
            metadata.duration = duration_str.parse::<f64>().ok();
        }
        metadata.container_format = format_obj
            .get("format_name")
            .and_then(|v| v.as_str())
            .map(|value| value.to_string());
    }

    let mut video_stream = None;
    if let Some(streams) = streams {
        for stream in streams {
            if stream.get("codec_type").and_then(|v| v.as_str()) == Some("video") {
                video_stream = Some(stream);
                break;
            }
        }

        metadata.has_audio = streams
            .iter()
            .any(|stream| stream.get("codec_type").and_then(|v| v.as_str()) == Some("audio"));
    }

    if let Some(stream) = video_stream {
        metadata.codec_name = stream
            .get("codec_name")
            .and_then(|v| v.as_str())
            .map(|value| value.to_string());
        metadata.codec_long_name = stream
            .get("codec_long_name")
            .and_then(|v| v.as_str())
            .map(|value| value.to_string());
        metadata.codec_profile = stream
            .get("profile")
            .and_then(|v| v.as_str())
            .map(|value| value.to_string());
        metadata.pix_fmt = stream
            .get("pix_fmt")
            .and_then(|v| v.as_str())
            .map(|value| value.to_string());
        metadata.bits_per_raw_sample = stream
            .get("bits_per_raw_sample")
            .and_then(|v| v.as_str())
            .and_then(|value| value.parse::<u32>().ok());
        metadata.rotation_degrees = read_rotation_degrees(stream);

        if let (Some(w), Some(h)) = (
            stream.get("width").and_then(|v| v.as_u64()),
            stream.get("height").and_then(|v| v.as_u64()),
        ) {
            metadata.resolution = Some(Resolution {
                width: w,
                height: h,
            });
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

fn read_rotation_degrees(stream: &Value) -> Option<i32> {
    stream
        .get("tags")
        .and_then(|tags| tags.get("rotate"))
        .and_then(|value| value.as_str())
        .and_then(|value| value.parse::<i32>().ok())
        .or_else(|| {
            stream
                .get("side_data_list")
                .and_then(|value| value.as_array())
                .and_then(|items| {
                    items.iter().find_map(|item| {
                        item.get("rotation")
                            .and_then(|value| value.as_f64())
                            .map(|value| value.round() as i32)
                    })
                })
        })
}
