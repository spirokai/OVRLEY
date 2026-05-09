-handle widget update rate by defining container fps different from real

-height of metric widget selection box does not correspond to the real height. Is that just stale state from font change?

We need to change how we handle ffmpeg integration:

- relocate ffmpeg (currently run from .ffmpeg) to a directory that will provide bin/ffmpeg.exe ("at arms length") in the final build, and more importantly will be compatible with the build process and compiling for both windows and macOS by GitHub actions
- write a workflow that will manually fire a build process for both Win and macOS using GitHub actions and then provide the build artifacts (exe, dmg) in the release page. Assess what is required to distribute the build as portable, i.e. without installation. At least ffmpeg and its required libraries must be separate from the main binary
- make sure skia-render can call of ffmpeg is system agnostic and works on both win and macOS
- create script that will install ffmpeg after "pnpm i" if ffmpeg is missing or version <8.1
- ask clarifying questions if something is ambiguous

-movie load and playback
-H264 and 265 compositing, including gpu-accelerated
