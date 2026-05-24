//! Pixel parity test comparing Rust Skia rendering to the
//! frontend React SVG canvas via Playwright.
//!
//! Feature-gated behind `canvas-parity`:
//!   cargo test -p ovrley_core --features canvas-parity --test canvas_parity_tests -- --nocapture
//!
//! Output artifacts (skia.png, canvas.png, canvas-parity.png, summary.json) are
//! written to:
//!   src-tauri/ovrley_core/tests/canvas_parity/
//! with intermediate temp files under:
//!   src-tauri/target/canvas-parity/
//!
//! Requires: Node.js, pnpm (or npm), ffmpeg, Playwright browsers installed.

mod common;
#[path = "common/canvas_parity.rs"]
mod canvas_parity;

use anyhow::{Context, Result};
use canvas_parity::{
    generate_diff_png, parse_fixtures, render_skia_preview, run_playwright_screenshot, run_ssim,
    test_app_paths, write_mock_data, ViteServer,
};
use ovrley_core::activity::build_dense_activity_report;
use std::fs;
use std::path::PathBuf;

/// The second within the activity to render (mid-activity, all widgets active).
const SELECTED_SECOND: u32 = 600;

/// SSIM threshold — accounts for minor glyph rasterization differences between
/// Skia's FreeType and Chromium's FreeType.
const SSIM_THRESHOLD: f64 = 0.98;

