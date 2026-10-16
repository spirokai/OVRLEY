//! Multi-piece batch outputs through the native executor and live FFmpeg:
//! parts render, join by stream copy, carry one audio pass and are verified.
//!
//! ## Type
//! Integration test. Requires live ffmpeg and ffprobe in `vendor/ffmpeg/bin/`.
//! Sources are tiny synthetic clips generated per test.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use ovrley_core::activity::schema::ParsedActivity;
use ovrley_core::debug::RenderProgress;
use ovrley_core::encode::progress::ProgressSink;
use ovrley_core::media::prepared_video::InspectedVideoSource;
use ovrley_core::paths::AppPaths;
use ovrley_core::render_jobs::contracts::*;
use ovrley_core::render_jobs::execution::RenderExecutionService;
use ovrley_core::render_jobs::inspection::VideoInspectionService;
use serde_json::{json, Value};

const WAIT: Duration = Duration::from_secs(120);

struct TerminalSink(Mutex<mpsc::Sender<BatchSnapshot>>);

impl ProgressSink for TerminalSink {
    fn emit_progress(&self, _: &RenderProgress) {}
    fn emit_batch_progress(&self, event: &BatchRenderEvent) {
        if let BatchRenderEvent::Snapshot(snapshot) = event {
            if !snapshot.renderer_busy {
                self.0.lock().unwrap().send(snapshot.clone()).unwrap();
            }
        }
    }
}

struct Workspace {
    paths: AppPaths,
    ffmpeg: PathBuf,
    inspection: VideoInspectionService,
    inspection_id: String,
}

impl Workspace {
    fn new(name: &str) -> Self {
        let paths = common::composite::test_paths_named(name);
        for entry in std::fs::read_dir(&paths.downloads_dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                std::fs::remove_dir_all(path).unwrap();
            } else {
                std::fs::remove_file(path).unwrap();
            }
        }
        let ffmpeg =
            ovrley_core::encode::ffmpeg::binary::resolve_ffmpeg_binary(&paths.repo_root).unwrap();
        let inspection = VideoInspectionService::default();
        let inspection_id = inspection.create_session().inspection_id;
        Self {
            paths,
            ffmpeg,
            inspection,
            inspection_id,
        }
    }

    /// A 64x32 clip at 30000/1001 FPS, with a mono 44.1 kHz tone when `audio`.
    fn source(&self, name: &str, seconds: f64, audio: bool) -> InspectedVideoSource {
        let path = self.paths.temp_dir.join(name);
        let mut command = Command::new(&self.ffmpeg);
        command.args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc2=size=64x32:rate=30000/1001:duration={seconds}"),
        ]);
        if audio {
            command.args([
                "-f",
                "lavfi",
                "-i",
                &format!("sine=frequency=440:sample_rate=44100:duration={seconds}"),
            ]);
        }
        let generated = command
            .args(["-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            generated.status.success(),
            "{}",
            String::from_utf8_lossy(&generated.stderr)
        );
        self.inspection
            .inspect_source(&self.paths, &self.inspection_id, path.to_str().unwrap())
            .unwrap()
    }

    fn request(
        &self,
        export_mode: BatchExportMode,
        destination: BatchDestination,
        outputs: Vec<BatchRenderOutput>,
    ) -> BatchRenderRequest {
        let activity: ParsedActivity = serde_json::from_value(json!({
            "sample_elapsed_seconds":[0,120], "trim_end_seconds":120, "speed":[0,120],
            "sync_time":"2026-05-20T12:00:00Z"
        }))
        .unwrap();
        let offsets = outputs
            .iter()
            .flat_map(|output| &output.pieces)
            .map(|piece| (piece.source_id.clone(), 5.0))
            .collect();
        BatchRenderRequest {
            inspection_id: self.inspection_id.clone(),
            template: common::builders::batch_template(),
            encoding: BatchEncodingSettings {
                export_mode,
                export_codec: match export_mode {
                    BatchExportMode::Composite => "libx264",
                    BatchExportMode::Transparent => "qtrle",
                }
                .into(),
                fps: 30,
                update_rate: match export_mode {
                    BatchExportMode::Composite => 1,
                    BatchExportMode::Transparent => 2,
                },
                quality_type: ovrley_core::encode::quality::QualityType::Bitrate,
                quality_value: 1.0,
                qsv_full_init_args: None,
            },
            activity: BatchActivity::ExternalActivity { activity, offsets },
            destination,
            outputs,
        }
    }

    fn render(&self, request: BatchRenderRequest) -> BatchSnapshot {
        let (send, terminal) = mpsc::channel();
        let service = RenderExecutionService::with_sink(Arc::new(TerminalSink(Mutex::new(send))));
        service
            .submit_batch(&self.paths, &self.inspection, request, None)
            .unwrap();
        terminal.recv_timeout(WAIT).unwrap()
    }

    fn parts_directories(&self) -> Vec<PathBuf> {
        std::fs::read_dir(&self.paths.downloads_dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.is_dir())
            .collect()
    }

    fn probe_streams(&self, path: &Path) -> Vec<Value> {
        let probed = Command::new(self.ffmpeg.with_file_name(if cfg!(windows) {
            "ffprobe.exe"
        } else {
            "ffprobe"
        }))
        .args([
            "-v",
            "error",
            "-count_packets",
            "-show_entries",
            "stream=codec_type,codec_name,codec_tag_string,nb_read_packets,duration,sample_rate,channels",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .unwrap();
        assert!(probed.status.success());
        serde_json::from_slice::<Value>(&probed.stdout).unwrap()["streams"]
            .as_array()
            .unwrap()
            .clone()
    }
}

fn piece(source: &InspectedVideoSource, start_frame: u64, end_frame: u64) -> BatchRenderPiece {
    BatchRenderPiece {
        source_id: source.source_id.clone(),
        start_frame,
        end_frame,
        skip_overlay: false,
    }
}

fn stream<'a>(streams: &'a [Value], kind: &str) -> Vec<&'a Value> {
    streams
        .iter()
        .filter(|stream| stream["codec_type"] == kind)
        .collect()
}

