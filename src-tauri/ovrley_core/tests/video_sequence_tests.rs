//! Real FFmpeg regression for virtual chapters with an unsupported camera data track.
use ovrley_core::media::video_sequence::{input_options, prepare_video_sequence};
use std::{fs, path::PathBuf, process::Command};

#[test]
#[ignore = "requires the bundled FFmpeg and ffprobe"]
fn exports_original_chapters_without_remuxing_camera_data() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let ffmpeg = root.join(if cfg!(windows) {
        "vendor/ffmpeg/bin/ffmpeg.exe"
    } else {
        "vendor/ffmpeg/bin/ffmpeg"
    });
    let temp = std::env::temp_dir().join(format!("ovrley sequence's {}", std::process::id()));
    fs::create_dir_all(&temp).unwrap();
    let payload = temp.join("data.srt");
    fs::write(&payload, "1\n00:00:00,000 --> 00:00:01,000\nCamera data\n").unwrap();
    let first = temp.join("chapter 1.mp4");
    let output = Command::new(&ffmpeg)
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=red:s=64x64:r=25:d=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
            "-i",
        ])
        .arg(&payload)
        .args([
            "-map", "0:v", "-map", "1:a", "-map", "2:0", "-c:v", "libx264", "-c:a", "aac", "-c:s",
            "mov_text", "-t", "1",
        ])
        .arg(&first)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Reproduce a camera track that FFmpeg can read but cannot write to MP4.
    let mut original = fs::read(&first).unwrap();
    let tag = original
        .windows(4)
        .position(|bytes| bytes == b"tx3g")
        .unwrap();
    original[tag..tag + 4].copy_from_slice(b"djmd");
    let handler = original
        .windows(4)
        .position(|bytes| bytes == b"sbtl")
        .unwrap();
    original[handler..handler + 4].copy_from_slice(b"meta");
    fs::write(&first, &original).unwrap();
    let second = temp.join("chapter 2.mp4");
    fs::copy(&first, &second).unwrap();
    let third = temp.join("chapter 3.mp4");
    fs::copy(&first, &third).unwrap();
    let paths: Vec<_> = [&first, &second, &third]
        .iter()
        .map(|path| path.to_str().unwrap().to_owned())
        .collect();
    let manifest = temp.join("sequence.ffconcat");
    if manifest.exists() {
        fs::remove_file(&manifest).unwrap();
    }
    let runtime = ovrley_core::paths::AppPaths::from_repo_root(root.clone());
    let sequence = prepare_video_sequence(&runtime, &paths, &manifest).unwrap();
    assert_eq!(sequence.metadata.duration, Some(3.0));
    assert_eq!(sequence.parts[2].start_seconds, 2.0);
    assert!(fs::metadata(&manifest).unwrap().len() < 2048);

    let old = Command::new(&ffmpeg)
        .args(["-y", "-v", "error"])
        .args(input_options(&manifest))
        .arg("-i")
        .arg(&manifest)
        .args(["-map", "0", "-copy_unknown", "-c", "copy"])
        .arg(temp.join("old-merge.mp4"))
        .output()
        .unwrap();
    assert!(
        !old.status.success(),
        "fixture must reproduce the old merge failure"
    );
    assert!(
        String::from_utf8_lossy(&old.stderr).contains("not currently supported in container"),
        "{}",
        String::from_utf8_lossy(&old.stderr)
    );

    // Seek into the first chapter and export across both chapter boundaries.
    let rendered = temp.join("export.mp4");
    let output = Command::new(&ffmpeg)
        .args(["-y", "-v", "error", "-ss", "0.5"])
        .args(input_options(&manifest))
        .arg("-i")
        .arg(&manifest)
        .args([
            "-t", "2", "-map", "0:v:0", "-map", "0:a:0", "-c:v", "libx264", "-c:a", "aac",
        ])
        .arg(&rendered)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let ffprobe = ffmpeg.with_file_name(if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    });
    let probe = Command::new(ffprobe)
        .args(["-v", "error", "-show_streams", "-of", "json"])
        .arg(rendered)
        .output()
        .unwrap();
    assert!(probe.status.success());
    let data: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
    let streams = data["streams"].as_array().unwrap();
    assert_eq!(streams.len(), 2);
    assert_eq!(streams[0]["nb_frames"], "50");
    assert_eq!(streams[1]["codec_type"], "audio");
    for path in paths {
        assert_eq!(fs::read(path).unwrap(), original);
    }
    fs::remove_dir_all(&temp).unwrap();
}
