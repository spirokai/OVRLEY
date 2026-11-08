//! Single integration-test binary for ovrley_core. Each file in this directory is
//! one module, so the crate and its dependencies are linked once instead of once
//! per file. Run one file with `cargo test -p ovrley_core --test it <module>::`.

#![recursion_limit = "256"]

#[path = "../common/mod.rs"]
mod common;

mod activity_extraction_tests;
mod activity_tests;
mod arc_gauge_tests;
mod backdrop_contract_tests;
mod batch_render_tests;
mod cancellation_tests;
mod codec_detect_tests;
mod commands_tests;
mod composite_profile_tests;
mod config_tests;
mod corner_gauge_tests;
mod csv_activity;
mod display_type_tests;
mod diving_activity;
mod dji_ac004_tests;
mod elevation_frame_state_tests;
mod elevation_geometry_tests;
mod error_display_tests;
mod ffmpeg_composite_tests;
mod ffmpeg_settings_tests;
mod fit_activity;
mod font_weight_tests;
mod format_tests;
mod fps_tests;
mod gauge_metric_tests;
mod g_force_frame_state_tests;
mod gpx_activity;
mod gradient_format_tests;
mod gt_gauge_tests;
mod heading_data_tests;
mod heading_widget_tests;
mod igc_activity;
mod lap_timer_tests;
mod lean_angle_tests;
mod linear_gauge_tests;
mod manifest_activity_tests;
mod map_frame_copy_tests;
mod metric_kind_serde_tests;
mod mp4_telemetry_fixture_tests;
mod plot_multiplicity_tests;
mod preview_canvas_parity_format_tests;
mod progress_tests;
mod rdp_tests;
mod render_baseline_suite;
mod render_execution_tests;
mod route_geometry_tests;
mod scuba_diving_tests;
mod srt_activity;
mod standard_metrics_display_type_tests;
mod tcx_activity;
mod value_widget_tests;
mod vbo_activity;
mod video_composite_pipeline_tests;
mod video_inspection_tests;
mod video_probe_tests;
mod wasm_metric_text_tests;
mod wasm_widget_preview_tests;
mod wave1_format_tests;
