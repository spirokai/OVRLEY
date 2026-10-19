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
            transitions: common::builders::hard_cuts(3),
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
            transitions: common::builders::hard_cuts(2),
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
            transitions: common::builders::hard_cuts(2),
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
            transitions: common::builders::hard_cuts(2),
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

fn xfade(name: &str, seconds: f64) -> BatchTransition {
    BatchTransition {
        transition_type: name.into(),
        duration_seconds: seconds,
    }
}

#[test]
fn transitions_shorten_the_joined_video_and_audio_by_their_durations() {
    let workspace = Workspace::new("batch-join-transitions");
    let first = workspace.source("first.mp4", 3.0, true);
    let silent = workspace.source("silent.mp4", 2.0, false);
    let last = workspace.source("last.mp4", 2.0, true);
    let target = workspace.paths.downloads_dir.join("joined.mp4");
    // 45 + 60 + 30 frames; a 15-frame wipe, then a hard cut.
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
            transitions: vec![xfade("wipeleft", 0.5), xfade("none", 0.0)],
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
    assert_eq!(video[0]["nb_read_packets"], "120");
    let video_seconds = 120.0 * 1001.0 / 30000.0;
    assert!((seconds(&video[0]["duration"]) - video_seconds).abs() < 0.01);
    let audio = stream(&streams, "audio");
    assert!(
        (seconds(&audio[0]["duration"]) - video_seconds).abs() < 0.03,
        "audio {} s, video {video_seconds} s",
        audio[0]["duration"]
    );
    assert!(workspace.parts_directories().is_empty());
}

#[test]
fn hevc_transitions_at_deleted_parts_encode_with_the_output_encoder() {
    let workspace = Workspace::new("batch-join-hevc-transitions");
    let source = workspace.source("ride.mp4", 3.0, true);
    let mut request = workspace.request(
        BatchExportMode::Composite,
        BatchDestination::SourceNamed {
            directory: workspace.paths.downloads_dir.to_str().unwrap().into(),
        },
        vec![BatchRenderOutput {
            id: "ride".into(),
            // Two deleted parts, bridged by a 15-frame dissolve and a 9-frame fade.
            pieces: vec![
                piece(&source, 0, 30),
                piece(&source, 40, 70),
                piece(&source, 80, 90),
            ],
            transitions: vec![xfade("dissolve", 0.5), xfade("fade", 0.3)],
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
    assert_eq!(video[0]["nb_read_packets"], (70 - 15 - 9).to_string());
    let video_seconds = 46.0 * 1001.0 / 30000.0;
    assert!((seconds(&stream(&streams, "audio")[0]["duration"]) - video_seconds).abs() < 0.03);
}

#[test]
fn transition_proxies_blend_the_join_at_preview_size() {
    use ovrley_core::encode::ffmpeg::transition_proxy::{
        clear_transition_proxies, remove_transition_proxy, render_transition_proxy,
        TransitionProxyRequest, TransitionProxySource,
    };
    let workspace = Workspace::new("transition-proxy");
    let first = workspace.source("first.mp4", 3.0, true);
    let silent = workspace.source("silent.mp4", 2.0, false);
    let request = TransitionProxyRequest {
        outgoing: TransitionProxySource {
            path: first.metadata.path.clone(),
            start_second: 2.0,
            has_audio: true,
        },
        incoming: TransitionProxySource {
            path: silent.metadata.path.clone(),
            start_second: 0.0,
            has_audio: false,
        },
        transition: "circleopen".into(),
        duration_seconds: 1.0,
        width: 32,
        height: 16,
        fps_num: 30,
        fps_den: 1,
    };
    let proxy = render_transition_proxy(&workspace.paths, &request).unwrap();
    let streams = workspace.probe_streams(&proxy);
    assert_eq!(stream(&streams, "video")[0]["nb_read_packets"], "30");
    assert!((seconds(&stream(&streams, "audio")[0]["duration"]) - 1.0).abs() < 0.03);
    remove_transition_proxy(&proxy).unwrap();
    assert!(!proxy.exists());
    let stale = render_transition_proxy(&workspace.paths, &request).unwrap();
    clear_transition_proxies(&workspace.paths).unwrap();
    assert!(!stale.exists());
}

#[test]
fn frames_without_activity_coverage_keep_only_raster_widgets() {
    let workspace = Workspace::new("raster-only-outside-coverage");
    let source = workspace.source("ride.mp4", 3.0, false);
    let logo = workspace.paths.temp_dir.join("logo.png");
    let generated = Command::new(&workspace.ffmpeg)
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=8x8",
            "-frames:v",
            "1",
        ])
        .arg(&logo)
        .output()
        .unwrap();
    assert!(generated.status.success());
    let mut request = workspace.request(
        BatchExportMode::Transparent,
        BatchDestination::SourceNamed {
            directory: workspace.paths.downloads_dir.to_str().unwrap().into(),
        },
        vec![BatchRenderOutput {
            id: "ride".into(),
            pieces: vec![common::builders::whole_source_piece(&source, false)],
            transitions: Vec::new(),
        }],
    );
    // The 120 s activity ends 1.5 s into the video.
    let BatchActivity::ExternalActivity { offsets, .. } = &mut request.activity else {
        unreachable!()
    };
    *offsets.values_mut().next().unwrap() = 118.5;
    request.template.rasters.push(
        serde_json::from_value(json!({
            "id": "logo", "x": 56, "y": 24, "width": 8, "height": 8,
            "rotation": 0, "opacity": 1, "path": logo
        }))
        .unwrap(),
    );
    let snapshot = workspace.render(request);
    assert_eq!(
        snapshot.phase,
        BatchPhase::Completed,
        "{:?}",
        snapshot.items[0].outcome
    );
    let output = workspace.paths.downloads_dir.join("ride_overlay.mov");
    let frame_at = |second: &str| {
        let decoded = Command::new(&workspace.ffmpeg)
            .args(["-v", "error", "-ss", second, "-i"])
            .arg(&output)
            .args([
                "-frames:v",
                "1",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgba",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(decoded.status.success());
        assert_eq!(decoded.stdout.len(), 64 * 32 * 4);
        decoded.stdout
    };
    let in_logo = |index: usize| (index % 64) >= 56 && (index / 64) >= 24;
    let covered = frame_at("0.5");
    let uncovered = frame_at("2.5");
    for frame in [&covered, &uncovered] {
        // The raster shows in every frame.
        let pixel = &frame[(28 * 64 + 60) * 4..(28 * 64 + 60) * 4 + 4];
        assert!(pixel[0] > 200 && pixel[3] > 200, "{pixel:?}");
    }
    let opaque_outside_logo = |frame: &[u8]| {
        (0..64 * 32)
            .filter(|index| !in_logo(*index) && frame[index * 4 + 3] > 0)
            .count()
    };
    assert!(opaque_outside_logo(&covered) > 0);
    assert_eq!(opaque_outside_logo(&uncovered), 0);
}
