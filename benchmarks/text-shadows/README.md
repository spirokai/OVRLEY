# Text shadow benchmark results

The diagnostic compared the former Skia `drop_shadow_only` text drawing with `MaskFilter::blur`, using shared Rust rendering code in native and WASM. It used an explicit image-filter implementation to measure the former algorithm after production switched to mask shadows. Benchmark sources, runners, and build wiring have since been removed; the results are retained.

Seven cases cover changing numeric text, proportional/monospace fonts, a longer changing label, small text, wider blur, partial opacity, and a text border. Every case has 256 different labels. Each method gets 256 warmup frames and three measured batches of 600 frames, with rotated method order. Text layout is prepared outside timing. Clear, shadow, border, and fill drawing are timed; pixel readback and transport are excluded. Targets use premultiplied CPU RGBA surfaces on both hosts.

Methods compare no shadow, fresh image filters, fresh mask filters, reused image-filter paints, and reused mask-filter paints. The mask path matches the existing shadow alpha multiplication. Pixel comparisons use premultiplied RGBA and measure differences rather than assuming equivalence. The benchmark also writes comparison sheets over white.

Visual comparisons:

- [Native comparison](native-comparison.png)
- [WASM comparison](wasm-comparison.png)

Local raw timing results remain in `debug/benchmarks/text-shadows/native.json` and `debug/benchmarks/text-shadows/wasm.json`.
