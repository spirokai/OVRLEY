-handle widget update rate by defining container fps different from real

-height of metric widget selection box does not correspond to the real height. Is that just stale state from font change?

-relocate ffmpeg to a directory that will provide bin/ffmpeg.exe in the final build ("at arms length"), and more importantly will be compatible with bundling during the Github Action build workflow process for both win and macOS
-write a github action script to build the project for both win and macOS, and place the build artifacts (.exe/.dmg) in the releases section
-make sure skia-render can call ffmpeg both on win and macOS; right now we are calling ffmpeg.exe only
-create script that will install ffmpeg after pnpm i if missing

-movie load and playback
-H264 and 265 compositing, including gpu-accelerated

cargo run --bin render_preview -- --payload ..\app\debug\Test_FIT-parse-debug.json --config ..\templates\new_template.json --second 600 --out ..\tmp\new_template_preview_600_current.png --timing-out ..\tmp\new_template_preview_600_current_timing.json
