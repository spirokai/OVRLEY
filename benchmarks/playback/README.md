# Playback benchmark

```sh
cargo run -p ovrley_ui --release --example benchmark_playback -- . timeline   # the timeline clock
cargo run -p ovrley_ui --release --example benchmark_playback -- . video      # a 60 fps clip as the clock
cargo run -p ovrley_ui --release --example benchmark_playback -- . video 20   # 20 measured seconds
```

`native.rs` (`ovrley_ui` example `benchmark_playback`) composes the app's window with fake services on Slint's testing backend, which runs a real event loop on the system clock but draws nothing. It loads the futuristic HUD template (18 widgets) and the committed `test-ride.fit`, in a 1600 × 1000 window at scale factor 2, waits until the preview has presented every widget, plays from 60 seconds, warms up for 2 seconds and measures 10.

- `timeline`: the player's own timer drives playback, as without video.
- `video`: a 1200-second 60 fps clip lies at the start of the timeline over the video background. A 60 Hz timer passes each frame on as the player does when MDK presents one (`playback::video_frame_presented`).

It prints JSON: preview frames per second, dispatches and projections per frame, the time the update loop spent in actions, reconcile steps and the history step (`update_ms_per_frame`) and in projections (`projection_ms_per_frame`), and the process's CPU time per second and per frame. Preview frames run as actions, so their drawing and readback count as update time.

Not measured: window rendering, texture uploads and presentation (the testing backend has no renderer), and MDK's decoding. The dispatch counts come from `Dispatcher::take_stats`, which exists only with `ovrley_app`'s `test-util` feature.
