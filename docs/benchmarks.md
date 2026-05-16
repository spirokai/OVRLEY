## Transparnet Overlay Benchmark

-- Loops over all available codecs/hardware encoders for transparent overlays
-- Outputs in debug/benchmarks/transparent.json

e.g. pnpm benchmark:transparent debug/activities/test-ride-parse-debug.json templates/recent-template.json

## Composite Overlay Benchmark

-- Loops over all available codecs/hardware encoders for video compositing
-- Outputs in debug/benchmarks/composite.json

e.g. pnpm benchmark:composite debug/activities/test-ride-parse-debug.json templates/recent-template.json debug/benchmarks/test-4k.mp4

## Update Rate Benchmark

-- Loops over h264 with nvidia/cuda and QSV hardware encoders, at different update rates - 1/1, 1/2, 1/3 and 1/6
-- Outputs in debug/benchmarks/update_rate.json

e.g. pnpm benchmark:widget-rate debug/activities/test-ride-parse-debug.json templates/recent-template.json debug/benchmarks/test-4k.mp4
