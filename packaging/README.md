# Packaging (developer notes)

How OVRLEY's packages are built. Nothing here ships: what users get are the guides in `install/` (as `INSTALL.txt`) and the generated `THIRD_PARTY_NOTICES.txt`.

OVRLEY keeps FFmpeg at arms length from the executable: it runs as a subprocess. Local and CI installs place it under:

```text
vendor/ffmpeg/bin/ffmpeg[.exe]
```

Packages also carry the MDK player's libraries, which the executable links. `pnpm install` fetches its SDK into `vendor/mdk-sdk/`, and `pnpm prepare:packaging` copies the libraries a package needs to `target/packaging/player/`.

`pnpm build` runs `scripts/package.mjs`: the release build, the packaging resources, cargo-packager (configured in `crates/ovrley_ui/Cargo.toml`) and, on Windows and macOS, the portable ZIP of `packaging/portable.mjs`. The DMG, the AppImage, the Flatpak and the Store's MSIX are built by their workflows. `pnpm install:packager` installs cargo-packager at the pinned revision, as the workflows do.

The updater (`ovrley_services::updater`) reads the release's `latest.json`, which `scripts/release/create-latest-json.mjs` writes. The artifacts it installs must be signed by cargo-packager (`CARGO_PACKAGER_SIGN_PRIVATE_KEY` and its `_PASSWORD`), which signs the version too; the AppImage is signed with `cargo packager signer sign --app-version <version>`.

At runtime a package's resources are found from the executable (`ovrley_services::runtime_paths`); development and CLI helper binaries use the repository. `OVRLEY_FFMPEG` overrides the bundled FFmpeg. Without either, `ffmpeg` on `PATH` is used.

## Windows

The ZIP contains:

```text
OVRLEY/
  OVRLEY.exe
  .ovrley-portable
  *.dll                       (the player)
  vendor/ffmpeg/bin/ffmpeg.exe
  vendor/ffmpeg/bin/ffprobe.exe
  fonts/
  templates/
  map-styles/
  THIRD_PARTY_NOTICES.txt
```

The `.ovrley-portable` marker means "updates come from elsewhere" and turns the updater off. The Debian package, the Flatpak and the Microsoft Store package carry it too.

The installer installs the same executable as `app.exe`, the name every Tauri release installed, so that updates, shortcuts and pins keep working.

Every release ships a `THIRD_PARTY_NOTICES.txt` that `pnpm prepare:packaging` generates: the static notices in the root `THIRD_PARTY_NOTICES.txt` and the bundled FFmpeg `-version` and `-L` output.

## macOS

The release is a DMG and the updater's `.app.tar.gz`. FFmpeg is inside the app's resources, the player in `Contents/Frameworks`. The app is ad-hoc signed and not notarized; `packaging/install/INSTALL-macos.txt` tells users how to open it.

## FFmpeg Updates

`pnpm install` runs `scripts/install-ffmpeg.mjs`. Windows and Linux are pinned to the BtbN master snapshot `autobuild-2026-09-30-13-08` (`N-127021-ge0c94b2d1c`), which has the Vulkan performance fixes missing from the 9.0 branch and predates the master CUDA `overlay_cuda` regression. BtbN prunes old snapshots, so mirror the archives before this one disappears. macOS is pinned to FFmpeg `9.0`. Every platform validates the pinned version, the required encoders and filters, and the `ffprobe` executable. Set `OVRLEY_FFMPEG_ARCHIVE_URL` to test or pin a different archive source, or set `OVRLEY_SKIP_FFMPEG_INSTALL=1` when preparing a build environment manually.
