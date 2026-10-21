//! End to end: a real FFmpeg composite export through the planned atlas decodes
//! to the same frames as the export through the full-frame plan, including the
//! frames after the activity ends, where only rasters show.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use super::diagnostics::EncoderMonitor;
use super::{run_frame_pipeline, VideoEncoding};
use crate::activity::schema::ParsedActivity;
use crate::activity::{parse_activity_json, validate_render_activity};
use crate::debug::RenderProfiler;
use crate::encode::ffmpeg::binary::resolve_ffmpeg_binary;
use crate::encode::ffmpeg::composite::CompositeEncoding;
use crate::encode::progress::RenderController;
use crate::normalize::{parse_template_value, RenderConfig};
use crate::output::{RenderOutputKind, RenderOutputTarget};
use crate::paths::AppPaths;
use crate::render::{AtlasPlan, ExportWidgets, FrameSize};
use crate::render_jobs::inspection::verify_composite_source_resolution;
use crate::render_jobs::planning::{plan_single_render, PlannedVideoRender, VideoRenderModePlan};

const RENDER_SECONDS: f64 = 1.0;
/// Seconds of the export left after the activity ends.
const UNCOVERED_SECONDS: f64 = 0.4;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixtures() -> PathBuf {
    manifest_dir().join("tests").join("fixtures")
}

fn test_paths() -> AppPaths {
    let git_root = manifest_dir().join("..").join("..");
    let test_root = manifest_dir().join("..").join("target").join("atlas_export_tests");
    let paths = AppPaths {
        repo_root: git_root.clone(),
        font_dirs: vec![git_root.join("fonts")],
        debug_render_dir: test_root.join("debug_render"),
        temp_dir: test_root.join("tmp"),
        bundled_templates_dirs: vec![git_root.join("templates")],
        user_templates_dir: test_root.join("templates"),
        downloads_dir: test_root.join("downloads"),
    };
    for directory in [&paths.debug_render_dir, &paths.temp_dir, &paths.downloads_dir] {
        fs::create_dir_all(directory).unwrap();
    }
    paths
}

fn activity() -> ParsedActivity {
    let path = fixtures().join("activity").join("fit-parse-debug.json");
    parse_activity_json(&fs::read_to_string(path).unwrap()).unwrap()
}

/// The 1080p test template over the sample video, synced so that the activity
/// ends `UNCOVERED_SECONDS` before the export does.
fn config(activity_end: f64) -> RenderConfig {
    let template = fs::read_to_string(fixtures().join("config").join("test-template-4k.json")).unwrap();
    let mut config = parse_template_value(&serde_json::from_str(&template).unwrap()).unwrap();
    let sync_offset = activity_end - (RENDER_SECONDS - UNCOVERED_SECONDS);
    let scene = &mut config.scene;
    scene.presentation.width = Some(1920);
    scene.presentation.height = Some(1080);
    scene.ffmpeg = serde_json::json!({"codec": "libx264"});
    scene.start = sync_offset;
    scene.end = sync_offset + RENDER_SECONDS;
    scene.fps = 30000.0 / 1001.0;
    scene.composite_video_path = Some(fixtures().join("video").join("test-1080p.mp4").to_string_lossy().into_owned());
    scene.quality_type = Some(crate::encode::quality::QualityType::Bitrate);
    scene.quality_value = Some(20.0);
    scene.composite_sync_offset = Some(sync_offset);
    scene.composite_video_fps_num = Some(30000);
    scene.composite_video_fps_den = Some(1001);
    scene.composite_video_duration = Some(20.0);
    scene.composite_render_duration = Some(RENDER_SECONDS);
    scene.composite_video_trim_start = Some(0.0);
    scene.composite_widget_update_rate = Some(1);
    config
}

