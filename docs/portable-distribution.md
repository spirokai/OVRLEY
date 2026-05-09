# Portable Distribution Notes

OVRLEY keeps FFmpeg at arms length from the main Tauri binary. Local and CI installs place it under:

```text
vendor/ffmpeg/bin/ffmpeg[.exe]
```

The Tauri bundle includes `vendor/ffmpeg`, `fonts`, and `templates` as resources. At runtime, packaged apps resolve FFmpeg from Tauri's resource directory first; development and CLI helper binaries resolve it from the repo root. `OVRLEY_FFMPEG` and `FFMPEG_BINARY` still override all bundled paths.

## Windows

By default, `pnpm tauri build` and `pnpm build` skip Tauri installer bundling and produce a portable ZIP. The ZIP contains:

```text
OVRLEY.exe
vendor/ffmpeg/bin/ffmpeg.exe
vendor/ffmpeg/bin/*.dll
fonts/
templates/
```

This is portable from the app's perspective: FFmpeg and its DLLs are separate from the main executable and travel beside it. The remaining external runtime requirement is the Microsoft Edge WebView2 runtime, which Tauri apps need on Windows unless it is bundled separately by installer configuration.

The manual release workflow explicitly passes Tauri bundle targets so it can upload installer artifacts in addition to the portable ZIP.

Portable archives include a generated `THIRD_PARTY_NOTICES.txt` with the bundled FFmpeg `-version` and `-L` output. The source repository does not commit FFmpeg binaries; this notice applies to generated release archives.

## macOS

The workflow uploads the `.dmg` and a zipped `.app`. The `.app` is the portable form on macOS; FFmpeg is included inside the app resources, separate from the app executable. Distribution outside a local machine should still be signed and notarized to avoid Gatekeeper warnings.

## FFmpeg Updates

`pnpm install` runs `scripts/install-ffmpeg.mjs`. The script skips an existing bundled FFmpeg when it is version `8.1` or newer. Set `OVRLEY_FFMPEG_ARCHIVE_URL` to test or pin a different ZIP source, or set `OVRLEY_SKIP_FFMPEG_INSTALL=1` when preparing a build environment manually.
