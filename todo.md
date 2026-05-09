-move resolution badge to top left corner of the canvas
-add grid display button and snap to grid button
-handle widget update rate by defining container fps different from real

-change logo to ours
-figure our src-tauri logos, why so many, bro?

-custom range export in sidebar enabled only if we have parsed activity
-gradient widget around causes vertical jitter while dragging around
-add shadows to icons and remaining map route/elevation profile - skia first, frotnend follows
-height of metric widget selection box does not correspond to the real height. Is that just stale state from font change?

-relocate ffmpeg to a directory that will give bin/ffmpeg.exe in the final build, and more importantly will be compatible with bundling during the build process for both win and macOS
-make sure skia-render can call ffmpeg both on win and macOS
-create script that will install ffmpeg after pnpm i if missing

-movie load and playback
-H264 and 265 compositing, including gpu-accelerated