/// Exports the composite through the planned atlas or the full-frame plan and
/// returns that plan with the output path.
fn export(paths: &AppPaths, planned: bool) -> (AtlasPlan, PathBuf) {
    let activity = activity();
    let activity_end = validate_render_activity(&activity).unwrap();
    let plan = plan_single_render(config(activity_end), activity_end, None).unwrap();
    let dense = plan.prepare_activity(&activity).unwrap();
    let PlannedVideoRender { config, mode, .. } = plan;
    let VideoRenderModePlan::Composite { render, .. } = mode else {
        panic!("a composite video path plans a composite render");
    };
    let size = FrameSize {
        width: config.scene.presentation.width,
        height: config.scene.presentation.height,
    };
    let covered = render.coverage.blank_leading_frame_count + render.coverage.frame_count;
    assert!(covered > 0 && covered < render.overlay_frame_count, "the export must cover and outlast the activity: {:?}", render.coverage);
    let source = verify_composite_source_resolution(paths, &render.video_path, size.width, size.height).unwrap();
    let mut profiler = RenderProfiler::default();
    let export = ExportWidgets::prepare(
        &paths.font_dirs,
        &config,
        &activity,
        &dense,
        size,
        render.coverage.blank_leading_frame_count,
        &mut profiler,
    )
    .unwrap();
    let atlas = if planned {
        export.plan_atlas(&render.frames.dense_frame_indices(), &mut profiler).unwrap()
    } else {
        export.full_frame_atlas()
    };
    let label = if planned { "planned" } else { "full-frame" };
    let output = paths.downloads_dir.join(format!("atlas-export-{label}.mp4"));
    let target = RenderOutputTarget::validate(output.to_str().unwrap(), RenderOutputKind::Composite, true).unwrap();
    let encoding = VideoEncoding::Composite(
        CompositeEncoding::new(size, render, &atlas, source.has_audio, source.audio_bit_rate, source.rotation_degrees, source.color, &target).unwrap(),
    );
    let prepared = export.prepare_atlas(atlas.clone(), &mut profiler).unwrap();
    let frame_plan = encoding.frame_plan(prepared, size, target.path(), "rgba");
    run_frame_pipeline(
        &resolve_ffmpeg_binary(&paths.repo_root).unwrap(),
        &export,
        frame_plan,
        &RenderController::default(),
        &target,
        &encoding,
        Arc::new(EncoderMonitor::default()),
        None,
    )
    .unwrap();
    (atlas, output)
}

/// Decodes every video frame of `video` as 8-bit yuv420p.
fn decode(paths: &AppPaths, video: &Path) -> Vec<u8> {
    let output = Command::new(resolve_ffmpeg_binary(&paths.repo_root).unwrap())
        .args(["-v", "error", "-i"])
        .arg(video)
        .args(["-map", "0:v:0", "-f", "rawvideo", "-pix_fmt", "yuv420p", "-"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    output.stdout
}

#[test]
fn atlas_export_decodes_to_the_full_frame_export() {
    let paths = test_paths();
    let (atlas, planned) = export(&paths, true);
    let frame = FrameSize {
        width: 1920,
        height: 1080,
    };
    assert!(atlas.slots().len() > 1, "the template must plan several slots: {atlas:?}");
    assert!(
        u64::from(atlas.size().width) * u64::from(atlas.size().height) < u64::from(frame.width) * u64::from(frame.height),
        "the planned atlas {:?} must be smaller than the frame",
        atlas.size()
    );
    let (full_frame, reference) = export(&paths, false);
    assert_eq!(full_frame.size(), frame);

    let planned = decode(&paths, &planned);
    let reference = decode(&paths, &reference);
    let frame_bytes = frame.width as usize * frame.height as usize * 3 / 2;
    assert_eq!(planned.len(), reference.len());
    assert!(!planned.is_empty() && planned.len() % frame_bytes == 0);
    for (index, (planned, reference)) in planned.chunks(frame_bytes).zip(reference.chunks(frame_bytes)).enumerate() {
        let total: u64 = planned.iter().zip(reference).map(|(a, b)| u64::from(a.abs_diff(*b))).sum();
        let mean = total as f64 / frame_bytes as f64;
        assert!(mean <= 0.25, "frame {index}: mean sample difference {mean:.3} between the atlas and full-frame exports");
    }
}
