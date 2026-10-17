//! --fonts prepares resources and validates configs; only --run renders frames.
use ovrley_core::{
    activity::{
        build_dense_activity_report_validated, parse_activity_json, validate_render_activity,
    },
    commands::parse_and_validate_config,
    debug::RenderProfiler,
    fonts::preview_font_resource,
    paths::AppPaths,
    render::{prepare_preview_assets, FrameSize, VideoFrameRenderer},
};
use ovrley_widgets::preview::FontRequest;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::PathBuf, time::Instant};

mod preview;

#[derive(Deserialize)]
struct Plan {
    root: PathBuf,
    frames: usize,
    warmup: usize,
    fps: u32,
    activities: BTreeMap<String, PathBuf>,
    cases: Vec<Case>,
    fonts: Vec<Value>,
}
#[derive(Deserialize)]
struct Case {
    id: String,
    activity: String,
    config: Value,
    start_frame: usize,
    dynamic: bool,
    request: Option<Value>,
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [mode, path] = args.as_slice() else {
        return Err(
            "Usage: benchmark_widgets --fonts|--run|--run-preview|--validate <plan.json>".into(),
        );
    };
    if !["--fonts", "--run", "--run-preview", "--validate"].contains(&mode.as_str()) {
        return Err("Unknown benchmark mode".into());
    }
    let path = PathBuf::from(path);
    let directory = path.parent().ok_or("Plan requires a parent directory")?;
    let plan: Plan = serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if plan.frames == 0 || plan.fps == 0 || plan.frames.checked_add(plan.warmup).is_none() {
        return Err("Invalid frame counts".into());
    }
    // Validate once at ingress, before either resources or playback are prepared.
    let configs = plan
        .cases
        .iter()
        .map(|case| {
            parse_and_validate_config(&case.config.to_string())
                .map_err(|e| format!("{}: {e}", case.id))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if mode == "--validate" {
        return Ok(());
    }
    if mode == "--run-preview" {
        return preview::run(&plan, directory);
    }
    let paths = AppPaths::from_runtime_roots(
        plan.root.clone(),
        plan.root.clone(),
        directory.join("runtime"),
    );
    if mode == "--fonts" {
        let mut fonts = Vec::new();
        for (index, request) in plan.fonts.iter().enumerate() {
            let font: FontRequest =
                serde_json::from_value(request.clone()).map_err(|e| e.to_string())?;
            let bytes =
                preview_font_resource(&paths.font_dirs, &font.font, font.font_weight, font.italic)
                    .map_err(|e| e.to_string())?;
            let filename = format!("font-{index}.bin");
            fs::write(directory.join(&filename), bytes).map_err(|e| e.to_string())?;
            fonts.push(json!({"request": request, "path": filename}));
        }
        fs::write(
            directory.join("fonts.json"),
            serde_json::to_vec(&fonts).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        return Ok(());
    }
    let mut results = Vec::new();
    for (case, config) in plan.cases.iter().zip(configs) {
        eprintln!("[native] {}", case.id);
        let setup = Instant::now();
        let activity_path = plan
            .activities
            .get(&case.activity)
            .ok_or_else(|| format!("Unknown activity: {}", case.activity))?;
        let activity =
            parse_activity_json(&fs::read_to_string(activity_path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        validate_render_activity(&activity).map_err(|e| e.to_string())?;
        let dense =
            build_dense_activity_report_validated(&activity, &config).map_err(|e| e.to_string())?;
        if dense.frame_count < plan.warmup + plan.frames {
            return Err(format!("{}: insufficient native frames", case.id));
        }
        for (index, second) in dense
            .frame_elapsed_seconds
            .iter()
            .take(plan.warmup + plan.frames)
            .enumerate()
        {
            if (*second - index as f64 / f64::from(plan.fps)).abs() > 1e-6 {
                return Err("Native frame timeline differs from browser timeline".into());
            }
        }
        let (assets, _, prepare_stages, _) =
            prepare_preview_assets(&paths, &config, &activity, &dense)
                .map_err(|e| e.to_string())?;
        let size = FrameSize {
            width: config.scene.presentation.width,
            height: config.scene.presentation.height,
        };
        let renderer = VideoFrameRenderer::new(&paths.font_dirs, &dense, &assets, size, 0)
            .map_err(|e| e.to_string())?;
        let mut pixels = vec![0; size.rgba_len().map_err(|e| e.to_string())?];
        let setup_ms = setup.elapsed().as_secs_f64() * 1000.0;
        let mut profiler = RenderProfiler::default();
        for index in 0..plan.warmup {
            renderer
                .render_rgba(index, &mut pixels, &mut profiler)
                .map_err(|e| e.to_string())?;
        }
        let mut profiler = RenderProfiler::default();
        let mut ms = vec![0.0; plan.frames];
        for (offset, duration) in ms.iter_mut().enumerate() {
            let started = Instant::now();
            renderer
                .render_rgba(plan.warmup + offset, &mut pixels, &mut profiler)
                .map_err(|e| e.to_string())?;
            *duration = started.elapsed().as_secs_f64() * 1000.0;
        }
        results.push(json!({"id": case.id, "backend": "native", "ms": ms, "redraws": null, "stages": profiler.summary(), "prepare_stages": prepare_stages, "setup_ms": setup_ms, "raster": [size.width, size.height]}));
        fs::write(
            directory.join("native.json"),
            serde_json::to_vec(&results).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}
