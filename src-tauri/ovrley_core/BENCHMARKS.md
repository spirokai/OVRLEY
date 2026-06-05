# Encoding Benchmark Suite

Performance benchmarks for ovrley overhead-video encoding. Each benchmark
runs real renders through `ovrley_core` and writes aggregated timing & file-size
results to a JSON file in this directory.

## Transparent Overlay Benchmark

Loops over all available transparent codecs (`prores_ks`, etc.), runs 3
full renders per codec at a fixed 60-second activity window (300s–360s),
and measures job time + output file size.

- **Use case:** Compare transparent (alpha-channel) codec performance
- **Output:** `debug/benchmarks/transparent.json`

```bash
# Usage: cargo run -p ovrley_core --bin benchmark_transparent -- <activity-path> <template-path>
cargo run -p ovrley_core --bin benchmark_transparent -- debug/activities/Test_FIT-parse-debug.json templates/recent-template.json
```

H:\tools\cyclemetry\src-tauri\ovrley_core\tests\fixtures\video\test-4k.mp4

## Composite Overlay Benchmark

Loops over all available composite codec profiles (software, NVENC, QSV,
VideoToolbox, VAAPI, AMF), runs 3 full composite renders per codec over a
60-second activity+video window, and measures job time + output file size.

- **Use case:** Compare video compositing codec performance
- **Output:** `debug/benchmarks/composite.json`

```bash
# Usage: cargo run -p ovrley_core --bin benchmark_composite -- <activity-path> <template-path> <video-path>
cargo run -p ovrley_core --bin benchmark_composite -- debug/activities/Test_FIT-parse-debug.json templates/recent-template.json debug/benchmarks/test-4k.mp4
```

## Widget Update Rate Benchmark

Tests how widget update rate (1, 2, 3, 6 activity-seconds between widget
recomputes) affects composite render throughput. Runs 3 iterations per
(codec, update_rate) pair for each available GPU composite codec
(`nnvgpu_h264`, `qsv_full_h264`).

- **Use case:** Determine optimal widget update rate for performance/quality
- **Output:** `debug/benchmarks/update_rate.json`

```bash
# Usage: cargo run -p ovrley_core --bin benchmark_widget_rate -- <activity-path> <template-path> <video-path>
cargo run -p ovrley_core --bin benchmark_widget_rate -- debug/activities/Test_FIT-parse-debug.json templates/recent-template.json debug/benchmarks/test-4k.mp4
```
