This directory stores rendered frame baselines for the integration suite in
`src-tauri/ovrley_core/tests/render_baseline_suite.rs`.

Subdirectories:
- `frame/` for preview PNG baselines
- `transparent/` for decoded transparent-video frame baselines
- `composite/` for decoded composite-video frame baselines

Record/update baselines with:
`OVRLEY_RECORD_BASELINES=1 cargo test -p ovrley_core --test render_baseline_suite -- --nocapture`