fn seconds(value: &Value) -> f64 {
    value.as_str().unwrap().parse().unwrap()
}

#[test]
fn a_combined_composite_joins_pieces_of_sources_with_and_without_audio() {
    let workspace = Workspace::new("batch-join-composite");
    let first = workspace.source("first.mp4", 3.0, true);
    let silent = workspace.source("silent.mp4", 2.0, false);
    let last = workspace.source("last.mp4", 2.0, true);
    let target = workspace.paths.downloads_dir.join("joined.mp4");
    // 45 + 60 + 30 frames; the silent and last pieces reach their sources' ends.
    let request = workspace.request(
        BatchExportMode::Composite,
        BatchDestination::File {
            path: target.to_str().unwrap().into(),
            overwrite: false,
        },
        vec![BatchRenderOutput {
            id: "joined".into(),
            pieces: vec![
                piece(&first, 15, 60),
                piece(&silent, 0, 60),
                piece(&last, 30, 60),
            ],
        }],
    );
    let snapshot = workspace.render(request);
    assert_eq!(
        snapshot.phase,
        BatchPhase::Completed,
        "{:?}",
        snapshot.items[0].outcome
    );
    assert_eq!(snapshot.items[0].planned_frames, 135);
    let streams = workspace.probe_streams(&target);
    let video = stream(&streams, "video");
    assert_eq!(video.len(), 1);
    assert_eq!(video[0]["codec_name"], "h264");
    assert_eq!(video[0]["nb_read_packets"], "135");
    let video_seconds = 135.0 * 1001.0 / 30000.0;
    assert!((seconds(&video[0]["duration"]) - video_seconds).abs() < 0.01);
    let audio = stream(&streams, "audio");
    assert_eq!(audio.len(), 1);
    assert_eq!(audio[0]["codec_name"], "aac");
    assert_eq!(audio[0]["sample_rate"], "48000");
    assert_eq!(audio[0]["channels"], 2);
    // One audio pass, as long as the video (within one AAC frame).
    assert!(
        (seconds(&audio[0]["duration"]) - video_seconds).abs() < 0.03,
        "audio {} s, video {video_seconds} s",
        audio[0]["duration"]
    );
    assert!(workspace.parts_directories().is_empty());
}