/// Compares a Skia-rendered frame to a Playwright-captured browser SVG frame.
#[test]
fn canvas_parity() -> Result<()> {
    // ── Resolve paths ──────────────────────────────────────────────────
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_root = manifest_dir.join("tests").join("fixtures");
    let git_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(&manifest_dir)
        .to_path_buf();
    let src_tauri = manifest_dir.parent().unwrap();

    let case_name = "canvas-parity";
    let case_root = src_tauri.join("target").join("canvas-parity").join(case_name);
    let mock_dir = case_root.join("mock-data");
    let artifacts_dir = manifest_dir.join("tests").join("canvas_parity");
    let skia_png = artifacts_dir.join("skia.png");
    let canvas_png = artifacts_dir.join("canvas.png");
    let diff_png = artifacts_dir.join("canvas-parity.png");
    let summary_json = artifacts_dir.join("summary.json");
    fs::create_dir_all(&artifacts_dir)?;

    println!("=== Canvas Parity Test ===");
    println!("  fixture root: {}", fixture_root.display());
    println!("  case root:    {}", case_root.display());

    // 1. Parse fixtures
    println!("[1/9] Parsing fixtures...");
    let (activity, config, activity_raw, config_raw) = parse_fixtures(&fixture_root)?;
    println!("  activity: {} samples, {}s duration",
        activity.sample_elapsed_seconds.len(),
        activity.trim_end_seconds);

    // 2. Build dense activity report
    println!("[2/9] Building dense activity report...");
    let dense_activity = build_dense_activity_report(&activity, &config)
        .context("build_dense_activity_report failed")?;
    println!("  dense frames: {}", dense_activity.frame_count);

    // 3. Set up AppPaths and prepare assets
    println!("[3/9] Setting up AppPaths...");
    let app_paths = test_app_paths(&git_root, &case_root)?;

    // 4. Render Skia PNG
    println!("[4/9] Rendering Skia preview...");
    render_skia_preview(&app_paths, &config, &activity, &dense_activity, SELECTED_SECOND, &skia_png)?;

    // 5. Write mock data files for Playwright
    println!("[5/9] Writing mock data files...");
    write_mock_data(&mock_dir, &config, &config_raw, &activity, &activity_raw, SELECTED_SECOND)?;

    // 6. Spawn Vite dev server
    println!("[6/9] Starting Vite dev server...");
    let vite = ViteServer::start(&git_root)?;

    // 7. Run Playwright screenshot script
    println!("[7/9] Running Playwright screenshot...");
    let script_path = manifest_dir.join("tests").join("scripts").join("canvas_screenshot.mjs");
    let info = run_playwright_screenshot(&script_path, &mock_dir, &vite.url(), &canvas_png)?;
    println!(
        "  Playwright captured transparent widget layer at {}x{}",
        info.width, info.height
    );
    drop(vite); // Vite no longer needed; kill early so it doesn't block the terminal.

    // 8. Run ffmpeg SSIM comparison
    println!("[8/9] Running SSIM comparison...");
    let ssim = run_ssim(&skia_png, &canvas_png, &git_root)?;

    // 9. Assert threshold
    println!("[9/9] Checking SSIM threshold...");
    let pass = ssim.combined >= SSIM_THRESHOLD;
    if pass {
        println!(
            "  SSIM: {:.4} (threshold: {SSIM_THRESHOLD}) — PASS",
            ssim.combined
        );
        println!(
            "  Y: {:.4}  U: {:.4}  V: {:.4}",
            ssim.y, ssim.u, ssim.v
        );
    } else {
        println!(
            "  SSIM: {:.4} (threshold: {SSIM_THRESHOLD}) — FAIL",
            ssim.combined
        );
        println!("  Y: {:.4}  U: {:.4}  V: {:.4}", ssim.y, ssim.u, ssim.v);
    }

    // Always generate a diff image so passing runs remain inspectable.
    let diff_stats = generate_diff_png(&skia_png, &canvas_png, &diff_png, &git_root)?;
    let total_pixels = info.width as u64 * info.height as u64;
    let raw_pct = (diff_stats.mismatch_pixels as f64 / total_pixels as f64) * 100.0;
    let alpha_masked_pct = if diff_stats.overlay_pixels > 0 {
        (diff_stats.overlay_mismatch_pixels as f64 / diff_stats.overlay_pixels as f64) * 100.0
    } else {
        0.0
    };
    let alpha_tolerant_pct = if diff_stats.overlay_pixels > 0 {
        (diff_stats.overlay_significant_mismatch_pixels as f64 / diff_stats.overlay_pixels as f64) * 100.0
    } else {
        0.0
    };
    let canvas_only_pct = if diff_stats.overlay_pixels > 0 {
        (diff_stats.canvas_only_pixels as f64 / diff_stats.overlay_pixels as f64) * 100.0
    } else {
        0.0
    };
    let skia_only_pct = if diff_stats.overlay_pixels > 0 {
        (diff_stats.skia_only_pixels as f64 / diff_stats.overlay_pixels as f64) * 100.0
    } else {
        0.0
    };
    let edge_insensitive_pct = if diff_stats.edge_compared_pixels > 0 {
        (diff_stats.edge_insensitive_mismatch_pixels as f64
            / diff_stats.edge_compared_pixels as f64)
            * 100.0
    } else {
        0.0
    };
    println!(
        "  diff image: {}",
        diff_png.display()
    );
     println!(
        "  ---------------------------------------------------------",
    ); 
     println!(
        "  PIXEL MISMATCH COUNT:",
    );
    println!(
        "  FULL FRAME ({} / {})- {raw_pct:.2}% ",
        diff_stats.mismatch_pixels,
        total_pixels
    );
    println!(
        "  OVERLAY ONLY ({} / {}) - {alpha_masked_pct:.2}%",
        diff_stats.overlay_mismatch_pixels,
        diff_stats.overlay_pixels
    );
    println!(
        "  THRESHOLD APPLIED ({} / {}) - {alpha_tolerant_pct:.2}%",
        diff_stats.overlay_significant_mismatch_pixels,
        diff_stats.overlay_pixels
    );
    println!(
        "  CLEAN (AA EXCLUDED) ({} / {}; ignored {}) - {edge_insensitive_pct:.2}%",
        diff_stats.edge_insensitive_mismatch_pixels,
        diff_stats.edge_compared_pixels,
        diff_stats.edge_ignored_pixels
    );
     println!(
        "  ---------------------------------------------------------",
    ); 
     println!(
        "  ORPHANED PIXELS:",
    );    
    println!(
        "  PREVIEW EXCLUSIVE ({} / {}) - {canvas_only_pct:.2}%",
        diff_stats.canvas_only_pixels,
        diff_stats.overlay_pixels
    );
    println!(
        "  RENDER EXCLUSIVE ({} / {}) - {skia_only_pct:.2}%",
        diff_stats.skia_only_pixels,
        diff_stats.overlay_pixels
    );
    println!(
        "  summary: {}",
        summary_json.display()
    );

    let summary = serde_json::json!({
        "ssim": {
            "combined": ssim.combined,
            "y": ssim.y,
            "u": ssim.u,
            "v": ssim.v,
            "threshold": SSIM_THRESHOLD,
            "pass": pass,
        },
        "pixel_mismatch": {
            "full_frame": {
                "count": diff_stats.mismatch_pixels,
                "total": total_pixels,
                "percent": format!("{:.2}", raw_pct),
            },
            "overlay": {
                "count": diff_stats.overlay_mismatch_pixels,
                "total": diff_stats.overlay_pixels,
                "percent": format!("{:.2}", alpha_masked_pct),
            },
            "threshold_applied": {
                "count": diff_stats.overlay_significant_mismatch_pixels,
                "total": diff_stats.overlay_pixels,
                "percent": format!("{:.2}", alpha_tolerant_pct),
            },
            "clean_aa_excluded": {
                "mismatch": diff_stats.edge_insensitive_mismatch_pixels,
                "compared": diff_stats.edge_compared_pixels,
                "ignored": diff_stats.edge_ignored_pixels,
                "percent": format!("{:.2}", edge_insensitive_pct),
            },
        },
        "orphaned_pixels": {
            "canvas_exclusive": {
                "count": diff_stats.canvas_only_pixels,
                "total": diff_stats.overlay_pixels,
                "percent": format!("{:.2}", canvas_only_pct),
            },
            "skia_exclusive": {
                "count": diff_stats.skia_only_pixels,
                "total": diff_stats.overlay_pixels,
                "percent": format!("{:.2}", skia_only_pct),
            },
        },
        "artifacts": {
            "skia": skia_png.to_string_lossy(),
            "canvas": canvas_png.to_string_lossy(),
            "diff": diff_png.to_string_lossy(),
        },
    });
    fs::write(&summary_json, serde_json::to_string_pretty(&summary)?)?;

    if !pass {
        anyhow::bail!(
            "SSIM {:.4} < {} — Y={:.4} U={:.4} V={:.4} — diff: {}",
            ssim.combined, SSIM_THRESHOLD, ssim.y, ssim.u, ssim.v, diff_png.display()
        );
    }

    Ok(())
}
