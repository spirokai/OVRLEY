# Patches

## telemetry-parser-dji-camera-metadata.patch

**Why:** telemetry-parser reads DJI per-frame `FrameMetaOfCamera` protobuf fields internally but does not insert them into the tag map. This patch adds `insert_tag!()` calls that expose ISO, shutter speed, white balance, and (for WA530/OQ101) f-number, focal length, and EV as structured `TagId` values.

**Applied by:** `build.rs` at compile time against cargo's git checkout of the pinned upstream commit.

**Upgrade:** When bumping the `rev` in `Cargo.toml`, if `src/dji/mod.rs` changed upstream the patch will fail to apply cleanly. Fix by rebasing the patch onto the new commit via `git format-patch`/`git am`, or remove this patch once upstream merges equivalent functionality.