#[test]
fn joined_outputs_without_any_audio_have_no_audio_track() {
    let workspace = Workspace::new("batch-join-silent");
    let silent = workspace.source("silent.mp4", 2.0, false);
    let request = workspace.request(
        BatchExportMode::Composite,
        BatchDestination::SourceNamed {
            directory: workspace.paths.downloads_dir.to_str().unwrap().into(),
        },
        vec![BatchRenderOutput {
            id: "silent".into(),
            // A deleted part between frames 20 and 40.
            pieces: vec![piece(&silent, 0, 20), piece(&silent, 40, 60)],
        }],
    );
    let snapshot = workspace.render(request);
    assert_eq!(
        snapshot.phase,
        BatchPhase::Completed,
        "{:?}",
        snapshot.items[0].outcome
    );
    let streams = workspace.probe_streams(&workspace.paths.downloads_dir.join("silent_video.mp4"));
    assert_eq!(stream(&streams, "video")[0]["nb_read_packets"], "40");
    assert!(stream(&streams, "audio").is_empty());
    assert!(workspace.parts_directories().is_empty());
}

#[test]
fn a_separate_transparent_output_joins_its_pieces_as_mov() {
    let workspace = Workspace::new("batch-join-transparent");
    let source = workspace.source("ride.mp4", 3.0, true);
    let request = workspace.request(
        BatchExportMode::Transparent,
        BatchDestination::SourceNamed {
            directory: workspace.paths.downloads_dir.to_str().unwrap().into(),
        },
        vec![BatchRenderOutput {
            id: "ride".into(),
            pieces: vec![piece(&source, 0, 30), piece(&source, 60, 90)],
        }],
    );
    let snapshot = workspace.render(request);
    assert_eq!(
        snapshot.phase,
        BatchPhase::Completed,
        "{:?}",
        snapshot.items[0].outcome
    );
    let planned = snapshot.items[0].planned_frames;
    let streams = workspace.probe_streams(&workspace.paths.downloads_dir.join("ride_overlay.mov"));
    let video = stream(&streams, "video");
    assert_eq!(video[0]["codec_name"], "qtrle");
    assert_eq!(video[0]["nb_read_packets"], planned.to_string());
    assert!(stream(&streams, "audio").is_empty());
    assert!(workspace.parts_directories().is_empty());
}

#[test]
fn joined_hevc_keeps_the_apple_compatible_sample_entry() {
    let workspace = Workspace::new("batch-join-hevc");
    let source = workspace.source("ride.mp4", 2.0, true);
    let mut request = workspace.request(
        BatchExportMode::Composite,
        BatchDestination::SourceNamed {
            directory: workspace.paths.downloads_dir.to_str().unwrap().into(),
        },
        vec![BatchRenderOutput {
            id: "ride".into(),
            pieces: vec![piece(&source, 0, 20), piece(&source, 40, 60)],
        }],
    );
    request.encoding.export_codec = "libx265".into();
    let snapshot = workspace.render(request);
    assert_eq!(
        snapshot.phase,
        BatchPhase::Completed,
        "{:?}",
        snapshot.items[0].outcome
    );
    let streams = workspace.probe_streams(&workspace.paths.downloads_dir.join("ride_video.mp4"));
    let video = stream(&streams, "video");
    assert_eq!(video[0]["codec_name"], "hevc");
    assert_eq!(video[0]["codec_tag_string"], "hvc1");
    assert_eq!(video[0]["nb_read_packets"], "40");
    assert_eq!(stream(&streams, "audio").len(), 1);
}
