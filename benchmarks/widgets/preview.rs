//! Runs the browser preview implementation natively, including sampling and RGBA readback.
use super::Plan;
use ovrley_widgets::preview::{FontRequest, PreviewSession};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{fs, path::Path, time::Instant};

#[derive(Deserialize)]
struct FontResource {
    request: Value,
    path: String,
}

pub(super) fn run(plan: &Plan, directory: &Path) -> Result<(), String> {
    let fonts: Vec<FontResource> = serde_json::from_slice(
        &fs::read(directory.join("fonts.json")).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let font_bytes = fonts
        .iter()
        .map(|font| fs::read(directory.join(&font.path)).map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut results = Vec::new();
    for case in &plan.cases {
        eprintln!("[native-preview] {}", case.id);
        let setup = Instant::now();
        let mut session = PreviewSession::default();
        for (resource, bytes) in fonts.iter().zip(&font_bytes) {
            let request: FontRequest = serde_json::from_value(resource.request.clone())
                .map_err(|error| error.to_string())?;
            session
                .fonts
                .register(request, bytes)
                .map_err(|error| error.to_string())?;
        }
        if case.dynamic {
            let path = plan
                .activities
                .get(&case.activity)
                .ok_or_else(|| format!("Unknown activity: {}", case.activity))?;
            session
                .replace_activity(&fs::read(path).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?;
        }
        if let Some(filename) = &case.split_table {
            session
                .replace_split_table(
                    &fs::read(directory.join(filename)).map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
        }
        if case
            .request
            .as_ref()
            .is_some_and(|request| request["type"] == "raster")
        {
            session
                .images
                .register(
                    "benchmark-image".into(),
                    &fs::read(plan.root.join("src-tauri/icons/128x128@2x.png"))
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
        }
        if let Some(size) = case.map_frame {
            session
                .map_frames
                .upload(size, size, &vec![255; size as usize * size as usize * 4])
                .map_err(|error| error.to_string())?;
        }
        let handle = case
            .request
            .as_ref()
            .map(|request| {
                let request =
                    serde_json::from_value(request.clone()).map_err(|error| error.to_string())?;
                session
                    .create_widget(request)
                    .map_err(|error| error.to_string())
            })
            .transpose()?;
        // Match browser.js: initial render, then warmup, then the measured interval.
        if case.dynamic {
            session
                .render(case.start_frame as f64 / f64::from(plan.fps))
                .map_err(|error| error.to_string())?;
            for index in 0..plan.warmup {
                session
                    .render((case.start_frame + index) as f64 / f64::from(plan.fps))
                    .map_err(|error| error.to_string())?;
            }
        }
        let setup_ms = setup.elapsed().as_secs_f64() * 1000.0;
        let mut revision = handle.map(|id| session.widgets[&id].revision());
        let mut ms = Vec::with_capacity(plan.frames);
        let mut redraw_ms = Vec::new();
        for index in 0..plan.frames {
            let started = Instant::now();
            if case.dynamic {
                session
                    .render((case.start_frame + plan.warmup + index) as f64 / f64::from(plan.fps))
                    .map_err(|error| error.to_string())?;
            }
            let duration = started.elapsed().as_secs_f64() * 1000.0;
            ms.push(duration);
            let current = handle.map(|id| session.widgets[&id].revision());
            if current != revision {
                redraw_ms.push(duration);
            }
            revision = current;
        }
        let raster = handle
            .map(|id| {
                let geometry: Value = serde_json::from_slice(session.widgets[&id].geometry())
                    .map_err(|error| error.to_string())?;
                Ok::<_, String>(json!([geometry["width"], geometry["height"]]))
            })
            .transpose()?;
        let total_ms = if case.dynamic {
            ms.iter().sum::<f64>()
        } else {
            0.0
        };
        results.push(json!({
            "id": case.id, "backend": "native-preview", "ms": ms,
            "redraws": redraw_ms.len(), "redraw_ms": redraw_ms,
            "stages": { "preview.render": { "count": if case.dynamic { plan.frames } else { 0 }, "total_ms": total_ms } },
            "setup_ms": setup_ms, "raster": raster,
        }));
        fs::write(
            directory.join("native-preview.json"),
            serde_json::to_vec(&results).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}
