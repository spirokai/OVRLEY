# Encoding Benchmark Suite

Performance benchmarks for encoding. Each one runs real renders through
`ovrley_core` and writes aggregated timing and file-size results to a JSON
file under `debug/benchmarks/` at the repository root.

They take a parsed activity (a debug build writes one to `debug/activities/`
when it imports a file) and a template, and cool down between codec groups,
so a run takes a while. The commands build in debug; add `--release` after
`cargo run` to measure what ships.

## Transparent Overlay Benchmark

Runs 3 full renders of each available transparent codec (`prores_ks`,
`prores_ks_vulkan`, `prores_videotoolbox`, `qtrle`) at a fixed 60-second
activity window (300s-360s), and measures job time and output file size.

- **Use case:** Compare transparent (alpha-channel) codec performance
- **Output:** `debug/benchmarks/transparent.json`

```bash
pnpm benchmark:transparent <activity-path> <template-path>
```

## Composite Overlay Benchmark

Runs 3 full composite renders of each available composite codec profile
(software, NVENC, QSV, VideoToolbox, VAAPI, AMF) over a 60-second
activity and video window, and measures job time and output file size.

- **Use case:** Compare video compositing codec performance
- **Output:** `debug/benchmarks/composite.json`

```bash
pnpm benchmark:composite <activity-path> <template-path> <video-path>
```

## Widget Update Rate Benchmark

Tests how the widget update rate (a recompute every 1, 2, 3 or 6 frames)
affects composite render throughput. Runs 3 iterations per (codec, rate)
pair for the available ones of `nnvgpu_h264` and `qsv_full_h264`.

- **Use case:** Determine the best widget update rate for performance and quality
- **Output:** `debug/benchmarks/update_rate.json`

```bash
pnpm benchmark:widget-rate <activity-path> <template-path> <video-path>
```
